use super::super::legacy_mactray::{
    observe_owned_tray_startup, plan_startup_restore, read_tray_startup_artifact_bytes,
    remove_tray_startup_artifact_exact, restore_tray_startup_artifact_if_absent,
    LegacyTrayStartupArtifact, LegacyTrayStartupLocator, LegacyTrayStartupScope,
    LegacyTrayStartupSource, StartupRestoreAction,
};
use super::storage::{
    atomic_json, create_migration_storage_root, migration_storage_root, read_json_bounded_under,
    secure_create_tree,
};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const STARTUP_RECEIPT_SCHEMA: &str = "mactype-control-center/legacy-tray-startup";
const STARTUP_RECEIPT_VERSION: u32 = 1;
const MAX_STARTUP_RECEIPT_ENTRIES: usize = 64;
const MAX_STARTUP_ARTIFACT_BYTES: usize = 1024 * 1024;
const CURRENT_USER_RECEIPT_FILE: &str = "legacy-tray-startup-current-user.json";
const LOCAL_MACHINE_RECEIPT_FILE: &str = "legacy-tray-startup-local-machine.json";
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const CURRENT_USER_RESTORE_SWITCH: &str = "--restore-current-user-legacy-tray-autostart";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StartupReceiptScope {
    CurrentUser,
    LocalMachine,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum StartupRestorationState {
    Pending,
    Restored,
    ManualRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct LegacyTrayStartupReceipt {
    schema: String,
    version: u32,
    scope: StartupReceiptScope,
    user_sid: String,
    pub(super) entries: Vec<LegacyTrayStartupArtifact>,
    recorded_at: u64,
    pub(super) restoration_state: StartupRestorationState,
}

pub(super) fn build_startup_receipt(
    scope: StartupReceiptScope,
    user_sid: &str,
    entries: Vec<LegacyTrayStartupArtifact>,
    recorded_at: u64,
) -> Result<LegacyTrayStartupReceipt, String> {
    if user_sid.is_empty()
        || recorded_at == 0
        || entries.is_empty()
        || entries.len() > MAX_STARTUP_RECEIPT_ENTRIES
    {
        return Err("legacy tray startup receipt metadata is invalid".to_owned());
    }
    for (index, entry) in entries.iter().enumerate() {
        if entry.user_sid != user_sid
            || entry.recorded_at == 0
            || entry.raw_bytes.is_empty()
            || entry.raw_bytes.len() > MAX_STARTUP_ARTIFACT_BYTES
            || !source_belongs_to_scope(entry.entry.source_kind, scope)
            || !is_exact_mactray_target(&entry.entry.target_path)
            || !is_exact_mactray_target(&entry.normalized_target_path)
            || !locator_matches_source(entry)
        {
            return Err("legacy tray startup receipt contains an unowned artifact".to_owned());
        }
        if entries[..index]
            .iter()
            .any(|previous| previous.locator == entry.locator)
        {
            return Err("legacy tray startup receipt contains a duplicate locator".to_owned());
        }
    }
    Ok(LegacyTrayStartupReceipt {
        schema: STARTUP_RECEIPT_SCHEMA.to_owned(),
        version: STARTUP_RECEIPT_VERSION,
        scope,
        user_sid: user_sid.to_owned(),
        entries,
        recorded_at,
        restoration_state: StartupRestorationState::Pending,
    })
}

impl StartupReceiptScope {
    fn startup_scope(self) -> LegacyTrayStartupScope {
        match self {
            Self::CurrentUser => LegacyTrayStartupScope::CurrentUser,
            Self::LocalMachine => LegacyTrayStartupScope::LocalMachine,
        }
    }

    fn receipt_name(self) -> &'static str {
        match self {
            Self::CurrentUser => CURRENT_USER_RECEIPT_FILE,
            Self::LocalMachine => LOCAL_MACHINE_RECEIPT_FILE,
        }
    }
}

fn source_belongs_to_scope(source: LegacyTrayStartupSource, scope: StartupReceiptScope) -> bool {
    match scope {
        StartupReceiptScope::CurrentUser => matches!(
            source,
            LegacyTrayStartupSource::CurrentUserRun32
                | LegacyTrayStartupSource::CurrentUserRun64
                | LegacyTrayStartupSource::CurrentUserStartup
        ),
        StartupReceiptScope::LocalMachine => matches!(
            source,
            LegacyTrayStartupSource::LocalMachineRun32 | LegacyTrayStartupSource::LocalMachineRun64
        ),
    }
}

fn locator_matches_source(artifact: &LegacyTrayStartupArtifact) -> bool {
    match (&artifact.entry.source_kind, &artifact.locator) {
        (
            LegacyTrayStartupSource::CurrentUserRun32,
            LegacyTrayStartupLocator::Registry {
                hive,
                view,
                subkey,
                value_name,
                value_type,
            },
        ) => {
            hive == "HKCU"
                && *view == 32
                && subkey == RUN_SUBKEY
                && value_name == &artifact.entry.display_name
                && matches!(*value_type, REG_SZ | REG_EXPAND_SZ)
        }
        (
            LegacyTrayStartupSource::CurrentUserRun64,
            LegacyTrayStartupLocator::Registry {
                hive,
                view,
                subkey,
                value_name,
                value_type,
            },
        ) => {
            hive == "HKCU"
                && *view == 64
                && subkey == RUN_SUBKEY
                && value_name == &artifact.entry.display_name
                && matches!(*value_type, REG_SZ | REG_EXPAND_SZ)
        }
        (
            LegacyTrayStartupSource::LocalMachineRun32,
            LegacyTrayStartupLocator::Registry {
                hive,
                view,
                subkey,
                value_name,
                value_type,
            },
        ) => {
            hive == "HKLM"
                && *view == 32
                && subkey == RUN_SUBKEY
                && value_name == &artifact.entry.display_name
                && matches!(*value_type, REG_SZ | REG_EXPAND_SZ)
        }
        (
            LegacyTrayStartupSource::LocalMachineRun64,
            LegacyTrayStartupLocator::Registry {
                hive,
                view,
                subkey,
                value_name,
                value_type,
            },
        ) => {
            hive == "HKLM"
                && *view == 64
                && subkey == RUN_SUBKEY
                && value_name == &artifact.entry.display_name
                && matches!(*value_type, REG_SZ | REG_EXPAND_SZ)
        }
        (
            LegacyTrayStartupSource::CurrentUserStartup,
            LegacyTrayStartupLocator::File { startup_file_path },
        ) => {
            startup_file_path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
                && startup_file_path
                    .file_stem()
                    .is_some_and(|stem| stem == artifact.entry.display_name.as_str())
        }
        _ => false,
    }
}

fn is_exact_mactray_target(path: &Path) -> bool {
    let Some(root) = super::super::legacy_mactray::trusted_installation_root() else {
        return cfg!(not(windows))
            && path
                .to_string_lossy()
                .eq_ignore_ascii_case(r"C:\Program Files\MacType\MacTray.exe");
    };
    super::super::legacy_mactray::same_windows_path(&root.join("MacTray.exe"), path)
}

pub(super) trait StartupRestoreBackend {
    fn current_bytes(
        &mut self,
        artifact: &LegacyTrayStartupArtifact,
    ) -> Result<Option<Vec<u8>>, String>;
    fn restore_original(&mut self, artifact: &LegacyTrayStartupArtifact) -> Result<(), String>;
    fn mark_restoration(&mut self, state: StartupRestorationState) -> Result<(), String>;
}

pub(super) fn restore_startup_with(
    backend: &mut impl StartupRestoreBackend,
    receipt: &LegacyTrayStartupReceipt,
) -> Result<(), String> {
    let mut actions = Vec::with_capacity(receipt.entries.len());
    for artifact in &receipt.entries {
        let current = backend.current_bytes(artifact)?;
        match plan_startup_restore(&artifact.raw_bytes, current.as_deref()) {
            Ok(action) => actions.push(action),
            Err(error) => {
                backend.mark_restoration(StartupRestorationState::ManualRequired)?;
                return Err(error);
            }
        }
    }
    for (artifact, action) in receipt.entries.iter().zip(actions) {
        if action == StartupRestoreAction::Restore {
            if let Err(error) = backend.restore_original(artifact) {
                backend.mark_restoration(StartupRestorationState::ManualRequired)?;
                return Err(error);
            }
        }
    }
    backend.mark_restoration(StartupRestorationState::Restored)
}

pub(crate) fn disable_startup_scope(scope: StartupReceiptScope) -> Result<(), String> {
    let entries = observe_owned_tray_startup(scope.startup_scope())?;
    if entries.is_empty() {
        return Ok(());
    }
    let user_sid = entries
        .first()
        .map(|entry| entry.user_sid.clone())
        .ok_or_else(|| "legacy tray startup observation lost its user identity".to_owned())?;
    let proposed = build_startup_receipt(scope, &user_sid, entries, timestamp_millis()?)?;
    let existing = read_receipt_if_present(scope)?;
    let receipt = select_startup_receipt_for_disable(
        existing.as_ref().map(|(_, _, receipt)| receipt),
        proposed,
    )?;
    let (root, path) = match existing {
        Some((root, path, _)) => (root, path),
        None => create_receipt_path(scope)?,
    };
    persist_verified_receipt(&root, &path, &receipt)?;

    let current = observe_owned_tray_startup(scope.startup_scope())?;
    if !same_artifact_snapshot(scope, &current, &receipt.entries) {
        return Err(restore_after_disable_failure(
            scope,
            "legacy MacTray startup changed before removal".to_owned(),
        ));
    }
    for artifact in &receipt.entries {
        if let Err(error) = remove_tray_startup_artifact_exact(artifact) {
            return Err(restore_after_disable_failure(scope, error));
        }
    }
    match observe_owned_tray_startup(scope.startup_scope()) {
        Ok(remaining) if remaining.is_empty() => Ok(()),
        Ok(_) => Err(restore_after_disable_failure(
            scope,
            "legacy MacTray startup remains after removal".to_owned(),
        )),
        Err(error) => Err(restore_after_disable_failure(scope, error)),
    }
}

pub(super) fn select_startup_receipt_for_disable(
    existing: Option<&LegacyTrayStartupReceipt>,
    proposed: LegacyTrayStartupReceipt,
) -> Result<LegacyTrayStartupReceipt, String> {
    let Some(existing) = existing else {
        return Ok(proposed);
    };
    if existing.scope != proposed.scope {
        return Err("the existing startup receipt belongs to another scope".to_owned());
    }
    match existing.restoration_state {
        StartupRestorationState::Restored => Ok(proposed),
        StartupRestorationState::ManualRequired => Err(
            "the existing startup receipt requires manual restoration before another disable"
                .to_owned(),
        ),
        StartupRestorationState::Pending => {
            if same_artifact_snapshot(existing.scope, &existing.entries, &proposed.entries) {
                Ok(existing.clone())
            } else {
                Err("a different pending startup receipt would be overwritten".to_owned())
            }
        }
    }
}

fn same_artifact_snapshot(
    scope: StartupReceiptScope,
    left: &[LegacyTrayStartupArtifact],
    right: &[LegacyTrayStartupArtifact],
) -> bool {
    left.len() == right.len()
        && left.iter().all(|candidate| {
            right
                .iter()
                .filter(|expected| {
                    candidate.entry == expected.entry
                        && candidate.locator == expected.locator
                        && candidate.raw_bytes == expected.raw_bytes
                        && candidate.normalized_target_path == expected.normalized_target_path
                        && (scope == StartupReceiptScope::LocalMachine
                            || candidate.user_sid == expected.user_sid)
                })
                .count()
                == 1
        })
}

pub(crate) fn restore_startup_scope(scope: StartupReceiptScope) -> Result<(), String> {
    let Some((root, path, receipt)) = read_receipt_if_present(scope)? else {
        return Ok(());
    };
    match receipt.restoration_state {
        StartupRestorationState::Restored => Ok(()),
        StartupRestorationState::ManualRequired => {
            Err("legacy MacTray startup restoration requires manual intervention".to_owned())
        }
        StartupRestorationState::Pending => {
            let mut backend = SystemStartupRestoreBackend {
                root,
                path,
                receipt: receipt.clone(),
            };
            restore_startup_with(&mut backend, &receipt)
        }
    }
}

pub(super) fn user_restore_requested_from_arguments<I>(arguments: I) -> Result<bool, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut arguments = arguments.into_iter();
    let _executable = arguments.next();
    let Some(switch) = arguments.next() else {
        return Ok(false);
    };
    if switch != OsStr::new(CURRENT_USER_RESTORE_SWITCH) {
        return Ok(false);
    }
    if arguments.next().is_some() {
        return Err("the current-user startup restore command rejects arguments".to_owned());
    }
    Ok(true)
}

pub(crate) fn dispatch_current_user_restore_command() -> Option<i32> {
    match user_restore_requested_from_arguments(std::env::args_os()) {
        Ok(false) => None,
        Ok(true) => Some(
            if restore_startup_scope(StartupReceiptScope::CurrentUser).is_ok() {
                0
            } else {
                21
            },
        ),
        Err(_) => Some(21),
    }
}

fn restore_after_disable_failure(scope: StartupReceiptScope, failure: String) -> String {
    match restore_startup_scope(scope) {
        Ok(()) => format!("{failure}; the original startup state was restored"),
        Err(restore_error) => format!("{failure}; startup restore also failed: {restore_error}"),
    }
}

struct SystemStartupRestoreBackend {
    root: PathBuf,
    path: PathBuf,
    receipt: LegacyTrayStartupReceipt,
}

impl StartupRestoreBackend for SystemStartupRestoreBackend {
    fn current_bytes(
        &mut self,
        artifact: &LegacyTrayStartupArtifact,
    ) -> Result<Option<Vec<u8>>, String> {
        read_tray_startup_artifact_bytes(artifact)
    }

    fn restore_original(&mut self, artifact: &LegacyTrayStartupArtifact) -> Result<(), String> {
        restore_tray_startup_artifact_if_absent(artifact)
    }

    fn mark_restoration(&mut self, state: StartupRestorationState) -> Result<(), String> {
        self.receipt.restoration_state = state;
        persist_verified_receipt(&self.root, &self.path, &self.receipt)
    }
}

fn create_receipt_path(scope: StartupReceiptScope) -> Result<(PathBuf, PathBuf), String> {
    let root = match scope {
        StartupReceiptScope::CurrentUser => create_current_user_storage_root()?,
        StartupReceiptScope::LocalMachine => create_migration_storage_root()?,
    };
    let path = root.join(scope.receipt_name());
    Ok((root, path))
}

fn existing_receipt_path(scope: StartupReceiptScope) -> Result<(PathBuf, PathBuf), String> {
    let root = match scope {
        StartupReceiptScope::CurrentUser => current_user_storage_root()?,
        StartupReceiptScope::LocalMachine => migration_storage_root()?,
    };
    let path = root.join(scope.receipt_name());
    Ok((root, path))
}

fn persist_verified_receipt(
    root: &Path,
    path: &Path,
    receipt: &LegacyTrayStartupReceipt,
) -> Result<(), String> {
    validate_startup_receipt(receipt)?;
    atomic_json(path, receipt)?;
    let stored: LegacyTrayStartupReceipt = read_json_bounded_under(root, path)?;
    validate_startup_receipt(&stored)?;
    if stored != *receipt {
        return Err("legacy tray startup receipt did not round-trip exactly".to_owned());
    }
    Ok(())
}

fn read_receipt_if_present(
    scope: StartupReceiptScope,
) -> Result<Option<(PathBuf, PathBuf, LegacyTrayStartupReceipt)>, String> {
    let (root, path) = match existing_receipt_path(scope) {
        Ok(value) => value,
        Err(error) if storage_absent(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let receipt: LegacyTrayStartupReceipt = read_json_bounded_under(&root, &path)?;
            validate_startup_receipt(&receipt)?;
            if receipt.scope != scope {
                return Err("legacy tray startup receipt scope does not match its store".to_owned());
            }
            Ok(Some((root, path, receipt)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn validate_startup_receipt(receipt: &LegacyTrayStartupReceipt) -> Result<(), String> {
    if receipt.schema != STARTUP_RECEIPT_SCHEMA
        || receipt.version != STARTUP_RECEIPT_VERSION
        || receipt.restoration_state == StartupRestorationState::ManualRequired
            && receipt.entries.is_empty()
    {
        return Err("legacy tray startup receipt header is invalid".to_owned());
    }
    let rebuilt = build_startup_receipt(
        receipt.scope,
        &receipt.user_sid,
        receipt.entries.clone(),
        receipt.recorded_at,
    )?;
    if rebuilt.schema != receipt.schema
        || rebuilt.version != receipt.version
        || rebuilt.scope != receipt.scope
        || rebuilt.user_sid != receipt.user_sid
        || rebuilt.entries != receipt.entries
        || rebuilt.recorded_at != receipt.recorded_at
    {
        return Err("legacy tray startup receipt failed exact validation".to_owned());
    }
    Ok(())
}

fn storage_absent(error: &str) -> bool {
    error == "legacy migration storage does not exist"
        || error == "current-user legacy migration storage does not exist"
}

fn timestamp_millis() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "system time exceeds receipt range".to_owned())
}

#[cfg(windows)]
fn local_app_data_root() -> Result<PathBuf, String> {
    mactype_service_platform::known_folder_path(mactype_service_platform::KnownFolder::LocalAppData)
        .map_err(|error| {
            let result = error.raw_os_error().unwrap_or(0);
            format!("SHGetKnownFolderPath(LocalAppData) failed with HRESULT {result}")
        })
}

#[cfg(not(windows))]
fn local_app_data_root() -> Result<PathBuf, String> {
    Err("legacy MacTray startup migration is available only on Windows".to_owned())
}

fn create_current_user_storage_root() -> Result<PathBuf, String> {
    let local_app_data = local_app_data_root()?;
    secure_create_tree(
        &local_app_data,
        &["MacType", "ControlCenter", "legacy-migration"],
    )
}

fn current_user_storage_root() -> Result<PathBuf, String> {
    let local_app_data = local_app_data_root()?;
    let root = local_app_data
        .join("MacType")
        .join("ControlCenter")
        .join("legacy-migration");
    if !root.is_dir() {
        return Err("current-user legacy migration storage does not exist".to_owned());
    }
    super::storage::validate_existing_path(&local_app_data, &root)?;
    Ok(root)
}
