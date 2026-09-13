use mactype_service_contract::event_log::{
    read_events, sanitize_text, EventArea, EventLogWriter, EventRecord, EventSeverity, EventSource,
    EventThrottle, EVENT_LOG_BACKUPS, MAX_EVENT_LOG_BYTES,
};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};

fn record(ts: u64, code: &str) -> EventRecord {
    EventRecord::new(
        ts,
        EventSeverity::Info,
        EventArea::Service,
        code,
        BTreeMap::new(),
        None,
        EventSource::ServiceHost,
    )
}

#[test]
fn three_hundred_writes_keep_bounded_rotation() {
    let root = tempfile_root("rotation");
    let path = root.join("service-host.log");
    let writer = EventLogWriter::new(path.clone());
    for index in 0..300 {
        let mut item = record(index, "fixture-event");
        item.detail = Some("x".repeat(24 * 1024));
        writer.append(&item).unwrap();
    }
    let files = fs::read_dir(&root)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(files.len() <= EVENT_LOG_BACKUPS + 1);
    assert!(files
        .iter()
        .all(|entry| entry.metadata().unwrap().len() <= MAX_EVENT_LOG_BYTES));
    assert_eq!(read_events(&[path], 1)[0].ts, 299);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sanitization_redacts_profiles_nonces_and_truncates_on_utf8_boundary() {
    let secret = "[General]\r\nPrivate=1";
    let value =
        format!("before\r\n{secret} 00112233445566778899aabbccddeeff 가나다라가나다라가나다라");
    let sanitized = sanitize_text(&value, &[secret], 64);
    assert!(!sanitized.contains(secret));
    assert!(!sanitized.contains("00112233445566778899aabbccddeeff"));
    assert!(sanitized.contains("[redacted-profile]"));
    assert!(sanitized.contains("[redacted-nonce]"));
    assert!(sanitized.ends_with(" [truncated]"));
    assert!(sanitized.len() <= 64);
}

#[test]
fn two_paths_merge_by_timestamp_then_file_order() {
    let root = tempfile_root("merge");
    let first = root.join("first.log");
    let second = root.join("second.log");
    let first_writer = EventLogWriter::new(first.clone());
    let second_writer = EventLogWriter::new(second.clone());
    first_writer.append(&record(20, "first-late")).unwrap();
    first_writer.append(&record(10, "first-tie")).unwrap();
    second_writer.append(&record(10, "second-tie")).unwrap();
    let codes = read_events(&[first, second], 20)
        .into_iter()
        .map(|record| record.code)
        .collect::<Vec<_>>();
    assert_eq!(codes, ["first-tie", "second-tie", "first-late"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reader_skips_unknown_and_malformed_lines() {
    let root = tempfile_root("unknown");
    let path = root.join("events.log");
    fs::create_dir_all(&root).unwrap();
    let good = serde_json::to_string(&record(7, "known-event")).unwrap();
    fs::write(
        &path,
        format!("{{not-json}}\n{{\"v\":99}}\n{good}\n{{\"v\":1,\"extra\":true}}\n"),
    )
    .unwrap();
    let events = read_events(&[path], 10);
    assert_eq!(events, [record(7, "known-event")]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn throttle_obeys_window_and_evicts_the_oldest_key() {
    let start = Instant::now();
    let mut throttle = EventThrottle::new();
    assert!(throttle.allow("oldest", start, Duration::from_secs(10)));
    assert!(!throttle.allow(
        "oldest",
        start + Duration::from_secs(9),
        Duration::from_secs(10)
    ));
    for index in 1..=256 {
        assert!(throttle.allow(
            &format!("key-{index}"),
            start + Duration::from_millis(index as u64),
            Duration::from_secs(10)
        ));
    }
    assert!(throttle.allow(
        "oldest",
        start + Duration::from_secs(1),
        Duration::from_secs(10)
    ));
    assert!(!throttle.allow(
        "oldest",
        start + Duration::from_secs(2),
        Duration::from_secs(10)
    ));
}

#[test]
fn record_validates_code_and_bounds_params() {
    let root = tempfile_root("record");
    let path = root.join("events.log");
    let writer = EventLogWriter::new(path.clone());
    let owned = (0..10)
        .map(|index| (format!("key-{index}"), "x".repeat(300)))
        .collect::<Vec<_>>();
    let params = owned
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    writer
        .record(
            EventSource::ServiceHost,
            EventSeverity::Warning,
            EventArea::Injection,
            "INVALID",
            &params,
            None,
            &[],
        )
        .unwrap();
    let event = read_events(&[path], 1).pop().unwrap();
    assert_eq!(event.code, "invalid-code");
    assert_eq!(event.params.len(), 8);
    assert!(event.params.values().all(|value| value.len() <= 260));
    fs::remove_dir_all(root).unwrap();
}

fn tempfile_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "mactype-event-log-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
