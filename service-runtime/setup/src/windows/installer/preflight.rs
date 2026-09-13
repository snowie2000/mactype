use mactype_service_contract::{GenerationId, MachinePaths};

use super::WindowsInstallerBackend;
use crate::storage::reject_reparse_ancestors;
use crate::{
    protected_installer_broker_layout, BootstrapMode, BootstrapPreflight, OpenServiceObservation,
    ProfileStore, ProtectedProfileObservation, ProtectedRuntimeObservation, RuntimeInstaller,
    SetupError,
};

impl WindowsInstallerBackend {
    pub(super) fn inspect_snapshot(&self) -> BootstrapPreflight {
        let installer = RuntimeInstaller::new(self.paths.clone());
        let current = installer.inspect_current_stable();
        let protected_runtime = match &current {
            Ok(Some(_)) => ProtectedRuntimeObservation::Active,
            Ok(None) => ProtectedRuntimeObservation::Absent,
            Err(_) => ProtectedRuntimeObservation::Unknown,
        };
        let protected_profile =
            match ProfileStore::new(self.paths.clone()).inspect_active_generation_stable() {
                Ok(Some(generation)) => {
                    ProtectedProfileObservation::Active(generation.directory_name().to_owned())
                }
                Ok(None) => ProtectedProfileObservation::Absent,
                Err(_) => ProtectedProfileObservation::Unknown,
            };
        let mut open_service = self.manager.observe_fixed_service();
        if matches!(
            open_service,
            OpenServiceObservation::OwnedStopped | OpenServiceObservation::OwnedRunning
        ) {
            open_service = match current {
                Ok(Some(current)) => match self
                    .manager
                    .owned_service_points_to(current.service_binary())
                {
                    Ok(true) => open_service,
                    Ok(false) | Err(_) => OpenServiceObservation::Unknown,
                },
                Ok(None) | Err(_) => OpenServiceObservation::Unknown,
            };
        }
        BootstrapPreflight {
            open_service,
            protected_profile,
            protected_runtime,
            legacy_service: self.manager.observe_legacy_service(),
            legacy_tray: super::super::legacy_tray::observe_conflict(),
            appinit: super::super::appinit::observe_conflict(),
        }
    }
}

pub(super) fn validate_mode_matches_profile(
    mode: &BootstrapMode,
    active: Option<&GenerationId>,
) -> Result<(), SetupError> {
    match (mode, active) {
        (BootstrapMode::FreshBundledDefault, None) => Ok(()),
        (BootstrapMode::PreserveExistingProfile { generation }, Some(active))
            if active.directory_name() == generation =>
        {
            Ok(())
        }
        _ => Err(SetupError::Runtime(
            "bootstrap mode no longer matches the protected active profile".to_owned(),
        )),
    }
}

pub(super) fn validate_and_harden_installer_root(paths: &MachinePaths) -> Result<(), SetupError> {
    let app_root = paths.service_root().parent().ok_or_else(|| {
        SetupError::Runtime("protected application root is unavailable".to_owned())
    })?;
    let program_files = app_root.parent().ok_or_else(|| {
        SetupError::Runtime("trusted Program Files root is unavailable".to_owned())
    })?;
    let executable = std::env::current_exe()?;
    if !protected_installer_broker_layout(program_files, &executable) {
        return Err(SetupError::Runtime(
            "installer broker is not in the fixed Program Files layout".to_owned(),
        ));
    }
    reject_reparse_ancestors(&executable)?;
    let expected = app_root
        .join("service-runtime")
        .join("mactype-service-setup.exe");
    let actual = executable.canonicalize()?;
    let expected = expected.canonicalize()?;
    if !actual
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        return Err(SetupError::Runtime(
            "installer broker canonical path differs from the fixed protected path".to_owned(),
        ));
    }
    super::super::acl::harden_machine_directory(app_root)?;
    reject_reparse_ancestors(&actual)?;
    Ok(())
}
