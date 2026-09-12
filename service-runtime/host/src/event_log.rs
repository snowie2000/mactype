#![forbid(unsafe_code)]

use mactype_service_contract::{
    event_log::{
        EventArea, EventLogWriter, EventSeverity, EventSource, EventThrottle,
        MAX_EVENT_DETAIL_BYTES,
    },
    HealthState, StructuredServiceError,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::{ProcessArchitecture, ProcessAttemptRecord, ProcessOutcome};

const SUMMARY_WINDOW: Duration = Duration::from_secs(60);
const DEDUPE_WINDOW: Duration = Duration::from_secs(10 * 60);

static LOGGER: OnceLock<Mutex<HostEventLogger>> = OnceLock::new();

pub(crate) fn initialize(path: PathBuf) {
    let _ = LOGGER.set(Mutex::new(HostEventLogger::new(path, Instant::now())));
}

pub(crate) fn service_started(version: &str) {
    with_logger(|logger| logger.service_started(version));
}

pub(crate) fn service_start_skipped(error: &StructuredServiceError) {
    with_logger(|logger| logger.service_start_skipped(error));
}

pub(crate) fn service_stopped() {
    with_logger(|logger| {
        logger.flush_summary(Instant::now());
        logger.write(
            EventSeverity::Info,
            EventArea::Service,
            "service-stopped",
            BTreeMap::new(),
            None,
        );
    });
}

pub(crate) fn health_changed(state: HealthState, error: Option<&StructuredServiceError>) {
    with_logger(|logger| logger.health_changed(state, error));
}

pub(crate) fn injection_result(record: &ProcessAttemptRecord, process: String) {
    with_logger(|logger| {
        let detail = if matches!(
            record.outcome,
            ProcessOutcome::Rejected | ProcessOutcome::RetryExhausted
        ) {
            diagnostic_detail(record)
        } else {
            String::new()
        };
        logger.injection_result(record, process, detail, Instant::now());
    });
}

pub(crate) fn injection_skipped() {
    with_logger(|logger| logger.injection_skipped(Instant::now()));
}

pub(crate) fn flush_elapsed_injection_summary() {
    with_logger(|logger| logger.flush_elapsed_summary(Instant::now()));
}

pub(crate) fn helper_broker_failed(
    architecture: ProcessArchitecture,
    code: &str,
    detail: Option<String>,
) {
    with_logger(|logger| logger.helper_broker_failed(architecture, code, detail, Instant::now()));
}

fn with_logger(action: impl FnOnce(&mut HostEventLogger)) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    if let Ok(mut logger) = logger.lock() {
        action(&mut logger);
    }
}

struct HostEventLogger {
    writer: EventLogWriter,
    write_error_reported: bool,
    health: Option<HealthState>,
    summary: InjectionSummary,
    injection_throttle: EventThrottle,
    helper_throttle: EventThrottle,
}

#[derive(Default)]
struct InjectionCounts {
    injected: u64,
    failed: u64,
    skipped: u64,
}

struct InjectionSummary {
    window_start: Instant,
    counts: InjectionCounts,
}

impl HostEventLogger {
    fn new(path: PathBuf, now: Instant) -> Self {
        Self {
            writer: EventLogWriter::new(path),
            write_error_reported: false,
            health: None,
            summary: InjectionSummary {
                window_start: now,
                counts: InjectionCounts::default(),
            },
            injection_throttle: EventThrottle::default(),
            helper_throttle: EventThrottle::default(),
        }
    }

    fn service_started(&mut self, version: &str) {
        self.write(
            EventSeverity::Info,
            EventArea::Service,
            "service-started",
            BTreeMap::from([("version".to_owned(), version.to_owned())]),
            None,
        );
        self.health = Some(HealthState::Ready);
    }

    fn service_start_skipped(&mut self, error: &StructuredServiceError) {
        self.write(
            EventSeverity::Info,
            EventArea::Service,
            "service-start-skipped",
            BTreeMap::from([
                ("message".to_owned(), error.message.clone()),
                ("reason".to_owned(), error.code.clone()),
            ]),
            None,
        );
    }

    fn health_changed(&mut self, state: HealthState, error: Option<&StructuredServiceError>) {
        if self.health == Some(state) {
            return;
        }
        let recovered = state == HealthState::Ready
            && self.health.is_some_and(|previous| {
                matches!(previous, HealthState::Degraded | HealthState::Failed)
            });
        self.health = Some(state);
        if !matches!(state, HealthState::Degraded | HealthState::Failed) && !recovered {
            return;
        }
        let severity = match state {
            HealthState::Degraded => EventSeverity::Notice,
            HealthState::Failed => EventSeverity::Error,
            HealthState::Ready => EventSeverity::Info,
            _ => return,
        };
        let mut params = BTreeMap::from([("state".to_owned(), health_name(state).to_owned())]);
        if let Some(error) = error {
            params.insert("code".to_owned(), error.code.clone());
            params.insert("message".to_owned(), error.message.clone());
        }
        self.write(
            severity,
            EventArea::Service,
            "service-health-changed",
            params,
            None,
        );
    }

    fn injection_skipped(&mut self, now: Instant) {
        self.flush_elapsed_summary(now);
        self.summary.counts.skipped += 1;
    }

    fn injection_result(
        &mut self,
        record: &ProcessAttemptRecord,
        process: String,
        detail: String,
        now: Instant,
    ) {
        self.flush_elapsed_summary(now);
        match record.outcome {
            ProcessOutcome::Injected => self.summary.counts.injected += 1,
            ProcessOutcome::Skipped => self.summary.counts.skipped += 1,
            ProcessOutcome::Rejected | ProcessOutcome::RetryExhausted => {
                self.summary.counts.failed += 1;
                let key = format!("{process}|{}", record.code);
                if self.injection_throttle.allow(&key, now, DEDUPE_WINDOW) {
                    self.write(
                        EventSeverity::Warning,
                        EventArea::Injection,
                        "injection-failed",
                        BTreeMap::from([
                            ("process".to_owned(), process),
                            ("reason".to_owned(), record.code.clone()),
                        ]),
                        Some(detail),
                    );
                }
            }
            ProcessOutcome::Deferred | ProcessOutcome::Duplicate | ProcessOutcome::Cancelled => {}
        }
    }

    fn helper_broker_failed(
        &mut self,
        architecture: ProcessArchitecture,
        code: &str,
        detail: Option<String>,
        now: Instant,
    ) {
        let architecture = match architecture {
            ProcessArchitecture::X86 => "x86",
            ProcessArchitecture::X64 => "x64",
        };
        if !self.helper_throttle.allow(architecture, now, DEDUPE_WINDOW) {
            return;
        }
        self.write(
            EventSeverity::Error,
            EventArea::Injection,
            "helper-broker-failed",
            BTreeMap::from([
                ("architecture".to_owned(), architecture.to_owned()),
                ("code".to_owned(), code.to_owned()),
            ]),
            detail,
        );
    }

    fn flush_elapsed_summary(&mut self, now: Instant) {
        if now.saturating_duration_since(self.summary.window_start) >= SUMMARY_WINDOW {
            self.flush_summary(now);
        }
    }

    fn flush_summary(&mut self, now: Instant) {
        let counts = std::mem::take(&mut self.summary.counts);
        self.summary.window_start = now;
        if counts.injected == 0 && counts.failed == 0 && counts.skipped == 0 {
            return;
        }
        self.write(
            EventSeverity::Info,
            EventArea::Injection,
            "injection-summary",
            BTreeMap::from([
                ("injected".to_owned(), counts.injected.to_string()),
                ("failed".to_owned(), counts.failed.to_string()),
                ("skipped".to_owned(), counts.skipped.to_string()),
            ]),
            None,
        );
    }

    fn write(
        &mut self,
        severity: EventSeverity,
        area: EventArea,
        code: &str,
        params: BTreeMap<String, String>,
        detail: Option<String>,
    ) {
        let params = params
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        if let Err(error) = self.writer.record(
            EventSource::ServiceHost,
            severity,
            area,
            code,
            &params,
            detail.as_deref(),
            &[],
        ) {
            if !self.write_error_reported {
                let error = error.to_string().replace(['\r', '\n'], " ");
                eprintln!("recording the service event log failed: {error}");
                self.write_error_reported = true;
            }
        }
    }
}

fn health_name(state: HealthState) -> &'static str {
    match state {
        HealthState::Unknown => "unknown",
        HealthState::Initializing => "initializing",
        HealthState::Ready => "ready",
        HealthState::Degraded => "degraded",
        HealthState::Failed => "failed",
    }
}

fn diagnostic_detail(record: &ProcessAttemptRecord) -> String {
    let detail = format!(
        "pid={} creation_time={} session_id={} disposition={:?} attempts={} reason={} win32={:?}",
        record.identity.pid,
        record.identity.creation_time,
        record.identity.session_id,
        record.outcome,
        record.attempts,
        record.code,
        record.win32_error
    );
    mactype_service_contract::event_log::sanitize_text(&detail, &[], MAX_EVENT_DETAIL_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProcessIdentity;
    use mactype_service_contract::event_log::read_events;

    fn result(outcome: ProcessOutcome, code: &str) -> ProcessAttemptRecord {
        ProcessAttemptRecord {
            identity: ProcessIdentity {
                pid: 7,
                creation_time: 8,
                session_id: 9,
                architecture: ProcessArchitecture::X64,
                protected: false,
                critical: false,
            },
            runtime_generation_id: "a".repeat(64),
            outcome,
            attempts: 1,
            code: code.to_owned(),
            win32_error: Some(5),
        }
    }

    #[test]
    fn supported_stop_records_an_info_event_with_the_reason_and_message() {
        let root =
            std::env::temp_dir().join(format!("host-service-start-skipped-{}", std::process::id()));
        let path = root.join("host.log");
        let mut logger = HostEventLogger::new(path.clone(), Instant::now());
        let error = StructuredServiceError {
            code: "runtime-profile-absent".to_owned(),
            message: "the generated profile is absent".to_owned(),
            win32_error: None,
        };

        logger.service_start_skipped(&error);

        let events = read_events(&[path], 20);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].severity, EventSeverity::Info);
        assert_eq!(events[0].code, "service-start-skipped");
        assert_eq!(
            events[0].params.get("reason").map(String::as_str),
            Some("runtime-profile-absent")
        );
        assert_eq!(
            events[0].params.get("message").map(String::as_str),
            Some("the generated profile is absent")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn injectable_clock_flushes_summary_and_resets_window() {
        let root = std::env::temp_dir().join(format!("host-summary-{}", std::process::id()));
        let path = root.join("host.log");
        let start = Instant::now();
        let mut logger = HostEventLogger::new(path.clone(), start);
        logger.injection_result(
            &result(ProcessOutcome::Injected, "ok"),
            "a.exe".to_owned(),
            String::new(),
            start,
        );
        logger.injection_result(
            &result(ProcessOutcome::Skipped, "quiet"),
            "b.exe".to_owned(),
            String::new(),
            start + SUMMARY_WINDOW,
        );
        logger.flush_summary(start + SUMMARY_WINDOW + Duration::from_secs(1));
        let events = read_events(&[path], 20);
        let summaries = events
            .iter()
            .filter(|event| event.code == "injection-summary")
            .collect::<Vec<_>>();
        assert_eq!(summaries.len(), 2);
        assert_eq!(
            summaries[0].params.get("injected").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            summaries[1].params.get("skipped").map(String::as_str),
            Some("1")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn quiet_skip_is_counted_without_a_warning_event() {
        let root = std::env::temp_dir().join(format!("host-quiet-skip-{}", std::process::id()));
        let path = root.join("host.log");
        let start = Instant::now();
        let mut logger = HostEventLogger::new(path.clone(), start);
        logger.injection_result(
            &result(ProcessOutcome::Skipped, "protected-process"),
            String::new(),
            String::new(),
            start,
        );
        logger.flush_summary(start + SUMMARY_WINDOW);
        let events = read_events(&[path], 20);
        assert!(events.iter().all(|event| event.code != "injection-failed"));
        assert_eq!(
            events[0].params.get("skipped").map(String::as_str),
            Some("1")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn helper_failures_are_deduplicated_per_architecture() {
        let root = std::env::temp_dir().join(format!("host-helper-dedupe-{}", std::process::id()));
        let path = root.join("host.log");
        let start = Instant::now();
        let mut logger = HostEventLogger::new(path.clone(), start);
        logger.helper_broker_failed(ProcessArchitecture::X86, "first", None, start);
        logger.helper_broker_failed(
            ProcessArchitecture::X86,
            "second",
            None,
            start + Duration::from_secs(1),
        );
        logger.helper_broker_failed(
            ProcessArchitecture::X64,
            "third",
            None,
            start + Duration::from_secs(1),
        );
        let events = read_events(&[path], 20);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events
                .iter()
                .map(|event| event.params["architecture"].as_str())
                .collect::<Vec<_>>(),
            ["x86", "x64"]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn injectable_clock_deduplicates_failures_for_ten_minutes() {
        let root = std::env::temp_dir().join(format!("host-dedupe-{}", std::process::id()));
        let path = root.join("host.log");
        let start = Instant::now();
        let mut logger = HostEventLogger::new(path.clone(), start);
        let failure = result(ProcessOutcome::Rejected, "denied");
        logger.injection_result(&failure, "game.exe".to_owned(), "first".to_owned(), start);
        logger.injection_result(
            &failure,
            "game.exe".to_owned(),
            "second".to_owned(),
            start + Duration::from_secs(599),
        );
        logger.injection_result(
            &failure,
            "game.exe".to_owned(),
            "third".to_owned(),
            start + Duration::from_secs(600),
        );
        let events = read_events(&[path], 20);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "injection-failed")
                .count(),
            2
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
