use mactype_service_contract::StructuredServiceError;
use mactype_service_host::{
    ProcessArchitecture, ProcessIdentity, ProcessInspector, ProcessTargetDecision,
    ProcessTargetValidator,
};

struct FixedInspector(Result<ProcessIdentity, StructuredServiceError>);

impl ProcessInspector for FixedInspector {
    fn inspect(&self, _pid: u32) -> Result<ProcessIdentity, StructuredServiceError> {
        self.0.clone()
    }
}

fn identity(pid: u32) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
        critical: false,
    }
}

#[test]
fn validator_returns_only_verified_eligible_identity() {
    let inspector = FixedInspector(Ok(identity(42)));
    let validator = ProcessTargetValidator::new(900, &inspector);

    assert_eq!(
        validator.validate(42).unwrap(),
        ProcessTargetDecision::Eligible(identity(42))
    );
}

#[test]
fn validator_skips_self_session_zero_protected_critical_and_target_scoped_failures() {
    for candidate in [
        identity(900),
        ProcessIdentity {
            session_id: 0,
            ..identity(42)
        },
        ProcessIdentity {
            protected: true,
            ..identity(42)
        },
        ProcessIdentity {
            critical: true,
            ..identity(42)
        },
    ] {
        let pid = candidate.pid;
        let inspector = FixedInspector(Ok(candidate));
        assert_eq!(
            ProcessTargetValidator::new(900, &inspector)
                .validate(pid)
                .unwrap(),
            ProcessTargetDecision::Skipped
        );
    }

    for code in [
        "process-protected-or-inaccessible",
        "process-creation-time-unavailable",
        "process-session-unavailable",
        "process-architecture-unavailable",
        "process-architecture-unsupported",
    ] {
        let inspector = FixedInspector(Err(StructuredServiceError {
            code: code.to_owned(),
            message: "target disappeared or cannot be inspected".to_owned(),
            win32_error: Some(5),
        }));
        assert_eq!(
            ProcessTargetValidator::new(900, &inspector)
                .validate(42)
                .unwrap(),
            ProcessTargetDecision::Skipped
        );
    }
}

#[test]
fn validator_rejects_identity_mismatch_and_propagates_infrastructure_failures() {
    let mismatch = FixedInspector(Ok(identity(43)));
    let error = ProcessTargetValidator::new(900, &mismatch)
        .validate(42)
        .unwrap_err();
    assert_eq!(error.code, "process-identity-mismatch");

    let infrastructure = FixedInspector(Err(StructuredServiceError {
        code: "process-inspector-unavailable".to_owned(),
        message: "inspector initialization failed".to_owned(),
        win32_error: Some(6),
    }));
    let error = ProcessTargetValidator::new(900, &infrastructure)
        .validate(42)
        .unwrap_err();
    assert_eq!(error.code, "process-inspector-unavailable");
}
