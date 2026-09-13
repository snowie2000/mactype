use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use mactype_service_contract::{HealthReport, MAX_PINNED_RUNTIMES};

use super::deferred_delete::{defer_directory, remove_directory_or_defer, remove_file_or_defer};
use super::journal::{safe_version_component, validate_runtime_pointer, MAX_POINTER_BYTES};
use super::RuntimeInstaller;
use crate::storage::{
    read_bounded_directory, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

const MAX_HEALTH_SNAPSHOT_BYTES: u64 = 16 * 1024;
const MAX_RUNTIME_GENERATIONS: usize = MAX_PINNED_RUNTIMES + 2;
const MAX_SERVICE_RUNTIME_ROOT_ENTRIES: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OwnedRemovalPlan {
    root_exists: bool,
    versions: Vec<String>,
    migration_pins: Vec<PathBuf>,
    remove_current: bool,
    remove_previous: bool,
    remove_health: bool,
    remove_versions_root: bool,
    remove_receipts_root: bool,
    remove_migration_pins_root: bool,
}

impl RuntimeInstaller {
    /// Removes only a complete runtime tree whose fixed files and generations can
    /// all be proven to belong to this installer. Protected profiles live below
    /// ProgramData and are deliberately outside this operation.
    pub fn remove_receipted_installation(&self) -> Result<bool, SetupError> {
        self.remove_receipted_installation_after(|| Ok(()))
    }

    pub(crate) fn remove_receipted_installation_after<F>(
        &self,
        before_cleanup: F,
    ) -> Result<bool, SetupError>
    where
        F: FnOnce() -> Result<(), SetupError>,
    {
        let before = self.prepare_owned_removal()?;
        before_cleanup()?;
        let after = self.prepare_owned_removal()?;
        if before != after {
            return Err(SetupError::CleanupUnknown(
                "service runtime changed while uninstalling; no runtime files were removed"
                    .to_owned(),
            ));
        }
        self.execute_owned_removal(after)
    }

    fn prepare_owned_removal(&self) -> Result<OwnedRemovalPlan, SetupError> {
        let root = self.paths.service_root();
        if !root.exists() {
            return Ok(OwnedRemovalPlan::default());
        }
        reject_reparse_ancestors(root)?;
        if !fs::metadata(root)?.is_dir() {
            return Err(SetupError::Runtime(
                "service runtime root is not a regular directory".to_owned(),
            ));
        }

        let mut plan = OwnedRemovalPlan {
            root_exists: true,
            ..OwnedRemovalPlan::default()
        };
        for entry in read_bounded_directory(
            root,
            MAX_SERVICE_RUNTIME_ROOT_ENTRIES,
            "service runtime root entry count",
        )? {
            let name = entry.file_name().into_string().map_err(|_| {
                SetupError::Runtime("service runtime entry name is not Unicode".to_owned())
            })?;
            reject_reparse_ancestors(&entry.path())?;
            match name.as_str() {
                "bin" => plan.remove_versions_root = true,
                "runtime-receipts" => plan.remove_receipts_root = true,
                "current.json" => plan.remove_current = true,
                "previous-runtime.json" => plan.remove_previous = true,
                "health.json" => plan.remove_health = true,
                "migration-runtime-pins" => plan.remove_migration_pins_root = true,
                "runtime-activation.json" | "runtime-repair.json" => {
                    return Err(SetupError::Runtime(
                        "a runtime transaction is pending during uninstall".to_owned(),
                    ));
                }
                _ => {
                    return Err(SetupError::Runtime(format!(
                        "unexpected service runtime entry blocks uninstall: {name}"
                    )));
                }
            }
        }

        let versions = self.verify_removable_generations(&plan)?;
        self.verify_runtime_pointer_for_removal(
            self.paths.runtime_pointer(),
            "active runtime pointer",
            plan.remove_current,
            &versions,
        )?;
        self.verify_runtime_pointer_for_removal(
            &self.previous_runtime_pointer_path(),
            "previous runtime pointer",
            plan.remove_previous,
            &versions,
        )?;
        self.verify_health_for_removal(plan.remove_health)?;
        plan.migration_pins = self.verify_migration_pins_for_removal(&plan)?;
        plan.versions = versions.into_iter().collect();
        Ok(plan)
    }

    fn verify_removable_generations(
        &self,
        plan: &OwnedRemovalPlan,
    ) -> Result<BTreeSet<String>, SetupError> {
        if plan.remove_versions_root != plan.remove_receipts_root {
            return Err(SetupError::Runtime(
                "runtime generations and receipts are not a complete owned pair".to_owned(),
            ));
        }
        if !plan.remove_versions_root {
            return Ok(BTreeSet::new());
        }
        reject_reparse_ancestors(self.paths.runtime_versions())?;
        if !fs::metadata(self.paths.runtime_versions())?.is_dir()
            || !fs::metadata(self.runtime_receipts_root())?.is_dir()
        {
            return Err(SetupError::Runtime(
                "runtime generation storage is not a regular directory".to_owned(),
            ));
        }

        let mut versions = BTreeSet::new();
        for entry in read_bounded_directory(
            self.paths.runtime_versions(),
            MAX_RUNTIME_GENERATIONS,
            "runtime generation count",
        )? {
            let version = entry.file_name().into_string().map_err(|_| {
                SetupError::Runtime("runtime generation name is not Unicode".to_owned())
            })?;
            if !safe_version_component(&version) {
                return Err(SetupError::Runtime(
                    "runtime generation name is not canonical".to_owned(),
                ));
            }
            self.verify_runtime_generation_receipt(&version, &entry.path())?;
            versions.insert(version);
        }

        let mut receipts = BTreeSet::new();
        for entry in read_bounded_directory(
            &self.runtime_receipts_root(),
            MAX_RUNTIME_GENERATIONS,
            "runtime receipt count",
        )? {
            reject_reparse_ancestors(&entry.path())?;
            let name = entry.file_name().into_string().map_err(|_| {
                SetupError::Runtime("runtime receipt name is not Unicode".to_owned())
            })?;
            let version = name.strip_suffix(".json").ok_or_else(|| {
                SetupError::Runtime("runtime receipt name is not canonical".to_owned())
            })?;
            if !safe_version_component(version) {
                return Err(SetupError::Runtime(
                    "runtime receipt name is not canonical".to_owned(),
                ));
            }
            receipts.insert(version.to_owned());
        }
        if receipts != versions {
            return Err(SetupError::Runtime(
                "runtime receipt set does not exactly match installed generations".to_owned(),
            ));
        }
        Ok(versions)
    }

    fn verify_runtime_pointer_for_removal(
        &self,
        path: &std::path::Path,
        description: &str,
        exists: bool,
        versions: &BTreeSet<String>,
    ) -> Result<(), SetupError> {
        if !exists {
            return Ok(());
        }
        let bytes = read_bounded_regular_file(path, MAX_POINTER_BYTES, description)?;
        let pointer = validate_runtime_pointer(&bytes)?;
        if !versions.contains(pointer.version()) {
            return Err(SetupError::Runtime(format!(
                "{description} does not name a receipted runtime generation"
            )));
        }
        Ok(())
    }

    fn verify_health_for_removal(&self, exists: bool) -> Result<(), SetupError> {
        if !exists {
            return Ok(());
        }
        let bytes = read_bounded_regular_file(
            &self.paths.service_root().join("health.json"),
            MAX_HEALTH_SNAPSHOT_BYTES,
            "persisted service health",
        )?;
        let report: HealthReport = serde_json::from_slice(&bytes)
            .map_err(|_| SetupError::Runtime("persisted service health is invalid".to_owned()))?;
        report
            .validate()
            .map_err(|_| SetupError::Runtime("persisted service health is invalid".to_owned()))
    }

    fn verify_migration_pins_for_removal(
        &self,
        plan: &OwnedRemovalPlan,
    ) -> Result<Vec<PathBuf>, SetupError> {
        if !plan.remove_migration_pins_root {
            return Ok(Vec::new());
        }
        self.load_verified_migration_pins()?;
        let root = self.migration_runtime_pins_root();
        let mut paths = Vec::new();
        for entry in
            read_bounded_directory(&root, MAX_PINNED_RUNTIMES, "migration runtime pin count")?
        {
            reject_reparse_ancestors(&entry.path())?;
            paths.push(entry.path());
        }
        paths.sort();
        Ok(paths)
    }

    fn execute_owned_removal(&self, plan: OwnedRemovalPlan) -> Result<bool, SetupError> {
        if !plan.root_exists {
            return Ok(false);
        }
        for version in &plan.versions {
            self.remove_manifest_verified_generation_for_uninstall(
                version,
                &self.paths.runtime_versions().join(version),
            )?;
        }
        if plan.remove_versions_root {
            remove_directory_or_defer(
                self.paths.runtime_versions(),
                "remove verified runtime versions directory",
            )?;
        }
        if plan.remove_receipts_root {
            remove_directory_or_defer(
                &self.runtime_receipts_root(),
                "remove verified runtime receipts directory",
            )?;
        }
        for pin in plan.migration_pins {
            remove_file_or_defer(&pin, "remove verified migration runtime pin")?;
        }
        if plan.remove_migration_pins_root {
            remove_directory_or_defer(
                &self.migration_runtime_pins_root(),
                "remove verified migration runtime pins directory",
            )?;
        }
        for (remove, path) in [
            (plan.remove_current, self.paths.runtime_pointer().to_owned()),
            (plan.remove_previous, self.previous_runtime_pointer_path()),
            (
                plan.remove_health,
                self.paths.service_root().join("health.json"),
            ),
        ] {
            if remove {
                remove_file_or_defer(&path, "remove verified runtime state file")?;
            }
        }
        remove_directory_or_defer(
            self.paths.service_root(),
            "remove verified service runtime directory",
        )?;
        if self.paths.service_root().exists() {
            let application_root = self.paths.service_root().parent().ok_or_else(|| {
                SetupError::CleanupUnknown(
                    "fixed service runtime directory has no application parent".to_owned(),
                )
            })?;
            defer_directory(
                application_root,
                "defer empty application directory cleanup until reboot",
            )?;
        }
        Ok(true)
    }
}
