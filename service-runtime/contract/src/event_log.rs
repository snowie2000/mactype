#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static APPEND_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub const EVENT_LOG_SCHEMA_VERSION: u16 = 1;
pub const MAX_EVENT_LOG_BYTES: u64 = 512 * 1024;
pub const EVENT_LOG_BACKUPS: usize = 4;
pub const MAX_EVENT_CODE_BYTES: usize = 64;
pub const MAX_EVENT_PARAMS: usize = 8;
pub const MAX_EVENT_PARAM_BYTES: usize = 260;
pub const MAX_EVENT_DETAIL_BYTES: usize = 24 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventSeverity {
    Info,
    Notice,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventArea {
    Service,
    Setup,
    Profile,
    Preview,
    Injection,
    ControlCenter,
    Tray,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventSource {
    ServiceHost,
    ServiceSetup,
    ControlCenter,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventRecord {
    pub v: u16,
    pub ts: u64,
    pub severity: EventSeverity,
    pub area: EventArea,
    pub code: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub source: EventSource,
}

impl EventRecord {
    pub fn new(
        ts: u64,
        severity: EventSeverity,
        area: EventArea,
        code: impl Into<String>,
        params: BTreeMap<String, String>,
        detail: Option<String>,
        source: EventSource,
    ) -> Self {
        Self {
            v: EVENT_LOG_SCHEMA_VERSION,
            ts,
            severity,
            area,
            code: code.into(),
            params,
            detail,
            source,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EventLogWriter {
    path: PathBuf,
}

impl EventLogWriter {
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn append(&self, record: &EventRecord) -> io::Result<()> {
        let _guard = APPEND_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut line = serde_json::to_vec(record)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        line.push(b'\n');
        if line.len() as u64 > MAX_EVENT_LOG_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "event record exceeds the event-log byte limit",
            ));
        }
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "event log path has no parent directory",
            )
        })?;
        fs::create_dir_all(parent)?;
        self.rotate_if_needed(line.len() as u64)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&line)?;
        file.flush()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        source: EventSource,
        severity: EventSeverity,
        area: EventArea,
        code: &str,
        params: &[(&str, &str)],
        detail: Option<&str>,
        redactions: &[&str],
    ) -> io::Result<()> {
        let ts = unix_time_ms(SystemTime::now())?;
        let record = EventRecord::new(
            ts,
            severity,
            area,
            code,
            params
                .iter()
                .take(MAX_EVENT_PARAMS)
                .map(|(key, value)| {
                    (
                        sanitize_text(key, redactions, MAX_EVENT_PARAM_BYTES),
                        sanitize_text(value, redactions, MAX_EVENT_PARAM_BYTES),
                    )
                })
                .collect(),
            detail.map(|value| sanitize_text(value, redactions, MAX_EVENT_DETAIL_BYTES)),
            source,
        );
        let record = EventRecord {
            code: if valid_code(&record.code) {
                record.code
            } else {
                "invalid-code".to_owned()
            },
            ..record
        };
        self.append(&record)
    }

    fn rotate_if_needed(&self, incoming: u64) -> io::Result<()> {
        let current_len = match fs::metadata(&self.path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error),
        };
        if current_len.saturating_add(incoming) <= MAX_EVENT_LOG_BYTES {
            return Ok(());
        }
        for index in (1..=EVENT_LOG_BACKUPS).rev() {
            let destination = backup_path(&self.path, index);
            if destination.exists() {
                fs::remove_file(&destination)?;
            }
            let source = if index == 1 {
                self.path.clone()
            } else {
                backup_path(&self.path, index - 1)
            };
            match fs::rename(source, destination) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

pub fn read_events(paths: &[PathBuf], limit: usize) -> Vec<EventRecord> {
    if limit == 0 {
        return Vec::new();
    }
    let mut entries = Vec::new();
    let mut order = 0_u64;
    for base in paths {
        for path in (1..=EVENT_LOG_BACKUPS)
            .rev()
            .map(|index| backup_path(base, index))
            .chain(std::iter::once(base.clone()))
        {
            let bytes = match fs::read(path) {
                Ok(bytes) if bytes.len() as u64 <= MAX_EVENT_LOG_BYTES => bytes,
                _ => continue,
            };
            for line in bytes.split(|byte| *byte == b'\n') {
                let Ok(record) = serde_json::from_slice::<EventRecord>(line) else {
                    order = order.saturating_add(1);
                    continue;
                };
                if valid_record(&record) {
                    entries.push((record, order));
                }
                order = order.saturating_add(1);
            }
        }
    }
    entries.sort_by_key(|(record, file_order)| (record.ts, *file_order));
    let start = entries.len().saturating_sub(limit);
    entries
        .into_iter()
        .skip(start)
        .map(|(record, _)| record)
        .collect()
}

pub fn sanitize_text(value: &str, redactions: &[&str], maximum_bytes: usize) -> String {
    let mut sanitized = value.replace(['\r', '\n'], " ");
    for secret in redactions.iter().filter(|secret| !secret.is_empty()) {
        let normalized = secret.replace(['\r', '\n'], " ");
        sanitized = sanitized.replace(&normalized, "[redacted-profile]");
    }
    bounded_text(&redact_nonce_candidates(&sanitized), maximum_bytes)
}

pub fn profile_file_name(value: &str) -> String {
    let name = value.rsplit(['\\', '/']).next().unwrap_or(value);
    sanitize_text(name, &[], MAX_EVENT_PARAM_BYTES)
}

pub struct EventThrottle {
    entries: HashMap<String, Instant>,
}

impl Default for EventThrottle {
    fn default() -> Self {
        Self::new()
    }
}

impl EventThrottle {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn allow(&mut self, key: &str, now: Instant, window: Duration) -> bool {
        if self
            .entries
            .get(key)
            .is_some_and(|previous| now.saturating_duration_since(*previous) < window)
        {
            return false;
        }
        if !self.entries.contains_key(key) && self.entries.len() >= 256 {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, instant)| **instant)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key.to_owned(), now);
        true
    }
}

fn unix_time_ms(now: SystemTime) -> io::Result<u64> {
    let millis = now
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .as_millis();
    u64::try_from(millis).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "system timestamp exceeds the event-log format",
        )
    })
}

fn valid_record(record: &EventRecord) -> bool {
    record.v == EVENT_LOG_SCHEMA_VERSION
        && valid_code(&record.code)
        && record.params.len() <= MAX_EVENT_PARAMS
        && record.params.iter().all(|(key, value)| {
            key.len() <= MAX_EVENT_PARAM_BYTES && value.len() <= MAX_EVENT_PARAM_BYTES
        })
        && record
            .detail
            .as_ref()
            .map_or(true, |detail| detail.len() <= MAX_EVENT_DETAIL_BYTES)
}

fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= MAX_EVENT_CODE_BYTES
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn backup_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn redact_nonce_candidates(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor].is_ascii_hexdigit() {
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_hexdigit() {
                cursor += 1;
            }
            if cursor - start == 32 {
                output.push_str("[redacted-nonce]");
            } else {
                output.push_str(&value[start..cursor]);
            }
        } else {
            let character = value[cursor..]
                .chars()
                .next()
                .expect("cursor remains on a character boundary");
            output.push(character);
            cursor += character.len_utf8();
        }
    }
    output
}

fn bounded_text(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    let suffix = " [truncated]";
    if maximum_bytes <= suffix.len() {
        return suffix[..maximum_bytes].to_owned();
    }
    let mut end = maximum_bytes - suffix.len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &value[..end], suffix)
}
