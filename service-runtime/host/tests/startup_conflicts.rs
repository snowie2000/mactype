use mactype_service_host::{LegacyServiceRuntimeState, StartupSafetySnapshot};

#[test]
fn only_stopped_or_absent_legacy_without_appinit_and_with_owned_image_is_safe() {
    for legacy in [
        LegacyServiceRuntimeState::Absent,
        LegacyServiceRuntimeState::Stopped,
    ] {
        assert!(StartupSafetySnapshot {
            app_init32_enabled: false,
            app_init64_enabled: false,
            legacy_state: legacy,
            open_service_image_owned: true,
        }
        .validate()
        .is_ok());
    }

    for conflicting_snapshot in [
        StartupSafetySnapshot {
            app_init32_enabled: true,
            app_init64_enabled: false,
            legacy_state: LegacyServiceRuntimeState::Stopped,
            open_service_image_owned: true,
        },
        StartupSafetySnapshot {
            app_init32_enabled: false,
            app_init64_enabled: true,
            legacy_state: LegacyServiceRuntimeState::Stopped,
            open_service_image_owned: true,
        },
        StartupSafetySnapshot {
            app_init32_enabled: false,
            app_init64_enabled: false,
            legacy_state: LegacyServiceRuntimeState::Running,
            open_service_image_owned: true,
        },
        StartupSafetySnapshot {
            app_init32_enabled: false,
            app_init64_enabled: false,
            legacy_state: LegacyServiceRuntimeState::StartPending,
            open_service_image_owned: true,
        },
        StartupSafetySnapshot {
            app_init32_enabled: false,
            app_init64_enabled: false,
            legacy_state: LegacyServiceRuntimeState::Stopped,
            open_service_image_owned: false,
        },
    ] {
        assert!(conflicting_snapshot.validate().is_err());
    }
}
