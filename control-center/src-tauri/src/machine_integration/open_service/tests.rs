mod migration;
mod profile_transfer;
mod request;
mod startup_lifecycle;
mod status;
mod windows_safety;

use super::expected_action_blocker;

#[test]
fn expected_state_blockers_are_not_classified_as_internal_failures() {
    for blocker in [
        "administrator approval was cancelled",
        "AppInit conflicts block this service change",
        "the legacy MacTray tray mode blocks this service change",
        "a legacy MacType service is still installed; migrate it first",
        "the fixed service name became foreign or inaccessible",
    ] {
        assert!(expected_action_blocker(blocker), "{blocker}");
    }
    assert!(!expected_action_blocker(
        "verify open service readiness: strict Ready timed out"
    ));
}
