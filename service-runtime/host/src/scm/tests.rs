use super::*;
use crate::session_event_queue::{SessionEventQueue, SESSION_EVENT_QUEUE_CAPACITY};
use crate::SessionChange;

#[test]
fn crash_once_adapter_accepts_only_the_fixed_marker_payload() {
    assert!(crash_adapter::valid_crash_once_marker(
        b"mactype-ci-crash-once\n"
    ));
    assert!(!crash_adapter::valid_crash_once_marker(b""));
    assert!(!crash_adapter::valid_crash_once_marker(
        b"mactype-ci-crash-once"
    ));
    assert!(!crash_adapter::valid_crash_once_marker(&[b'x'; 65]));
}

#[test]
fn panic_failure_snapshot_is_a_valid_structured_report() {
    let report = failed_health_report("service-panic", "panic boundary", None);
    assert_eq!(report.health, HealthState::Failed);
    assert_eq!(report.service_version, service_runtime_version());
    assert!(report.validate().is_ok());
    assert_eq!(report.last_error.unwrap().code, "service-panic");
}

#[test]
fn registered_service_returns_its_terminal_failure_to_service_main() {
    let _runner: fn(MachinePaths, &FileHealthPublisher) -> Result<(), StructuredServiceError> =
        run_registered_service;
}

#[test]
fn session_event_queue_preserves_a_burst_instead_of_overwriting_it() {
    let queue = SessionEventQueue::new();
    assert!(queue.push(5, 10));
    assert!(queue.push(6, 11));
    assert!(queue.push(7, 12));

    assert_eq!(
        queue.pop(),
        Some(SessionChange {
            event_type: 5,
            session_id: 10,
        })
    );
    assert_eq!(
        queue.pop(),
        Some(SessionChange {
            event_type: 6,
            session_id: 11,
        })
    );
    assert_eq!(
        queue.pop(),
        Some(SessionChange {
            event_type: 7,
            session_id: 12,
        })
    );
    assert_eq!(queue.pop(), None);
}

#[test]
fn session_event_queue_overflow_requests_conservative_full_invalidation() {
    let queue = SessionEventQueue::new();
    for session_id in 0..SESSION_EVENT_QUEUE_CAPACITY as u32 {
        assert!(queue.push(6, session_id));
    }
    assert!(!queue.push(6, 999));

    assert_eq!(queue.pop(), Some(SessionChange::overflow()));
}
