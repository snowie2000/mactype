use std::io;
use std::path::{Path, PathBuf};

use mactype_service_platform::{
    AclValidationError, AllowedAce, LocalSecurityDescriptor, OwnedSid, SecurityAclError,
    SecurityDescriptor,
};
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, GENERIC_EXECUTE, GENERIC_READ,
};
use windows_sys::Win32::Security::{INHERIT_ONLY_ACE, SE_DACL_PROTECTED};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ,
};

use crate::storage::reject_reparse_ancestors;
use crate::SetupError;

const MACHINE_TREE_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)(A;OICI;GRGX;;;AC)(A;OICI;GRGX;;;S-1-15-2-2)";
const SYSTEM_SID: &str = "S-1-5-18";
const ADMINISTRATORS_SID: &str = "S-1-5-32-544";
const USERS_SID: &str = "S-1-5-32-545";
const ALL_APPLICATION_PACKAGES_SID: &str = "S-1-15-2-1";
const ALL_RESTRICTED_APPLICATION_PACKAGES_SID: &str = "S-1-15-2-2";
const MAX_PROTECTED_TREE_ENTRIES: usize = 100_000;

pub fn harden_machine_directory(path: &Path) -> Result<(), SetupError> {
    harden_machine_directory_observed(path, |_, _| {})
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HardeningObservation {
    BeforeInspect,
    BeforeVerify,
}

fn harden_machine_directory_observed(
    path: &Path,
    mut observe: impl FnMut(HardeningObservation, &Path),
) -> Result<(), SetupError> {
    let paths = collect_protected_tree(path, &mut observe)
        .map_err(|error| error.at_machine_path("enumerate protected ACL tree", path))?;
    let descriptor = SecurityDescriptor::from_sddl(MACHINE_TREE_SDDL).map_err(|error| {
        SetupError::Io(error).at_machine_path("build protected ACL descriptor", path)
    })?;
    descriptor
        .apply_to_tree(path, true)
        .map_err(|error| SetupError::Io(error).at_machine_path("reset protected ACL tree", path))?;

    let sids = ExpectedSids::new()
        .map_err(|error| error.at_machine_path("build protected ACL trustees", path))?;
    for entry in paths {
        observe(HardeningObservation::BeforeVerify, &entry);
        if let Err(error) = verify_protected_acl(&entry, &sids, entry == path) {
            if entry != path && setup_error_confirms_disappeared_child(&entry, &error) {
                continue;
            }
            return Err(error.at_machine_path("verify protected ACL entry", &entry));
        }
    }
    Ok(())
}

fn collect_protected_tree(
    root: &Path,
    observe: &mut impl FnMut(HardeningObservation, &Path),
) -> Result<Vec<PathBuf>, SetupError> {
    reject_reparse_ancestors(root)?;
    let mut pending = vec![root.to_owned()];
    let mut result = Vec::new();
    let mut discovered = 1usize;
    while let Some(path) = pending.pop() {
        observe(HardeningObservation::BeforeInspect, &path);
        if let Err(error) = reject_reparse_ancestors(&path) {
            if path != root && setup_error_confirms_disappeared_child(&path, &error) {
                continue;
            }
            return Err(error);
        }
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if path != root && io_error_confirms_disappeared_child(&path, &error) => {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.is_dir() {
            let children = match std::fs::read_dir(&path) {
                Ok(children) => children,
                Err(error)
                    if path != root && io_error_confirms_disappeared_child(&path, &error) =>
                {
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            for entry in children {
                if discovered == MAX_PROTECTED_TREE_ENTRIES {
                    return Err(SetupError::Runtime(
                        "protected machine tree exceeds the fixed ACL verification bound"
                            .to_owned(),
                    ));
                }
                pending.push(entry?.path());
                discovered += 1;
            }
        }
        result.push(path);
    }
    Ok(result)
}

fn setup_error_confirms_disappeared_child(path: &Path, error: &SetupError) -> bool {
    match error {
        SetupError::Io(error) => io_error_confirms_disappeared_child(path, error),
        _ => false,
    }
}

fn io_error_confirms_disappeared_child(path: &Path, error: &io::Error) -> bool {
    is_windows_not_found(error)
        && std::fs::symlink_metadata(path)
            .is_err_and(|probe_error| is_windows_not_found(&probe_error))
}

fn is_windows_not_found(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code)
            if code == ERROR_FILE_NOT_FOUND as i32 || code == ERROR_PATH_NOT_FOUND as i32
    )
}

fn verify_protected_acl(
    path: &Path,
    sids: &ExpectedSids,
    require_protected: bool,
) -> Result<(), SetupError> {
    let descriptor = LocalSecurityDescriptor::query_named_file(path)
        .map_err(|status| SetupError::Io(io::Error::from_raw_os_error(status as i32)))?;

    let control = descriptor.control().map_err(SetupError::Io)?;
    if require_protected && control & SE_DACL_PROTECTED == 0 {
        return Err(SetupError::Runtime(format!(
            "protected machine ACL still permits inheritance: {}",
            path.display()
        )));
    }

    // The platform validates every ACE before any is exposed: a deny or
    // audit entry anywhere in the DACL is the non-allow rejection, and any
    // other malformed structure is treated as an ACL that does not match.
    let dacl = match descriptor.dacl() {
        Ok(dacl) => dacl,
        Err(SecurityAclError::Io(error)) => return Err(SetupError::Io(error)),
        Err(SecurityAclError::Invalid(AclValidationError::UnsupportedAceType)) => {
            return Err(SetupError::Runtime(format!(
                "protected machine ACL contains a non-allow ACE: {}",
                path.display()
            )));
        }
        Err(SecurityAclError::Invalid(_)) => return Err(invalid_acl(path)),
    };

    let mut saw_system = false;
    let mut saw_administrators = false;
    let mut saw_users = false;
    let mut saw_all_application_packages = false;
    let mut saw_all_restricted_application_packages = false;
    for ace in dacl.allowed_aces() {
        if ace.trustee_matches(&sids.system) {
            if ace.mask() != FILE_ALL_ACCESS {
                return Err(invalid_acl(path));
            }
            if ace_applies_to_current_object(ace.flags()) {
                if saw_system {
                    return Err(invalid_acl(path));
                }
                saw_system = true;
            }
        } else if ace.trustee_matches(&sids.administrators) {
            if ace.mask() != FILE_ALL_ACCESS {
                return Err(invalid_acl(path));
            }
            if ace_applies_to_current_object(ace.flags()) {
                if saw_administrators {
                    return Err(invalid_acl(path));
                }
                saw_administrators = true;
            }
        } else if ace.trustee_matches(&sids.users) {
            if !is_read_execute_mask(ace.mask()) {
                return Err(invalid_acl_ace(path, "Users", ace));
            }
            if ace_applies_to_current_object(ace.flags()) {
                saw_users = true;
            }
        } else if ace.trustee_matches(&sids.all_application_packages) {
            if !is_read_execute_mask(ace.mask()) {
                return Err(invalid_acl_ace(path, "AllApplicationPackages", ace));
            }
            if ace_applies_to_current_object(ace.flags()) {
                saw_all_application_packages = true;
            }
        } else if ace.trustee_matches(&sids.all_restricted_application_packages) {
            if !is_read_execute_mask(ace.mask()) {
                return Err(invalid_acl_ace(
                    path,
                    "AllRestrictedApplicationPackages",
                    ace,
                ));
            }
            if ace_applies_to_current_object(ace.flags()) {
                saw_all_restricted_application_packages = true;
            }
        } else {
            return Err(SetupError::Runtime(format!(
                "protected machine ACL contains an unapproved allow ACE: {}",
                path.display()
            )));
        }
    }
    if !saw_system
        || !saw_administrators
        || !saw_users
        || !saw_all_application_packages
        || !saw_all_restricted_application_packages
    {
        return Err(invalid_acl(path));
    }
    Ok(())
}

fn is_read_execute_mask(mask: u32) -> bool {
    mask == (GENERIC_READ | GENERIC_EXECUTE) || mask == (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE)
}

fn ace_applies_to_current_object(flags: u8) -> bool {
    flags & INHERIT_ONLY_ACE as u8 == 0
}

fn invalid_acl_ace(path: &Path, trustee: &str, ace: &AllowedAce) -> SetupError {
    SetupError::Runtime(format!(
        "protected machine ACL has invalid {trustee} rights (mask=0x{:08X}, flags=0x{:02X}, expected=0x{:08X} or 0x{:08X}): {}",
        ace.mask(),
        ace.flags(),
        GENERIC_READ | GENERIC_EXECUTE,
        FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
        path.display()
    ))
}

fn invalid_acl(path: &Path) -> SetupError {
    SetupError::Runtime(format!(
        "protected machine ACL does not match SYSTEM/Admin Full, Users Read+Execute, ALL APPLICATION PACKAGES Read+Execute, and ALL RESTRICTED APPLICATION PACKAGES Read+Execute: {}",
        path.display()
    ))
}

struct ExpectedSids {
    system: OwnedSid,
    administrators: OwnedSid,
    users: OwnedSid,
    all_application_packages: OwnedSid,
    all_restricted_application_packages: OwnedSid,
}

impl ExpectedSids {
    fn new() -> Result<Self, SetupError> {
        Ok(Self {
            system: OwnedSid::from_string(SYSTEM_SID).map_err(SetupError::Io)?,
            administrators: OwnedSid::from_string(ADMINISTRATORS_SID).map_err(SetupError::Io)?,
            users: OwnedSid::from_string(USERS_SID).map_err(SetupError::Io)?,
            all_application_packages: OwnedSid::from_string(ALL_APPLICATION_PACKAGES_SID)
                .map_err(SetupError::Io)?,
            all_restricted_application_packages: OwnedSid::from_string(
                ALL_RESTRICTED_APPLICATION_PACKAGES_SID,
            )
            .map_err(SetupError::Io)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use mactype_service_contract::BrokerCommand;
    #[cfg(feature = "ci-test-adapter")]
    use mactype_service_contract::{sha256_digest, MachinePaths};
    #[cfg(feature = "ci-test-adapter")]
    use mactype_service_platform::{current_token_is_member_of, OwnedSid};
    use mactype_service_platform::{LocalSecurityDescriptor, SecurityDescriptor};
    #[cfg(feature = "ci-test-adapter")]
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use windows_sys::Win32::Security::{CONTAINER_INHERIT_ACE, INHERITED_ACE, OBJECT_INHERIT_ACE};

    use crate::windows::broker::prepare_machine_storage_for_command;
    #[cfg(feature = "ci-test-adapter")]
    use crate::{FixedPayload, RuntimeInstaller};

    #[cfg(feature = "ci-test-adapter")]
    use super::ADMINISTRATORS_SID;
    use super::{
        ace_applies_to_current_object, harden_machine_directory, harden_machine_directory_observed,
        is_read_execute_mask, verify_protected_acl, ExpectedSids, HardeningObservation,
        FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, GENERIC_EXECUTE, GENERIC_READ, INHERIT_ONLY_ACE,
        MACHINE_TREE_SDDL,
    };

    const BASE_ACL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)(A;OICI;GRGX;;;AC)(A;OICI;GRGX;;;S-1-15-2-2)";
    const OLD_ACL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)";

    #[test]
    fn machine_tree_sddl_builds_a_security_descriptor() {
        SecurityDescriptor::from_sddl(MACHINE_TREE_SDDL).unwrap();
    }

    #[test]
    fn hardening_accepts_the_generic_read_execute_ace_emitted_for_the_root() {
        let directory = tempfile::tempdir().unwrap();

        let result = harden_machine_directory(directory.path());

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn hardening_errors_identify_the_bounded_operation_and_path() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing-machine-root");

        let error = harden_machine_directory(&missing).unwrap_err();
        let message = error.to_string();

        assert!(
            message.contains("enumerate protected ACL tree"),
            "{message}"
        );
        assert!(
            message.contains(&missing.display().to_string()),
            "{message}"
        );
    }

    #[test]
    fn hardening_tolerates_a_discovered_child_that_disappears_before_inspection() {
        let directory = tempfile::tempdir().unwrap();
        let transient = directory.path().join("profile.ini.tmp");
        std::fs::write(&transient, b"temporary profile").unwrap();
        let mut removed = false;

        let result = harden_machine_directory_observed(directory.path(), |observation, path| {
            if observation == HardeningObservation::BeforeInspect && path == transient {
                std::fs::remove_file(path).unwrap();
                removed = true;
            }
        });

        assert!(removed, "the fixture must reproduce the metadata race");
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn hardening_tolerates_a_collected_child_that_disappears_before_acl_verification() {
        let directory = tempfile::tempdir().unwrap();
        let transient = directory.path().join("health.json.tmp");
        std::fs::write(&transient, b"temporary health snapshot").unwrap();
        let mut removed = false;

        let result = harden_machine_directory_observed(directory.path(), |observation, path| {
            if observation == HardeningObservation::BeforeVerify && path == transient {
                apply_acl(path, "D:P(A;;FA;;;WD)");
                std::fs::remove_file(path).unwrap();
                removed = true;
            }
        });

        assert!(
            removed,
            "the fixture must reproduce the ACL verification race"
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn hardening_verifies_nested_directories_and_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("payload").join("generation-1");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("manifest.json"), b"{}").unwrap();

        let result = harden_machine_directory(directory.path());

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            acl_snapshot(&nested),
            vec![
                (
                    "SYSTEM",
                    super::FILE_ALL_ACCESS,
                    (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE) as u8,
                ),
                (
                    "Administrators",
                    super::FILE_ALL_ACCESS,
                    (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE) as u8,
                ),
                (
                    "Users",
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                    INHERITED_ACE as u8,
                ),
                (
                    "Users",
                    GENERIC_READ | GENERIC_EXECUTE,
                    (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE | INHERITED_ACE)
                        as u8,
                ),
                (
                    "AllApplicationPackages",
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                    INHERITED_ACE as u8,
                ),
                (
                    "AllApplicationPackages",
                    GENERIC_READ | GENERIC_EXECUTE,
                    (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE | INHERITED_ACE)
                        as u8,
                ),
                (
                    "AllRestrictedApplicationPackages",
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                    INHERITED_ACE as u8,
                ),
                (
                    "AllRestrictedApplicationPackages",
                    GENERIC_READ | GENERIC_EXECUTE,
                    (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE | INHERITED_ACE)
                        as u8,
                ),
            ]
        );
        assert_eq!(
            acl_snapshot(&nested.join("manifest.json")),
            vec![
                ("SYSTEM", super::FILE_ALL_ACCESS, INHERITED_ACE as u8),
                (
                    "Administrators",
                    super::FILE_ALL_ACCESS,
                    INHERITED_ACE as u8,
                ),
                (
                    "Users",
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                    INHERITED_ACE as u8,
                ),
                (
                    "AllApplicationPackages",
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                    INHERITED_ACE as u8,
                ),
                (
                    "AllRestrictedApplicationPackages",
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                    INHERITED_ACE as u8,
                ),
            ]
        );
    }

    #[test]
    fn hardening_repairs_a_tree_that_predates_package_read_access() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("payload");
        std::fs::create_dir(&nested).unwrap();
        let file = nested.join("manifest.json");
        std::fs::write(&file, b"{}").unwrap();
        apply_acl(directory.path(), OLD_ACL);
        let sids = ExpectedSids::new().unwrap();

        verify_protected_acl(directory.path(), &sids, true)
            .expect_err("the old ACL must omit application package read access");

        harden_machine_directory(directory.path()).unwrap();

        verify_protected_acl(directory.path(), &sids, true).unwrap();
        verify_protected_acl(&file, &sids, false).unwrap();
    }

    #[test]
    fn repair_preflight_removes_users_modify_from_a_runtime_file_before_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = directory.path().join("bin").join("0.2.0");
        std::fs::create_dir_all(&runtime).unwrap();
        let service = runtime.join("mactype-service.exe");
        std::fs::write(&service, b"service").unwrap();
        harden_machine_directory(directory.path()).unwrap();
        apply_acl(&service, "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x001301BF;;;BU)");
        let sids = ExpectedSids::new().unwrap();
        verify_protected_acl(&service, &sids, false)
            .expect_err("the regression fixture must grant Users Modify");

        prepare_machine_storage_for_command(BrokerCommand::Repair, directory.path()).unwrap();

        verify_protected_acl(directory.path(), &sids, true).unwrap();
        verify_protected_acl(&service, &sids, false).unwrap();
    }

    #[test]
    fn hardening_removes_the_exact_users_modify_ace_emitted_by_icacls() {
        let directory = tempfile::tempdir().unwrap();
        let _cleanup = ResetFixtureAclOnDrop(directory.path().to_owned());
        let runtime = directory.path().join("bin").join("0.2.0");
        std::fs::create_dir_all(&runtime).unwrap();
        let service = runtime.join("mactype-service.exe");
        std::fs::write(&service, b"service").unwrap();
        harden_machine_directory(directory.path()).unwrap();

        grant_users_modify_with_icacls(&service);
        let sids = ExpectedSids::new().unwrap();
        verify_protected_acl(&service, &sids, false)
            .expect_err("the exact hosted-CI fixture must grant Users Modify");

        harden_machine_directory(directory.path()).unwrap();

        verify_protected_acl(directory.path(), &sids, true).unwrap();
        verify_protected_acl(&service, &sids, false).unwrap();
    }

    #[cfg(feature = "ci-test-adapter")]
    #[test]
    fn administrator_required_fixture_never_skips_in_ci() {
        assert_eq!(administrator_fixture_policy(false, true), Ok(true));
        assert_eq!(administrator_fixture_policy(true, true), Ok(true));
        assert_eq!(administrator_fixture_policy(false, false), Ok(false));
        assert!(administrator_fixture_policy(true, false).is_err());
    }

    #[cfg(feature = "ci-test-adapter")]
    #[test]
    fn repair_lifecycle_survives_the_exact_users_modify_ace_emitted_by_icacls() {
        match administrator_fixture_policy(
            running_in_ci(),
            current_token_is_enabled_administrator(),
        ) {
            Ok(true) => {}
            Ok(false) => {
                eprintln!(
                    "skipped locally: protected runtime repair requires an enabled Administrator token"
                );
                return;
            }
            Err(message) => panic!("{message}"),
        }
        let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let _cleanup = ResetFixtureAclOnDrop(base.path().to_owned());
        let program_files = base.path().join("Program Files");
        let program_data = base.path().join("ProgramData");
        std::fs::create_dir_all(&program_files).unwrap();
        std::fs::create_dir_all(&program_data).unwrap();
        let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
        let payload = test_payload(base.path(), "0.2.0");
        let installer = RuntimeInstaller::new(paths.clone());
        installer
            .deploy_with_health_check(&payload, |_| Ok(()))
            .unwrap();
        harden_machine_directory(paths.service_root()).unwrap();
        let service = paths
            .runtime_versions()
            .join("0.2.0")
            .join("mactype-service.exe");
        grant_users_modify_with_icacls(&service);

        prepare_machine_storage_for_command(BrokerCommand::Repair, paths.service_root()).unwrap();
        installer
            .repair_current_with_health_check(&payload, |_| Ok(()))
            .unwrap();

        verify_protected_acl(&service, &ExpectedSids::new().unwrap(), false).unwrap();
    }

    #[test]
    fn machine_root_with_inheritable_dacl_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        apply_acl_with_protection(
            directory.path(),
            "D:(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)",
            false,
        );

        let error = verify_protected_acl(directory.path(), &ExpectedSids::new().unwrap(), true)
            .expect_err("machine root must block parent ACL inheritance");

        assert!(error.to_string().contains("still permits inheritance"));
    }

    #[test]
    fn descendant_users_write_access_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("runtime-receipts");
        std::fs::create_dir(&child).unwrap();
        apply_acl(directory.path(), &format!("{BASE_ACL}(A;OICI;GW;;;BU)"));

        let error = verify_protected_acl(&child, &ExpectedSids::new().unwrap(), false)
            .expect_err("descendant Users write access must fail closed");

        assert!(error.to_string().contains("invalid Users rights"));
    }

    #[test]
    fn descendant_unapproved_write_trustee_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("runtime-receipts");
        std::fs::create_dir(&child).unwrap();
        apply_acl(directory.path(), &format!("{BASE_ACL}(A;OICI;GW;;;WD)"));

        let error = verify_protected_acl(&child, &ExpectedSids::new().unwrap(), false)
            .expect_err("descendant unapproved write trustee must fail closed");

        assert!(error.to_string().contains("unapproved allow ACE"));
    }

    #[test]
    fn inherit_only_trusted_writer_aces_do_not_replace_current_object_access() {
        for sddl in [
            "D:P(A;OICIIO;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)(A;OICI;GRGX;;;AC)(A;OICI;GRGX;;;S-1-15-2-2)",
            "D:P(A;OICI;FA;;;SY)(A;OICIIO;FA;;;BA)(A;OICI;GRGX;;;BU)(A;OICI;GRGX;;;AC)(A;OICI;GRGX;;;S-1-15-2-2)",
        ] {
            let error = verify_acl_fixture(sddl)
                .expect_err("inherit-only trusted writer ACE must not satisfy root access");

            assert!(error
                .to_string()
                .contains("does not match SYSTEM/Admin Full"));
        }
    }

    #[test]
    fn supplemental_inherit_only_trusted_writer_aces_are_allowed() {
        verify_acl_fixture(&format!("{BASE_ACL}(A;OICIIO;FA;;;SY)(A;OICIIO;FA;;;BA)"))
            .expect("inherit-only propagation ACEs may accompany current-object writer ACEs");
    }

    #[test]
    fn hosted_root_and_mapped_child_read_execute_masks_are_safe() {
        assert!(is_read_execute_mask(0xA000_0000));
        assert!(is_read_execute_mask(0x0012_00A9));
        assert_eq!(GENERIC_READ | GENERIC_EXECUTE, 0xA000_0000);
        assert_eq!(FILE_GENERIC_READ | FILE_GENERIC_EXECUTE, 0x0012_00A9);
        assert!(!ace_applies_to_current_object(
            INHERIT_ONLY_ACE as u8 | 0x03
        ));
        assert!(ace_applies_to_current_object(0x10));
    }

    #[test]
    fn users_write_access_is_rejected() {
        let error = verify_acl_fixture(&format!("{BASE_ACL}(A;OICI;GW;;;BU)"))
            .expect_err("Users write access must fail closed");

        assert!(error.to_string().contains("invalid Users rights"));
    }

    #[test]
    fn verification_rejects_package_modify_rights() {
        let error = verify_acl_fixture(
            "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;GRGX;;;BU)(A;OICI;FA;;;AC)(A;OICI;GRGX;;;S-1-15-2-2)",
        )
        .expect_err("application package write access must fail closed");

        assert!(error.to_string().contains("AllApplicationPackages"));
    }

    #[test]
    fn everyone_write_access_is_rejected() {
        let error = verify_acl_fixture(&format!("{BASE_ACL}(A;OICI;GW;;;WD)"))
            .expect_err("Everyone write access must fail closed");

        assert!(error.to_string().contains("unapproved allow ACE"));
    }

    #[test]
    fn authenticated_users_write_access_is_rejected() {
        let error = verify_acl_fixture(&format!("{BASE_ACL}(A;OICI;GW;;;AU)"))
            .expect_err("Authenticated Users write access must fail closed");

        assert!(error.to_string().contains("unapproved allow ACE"));
    }

    fn verify_acl_fixture(sddl: &str) -> Result<(), super::SetupError> {
        let directory = tempfile::tempdir().unwrap();
        apply_acl(directory.path(), sddl);
        verify_protected_acl(directory.path(), &ExpectedSids::new().unwrap(), true)
    }

    fn apply_acl(path: &Path, sddl: &str) {
        apply_acl_with_protection(path, sddl, true);
    }

    fn apply_acl_with_protection(path: &Path, sddl: &str, protect: bool) {
        let descriptor = SecurityDescriptor::from_sddl(sddl).unwrap();
        descriptor.apply_to_tree(path, protect).unwrap();
    }

    fn grant_users_modify_with_icacls(path: &Path) {
        let output = Command::new(r"C:\Windows\System32\icacls.exe")
            .arg(path)
            .args(["/grant", "*S-1-5-32-545:(M)"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "icacls failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    struct ResetFixtureAclOnDrop(PathBuf);

    impl Drop for ResetFixtureAclOnDrop {
        fn drop(&mut self) {
            let _ = Command::new(r"C:\Windows\System32\icacls.exe")
                .arg(&self.0)
                .args(["/reset", "/T", "/C", "/Q"])
                .output();
        }
    }

    #[cfg(feature = "ci-test-adapter")]
    fn administrator_fixture_policy(
        running_in_ci: bool,
        enabled_administrator: bool,
    ) -> Result<bool, &'static str> {
        if enabled_administrator {
            Ok(true)
        } else if running_in_ci {
            Err("CI must provide an enabled Administrator token for the protected repair fixture")
        } else {
            Ok(false)
        }
    }

    #[cfg(feature = "ci-test-adapter")]
    fn running_in_ci() -> bool {
        ["GITHUB_ACTIONS", "CI"]
            .iter()
            .any(|name| std::env::var(name).is_ok_and(|value| value.eq_ignore_ascii_case("true")))
    }

    #[cfg(feature = "ci-test-adapter")]
    fn current_token_is_enabled_administrator() -> bool {
        OwnedSid::from_string(ADMINISTRATORS_SID)
            .map(|administrators| current_token_is_member_of(&administrators))
            .unwrap_or(false)
    }

    #[cfg(feature = "ci-test-adapter")]
    fn test_payload(base: &Path, version: &str) -> FixedPayload {
        let root = base.join("payload");
        let files_root = root.join("files");
        std::fs::create_dir_all(&files_root).unwrap();
        let payload_files: [(&str, &[u8]); 5] = [
            ("mactype-service.exe", b"service"),
            ("mactype-injector32.exe", b"injector-32"),
            ("mactype-injector64.exe", b"injector-64"),
            ("MacType.dll", b"mactype-32"),
            ("MacType64.dll", b"mactype-64"),
        ];
        let mut files = BTreeMap::new();
        for (name, contents) in payload_files {
            std::fs::write(files_root.join(name), contents).unwrap();
            files.insert(name.to_owned(), sha256_digest(contents));
        }
        std::fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema": 1,
                "version": version,
                "files": files,
            }))
            .unwrap(),
        )
        .unwrap();
        FixedPayload::from_test_root(root).unwrap()
    }

    fn acl_snapshot(path: &Path) -> Vec<(&'static str, u32, u8)> {
        let descriptor = LocalSecurityDescriptor::query_named_file(path)
            .expect("the fixture path has a readable DACL");
        let dacl = descriptor
            .dacl()
            .expect("the fixture DACL holds only allow ACEs");
        let sids = ExpectedSids::new().unwrap();
        dacl.allowed_aces()
            .map(|ace| {
                let trustee = if ace.trustee_matches(&sids.system) {
                    "SYSTEM"
                } else if ace.trustee_matches(&sids.administrators) {
                    "Administrators"
                } else if ace.trustee_matches(&sids.users) {
                    "Users"
                } else if ace.trustee_matches(&sids.all_application_packages) {
                    "AllApplicationPackages"
                } else if ace.trustee_matches(&sids.all_restricted_application_packages) {
                    "AllRestrictedApplicationPackages"
                } else {
                    "Unknown"
                };
                (trustee, ace.mask(), ace.flags())
            })
            .collect()
    }
}
