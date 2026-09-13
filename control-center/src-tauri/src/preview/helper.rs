use super::{
    protocol::{
        read_frame, write_frame, Frame, HELLO, HELLO_ACK, NATIVE_PREVIEW_STATE, SHUTDOWN, VERSION,
    },
    render::NativePreviewState,
    PreviewEngine,
};
use mactype_service_contract::event_log::EventSeverity;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, VecDeque},
    env,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter};

const DIAGNOSTIC_LIMIT: usize = 100;

fn helper_path() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("MACTYPE_PREVIEW_HELPER") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
    }
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    if let Some(parent) = executable.parent() {
        let installed = parent.join("mactype-preview32.exe");
        if installed.is_file() {
            return Ok(installed);
        }
    }
    for ancestor in executable.ancestors() {
        for relative in [
            Path::new("build/preview-helper/Release/mactype-preview32.exe"),
            Path::new("preview-helper/build/Release/mactype-preview32.exe"),
        ] {
            let candidate = ancestor.join(relative);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(
        "mactype-preview32.exe was not found beside the app or in a development build directory"
            .to_owned(),
    )
}

type EventCallback = Box<dyn Fn(Frame) + Send + 'static>;

fn route_frame(
    frame: Frame,
    responses: &mpsc::Sender<Result<Frame, String>>,
    on_event: &dyn Fn(Frame),
) -> bool {
    if frame.request_id == 0 {
        on_event(frame);
        true
    } else {
        responses.send(Ok(frame)).is_ok()
    }
}

fn parse_native_preview_event(frame: &Frame) -> Result<NativePreviewState, String> {
    if frame.kind != NATIVE_PREVIEW_STATE {
        return Err(format!(
            "ignored unsolicited preview frame with kind {}",
            frame.kind
        ));
    }
    serde_json::from_slice(&frame.json)
        .map_err(|error| format!("ignored malformed unsolicited native preview state: {error}"))
}

struct HelperProcess {
    child: Child,
    input: ChildStdin,
    responses: mpsc::Receiver<Result<Frame, String>>,
    diagnostics: Arc<Mutex<VecDeque<String>>>,
}

impl HelperProcess {
    fn spawn(
        install_root: &Path,
        engine: PreviewEngine,
        event_callback: EventCallback,
    ) -> Result<Self, String> {
        let mut command = Command::new(helper_path()?);
        command.arg("--install-root").arg(install_root);
        if engine == PreviewEngine::Plain {
            command.arg("--engine").arg("plain");
            // Set the existing renderer opt-out before process creation: the
            // service can inject before the helper reaches its plain startup.
            command.env_remove("MACTYPE_FORCE_LOAD");
            command.env("MACTYPE_FORCE_EXCLUDE", "1");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| "preview helper stdin is unavailable".to_owned())?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| "preview helper stdout is unavailable".to_owned())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "preview helper stderr is unavailable".to_owned())?;
        let diagnostics = Arc::new(Mutex::new(VecDeque::with_capacity(DIAGNOSTIC_LIMIT)));
        let (sender, responses) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                match read_frame(&mut reader) {
                    Ok(frame) => {
                        if !route_frame(frame, &sender, event_callback.as_ref()) {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });
        let diagnostic_target = Arc::clone(&diagnostics);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Ok(mut entries) = diagnostic_target.lock() {
                    if entries.len() == DIAGNOSTIC_LIMIT {
                        entries.pop_front();
                    }
                    entries.push_back(line);
                }
            }
        });
        Ok(Self {
            child,
            input,
            responses,
            diagnostics,
        })
    }

    fn request(&mut self, frame: Frame) -> Result<Frame, String> {
        let expected_id = frame.request_id;
        write_frame(&mut self.input, &frame)?;
        let response = self
            .responses
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => {
                    "preview helper did not respond within 2 seconds".to_owned()
                }
                mpsc::RecvTimeoutError::Disconnected => {
                    "preview helper stopped before responding".to_owned()
                }
            })??;
        if response.request_id != expected_id {
            return Err("preview helper returned a mismatched request id".to_owned());
        }
        if response.kind == super::protocol::ERROR {
            return Err(format!("preview helper error: {}", response.json_text()?));
        }
        Ok(response)
    }

    fn stop(&mut self, request_id: u64) {
        let _ = self.request(Frame {
            kind: SHUTDOWN,
            request_id,
            json: Vec::new(),
            binary: Vec::new(),
        });
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct EngineProcess {
    helper: Option<HelperProcess>,
    install_root: Option<PathBuf>,
    core_version: Option<u32>,
}

#[derive(Default)]
pub(super) struct PreviewManager {
    mactype: EngineProcess,
    plain: EngineProcess,
    next_request_id: u64,
    diagnostics: Arc<Mutex<VecDeque<(PreviewEngine, String)>>>,
    app: Arc<Mutex<Option<AppHandle>>>,
}

impl PreviewManager {
    fn slot(&self, engine: PreviewEngine) -> &EngineProcess {
        match engine {
            PreviewEngine::Mactype => &self.mactype,
            PreviewEngine::Plain => &self.plain,
        }
    }

    fn slot_mut(&mut self, engine: PreviewEngine) -> &mut EngineProcess {
        match engine {
            PreviewEngine::Mactype => &mut self.mactype,
            PreviewEngine::Plain => &mut self.plain,
        }
    }

    fn next_id(&mut self) -> u64 {
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        self.next_request_id
    }

    fn record_diagnostic(&mut self, engine: PreviewEngine, message: String) {
        if let Ok(mut diagnostics) = self.diagnostics.lock() {
            if diagnostics.len() == DIAGNOSTIC_LIMIT {
                diagnostics.pop_front();
            }
            diagnostics.push_back((engine, message));
        }
    }

    pub(super) fn attach_app(&mut self, app: AppHandle) {
        if let Ok(mut attached) = self.app.lock() {
            if attached.is_none() {
                *attached = Some(app);
            }
        }
    }

    fn event_callback(&self, engine: PreviewEngine) -> EventCallback {
        let app = Arc::clone(&self.app);
        let diagnostics = Arc::clone(&self.diagnostics);
        Box::new(move |frame| match parse_native_preview_event(&frame) {
            Ok(state) => {
                if let Ok(attached) = app.lock() {
                    if let Some(app) = attached.as_ref() {
                        let _ = app.emit("native-preview:state", state);
                    }
                }
            }
            Err(line) => {
                if let Ok(mut entries) = diagnostics.lock() {
                    if entries.len() == DIAGNOSTIC_LIMIT {
                        entries.pop_front();
                    }
                    entries.push_back((engine, line));
                }
            }
        })
    }

    fn start(&mut self, install_root: &Path, engine: PreviewEngine) -> Result<(), String> {
        let mut helper = HelperProcess::spawn(install_root, engine, self.event_callback(engine))?;
        let hello_request_id = self.next_id();
        let hello = match helper.request(Frame {
            kind: HELLO,
            request_id: hello_request_id,
            json: br#"{"client":"mactype-control-center","protocolVersion":1}"#.to_vec(),
            binary: Vec::new(),
        }) {
            Ok(response) => response,
            Err(error) => {
                helper.stop(self.next_id());
                return Err(error);
            }
        };
        if hello.kind != HELLO_ACK {
            helper.stop(self.next_id());
            return Err("preview helper did not acknowledge the protocol".to_owned());
        }
        let metadata: HelloMetadata = match serde_json::from_slice(&hello.json) {
            Ok(metadata) => metadata,
            Err(error) => {
                helper.stop(self.next_id());
                return Err(error.to_string());
            }
        };
        if !valid_hello(engine, &metadata) {
            helper.stop(self.next_id());
            return Err("preview helper returned invalid core metadata".to_owned());
        }
        let slot = self.slot_mut(engine);
        slot.install_root = Some(install_root.to_path_buf());
        slot.core_version = Some(metadata.core_version);
        slot.helper = Some(helper);
        if engine == PreviewEngine::Mactype {
            crate::diagnostics::record_preview_event(
                EventSeverity::Info,
                "preview-helper-connected",
                BTreeMap::from([
                    ("architecture".to_owned(), "x86".to_owned()),
                    (
                        "coreVersion".to_owned(),
                        super::installation::format_core_version(metadata.core_version),
                    ),
                ]),
            );
        }
        Ok(())
    }

    pub(super) fn stop_engine(&mut self, engine: PreviewEngine) {
        let id = self.next_id();
        let mut helper = self.slot_mut(engine).helper.take();
        if let Some(ref mut process) = helper {
            let entries = process
                .diagnostics
                .lock()
                .map(|entries| entries.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            for entry in entries {
                self.record_diagnostic(engine, entry);
            }
            process.stop(id);
        }
        let slot = self.slot_mut(engine);
        slot.install_root = None;
        slot.core_version = None;
    }

    fn stop(&mut self) {
        self.stop_engine(PreviewEngine::Mactype);
        self.stop_engine(PreviewEngine::Plain);
    }

    pub(super) fn request_built<F>(
        &mut self,
        install_root: &Path,
        engine: PreviewEngine,
        kind: u16,
        mut build_json: F,
    ) -> Result<Frame, String>
    where
        F: FnMut(u64) -> Result<Vec<u8>, String>,
    {
        for attempt in 0..2 {
            let slot = self.slot(engine);
            if slot.install_root.as_deref() != Some(install_root) || slot.helper.is_none() {
                self.stop_engine(engine);
                if let Err(error) = self.start(install_root, engine) {
                    record_helper_failure(engine, &error);
                    return Err(error);
                }
            }
            let request_id = self.next_id();
            let json = build_json(request_id)?;
            let result = self
                .slot_mut(engine)
                .helper
                .as_mut()
                .ok_or_else(|| "preview helper is unavailable after startup".to_owned())?
                .request(Frame {
                    kind,
                    request_id,
                    json,
                    binary: Vec::new(),
                });
            match result {
                Ok(response) => return Ok(response),
                Err(error) if attempt == 0 => {
                    self.record_diagnostic(
                        engine,
                        format!("helper restart after request failure: {error}"),
                    );
                    if engine == PreviewEngine::Mactype {
                        crate::diagnostics::record_preview_event(
                            EventSeverity::Warning,
                            "preview-helper-restarted",
                            BTreeMap::new(),
                        );
                    }
                    self.stop_engine(engine);
                }
                Err(error) => return Err(error),
            }
        }
        Err("preview helper retry loop ended unexpectedly".to_owned())
    }

    pub(super) fn request(
        &mut self,
        install_root: &Path,
        engine: PreviewEngine,
        kind: u16,
        json: Vec<u8>,
    ) -> Result<Frame, String> {
        self.request_built(install_root, engine, kind, |_| Ok(json.clone()))
    }

    pub(super) fn diagnostics(&self) -> Vec<String> {
        let mut result = self
            .diagnostics
            .lock()
            .map(|entries| {
                entries
                    .iter()
                    .map(|(engine, entry)| format!("[{}] {entry}", engine.as_str()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for engine in [PreviewEngine::Mactype, PreviewEngine::Plain] {
            if let Some(helper) = &self.slot(engine).helper {
                if let Ok(entries) = helper.diagnostics.lock() {
                    result.extend(
                        entries
                            .iter()
                            .map(|entry| format!("[{}] {entry}", engine.as_str())),
                    );
                }
            }
        }
        result
    }

    pub(super) fn probe_core_version(&mut self, install_root: &Path) -> Result<u32, String> {
        let slot = self.slot(PreviewEngine::Mactype);
        if slot.install_root.as_deref() != Some(install_root) || slot.helper.is_none() {
            self.stop_engine(PreviewEngine::Mactype);
            if let Err(error) = self.start(install_root, PreviewEngine::Mactype) {
                record_helper_failure(PreviewEngine::Mactype, &error);
                return Err(error);
            }
        }
        self.mactype
            .core_version
            .ok_or_else(|| "preview helper did not report a core version".to_owned())
    }

    pub(super) fn reconnect(&mut self, install_root: &Path) -> Result<u32, String> {
        self.stop_engine(PreviewEngine::Mactype);
        self.probe_core_version(install_root)
    }

    pub(super) fn force_terminate_for_ci(&mut self) -> Result<(), String> {
        let helper = self
            .mactype
            .helper
            .as_mut()
            .ok_or_else(|| "preview helper is not running".to_owned())?;
        helper.child.kill().map_err(|error| error.to_string())?;
        helper.child.wait().map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// A failed start of the MacType helper is an error the user should see in
/// the timeline; the plain engine failing only removes the comparison.
fn record_helper_failure(engine: PreviewEngine, error: &str) {
    if engine != PreviewEngine::Mactype {
        return;
    }
    crate::diagnostics::record_preview_event(
        EventSeverity::Error,
        "preview-helper-failed",
        BTreeMap::from([("reason".to_owned(), error.to_owned())]),
    );
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelloMetadata {
    protocol_version: u16,
    loads_mac_type: bool,
    core_version: u32,
}

fn valid_hello(engine: PreviewEngine, metadata: &HelloMetadata) -> bool {
    metadata.protocol_version == VERSION
        && match engine {
            PreviewEngine::Mactype => metadata.core_version != 0,
            PreviewEngine::Plain => !metadata.loads_mac_type && metadata.core_version == 0,
        }
}

impl Drop for PreviewManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(json: &[u8]) -> HelloMetadata {
        serde_json::from_slice(json).unwrap()
    }

    #[test]
    fn hello_metadata_requires_the_real_core_version_for_mactype() {
        let valid = metadata(
            br#"{"protocolVersion":1,"loadsMacType":true,"coreVersion":20220712,"dllGetVersion":true}"#,
        );
        assert!(valid_hello(PreviewEngine::Mactype, &valid));
        assert!(!valid_hello(PreviewEngine::Plain, &valid));
        assert!(serde_json::from_slice::<HelloMetadata>(br#"{"protocolVersion":1}"#).is_err());
    }

    #[test]
    fn route_frame_sends_unsolicited_native_state_only_to_event_callback() {
        let (sender, responses) = mpsc::channel();
        let events = Mutex::new(Vec::new());
        assert!(route_frame(
            Frame {
                kind: NATIVE_PREVIEW_STATE,
                request_id: 0,
                json: br##"{"visible":false,"displayMode":"sample","background":"#FFFFFF"}"##
                    .to_vec(),
                binary: Vec::new(),
            },
            &sender,
            &|frame| events.lock().unwrap().push(frame),
        ));
        assert!(responses.try_recv().is_err());
        let events = events.into_inner().unwrap();
        assert_eq!(events.len(), 1);
        assert!(!parse_native_preview_event(&events[0]).unwrap().visible);
    }

    #[test]
    fn unsolicited_wrong_kind_is_dropped_with_one_diagnostic() {
        let (sender, responses) = mpsc::channel();
        let diagnostics = Mutex::new(Vec::new());
        assert!(route_frame(
            Frame {
                kind: HELLO_ACK,
                request_id: 0,
                json: Vec::new(),
                binary: Vec::new(),
            },
            &sender,
            &|frame| {
                if let Err(line) = parse_native_preview_event(&frame) {
                    diagnostics.lock().unwrap().push(line);
                }
            },
        ));
        assert!(responses.try_recv().is_err());
        assert_eq!(diagnostics.into_inner().unwrap().len(), 1);
    }

    #[test]
    fn route_frame_sends_normal_response_to_response_channel() {
        let (sender, responses) = mpsc::channel();
        let events = Mutex::new(Vec::new());
        assert!(route_frame(
            Frame {
                kind: HELLO_ACK,
                request_id: 7,
                json: Vec::new(),
                binary: Vec::new(),
            },
            &sender,
            &|frame| events.lock().unwrap().push(frame),
        ));
        assert_eq!(responses.recv().unwrap().unwrap().request_id, 7);
        assert!(events.into_inner().unwrap().is_empty());
    }

    #[test]
    fn plain_hello_requires_zero_core_and_no_mactype_load() {
        let valid = metadata(
            br#"{"protocolVersion":1,"renderer":"gdi-plain","loadsMacType":false,"coreVersion":0,"dllGetVersion":false}"#,
        );
        assert!(valid_hello(PreviewEngine::Plain, &valid));
        assert!(!valid_hello(PreviewEngine::Mactype, &valid));

        let nonzero = metadata(br#"{"protocolVersion":1,"loadsMacType":false,"coreVersion":1}"#);
        assert!(!valid_hello(PreviewEngine::Plain, &nonzero));
        let loaded = metadata(br#"{"protocolVersion":1,"loadsMacType":true,"coreVersion":0}"#);
        assert!(!valid_hello(PreviewEngine::Plain, &loaded));
    }
}
