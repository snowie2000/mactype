use crate::{BootstrapOutcome, UninstallOutcome};

impl BootstrapOutcome {
    pub(crate) fn to_json(&self, verb: &str) -> String {
        match self {
            Self::Applied {
                active_profile_digest,
                preserved_existing_profile,
            } => format!(
                "{{\"ok\":true,\"verb\":\"{verb}\",\"outcome\":\"applied\",\"activeProfileDigest\":\"{active_profile_digest}\",\"preservedExistingProfile\":{preserved_existing_profile}}}"
            ),
            Self::SkippedBlocked { reason } => format!(
                "{{\"ok\":true,\"verb\":\"{verb}\",\"outcome\":\"skipped-blocked\",\"reason\":\"{}\"}}",
                reason.code()
            ),
        }
    }
}

impl UninstallOutcome {
    pub(crate) fn to_json(&self) -> String {
        match self {
            Self::Removed => {
                "{\"ok\":true,\"verb\":\"uninstall-owned\",\"outcome\":\"removed\"}".to_owned()
            }
            Self::AlreadyAbsent => {
                "{\"ok\":true,\"verb\":\"uninstall-owned\",\"outcome\":\"already-absent\"}"
                    .to_owned()
            }
            Self::SkippedBlocked { reason } => format!(
                "{{\"ok\":true,\"verb\":\"uninstall-owned\",\"outcome\":\"skipped-blocked\",\"reason\":\"{}\"}}",
                reason.code()
            ),
        }
    }
}

impl crate::BootstrapBlocker {
    fn code(&self) -> &'static str {
        match self {
            Self::LegacyService => "legacy-service",
            Self::LegacyTrayMode => "legacy-tray-mode",
            Self::AppInit => "appinit",
            Self::ForeignOpenService => "foreign-open-service",
            Self::UnknownMachineState => "unknown-machine-state",
            Self::InconsistentOwnedState => "inconsistent-owned-state",
        }
    }
}
