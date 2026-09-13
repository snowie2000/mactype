use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MigrationVerification {
    pub(super) scm_running_ready: bool,
    pub(super) active_digest_match: bool,
    pub(super) telemetry_verified: bool,
}

impl MigrationVerification {
    #[cfg(test)]
    pub(super) const fn fully_verified() -> Self {
        Self {
            scm_running_ready: true,
            active_digest_match: true,
            telemetry_verified: true,
        }
    }

    pub(super) const fn permits_removal(self) -> bool {
        self.scm_running_ready && self.active_digest_match && self.telemetry_verified
    }
}

pub(super) trait MigrationBackend {
    type OpenServiceSnapshot;

    fn prepare_legacy_backup(&mut self) -> Result<(), String>;
    fn legacy_backup_is_valid(&mut self) -> bool;
    fn capture_open_service(&mut self) -> Result<Self::OpenServiceSnapshot, String>;
    fn stop_legacy(&mut self) -> Result<(), String>;
    fn publish_profile(
        &mut self,
        snapshot: &Self::OpenServiceSnapshot,
        profile: &[u8],
    ) -> Result<(), String>;
    fn activate_open_service(&mut self, snapshot: &Self::OpenServiceSnapshot)
        -> Result<(), String>;
    fn strict_ready(&mut self, expected_digest: &str) -> Result<bool, String>;
    fn verify_injection_smoke(&mut self, expected_digest: &str) -> Result<bool, String>;
    fn complete_migration(&mut self, snapshot: &Self::OpenServiceSnapshot) -> Result<(), String>;
    fn rollback_open_service(&mut self, snapshot: &Self::OpenServiceSnapshot)
        -> Result<(), String>;
    fn restore_legacy(&mut self) -> Result<(), String>;
    fn removal_verification(
        &mut self,
        expected_digest: &str,
    ) -> Result<MigrationVerification, String>;
    fn remove_legacy(&mut self) -> Result<(), String>;
}

pub(super) fn migration_activation_actions(
    installation: InstallationState,
) -> Result<&'static [SystemServiceAction], String> {
    match installation {
        InstallationState::Absent => {
            Ok(&[SystemServiceAction::Install, SystemServiceAction::Start])
        }
        InstallationState::Outdated => {
            Ok(&[SystemServiceAction::Upgrade, SystemServiceAction::Start])
        }
        InstallationState::Current => Ok(&[SystemServiceAction::Start]),
        _ => Err("the open service snapshot cannot be activated safely".to_owned()),
    }
}

pub(super) fn migrate_from_legacy(
    backend: &mut impl MigrationBackend,
    profile: &[u8],
) -> Result<(), String> {
    backend
        .prepare_legacy_backup()
        .map_err(|error| format!("prepare legacy backup: {error}"))?;
    if !backend.legacy_backup_is_valid() {
        return Err("validate legacy backup: backup receipt is invalid".to_owned());
    }
    let snapshot = backend
        .capture_open_service()
        .map_err(|error| format!("capture open service state: {error}"))?;
    if let Err(error) = backend.stop_legacy() {
        return Err(rollback_migration_failure(
            backend,
            &snapshot,
            "stop legacy service",
            error,
        ));
    }
    if let Err(error) = backend.publish_profile(&snapshot, profile) {
        return Err(rollback_migration_failure(
            backend,
            &snapshot,
            "publish active profile",
            error,
        ));
    }
    if let Err(error) = backend.activate_open_service(&snapshot) {
        return Err(rollback_migration_failure(
            backend,
            &snapshot,
            "activate open service",
            error,
        ));
    }
    let expected = GenerationId::from_profile_bytes(profile);
    match backend.strict_ready(expected.as_str()) {
        Ok(true) => {}
        Ok(false) => {
            return Err(rollback_migration_failure(
                backend,
                &snapshot,
                "verify open service readiness",
                "strict Ready or profile digest mismatch".to_owned(),
            ));
        }
        Err(error) => {
            return Err(rollback_migration_failure(
                backend,
                &snapshot,
                "verify open service readiness",
                error,
            ));
        }
    }
    match backend.verify_injection_smoke(expected.as_str()) {
        Ok(true) => {}
        Ok(false) => {
            return Err(rollback_migration_failure(
                backend,
                &snapshot,
                "verify x86/x64 injection smoke",
                "both fixed-architecture marker processes were not injected".to_owned(),
            ));
        }
        Err(error) => {
            return Err(rollback_migration_failure(
                backend,
                &snapshot,
                "verify x86/x64 injection smoke",
                error,
            ));
        }
    }
    if let Err(error) = backend.complete_migration(&snapshot) {
        return Err(rollback_migration_failure(
            backend,
            &snapshot,
            "complete migration runtime protection",
            error,
        ));
    }
    Ok(())
}

fn rollback_migration_failure<B: MigrationBackend>(
    backend: &mut B,
    snapshot: &B::OpenServiceSnapshot,
    stage: &str,
    primary: String,
) -> String {
    let open_rollback = backend.rollback_open_service(snapshot).err();
    let legacy_rollback = backend.restore_legacy().err();
    let mut error = format!("{stage}: {primary}");
    if let Some(rollback) = open_rollback {
        error.push_str(&format!("; open service rollback failed: {rollback}"));
    }
    if let Some(rollback) = legacy_rollback {
        error.push_str(&format!("; legacy service restore failed: {rollback}"));
    }
    error
}

pub(super) fn remove_legacy_after_verification(
    backend: &mut impl MigrationBackend,
    profile: &[u8],
) -> Result<(), String> {
    if !backend.legacy_backup_is_valid() {
        return Err("validate legacy backup: backup receipt is invalid".to_owned());
    }
    let expected = GenerationId::from_profile_bytes(profile);
    let verification = backend
        .removal_verification(expected.as_str())
        .map_err(|error| format!("verify legacy removal gate: {error}"))?;
    if !verification.permits_removal() {
        return Err(
            "verify legacy removal gate: Ready, profile digest, or x86/x64 telemetry is missing"
                .to_owned(),
        );
    }
    if let Err(error) = backend.remove_legacy() {
        // Do not restore (restart) the legacy service on a removal failure. By the
        // time removal runs, the new service is already the verified live injector,
        // so restarting the legacy service would double-inject. The legacy service
        // is stopped and disabled, so leaving it in place is safe; the caller can
        // retry removal or roll back explicitly.
        return Err(format!(
            "remove legacy service: {error}; the legacy service remains stopped and disabled"
        ));
    }
    Ok(())
}
