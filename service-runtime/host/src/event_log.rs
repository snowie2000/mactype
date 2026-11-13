#![forbid(unsafe_code)]

use mactype_service_contract::{
    HealthState, StructuredServiceError,
    event_log::{
        EventArea, EventLogWriter, EventSeverity, EventSource, EventThrottle,
        MAX_EVENT_DETAIL_BYTES,
    },
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::{ProcessArchitecture, ProcessAttemptRecord, ProcessOutcome};

const SUMMARY_WINDOW: Duration = Duration::from_secs(60);
const DEDUPE_WINDOW: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEvent {
    ServiceStarted {
        version: String,
    },
    ServiceStartSkipped {
        error: StructuredServiceError,
    },
    ServiceStopped,
    HealthChanged {
        state: HealthState,
        error: Option<StructuredServiceError>,
    },
    InjectionResult {
        record: ProcessAttemptRecord,
        process: String,
    },
    InjectionSkipped {
        reason: &'static str,
    },
    /// One trip through the orchestration loop: how long the target took to
    /// handle, how deep the startup backlog still was, and how old the process
    /// already was when handling began. Targets are handled one helper launch
    /// at a time, so the first two numbers say whether a slow pipeline or a
    /// long queue is what keeps a newly created process waiting, and the third
    /// says how much of the wait was over before the service heard anything.
    /// Only a target the live event source announced has an age; one drained
    /// from a startup or recovery snapshot may have been running for days.
    InjectionPipelineSample {
        millis: u64,
        backlog: usize,
        age_millis: Option<u64>,
    },
    /// Which of the two process event sources the service is actually running
    /// on. It changes no event and no message parameter; it only names the
    /// source in the summary detail, where a maintainer reading a log can see
    /// whether the fast source was available on that machine.
    LiveObserverSelected(LiveObserver),
    FlushInjectionSummary,
    HelperBrokerFailed {
        architecture: ProcessArchitecture,
        code: String,
        detail: Option<String>,
    },
}

/// The process event source the service is running on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveObserver {
    /// A private real-time ETW session on the kernel process provider.
    Etw,
    /// The WMI subscription, used when no real-time session could be started.
    Wmi,
}

impl LiveObserver {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Etw => "etw",
            Self::Wmi => "wmi",
        }
    }
}

pub trait HostEventSink: Send + Sync {
    fn record(&self, event: HostEvent);
}

pub struct HostEventLogger {
    state: Mutex<HostEventLoggerState>,
}

impl HostEventLogger {
    pub fn new(path: PathBuf) -> Self {
        Self::new_at(path, Instant::now())
    }

    pub fn new_at(path: PathBuf, now: Instant) -> Self {
        Self {
            state: Mutex::new(HostEventLoggerState::new(path, now)),
        }
    }

    pub fn record_at(&self, event: HostEvent, now: Instant) {
        let mut logger = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match event {
            HostEvent::ServiceStarted { version } => logger.service_started(&version),
            HostEvent::ServiceStartSkipped { error } => logger.service_start_skipped(&error),
            HostEvent::ServiceStopped => {
                logger.flush_summary(now);
                logger.write(
                    EventSeverity::Info,
                    EventArea::Service,
                    "service-stopped",
                    BTreeMap::new(),
                    None,
                );
            }
            HostEvent::HealthChanged { state, error } => {
                logger.health_changed(state, error.as_ref());
            }
            HostEvent::InjectionResult { record, process } => {
                let detail = if matches!(
                    record.outcome,
                    ProcessOutcome::Rejected | ProcessOutcome::RetryExhausted
                ) {
                    diagnostic_detail(&record)
                } else {
                    String::new()
                };
                logger.injection_result(&record, process, detail, now);
            }
            HostEvent::InjectionSkipped { reason } => logger.injection_skipped(reason, now),
            HostEvent::InjectionPipelineSample {
                millis,
                backlog,
                age_millis,
            } => {
                logger.injection_pipeline_sample(millis, backlog, age_millis);
            }
            HostEvent::LiveObserverSelected(observer) => logger.observer = Some(observer),
            HostEvent::FlushInjectionSummary => logger.flush_elapsed_summary(now),
            HostEvent::HelperBrokerFailed {
                architecture,
                code,
                detail,
            } => logger.helper_broker_failed(architecture, &code, detail, now),
        }
    }
}

impl HostEventSink for HostEventLogger {
    fn record(&self, event: HostEvent) {
        self.record_at(event, Instant::now());
    }
}

struct HostEventLoggerState {
    writer: EventLogWriter,
    write_error_reported: bool,
    health: Option<HealthState>,
    observer: Option<LiveObserver>,
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
    /// Why targets were passed over, counted per reason. A skipped target
    /// writes no event of its own, so this is the only place the eleven skip
    /// reasons and the helper's own skip codes ever become visible.
    skip_reasons: BTreeMap<String, u64>,
    pipeline: PipelineSamples,
}

#[derive(Default)]
struct PipelineSamples {
    count: u64,
    millis_sum: u64,
    millis_max: u64,
    backlog_max: usize,
    /// Ages are counted separately because only a target the live event
    /// source announced has one; averaging over every sample would divide the
    /// live ages by a window full of snapshot targets.
    age_count: u64,
    age_sum: u64,
    age_max: u64,
}

impl PipelineSamples {
    fn record(&mut self, millis: u64, backlog: usize, age_millis: Option<u64>) {
        self.count += 1;
        self.millis_sum = self.millis_sum.saturating_add(millis);
        self.millis_max = self.millis_max.max(millis);
        self.backlog_max = self.backlog_max.max(backlog);
        if let Some(age) = age_millis {
            self.age_count += 1;
            self.age_sum = self.age_sum.saturating_add(age);
            self.age_max = self.age_max.max(age);
        }
    }

    fn average_millis(&self) -> u64 {
        self.millis_sum.checked_div(self.count).unwrap_or(0)
    }

    fn average_age_millis(&self) -> u64 {
        self.age_sum.checked_div(self.age_count).unwrap_or(0)
    }
}

impl HostEventLoggerState {
    fn new(path: PathBuf, now: Instant) -> Self {
        Self {
            writer: EventLogWriter::new(path),
            write_error_reported: false,
            health: None,
            observer: None,
            summary: InjectionSummary {
                window_start: now,
                counts: InjectionCounts::default(),
                skip_reasons: BTreeMap::new(),
                pipeline: PipelineSamples::default(),
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

    fn injection_skipped(&mut self, reason: &'static str, now: Instant) {
        self.flush_elapsed_summary(now);
        self.summary.counts.skipped += 1;
        *self
            .summary
            .skip_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    fn injection_pipeline_sample(&mut self, millis: u64, backlog: usize, age_millis: Option<u64>) {
        self.summary.pipeline.record(millis, backlog, age_millis);
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
            ProcessOutcome::Skipped => {
                self.summary.counts.skipped += 1;
                // The helper's own skip codes belong in the same tally. This is
                // what separates a target the in-process relay already injected
                // (`module-already-loaded`) from one we could not reach.
                *self
                    .summary
                    .skip_reasons
                    .entry(record.code.clone())
                    .or_default() += 1;
            }
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
        let reasons = std::mem::take(&mut self.summary.skip_reasons);
        let pipeline = std::mem::take(&mut self.summary.pipeline);
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
            summary_detail(&reasons, &pipeline, self.observer),
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
        ) && !self.write_error_reported
        {
            let error = error.to_string().replace(['\r', '\n'], " ");
            eprintln!("recording the service event log failed: {error}");
            self.write_error_reported = true;
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

/// Renders the pipeline numbers and the skip tally for one summary window.
/// Both belong in the detail rather than the parameters: parameters feed the
/// localized message templates, and every locale catalogue must carry the same
/// placeholder set, so a number that exists to be read by a maintainer would
/// cost ten translations to add and ten more to change.
fn summary_detail(
    reasons: &BTreeMap<String, u64>,
    pipeline: &PipelineSamples,
    observer: Option<LiveObserver>,
) -> Option<String> {
    let mut detail = String::new();
    if pipeline.count > 0 {
        detail.push_str(&format!(
            "handle_avg_ms={} handle_max_ms={}",
            pipeline.average_millis(),
            pipeline.millis_max,
        ));
        if pipeline.age_count > 0 {
            detail.push_str(&format!(
                " age_avg_ms={} age_max_ms={}",
                pipeline.average_age_millis(),
                pipeline.age_max,
            ));
        }
        detail.push_str(&format!(
            " backlog_max={} n={}",
            pipeline.backlog_max, pipeline.count
        ));
    }
    if let Some(observer) = observer {
        if !detail.is_empty() {
            detail.push(' ');
        }
        detail.push_str(&format!("observer={}", observer.name()));
    }
    if !reasons.is_empty() {
        if !detail.is_empty() {
            detail.push_str(" | ");
        }
        detail.push_str("skip:");
        let mut ranked: Vec<(&String, &u64)> = reasons.iter().collect();
        ranked.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
        for (reason, count) in ranked {
            detail.push_str(&format!(" {reason}={count}"));
        }
    }
    if detail.is_empty() {
        return None;
    }
    if detail.len() > MAX_EVENT_DETAIL_BYTES {
        // The reasons are helper-supplied strings, so step back to a character
        // boundary rather than trusting them to be ASCII. Cutting mid-character
        // would panic inside the service.
        let mut cut = MAX_EVENT_DETAIL_BYTES - "...".len();
        while cut > 0 && !detail.is_char_boundary(cut) {
            cut -= 1;
        }
        detail.truncate(cut);
        detail.push_str("...");
    }
    Some(detail)
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
            },
            runtime_generation_id: "a".repeat(64),
            outcome,
            attempts: 1,
            code: code.to_owned(),
            win32_error: Some(5),
        }
    }

    #[test]
    fn a_window_with_no_samples_no_skips_and_no_named_source_carries_no_detail() {
        assert_eq!(
            summary_detail(&BTreeMap::new(), &PipelineSamples::default(), None),
            None
        );
    }

    #[test]
    fn the_summary_detail_averages_the_pipeline_and_ranks_the_skip_reasons() {
        let mut pipeline = PipelineSamples::default();
        pipeline.record(10, 3, None);
        pipeline.record(30, 140, None);
        pipeline.record(20, 7, None);
        let reasons = BTreeMap::from([
            ("module-already-loaded".to_owned(), 31),
            ("session-zero".to_owned(), 120),
            ("protected".to_owned(), 31),
        ]);

        let detail =
            summary_detail(&reasons, &pipeline, None).expect("a populated window has detail");

        assert_eq!(
            detail,
            "handle_avg_ms=20 handle_max_ms=30 backlog_max=140 n=3 \
             | skip: session-zero=120 module-already-loaded=31 protected=31"
        );
    }

    #[test]
    fn ages_are_averaged_over_the_samples_that_have_one_and_the_source_is_named() {
        let mut pipeline = PipelineSamples::default();
        pipeline.record(10, 0, Some(400));
        pipeline.record(30, 0, None);
        pipeline.record(20, 0, Some(100));

        let detail = summary_detail(&BTreeMap::new(), &pipeline, Some(LiveObserver::Etw))
            .expect("a populated window has detail");

        assert_eq!(
            detail,
            "handle_avg_ms=20 handle_max_ms=30 age_avg_ms=250 age_max_ms=400 \
             backlog_max=0 n=3 observer=etw"
        );
    }

    #[test]
    fn a_window_of_snapshot_targets_alone_reports_no_age_at_all() {
        let mut pipeline = PipelineSamples::default();
        pipeline.record(5, 12, None);

        let detail = summary_detail(&BTreeMap::new(), &pipeline, Some(LiveObserver::Wmi))
            .expect("a populated window has detail");

        assert_eq!(
            detail,
            "handle_avg_ms=5 handle_max_ms=5 backlog_max=12 n=1 observer=wmi"
        );
    }

    #[test]
    fn the_source_is_named_even_in_a_window_that_only_skipped() {
        let reasons = BTreeMap::from([("session-zero".to_owned(), 2)]);

        let detail = summary_detail(
            &reasons,
            &PipelineSamples::default(),
            Some(LiveObserver::Wmi),
        )
        .expect("a skip tally has detail");

        assert_eq!(detail, "observer=wmi | skip: session-zero=2");
    }

    #[test]
    fn a_skip_tally_too_long_for_one_record_is_truncated_rather_than_dropped() {
        let reasons: BTreeMap<String, u64> = (0..MAX_EVENT_DETAIL_BYTES)
            .map(|index| (format!("reason-{index:06}"), 1))
            .collect();

        let detail = summary_detail(&reasons, &PipelineSamples::default(), None)
            .expect("a tally has detail");

        assert!(detail.len() <= MAX_EVENT_DETAIL_BYTES);
        assert!(detail.ends_with("..."));
    }

    #[test]
    fn a_helper_skip_is_counted_under_its_own_code_so_a_relayed_target_is_not_a_failure() {
        let mut state = HostEventLoggerState::new(
            std::env::temp_dir().join("host-skip-tally-unused.log"),
            Instant::now(),
        );

        state.injection_skipped("session-zero", Instant::now());
        state.injection_result(
            &result(ProcessOutcome::Skipped, "module-already-loaded"),
            "notepad.exe".to_owned(),
            String::new(),
            Instant::now(),
        );

        assert_eq!(state.summary.counts.skipped, 2);
        assert_eq!(state.summary.skip_reasons.get("session-zero"), Some(&1));
        assert_eq!(
            state.summary.skip_reasons.get("module-already-loaded"),
            Some(&1)
        );
    }

    #[test]
    fn supported_stop_records_an_info_event_with_the_reason_and_message() {
        let root =
            std::env::temp_dir().join(format!("host-service-start-skipped-{}", std::process::id()));
        let path = root.join("host.log");
        let mut logger = HostEventLoggerState::new(path.clone(), Instant::now());
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
        let mut logger = HostEventLoggerState::new(path.clone(), start);
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
        let mut logger = HostEventLoggerState::new(path.clone(), start);
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
        let mut logger = HostEventLoggerState::new(path.clone(), start);
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
        let mut logger = HostEventLoggerState::new(path.clone(), start);
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
