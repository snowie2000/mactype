#[path = "support/identity.rs"]
mod identity_support;
#[path = "support/inspector.rs"]
mod inspector_support;

use mactype_service_contract::StructuredServiceError;
use mactype_service_host::{
    InspectedProcess, ProcessFacts, ProcessIdentity, ProcessTargetDecision, ProcessTargetValidator,
    SkipReason, TargetLifecycle,
};

use identity_support::identity;
use inspector_support::{InspectorResponse, ScriptedInspector};

fn facts() -> ProcessFacts {
    ProcessFacts {
        critical_or_unknown: false,
        prohibits_dynamic_code: false,
        restricts_binary_signature: false,
        image_name: Some("target.exe".to_owned()),
    }
}

fn inspected(identity: ProcessIdentity, facts: ProcessFacts) -> InspectedProcess {
    InspectedProcess { identity, facts }
}

fn inspector(inspected: InspectedProcess) -> ScriptedInspector {
    ScriptedInspector::new([(
        inspected.identity.pid,
        InspectorResponse::inspected(inspected, TargetLifecycle::Running),
    )])
}

#[test]
fn validator_returns_only_verified_eligible_identity() {
    let inspector = inspector(inspected(identity(42, 100), facts()));
    let validator = ProcessTargetValidator::new(900, &inspector);

    assert_eq!(
        validator.validate(42).unwrap(),
        ProcessTargetDecision::Eligible(identity(42, 100))
    );
}

#[test]
fn validator_classifies_every_normal_skip_reason() {
    let mut session_zero = identity(42, 100);
    session_zero.session_id = 0;
    let mut protected = identity(42, 100);
    protected.protected = true;

    let rows = [
        (
            900,
            inspected(identity(900, 100), facts()),
            TargetLifecycle::Running,
            SkipReason::SelfProcess,
        ),
        (
            42,
            inspected(session_zero, facts()),
            TargetLifecycle::Running,
            SkipReason::SessionZero,
        ),
        (
            42,
            inspected(protected, facts()),
            TargetLifecycle::Running,
            SkipReason::Protected,
        ),
        (
            42,
            inspected(
                identity(42, 100),
                ProcessFacts {
                    critical_or_unknown: true,
                    ..facts()
                },
            ),
            TargetLifecycle::Running,
            SkipReason::CriticalOrUnknown,
        ),
        (
            42,
            inspected(
                identity(42, 100),
                ProcessFacts {
                    prohibits_dynamic_code: true,
                    ..facts()
                },
            ),
            TargetLifecycle::Running,
            SkipReason::DynamicCodeMitigation,
        ),
        (
            42,
            inspected(
                identity(42, 100),
                ProcessFacts {
                    restricts_binary_signature: true,
                    ..facts()
                },
            ),
            TargetLifecycle::Running,
            SkipReason::BinarySignatureMitigation,
        ),
        (
            42,
            inspected(
                identity(42, 100),
                ProcessFacts {
                    image_name: Some("services.exe".to_owned()),
                    ..facts()
                },
            ),
            TargetLifecycle::Running,
            SkipReason::ImportantWindowsProcess,
        ),
        (
            42,
            inspected(
                identity(42, 100),
                ProcessFacts {
                    image_name: Some("_unins001.tmp".to_owned()),
                    ..facts()
                },
            ),
            TargetLifecycle::Running,
            SkipReason::InstallerControlProcess,
        ),
        (
            42,
            inspected(
                identity(42, 100),
                ProcessFacts {
                    image_name: None,
                    ..facts()
                },
            ),
            TargetLifecycle::Running,
            SkipReason::ImageNameUnavailable,
        ),
        (
            42,
            inspected(identity(42, 100), facts()),
            TargetLifecycle::Exiting,
            SkipReason::Exiting,
        ),
    ];

    for (pid, inspected, lifecycle, reason) in rows {
        let inspector =
            ScriptedInspector::new([(pid, InspectorResponse::inspected(inspected, lifecycle))]);
        assert_eq!(
            ProcessTargetValidator::new(900, &inspector)
                .validate(pid)
                .unwrap(),
            ProcessTargetDecision::Skipped(reason)
        );
    }
}

#[test]
fn validator_classifies_target_scoped_inspection_failures() {
    for code in [
        "process-protected-or-inaccessible",
        "process-creation-time-unavailable",
        "process-session-unavailable",
        "process-architecture-unavailable",
        "process-architecture-unsupported",
    ] {
        let inspector = ScriptedInspector::new([(
            42,
            InspectorResponse::failure(StructuredServiceError {
                code: code.to_owned(),
                message: "target disappeared or cannot be inspected".to_owned(),
                win32_error: Some(5),
            }),
        )]);
        assert_eq!(
            ProcessTargetValidator::new(900, &inspector)
                .validate(42)
                .unwrap(),
            ProcessTargetDecision::Skipped(SkipReason::InspectionFailed(code))
        );
    }
}

#[test]
fn validator_rejects_identity_mismatch_and_propagates_infrastructure_failures() {
    let mismatch = ScriptedInspector::new([(
        42,
        InspectorResponse::inspected(
            inspected(identity(43, 100), facts()),
            TargetLifecycle::Running,
        ),
    )]);
    let error = ProcessTargetValidator::new(900, &mismatch)
        .validate(42)
        .unwrap_err();
    assert_eq!(error.code, "process-identity-mismatch");

    let infrastructure = ScriptedInspector::new([(
        42,
        InspectorResponse::failure(StructuredServiceError {
            code: "process-inspector-unavailable".to_owned(),
            message: "inspector initialization failed".to_owned(),
            win32_error: Some(6),
        }),
    )]);
    let error = ProcessTargetValidator::new(900, &infrastructure)
        .validate(42)
        .unwrap_err();
    assert_eq!(error.code, "process-inspector-unavailable");
}
