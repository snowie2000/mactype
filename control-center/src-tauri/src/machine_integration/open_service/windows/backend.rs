use super::*;
use crate::machine_integration::legacy_migration;

#[derive(Default)]
pub(in crate::machine_integration::open_service) struct SystemMigrationBackend {
    removal_digest: Option<String>,
    published_profile: Option<Vec<u8>>,
}

impl MigrationBackend for SystemMigrationBackend {
    type OpenServiceSnapshot = SystemOpenServiceSnapshot;

    fn prepare_legacy_backup(&mut self) -> Result<(), String> {
        legacy_migration::prepare_backup().map(|_| ())
    }

    fn legacy_backup_is_valid(&mut self) -> bool {
        legacy_migration::backup_is_valid()
    }

    fn capture_open_service(&mut self) -> Result<Self::OpenServiceSnapshot, String> {
        capture_open_service_snapshot()
    }

    fn stop_legacy(&mut self) -> Result<(), String> {
        legacy_migration::stop_legacy().map(|_| ())
    }

    fn publish_profile(
        &mut self,
        snapshot: &Self::OpenServiceSnapshot,
        profile: &[u8],
    ) -> Result<(), String> {
        ensure_open_service_unchanged(snapshot)?;
        self.published_profile = Some(profile.to_vec());
        let operation = (|| {
            let state = query();
            if state.runtime == RuntimeState::Running {
                run_setup(SystemServiceAction::Stop, None)?;
            }
            run_setup(SystemServiceAction::PublishProfile, Some(profile))
        })();
        combine_mutation_recording(operation, record_protected_mutations(snapshot, profile))
    }

    fn activate_open_service(
        &mut self,
        snapshot: &Self::OpenServiceSnapshot,
    ) -> Result<(), String> {
        ensure_open_service_unchanged(snapshot)?;
        let profile = self
            .published_profile
            .as_deref()
            .ok_or_else(|| "migration activation has no published profile receipt".to_owned())?;
        let operation = (|| {
            for action in migration_activation_actions(snapshot.status.installation)? {
                run_setup(*action, None)?;
            }
            Ok(())
        })();
        combine_mutation_recording(operation, record_protected_mutations(snapshot, profile))
    }

    fn strict_ready(&mut self, expected_digest: &str) -> Result<bool, String> {
        wait_for_strict_ready_with(expected_digest, 200, query, || {
            std::thread::sleep(std::time::Duration::from_millis(50))
        })
    }

    fn verify_injection_smoke(&mut self, expected_digest: &str) -> Result<bool, String> {
        system_injection_smoke(expected_digest)
    }

    fn complete_migration(&mut self, snapshot: &Self::OpenServiceSnapshot) -> Result<(), String> {
        release_migration_runtime_pin(snapshot)
    }

    fn rollback_open_service(
        &mut self,
        snapshot: &Self::OpenServiceSnapshot,
    ) -> Result<(), String> {
        rollback_open_service_snapshot(snapshot)
    }

    fn restore_legacy(&mut self) -> Result<(), String> {
        legacy_migration::rollback().map(|_| ())
    }

    fn removal_verification(
        &mut self,
        expected_digest: &str,
    ) -> Result<MigrationVerification, String> {
        let verification = system_removal_verification(expected_digest)?;
        self.removal_digest = verification
            .permits_removal()
            .then(|| expected_digest.to_owned());
        Ok(verification)
    }

    fn remove_legacy(&mut self) -> Result<(), String> {
        let expected = self
            .removal_digest
            .as_deref()
            .ok_or_else(|| "legacy removal has no verified profile digest".to_owned())?;
        let verification = system_removal_verification(expected)?;
        if !verification.permits_removal() {
            return Err("legacy removal verification changed before SCM deletion".to_owned());
        }
        legacy_migration::remove_after_verified(legacy_migration::RemovalVerification {
            new_service_ready: verification.scm_running_ready,
            active_digest_match: verification.active_digest_match,
            backup_valid: legacy_migration::backup_is_valid(),
        })
        .map(|_| ())
    }
}

fn wait_for_strict_ready_with(
    expected_digest: &str,
    maximum_attempts: usize,
    mut observe: impl FnMut() -> SystemServiceStatus,
    mut wait: impl FnMut(),
) -> Result<bool, String> {
    if maximum_attempts == 0 {
        return Err("strict Ready verification has no polling budget".to_owned());
    }
    let mut last = None;
    for attempt in 0..maximum_attempts {
        let status = observe();
        if status.system_injection_active(Some(expected_digest)) {
            return Ok(true);
        }
        last = Some(status);
        if attempt + 1 < maximum_attempts {
            wait();
        }
    }
    let status = last.expect("at least one strict Ready observation");
    Err(format!(
        "strict Ready timed out: backend={:?}, installation={:?}, runtime={:?}, health={:?}, activeProfileDigest={}, expectedProfileDigest={expected_digest}, win32Error={}",
        status.backend,
        status.installation,
        status.runtime,
        status.health,
        status.active_profile_digest.as_deref().unwrap_or("missing"),
        status
            .win32_error
            .map_or_else(|| "none".to_owned(), |error| error.to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running_ready(digest: Option<&str>) -> SystemServiceStatus {
        SystemServiceStatus {
            backend: ServiceBackend::OpenSource,
            installation: InstallationState::Current,
            runtime: RuntimeState::Running,
            health: HealthState::Ready,
            binary_path: Some("protected-service.exe".to_owned()),
            win32_error: None,
            active_profile_digest: digest.map(str::to_owned),
            configuration_drift: false,
            can_install: false,
            can_remove: true,
            can_start: false,
            can_stop: true,
            can_repair: false,
            can_upgrade: false,
        }
    }

    #[test]
    fn strict_ready_waits_for_the_live_health_digest_after_persisted_ready() {
        let expected = "sha256:expected";
        let mut observations =
            std::collections::VecDeque::from([running_ready(None), running_ready(Some(expected))]);
        let mut sleeps = 0;

        let result = wait_for_strict_ready_with(
            expected,
            3,
            || observations.pop_front().unwrap(),
            || sleeps += 1,
        );

        assert_eq!(result, Ok(true));
        assert_eq!(sleeps, 1);
    }

    #[test]
    fn strict_ready_timeout_reports_the_observed_status_without_the_profile() {
        let error = wait_for_strict_ready_with("sha256:expected", 2, || running_ready(None), || {})
            .unwrap_err();

        assert!(error.contains("runtime=Running"), "{error}");
        assert!(error.contains("health=Ready"), "{error}");
        assert!(error.contains("activeProfileDigest=missing"), "{error}");
        assert!(!error.contains("[General]"), "{error}");
    }
}
