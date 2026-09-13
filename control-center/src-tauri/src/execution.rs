use crate::{bounded_io::read_bounded_file, installation_root, profile::ProfileState};
use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use tauri::State;

mod autostart;
mod process_candidates;
mod runtime;
mod session;
mod storage;

use autostart::autostart_value;
use process_candidates::list_manual_launch_candidates_impl;
pub use process_candidates::ManualLaunchCandidate;
#[cfg(test)]
use runtime::prepare_runtime_at;
pub(crate) use runtime::record_system_injection_choice;
use runtime::{active_runtime, active_system_profile_payload, system_injection_paused};
pub use runtime::{apply_profile, AppliedProfile};
use session::{
    launch_registered_targets_impl, launch_with_mactype_impl, register_session_target_impl,
    remove_session_target_impl,
};
pub use session::{session_targets, SessionTarget};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStatus {
    pub tray_available: bool,
    pub auto_start: bool,
    pub manual_launcher_available: bool,
    pub service_management_package: crate::service_contract::ServiceManagementPackageState,
    pub system_service: crate::service_contract::SystemServiceStatus,
    pub legacy_mac_tray: Option<crate::machine_integration::LegacyServiceStatus>,
    pub legacy_tray: crate::machine_integration::LegacyTrayStatus,
    pub registry_mode_detected: bool,
    pub system_modes_supported: bool,
    pub system_injection_active: bool,
    pub injection_ready: bool,
    pub active_profile: Option<String>,
    pub expected_profile_digest: Option<String>,
    pub session_targets: Vec<SessionTarget>,
}

fn record_legacy_tray_observation(
    service: Option<&crate::machine_integration::LegacyServiceStatus>,
    tray: &crate::machine_integration::LegacyTrayStatus,
) {
    use crate::machine_integration::LegacyTrayProcessState;
    use std::sync::atomic::{AtomicBool, Ordering};

    static OBSERVED: AtomicBool = AtomicBool::new(false);
    let observation = match &tray.process {
        LegacyTrayProcessState::TrustedCurrentSession { path, .. } => Some((
            "process",
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        )),
        _ if service.is_some() => Some(("service", None)),
        _ => None,
    };
    if let Some((kind, process)) = observation {
        if OBSERVED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            crate::diagnostics::record_legacy_tray_detected(kind, process.as_deref());
        }
    }
}

enum LocalProfileObservation {
    Missing,
    Ready {
        runtime: runtime::ActiveRuntime,
        profile: Vec<u8>,
    },
    Invalid,
}

struct ProfileObservation {
    local_runtime: Option<runtime::ActiveRuntime>,
    expected_source: Option<PathBuf>,
    expected_profile: Option<Vec<u8>>,
}

fn project_profile_observation(
    local: LocalProfileObservation,
    bundled_default: Option<(PathBuf, Vec<u8>)>,
) -> ProfileObservation {
    match local {
        LocalProfileObservation::Missing => {
            let (expected_source, expected_profile) = bundled_default
                .map(|(path, bytes)| (Some(path), Some(bytes)))
                .unwrap_or_default();
            ProfileObservation {
                local_runtime: None,
                expected_source,
                expected_profile,
            }
        }
        LocalProfileObservation::Ready { runtime, profile } => ProfileObservation {
            expected_source: Some(runtime.source_profile.clone()),
            expected_profile: Some(profile),
            local_runtime: Some(runtime),
        },
        LocalProfileObservation::Invalid => ProfileObservation {
            local_runtime: None,
            expected_source: None,
            expected_profile: None,
        },
    }
}

fn local_profile_observation_from(base: &Path) -> LocalProfileObservation {
    match base.join("active.json").try_exists() {
        Ok(false) => LocalProfileObservation::Missing,
        Ok(true) => match runtime::active_runtime_from(base).and_then(|runtime| {
            runtime::active_profile_payload_for(&runtime).map(|profile| (runtime, profile))
        }) {
            Ok((runtime, profile)) => LocalProfileObservation::Ready { runtime, profile },
            Err(_) => LocalProfileObservation::Invalid,
        },
        Err(_) => LocalProfileObservation::Invalid,
    }
}

fn observe_profile(installation: Option<&Path>) -> ProfileObservation {
    let local = runtime::runtime_root()
        .map(|base| local_profile_observation_from(&base))
        .unwrap_or(LocalProfileObservation::Invalid);
    let bundled_default = if matches!(local, LocalProfileObservation::Missing) {
        installation
            .and_then(|root| crate::profile::bundled_default_profile_at(root).ok())
            .flatten()
    } else {
        None
    };
    project_profile_observation(local, bundled_default)
}

fn profile_publish_supported_for(
    service: &crate::service_contract::SystemServiceStatus,
    registry_mode_detected: bool,
) -> bool {
    !registry_mode_detected
        && service.backend != crate::service_contract::ServiceBackend::Foreign
        && matches!(
            service.installation,
            crate::service_contract::InstallationState::Absent
                | crate::service_contract::InstallationState::Current
                | crate::service_contract::InstallationState::Outdated
        )
        && matches!(
            service.runtime,
            crate::service_contract::RuntimeState::Running
                | crate::service_contract::RuntimeState::Stopped
        )
}

pub fn status(installation_root: Option<&Path>) -> ExecutionStatus {
    let observation = observe_profile(installation_root);
    let machine = crate::machine_integration::status(observation.expected_profile.as_deref());
    let registry_mode_detected = machine.registry_conflict;
    let mut system_service = machine.new_service;
    let service_management_package = crate::machine_integration::service_management_package_state();
    if service_management_package != crate::service_contract::ServiceManagementPackageState::Ready {
        system_service.can_install = false;
        system_service.can_remove = false;
        system_service.can_start = false;
        system_service.can_stop = false;
        system_service.can_repair = false;
        system_service.can_upgrade = false;
    }
    let expected_profile_digest = machine.expected_profile_digest;
    let system_injection_active = machine.system_injection_active;
    let legacy_mac_tray = machine.legacy_service;
    let legacy_tray = machine.legacy_tray;
    record_legacy_tray_observation(legacy_mac_tray.as_ref(), &legacy_tray);
    let system_modes_supported = service_management_package
        == crate::service_contract::ServiceManagementPackageState::Ready
        && profile_publish_supported_for(&system_service, registry_mode_detected);
    ExecutionStatus {
        tray_available: true,
        auto_start: autostart_value().is_some(),
        manual_launcher_available: installation_root.is_some()
            && observation.local_runtime.is_some(),
        service_management_package,
        system_service,
        legacy_mac_tray,
        legacy_tray,
        registry_mode_detected,
        system_modes_supported,
        system_injection_active,
        injection_ready: observation.local_runtime.is_some(),
        active_profile: observation.expected_source.map(|path| {
            installation_root
                .map(|root| crate::profile::source_profile_reference(root, &path))
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        }),
        expected_profile_digest,
        session_targets: session_targets().unwrap_or_default(),
    }
}

pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    autostart::set_autostart(enabled)
}

#[tauri::command]
pub(crate) fn execution_status() -> ExecutionStatus {
    status(installation_root().as_deref())
}

#[tauri::command]
pub(crate) fn request_legacy_tray_exit(
    expected_identity: crate::machine_integration::LegacyTrayExitRequest,
) -> Result<ExecutionStatus, String> {
    crate::machine_integration::request_legacy_tray_exit(&expected_identity)?;
    Ok(status(installation_root().as_deref()))
}

#[tauri::command]
pub(crate) fn disable_legacy_tray_autostart() -> Result<ExecutionStatus, String> {
    crate::machine_integration::disable_legacy_tray_startup()?;
    Ok(status(installation_root().as_deref()))
}

#[tauri::command]
pub(crate) fn set_session_autostart(enabled: bool) -> Result<bool, String> {
    set_autostart(enabled)
}

#[tauri::command]
pub(crate) fn launch_with_mactype(target: String, arguments: Vec<String>) -> Result<u32, String> {
    launch_with_mactype_impl(&target, &arguments)
}

#[tauri::command]
pub(crate) fn list_manual_launch_candidates() -> Result<Vec<ManualLaunchCandidate>, String> {
    list_manual_launch_candidates_impl()
}

fn execute_machine_action(
    action: crate::machine_integration::MachineAction,
    profile: Option<&[u8]>,
) -> Result<(), String> {
    crate::machine_integration::execute(action, profile)?;
    record_system_injection_choice(!matches!(
        action,
        crate::machine_integration::MachineAction::Stop
            | crate::machine_integration::MachineAction::Remove
    ))
}

fn record_successful_activity(
    action: crate::machine_integration::MachineAction,
    before_running: bool,
    current: &ExecutionStatus,
) {
    use crate::diagnostics::ActivityKind;

    if !before_running
        && current.system_service.runtime == crate::service_contract::RuntimeState::Running
    {
        let _ = crate::diagnostics::record_activity(ActivityKind::ServiceStarted, None);
    }
    match action {
        crate::machine_integration::MachineAction::Install => {
            let _ = crate::diagnostics::record_activity(ActivityKind::ServiceInstalled, None);
        }
        crate::machine_integration::MachineAction::Stop => {
            let _ = crate::diagnostics::record_activity(ActivityKind::ServiceStopped, None);
        }
        crate::machine_integration::MachineAction::Start
        | crate::machine_integration::MachineAction::PublishProfile
        | crate::machine_integration::MachineAction::MigrateFromLegacy => {
            let profile = current.active_profile.as_deref();
            let _ = crate::diagnostics::record_activity(ActivityKind::ProfileVerified, profile);
            let _ = crate::diagnostics::record_activity(ActivityKind::ProfileApplied, profile);
        }
        _ => {}
    }
}

#[tauri::command]
pub(crate) fn apply_open_profile(state: State<'_, ProfileState>) -> Result<AppliedProfile, String> {
    let root =
        installation_root().ok_or_else(|| "MacType installation was not found".to_owned())?;
    let (profile_path, profile_bytes) = state.active_payload()?;
    let applied = apply_profile(&root, &profile_path, &profile_bytes)?;
    if env::var_os("MACTYPE_CI_SMOKE_FILE").is_none() {
        let before = status(Some(&root));
        execute_machine_action(
            crate::machine_integration::MachineAction::PublishProfile,
            Some(&profile_bytes),
        )?;
        let current = status(Some(&root));
        record_successful_activity(
            crate::machine_integration::MachineAction::PublishProfile,
            before.system_service.runtime == crate::service_contract::RuntimeState::Running,
            &current,
        );
    }
    Ok(applied)
}

fn ensure_active_runtime() -> Result<bool, String> {
    if active_runtime().is_ok() {
        return Ok(false);
    }
    let root =
        installation_root().ok_or_else(|| "MacType installation was not found".to_owned())?;
    let (path, bytes) = bundled_default_profile_payload(&root)?;
    apply_profile(&root, &path, &bytes)?;
    Ok(true)
}

/// The one profile the machine may pick on its own when none is applied: the
/// bundled ini\Default.ini, validated. Anything else must be an explicit user
/// choice (apply or the migrate funnel), so a missing or unreadable
/// Default.ini fails with a coded error instead of guessing.
fn bundled_default_profile_payload(installation_root: &Path) -> Result<(PathBuf, Vec<u8>), String> {
    crate::profile::bundled_default_profile_at(installation_root)
        .map_err(|error| {
            format!(
                "control-center-default-profile-invalid: no profile is applied and ini\\Default.ini could not be validated: {error}"
            )
        })?
        .ok_or_else(|| {
            "control-center-default-profile-missing: no profile is applied and ini\\Default.ini was not found; apply a profile first"
                .to_owned()
        })
}

fn service_start_profile_at(
    runtime_root: &Path,
    installation_root: &Path,
) -> Result<Vec<u8>, String> {
    let active = match runtime::active_runtime_from(runtime_root) {
        Ok(active) => active,
        Err(_) => {
            let (path, bytes) = bundled_default_profile_payload(installation_root)?;
            runtime::prepare_runtime_at(runtime_root, installation_root, &path, &bytes)?
        }
    };
    runtime::active_profile_payload_for(&active)
}

pub(crate) fn observe_machine_on_tray_login(
) -> Result<crate::machine_integration::TrayLoginState, String> {
    let root = installation_root();
    let observation = observe_profile(root.as_deref());
    Ok(crate::machine_integration::tray_login(
        system_injection_paused(),
        env::var_os("MACTYPE_CI_SMOKE_FILE").is_some(),
        observation.expected_profile.as_deref(),
    ))
}

pub(crate) fn apply_system_injection_from_tray_menu() -> Result<(), String> {
    if system_injection_paused() {
        return Err("system injection is paused".to_owned());
    }
    ensure_active_runtime()?;
    let profile = active_system_profile_payload()?;
    let before = status(installation_root().as_deref());
    crate::machine_integration::tray_apply(false, &profile)?;
    record_system_injection_choice(true)?;
    let current = status(installation_root().as_deref());
    record_successful_activity(
        crate::machine_integration::MachineAction::PublishProfile,
        before.system_service.runtime == crate::service_contract::RuntimeState::Running,
        &current,
    );
    Ok(())
}

#[tauri::command]
pub(crate) fn activate_system_injection() -> Result<ExecutionStatus, String> {
    ensure_active_runtime()?;
    let profile = active_system_profile_payload()?;
    let before = status(installation_root().as_deref());
    execute_machine_action(
        crate::machine_integration::MachineAction::PublishProfile,
        Some(&profile),
    )?;
    let current = status(installation_root().as_deref());
    record_successful_activity(
        crate::machine_integration::MachineAction::PublishProfile,
        before.system_service.runtime == crate::service_contract::RuntimeState::Running,
        &current,
    );
    Ok(current)
}

#[tauri::command]
pub(crate) fn manage_system_service(
    action: crate::machine_integration::PublicMachineAction,
) -> Result<ExecutionStatus, String> {
    let action = crate::machine_integration::MachineAction::from(action);
    let installation = installation_root();
    let before = status(installation.as_deref());
    let profile = match action {
        crate::machine_integration::MachineAction::Start => {
            let root = installation
                .as_deref()
                .ok_or_else(|| "MacType installation was not found".to_owned())?;
            let runtime_root = runtime::runtime_root()?;
            Some(service_start_profile_at(&runtime_root, root)?)
        }
        crate::machine_integration::MachineAction::PublishProfile
        | crate::machine_integration::MachineAction::MigrateFromLegacy
        | crate::machine_integration::MachineAction::RemoveLegacy => {
            ensure_active_runtime()?;
            Some(active_system_profile_payload()?)
        }
        _ => None,
    };
    execute_machine_action(action, profile.as_deref())?;
    let current = status(installation.as_deref());
    record_successful_activity(
        action,
        before.system_service.runtime == crate::service_contract::RuntimeState::Running,
        &current,
    );
    Ok(current)
}

#[tauri::command]
pub(crate) fn register_session_target(
    target: String,
    arguments: Vec<String>,
) -> Result<Vec<SessionTarget>, String> {
    register_session_target_impl(&target, &arguments)
}

#[tauri::command]
pub(crate) fn remove_session_target(target: String) -> Result<Vec<SessionTarget>, String> {
    remove_session_target_impl(&target)
}

#[tauri::command]
pub(crate) fn launch_registered_targets() -> Result<Vec<u32>, String> {
    launch_registered_targets_impl()
}

#[tauri::command]
pub(crate) fn ci_verify_injection_workflow() -> Result<(), String> {
    let smoke_marker = env::var_os("MACTYPE_CI_SMOKE_FILE").ok_or_else(|| {
        "injection verification is available only during CI smoke tests".to_owned()
    })?;
    let target = env::var_os("MACTYPE_CI_MANUAL_TARGET")
        .ok_or_else(|| "MACTYPE_CI_MANUAL_TARGET is not available".to_owned())?;
    let marker = PathBuf::from(smoke_marker)
        .parent()
        .ok_or_else(|| "CI marker has no parent directory".to_owned())?
        .join("injection.ready");
    if marker.exists() {
        fs::remove_file(&marker).map_err(|error| error.to_string())?;
    }
    let target = target.to_string_lossy().into_owned();
    let arguments = vec![marker.to_string_lossy().into_owned()];
    register_session_target_impl(&target, &arguments)?;
    launch_registered_targets_impl()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !marker.is_file() && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    remove_session_target_impl(&target)?;
    if !marker.is_file() {
        return Err("managed MacLoader did not start the registered injected target".to_owned());
    }
    let content = String::from_utf8(read_bounded_file(&marker, 4096, "CI injection marker")?)
        .map_err(|error| error.to_string())?;
    fs::remove_file(&marker).map_err(|error| error.to_string())?;
    if content.trim() != "mactype-manual-launch-ready" {
        return Err(format!(
            "injected target wrote an invalid marker: {content}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_machine_action_rejects_internal_rollback() {
        assert!(
            serde_json::from_str::<crate::machine_integration::PublicMachineAction>(
                r#""rollback""#,
            )
            .is_err()
        );
    }

    #[test]
    fn manual_launcher_rejects_non_executable_targets() {
        let error = launch_with_mactype_impl("Cargo.toml", &[]).unwrap_err();
        assert!(error.contains("existing .exe") || error.contains("cannot find"));
    }

    #[test]
    fn system_injection_status_matches_the_verified_service() {
        let status = status(None);
        let service = &status.system_service;
        assert_eq!(
            status.system_modes_supported,
            profile_publish_supported_for(service, status.registry_mode_detected)
        );
        assert_eq!(
            status.system_injection_active,
            !status.registry_mode_detected
                && service.system_injection_active(status.expected_profile_digest.as_deref())
        );
    }

    #[test]
    fn missing_local_pointer_uses_bundled_default_for_ready_service_observation() {
        let bundled_path = PathBuf::from("C:/Program Files/MacType Control Center/ini/Default.ini");
        let bundled = b"[General]\r\nNormalWeight=2\r\n".to_vec();

        let projection = project_profile_observation(
            LocalProfileObservation::Missing,
            Some((bundled_path.clone(), bundled.clone())),
        );

        assert!(projection.local_runtime.is_none());
        assert_eq!(projection.expected_source, Some(bundled_path));
        assert_eq!(
            projection.expected_profile.as_deref(),
            Some(bundled.as_slice())
        );
        let digest = mactype_service_contract::GenerationId::from_profile_bytes(&bundled)
            .as_str()
            .to_owned();
        let service = crate::service_contract::SystemServiceStatus {
            backend: crate::service_contract::ServiceBackend::OpenSource,
            installation: crate::service_contract::InstallationState::Current,
            runtime: crate::service_contract::RuntimeState::Running,
            health: crate::service_contract::HealthState::Ready,
            binary_path: None,
            win32_error: None,
            active_profile_digest: Some(digest.clone()),
            configuration_drift: false,
            can_install: false,
            can_remove: true,
            can_start: false,
            can_stop: true,
            can_repair: false,
            can_upgrade: false,
        };
        assert!(service.system_injection_active(Some(&digest)));
    }

    #[test]
    fn valid_local_runtime_always_wins_over_the_bundled_default() {
        let local_path = PathBuf::from("C:/Users/Test/Local.ini");
        let local = b"[General]\r\nNormalWeight=7\r\n".to_vec();
        let projection = project_profile_observation(
            LocalProfileObservation::Ready {
                runtime: runtime::ActiveRuntime {
                    runtime_root: PathBuf::from("C:/runtime/generations/local"),
                    source_profile: local_path.clone(),
                },
                profile: local.clone(),
            },
            Some((
                PathBuf::from("C:/Program Files/MacType Control Center/ini/Default.ini"),
                b"[General]\r\nNormalWeight=2\r\n".to_vec(),
            )),
        );

        assert!(projection.local_runtime.is_some());
        assert_eq!(projection.expected_source, Some(local_path));
        assert_eq!(
            projection.expected_profile.as_deref(),
            Some(local.as_slice())
        );
    }

    #[test]
    fn malformed_local_pointer_fails_closed_instead_of_using_default() {
        let projection = project_profile_observation(
            LocalProfileObservation::Invalid,
            Some((
                PathBuf::from("C:/Program Files/MacType Control Center/ini/Default.ini"),
                b"[General]\r\nNormalWeight=2\r\n".to_vec(),
            )),
        );

        assert!(projection.local_runtime.is_none());
        assert!(projection.expected_source.is_none());
        assert!(projection.expected_profile.is_none());
    }

    #[test]
    fn existing_malformed_pointer_is_classified_as_invalid() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-malformed-runtime-{unique}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("active.json"), b"not json").unwrap();

        assert!(matches!(
            local_profile_observation_from(&root),
            LocalProfileObservation::Invalid
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn protected_custom_profile_does_not_match_the_bundled_default() {
        let bundled = b"[General]\r\nNormalWeight=2\r\n".to_vec();
        let custom = b"[General]\r\nNormalWeight=7\r\n";
        let projection = project_profile_observation(
            LocalProfileObservation::Missing,
            Some((PathBuf::from("C:/app/ini/Default.ini"), bundled.clone())),
        );
        let expected = mactype_service_contract::GenerationId::from_profile_bytes(&bundled)
            .as_str()
            .to_owned();
        let custom_digest = mactype_service_contract::GenerationId::from_profile_bytes(custom)
            .as_str()
            .to_owned();
        let service = crate::service_contract::SystemServiceStatus {
            backend: crate::service_contract::ServiceBackend::OpenSource,
            installation: crate::service_contract::InstallationState::Current,
            runtime: crate::service_contract::RuntimeState::Running,
            health: crate::service_contract::HealthState::Ready,
            binary_path: None,
            win32_error: None,
            active_profile_digest: Some(custom_digest),
            configuration_drift: false,
            can_install: false,
            can_remove: true,
            can_start: false,
            can_stop: true,
            can_repair: false,
            can_upgrade: false,
        };

        assert!(projection.expected_profile.is_some());
        assert!(!service.system_injection_active(Some(&expected)));
    }

    #[test]
    fn first_service_start_uses_only_the_bundled_default_profile() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-first-service-start-{unique}"));
        let installation = root.join("installation");
        let runtime_root = root.join("runtime");
        fs::create_dir_all(installation.join("ini")).unwrap();
        fs::write(installation.join("MacLoader.exe"), b"loader").unwrap();
        fs::write(installation.join("MacType.dll"), b"core").unwrap();
        let default = b"[General]\r\nNormalWeight=2\r\n";
        fs::write(installation.join("ini").join("Default.ini"), default).unwrap();
        fs::write(
            installation.join("ini").join("Another.ini"),
            b"[General]\r\nNormalWeight=9\r\n",
        )
        .unwrap();
        fs::write(
            installation.join("MacType.ini"),
            b"[General]\r\nAlternativeFile=ini\\Another.ini\r\n",
        )
        .unwrap();

        let payload = service_start_profile_at(&runtime_root, &installation).unwrap();

        assert_eq!(payload, default);
        let active = runtime::active_runtime_from(&runtime_root).unwrap();
        assert_eq!(active.source_profile, Path::new(r"ini\Default.ini"));
        assert_eq!(
            fs::read(active.runtime_root.join("profile.ini")).unwrap(),
            default
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn first_service_start_without_default_profile_preserves_local_state() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-missing-start-profile-{unique}"));
        let installation = root.join("installation");
        let runtime_root = root.join("runtime");
        fs::create_dir_all(installation.join("ini")).unwrap();
        fs::write(installation.join("MacLoader.exe"), b"loader").unwrap();
        fs::write(installation.join("MacType.dll"), b"core").unwrap();
        fs::write(
            installation.join("ini").join("Another.ini"),
            b"[General]\r\nNormalWeight=9\r\n",
        )
        .unwrap();
        fs::write(
            installation.join("MacType.ini"),
            b"[General]\r\nAlternativeFile=ini\\Another.ini\r\n",
        )
        .unwrap();

        let error = service_start_profile_at(&runtime_root, &installation).unwrap_err();

        assert!(error.contains(r"ini\Default.ini"), "{error}");
        assert!(
            error.starts_with("control-center-default-profile-missing:"),
            "{error}"
        );
        assert!(!runtime_root.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn machine_default_profile_fallback_never_picks_another_profile() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-machine-default-{unique}"));
        let installation = root.join("installation");
        fs::create_dir_all(installation.join("ini")).unwrap();
        fs::write(
            installation.join("ini").join("Another.ini"),
            b"[General]\r\nNormalWeight=9\r\n",
        )
        .unwrap();
        fs::write(
            installation.join("MacType.ini"),
            b"[General]\r\nAlternativeFile=ini\\Another.ini\r\n",
        )
        .unwrap();

        let missing = bundled_default_profile_payload(&installation).unwrap_err();
        assert!(
            missing.starts_with("control-center-default-profile-missing:"),
            "{missing}"
        );

        let oversized = vec![b';'; mactype_service_contract::MAX_PROFILE_BYTES + 1];
        fs::write(installation.join("ini").join("Default.ini"), &oversized).unwrap();
        let invalid = bundled_default_profile_payload(&installation).unwrap_err();
        assert!(
            invalid.starts_with("control-center-default-profile-invalid:"),
            "{invalid}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn later_service_start_keeps_the_explicitly_applied_profile() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-later-service-start-{unique}"));
        let installation = root.join("installation");
        let runtime_root = root.join("runtime");
        fs::create_dir_all(installation.join("ini")).unwrap();
        fs::write(installation.join("MacLoader.exe"), b"loader").unwrap();
        fs::write(installation.join("MacType.dll"), b"core").unwrap();
        fs::write(
            installation.join("ini").join("Default.ini"),
            b"[General]\r\nNormalWeight=2\r\n",
        )
        .unwrap();
        let custom_path = installation.join("ini").join("Custom.ini");
        let custom = b"[General]\r\nNormalWeight=8\r\n";
        fs::write(&custom_path, custom).unwrap();
        runtime::prepare_runtime_at(&runtime_root, &installation, &custom_path, custom).unwrap();

        let payload = service_start_profile_at(&runtime_root, &installation).unwrap();

        assert_eq!(payload, custom);
        let active = runtime::active_runtime_from(&runtime_root).unwrap();
        assert_eq!(active.source_profile, Path::new(r"ini\Custom.ini"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_publish_capability_requires_no_appinit_and_a_safe_stable_installation() {
        let mut service = crate::service_contract::SystemServiceStatus {
            backend: crate::service_contract::ServiceBackend::OpenSource,
            installation: crate::service_contract::InstallationState::Current,
            runtime: crate::service_contract::RuntimeState::Running,
            health: crate::service_contract::HealthState::Ready,
            binary_path: None,
            win32_error: None,
            active_profile_digest: None,
            configuration_drift: false,
            can_install: false,
            can_remove: true,
            can_start: false,
            can_stop: true,
            can_repair: true,
            can_upgrade: false,
        };
        assert!(profile_publish_supported_for(&service, false));
        assert!(!profile_publish_supported_for(&service, true));

        for installation in [
            crate::service_contract::InstallationState::Invalid,
            crate::service_contract::InstallationState::Inaccessible,
            crate::service_contract::InstallationState::DeletePending,
        ] {
            service.installation = installation;
            assert!(!profile_publish_supported_for(&service, false));
        }
        service.installation = crate::service_contract::InstallationState::Current;
        for runtime in [
            crate::service_contract::RuntimeState::StartPending,
            crate::service_contract::RuntimeState::StopPending,
            crate::service_contract::RuntimeState::Paused,
            crate::service_contract::RuntimeState::Unknown,
        ] {
            service.runtime = runtime;
            assert!(!profile_publish_supported_for(&service, false));
        }
        service.runtime = crate::service_contract::RuntimeState::Stopped;
        service.installation = crate::service_contract::InstallationState::Absent;
        service.backend = crate::service_contract::ServiceBackend::None;
        assert!(profile_publish_supported_for(&service, false));
    }

    #[test]
    fn applied_installed_profile_keeps_a_relative_source_reference() {
        let root = env::temp_dir().join(format!("mactype-runtime-test-{}", std::process::id()));
        let installation = root.join("installation");
        let runtime = root.join("runtime");
        fs::create_dir_all(&installation).unwrap();
        fs::write(installation.join("MacLoader.exe"), b"loader").unwrap();
        fs::write(installation.join("MacType.dll"), b"core").unwrap();
        let profile = b"[General]\r\nNormalWeight=7\r\n";
        let active = prepare_runtime_at(
            &runtime,
            &installation,
            &installation.join("ini").join("User.ini"),
            profile,
        )
        .unwrap();
        assert_eq!(
            fs::read(active.runtime_root.join("profile.ini")).unwrap(),
            profile
        );
        assert_eq!(
            fs::read(active.runtime_root.join("MacType.ini")).unwrap(),
            b"[General]\r\nAlternativeFile=profile.ini\r\n"
        );
        assert!(active.runtime_root.join("MacLoader.exe").is_file());
        assert!(active.runtime_root.join("MacType.dll").is_file());
        let reopened = runtime::active_runtime_from(&runtime).unwrap();
        assert_eq!(reopened.source_profile, Path::new(r"ini\User.ini"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_runtime_rejects_an_oversized_pointer_before_json_parsing() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-runtime-pointer-{unique}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("active.json"), vec![b' '; 512 * 1024 + 1]).unwrap();

        let error = runtime::active_runtime_from(&root).unwrap_err();

        assert!(error.contains("byte limit"), "unexpected error: {error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_profile_payload_rejects_growth_beyond_the_profile_contract() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("mactype-active-profile-{unique}.ini"));
        fs::write(
            &path,
            vec![b';'; mactype_service_contract::MAX_PROFILE_BYTES + 1],
        )
        .unwrap();

        let error = runtime::active_profile_payload_at(&path).unwrap_err();

        assert!(error.contains("byte limit"), "unexpected error: {error}");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn runtime_generation_rejects_an_oversized_installation_artifact() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-runtime-artifact-{unique}"));
        let installation = root.join("installation");
        let runtime = root.join("runtime");
        fs::create_dir_all(&installation).unwrap();
        let loader = fs::File::create(installation.join("MacLoader.exe")).unwrap();
        loader
            .set_len(mactype_service_contract::MAX_RUNTIME_FILE_BYTES as u64 + 1)
            .unwrap();
        fs::write(installation.join("MacType.dll"), b"core").unwrap();

        let error = prepare_runtime_at(
            &runtime,
            &installation,
            Path::new("C:/profiles/User.ini"),
            b"[General]\r\nNormalWeight=7\r\n",
        )
        .unwrap_err();

        assert!(error.contains("byte limit"), "unexpected error: {error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_generation_rejects_an_oversized_profile_payload() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mactype-runtime-profile-{unique}"));
        let installation = root.join("installation");
        let runtime = root.join("runtime");
        fs::create_dir_all(&installation).unwrap();
        fs::write(installation.join("MacLoader.exe"), b"loader").unwrap();
        fs::write(installation.join("MacType.dll"), b"core").unwrap();
        let profile = vec![b';'; mactype_service_contract::MAX_PROFILE_BYTES + 1];

        let error = prepare_runtime_at(
            &runtime,
            &installation,
            Path::new("C:/profiles/User.ini"),
            &profile,
        )
        .unwrap_err();

        assert!(error.contains("profile") && error.contains("byte limit"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_targets_reject_an_oversized_json_file_before_parsing() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("mactype-session-targets-{unique}.json"));
        fs::write(&path, vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();

        let error = session::session_targets_from(&path).unwrap_err();

        assert!(error.contains("byte limit"), "unexpected error: {error}");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn session_target_writer_never_persists_state_it_cannot_read_back() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("mactype-session-target-write-{unique}.json"));
        fs::write(&path, b"preserved").unwrap();
        let argument = "\u{0001}".repeat(4096);
        let targets = (0..32)
            .map(|index| SessionTarget {
                target: format!("C:/target-{index}.exe"),
                arguments: vec![argument.clone(); 32],
            })
            .collect::<Vec<_>>();

        let error = session::write_session_targets_to(&path, &targets).unwrap_err();

        assert!(error.contains("byte limit"), "unexpected error: {error}");
        assert_eq!(fs::read(&path).unwrap(), b"preserved");
        fs::remove_file(path).unwrap();
    }
}
