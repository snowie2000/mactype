use super::*;
use mactype_service_platform::{
    current_user_sid_string, expand_environment_strings, file_attributes, known_folder_path,
    read_shortcut, ComApartment, ComThreading, DeleteValueOutcome, KnownFolder, RegistryKey,
    RegistryRoot, RegistryView,
};
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{Read, Write},
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};
use windows_sys::Win32::System::Registry::{REG_EXPAND_SZ, REG_SZ};

const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const MAX_REGISTRY_NAME_UNITS: usize = 16_384;
const MAX_REGISTRY_VALUE_BYTES: usize = 65_536;
const MAX_LINK_BYTES: u64 = 1_048_576;
const MAX_WIDE_UNITS: usize = 32_768;

struct RegistrySource {
    root: RegistryRoot,
    hive: &'static str,
    view: u32,
    access_view: RegistryView,
    source: LegacyTrayStartupSource,
}

struct RegistryObservationContext<'a> {
    expected: &'a Path,
    user_sid: &'a str,
    recorded_at: u64,
}

pub(super) fn observe() -> LegacyTrayStartupState {
    let Some(expected) = super::super::windows::expected_mactray_path() else {
        return LegacyTrayStartupState::Unknown {
            error: error(
                "legacy-tray-startup-program-files-unavailable",
                "the fixed Program Files MacTray path could not be determined",
                None,
            ),
        };
    };
    let user_sid = match current_user_sid() {
        Ok(sid) => sid,
        Err(error) => return LegacyTrayStartupState::Unknown { error },
    };
    let recorded_at = match recorded_at() {
        Ok(value) => value,
        Err(error) => return LegacyTrayStartupState::Unknown { error },
    };
    let registry_context = RegistryObservationContext {
        expected: &expected,
        user_sid: &user_sid,
        recorded_at,
    };
    let mut observations = Vec::new();
    for source in registry_sources_for_observation() {
        match observe_registry_source(&source, &registry_context) {
            Ok(found) => observations.extend(found),
            Err(error) => observations.push(LegacyTrayStartupObservation::Unknown(error)),
        }
    }
    match observe_startup_folder(
        LegacyTrayStartupSource::CurrentUserStartup,
        &expected,
        &user_sid,
        recorded_at,
    ) {
        Ok(found) => observations.extend(found),
        Err(error) => observations.push(LegacyTrayStartupObservation::Unknown(error)),
    }
    classify_startup_inventory(observations)
}

pub(super) fn observe_owned(
    scope: LegacyTrayStartupScope,
) -> Result<Vec<LegacyTrayStartupArtifact>, StructuredServiceError> {
    let expected = super::super::windows::expected_mactray_path().ok_or_else(|| {
        error(
            "legacy-tray-startup-program-files-unavailable",
            "the fixed Program Files MacTray path could not be determined",
            None,
        )
    })?;
    let user_sid = current_user_sid()?;
    let recorded_at = recorded_at()?;
    let registry_context = RegistryObservationContext {
        expected: &expected,
        user_sid: &user_sid,
        recorded_at,
    };
    let mut observations = Vec::new();
    for source in registry_sources(scope) {
        observations.extend(observe_registry_source(&source, &registry_context)?);
    }
    if let Some(source) = startup_folder_source(scope) {
        observations.extend(observe_startup_folder(
            source,
            &expected,
            &user_sid,
            recorded_at,
        )?);
    }

    let mut owned = Vec::new();
    for observation in observations {
        match observation {
            LegacyTrayStartupObservation::Owned(artifact) => owned.push(artifact),
            LegacyTrayStartupObservation::Untrusted(_) => {
                return Err(error(
                    "legacy-tray-startup-untrusted",
                    "an untrusted MacTray startup entry exists in the requested scope",
                    None,
                ));
            }
            LegacyTrayStartupObservation::Unknown(problem) => return Err(problem),
        }
    }
    Ok(owned)
}

// `HKEY_CURRENT_USER\...\CurrentVersion\Run` is not subject to WOW64 registry
// redirection (only `HKCU\Software\Classes` is), so its 32-bit and 64-bit views
// alias to one physical key. Probing both would observe the same value twice and
// then break exact removal — the second delete finds the value already gone.
// `HKEY_LOCAL_MACHINE\...\Run` IS redirected, so its two views are distinct
// physical keys and both must be probed.
fn current_user_run_source() -> RegistrySource {
    RegistrySource {
        root: RegistryRoot::CurrentUser,
        hive: "HKCU",
        view: 64,
        access_view: RegistryView::Native64,
        source: LegacyTrayStartupSource::CurrentUserRun64,
    }
}

fn local_machine_run_sources() -> [RegistrySource; 2] {
    [
        RegistrySource {
            root: RegistryRoot::LocalMachine,
            hive: "HKLM",
            view: 32,
            access_view: RegistryView::Native32,
            source: LegacyTrayStartupSource::LocalMachineRun32,
        },
        RegistrySource {
            root: RegistryRoot::LocalMachine,
            hive: "HKLM",
            view: 64,
            access_view: RegistryView::Native64,
            source: LegacyTrayStartupSource::LocalMachineRun64,
        },
    ]
}

fn registry_sources_for_observation() -> Vec<RegistrySource> {
    let mut sources = vec![current_user_run_source()];
    sources.extend(local_machine_run_sources());
    sources
}

fn registry_sources(scope: LegacyTrayStartupScope) -> Vec<RegistrySource> {
    match scope {
        LegacyTrayStartupScope::CurrentUser => vec![current_user_run_source()],
        LegacyTrayStartupScope::LocalMachine => local_machine_run_sources().into_iter().collect(),
    }
}

fn startup_folder_source(scope: LegacyTrayStartupScope) -> Option<LegacyTrayStartupSource> {
    match scope {
        LegacyTrayStartupScope::CurrentUser => Some(LegacyTrayStartupSource::CurrentUserStartup),
        LegacyTrayStartupScope::LocalMachine => None,
    }
}

pub(super) fn read_artifact_bytes(
    artifact: &LegacyTrayStartupArtifact,
) -> Result<Option<Vec<u8>>, StructuredServiceError> {
    validate_artifact(artifact)?;
    match &artifact.locator {
        LegacyTrayStartupLocator::Registry {
            value_name,
            value_type,
            ..
        } => {
            let source = registry_source_for_artifact(artifact)?;
            let Some(key) = open_registry_key(&source, false)? else {
                return Ok(None);
            };
            match read_registry_value(&key, value_name)? {
                None => Ok(None),
                Some(current) if current.kind == *value_type => Ok(Some(current.bytes)),
                Some(_) => Err(error(
                    "legacy-tray-startup-registry-type-changed",
                    "the receipt-named Run value type no longer matches the receipt",
                    None,
                )),
            }
        }
        LegacyTrayStartupLocator::File { startup_file_path } => {
            read_link_if_present(artifact, startup_file_path)
        }
    }
}

pub(super) fn remove_artifact_exact(
    artifact: &LegacyTrayStartupArtifact,
) -> Result<(), StructuredServiceError> {
    validate_artifact(artifact)?;
    match &artifact.locator {
        LegacyTrayStartupLocator::Registry {
            value_name,
            value_type,
            ..
        } => {
            let source = registry_source_for_artifact(artifact)?;
            let key = open_registry_key(&source, true)?.ok_or_else(|| {
                error(
                    "legacy-tray-startup-changed",
                    "the fixed Run key disappeared before removal",
                    None,
                )
            })?;
            let current = read_registry_value(&key, value_name)?;
            if current
                .as_ref()
                .map(|value| (value.kind, value.bytes.as_slice()))
                != Some((*value_type, artifact.raw_bytes.as_slice()))
            {
                return Err(error(
                    "legacy-tray-startup-changed",
                    "the Run value changed before exact removal",
                    None,
                ));
            }
            match key.delete_value(value_name) {
                Ok(DeleteValueOutcome::Deleted) => Ok(()),
                Ok(DeleteValueOutcome::Absent) => Err(error(
                    "legacy-tray-startup-registry-delete-failed",
                    "the verified Run value could not be removed",
                    None,
                )),
                Err(io) => Err(io_error(
                    "legacy-tray-startup-registry-delete-failed",
                    "the verified Run value could not be removed",
                    io,
                )),
            }
        }
        LegacyTrayStartupLocator::File { startup_file_path } => {
            let current = read_link_if_present(artifact, startup_file_path)?;
            if current.as_deref() != Some(artifact.raw_bytes.as_slice()) {
                return Err(error(
                    "legacy-tray-startup-changed",
                    "the Startup shortcut changed before exact removal",
                    None,
                ));
            }
            std::fs::remove_file(startup_file_path).map_err(|io| {
                error(
                    "legacy-tray-startup-link-delete-failed",
                    &format!("the verified Startup shortcut could not be removed: {io}"),
                    io.raw_os_error().map(|value| value as u32),
                )
            })
        }
    }
}

pub(super) fn restore_artifact_if_absent(
    artifact: &LegacyTrayStartupArtifact,
) -> Result<(), StructuredServiceError> {
    validate_artifact(artifact)?;
    match &artifact.locator {
        LegacyTrayStartupLocator::Registry {
            value_name,
            value_type,
            ..
        } => {
            let source = registry_source_for_artifact(artifact)?;
            let key = open_registry_key(&source, true)?.ok_or_else(|| {
                error(
                    "legacy-tray-startup-registry-key-missing",
                    "the fixed Run key is absent and will not be created during restore",
                    None,
                )
            })?;
            if read_registry_value(&key, value_name)?.is_some() {
                return Err(error(
                    "legacy-tray-startup-changed",
                    "the Run value is no longer absent before restore",
                    None,
                ));
            }
            key.set_raw(value_name, *value_type, &artifact.raw_bytes)
                .map_err(|io| {
                    io_error(
                        "legacy-tray-startup-registry-restore-failed",
                        "the original Run value bytes could not be restored",
                        io,
                    )
                })?;
            let restored = read_registry_value(&key, artifact.entry.display_name.as_str())?;
            if restored
                .as_ref()
                .map(|value| (value.kind, value.bytes.as_slice()))
                != Some((*value_type, artifact.raw_bytes.as_slice()))
            {
                return Err(error(
                    "legacy-tray-startup-registry-restore-unverified",
                    "the restored Run value does not match the receipt",
                    None,
                ));
            }
            Ok(())
        }
        LegacyTrayStartupLocator::File { startup_file_path } => {
            if read_link_if_present(artifact, startup_file_path)?.is_some() {
                return Err(error(
                    "legacy-tray-startup-changed",
                    "the Startup shortcut is no longer absent before restore",
                    None,
                ));
            }
            restore_link_atomically(startup_file_path, &artifact.raw_bytes)?;
            let restored = read_link_if_present(artifact, startup_file_path)?;
            if restored.as_deref() != Some(artifact.raw_bytes.as_slice()) {
                return Err(error(
                    "legacy-tray-startup-link-restore-unverified",
                    "the restored Startup shortcut does not match the receipt",
                    None,
                ));
            }
            Ok(())
        }
    }
}

fn validate_artifact(artifact: &LegacyTrayStartupArtifact) -> Result<(), StructuredServiceError> {
    let expected = super::super::windows::expected_mactray_path().ok_or_else(|| {
        error(
            "legacy-tray-startup-program-files-unavailable",
            "the fixed Program Files MacTray path could not be determined",
            None,
        )
    })?;
    if artifact.raw_bytes.is_empty()
        || artifact.raw_bytes.len() > MAX_LINK_BYTES as usize
        || !same_windows_path(&artifact.normalized_target_path, &expected)
        || !same_windows_path(&artifact.entry.target_path, &expected)
    {
        return Err(error(
            "legacy-tray-startup-receipt-untrusted",
            "the startup receipt artifact is not an exact bounded MacTray target",
            None,
        ));
    }
    if startup_source_requires_current_user_sid(artifact.entry.source_kind)
        && artifact.user_sid != current_user_sid()?
    {
        return Err(error(
            "legacy-tray-startup-receipt-user-mismatch",
            "the startup receipt does not belong to the current user",
            None,
        ));
    }
    match &artifact.locator {
        LegacyTrayStartupLocator::Registry {
            subkey,
            value_name,
            value_type,
            ..
        } => {
            if subkey != RUN_SUBKEY
                || value_name != &artifact.entry.display_name
                || (*value_type != REG_SZ && *value_type != REG_EXPAND_SZ)
                || artifact.raw_bytes.len() > MAX_REGISTRY_VALUE_BYTES
            {
                return Err(error(
                    "legacy-tray-startup-receipt-locator-untrusted",
                    "the startup receipt does not name a supported fixed Run value",
                    None,
                ));
            }
            registry_source_for_artifact(artifact)?;
            let decoded = decode_registry_string(&artifact.raw_bytes)?;
            let command = if *value_type == REG_EXPAND_SZ {
                expand_environment(&decoded)?
            } else {
                decoded
            };
            if classify_startup_command(&command, &expected)
                != StartupTargetClassification::Owned(expected.clone())
            {
                return Err(error(
                    "legacy-tray-startup-receipt-untrusted",
                    "the receipt Run bytes do not encode the fixed MacTray command",
                    None,
                ));
            }
        }
        LegacyTrayStartupLocator::File { startup_file_path } => {
            validate_link_locator(artifact, startup_file_path)?;
        }
    }
    Ok(())
}

fn registry_source_for_artifact(
    artifact: &LegacyTrayStartupArtifact,
) -> Result<RegistrySource, StructuredServiceError> {
    let source = match artifact.entry.source_kind {
        LegacyTrayStartupSource::CurrentUserRun32 => RegistrySource {
            root: RegistryRoot::CurrentUser,
            hive: "HKCU",
            view: 32,
            access_view: RegistryView::Native32,
            source: LegacyTrayStartupSource::CurrentUserRun32,
        },
        LegacyTrayStartupSource::CurrentUserRun64 => RegistrySource {
            root: RegistryRoot::CurrentUser,
            hive: "HKCU",
            view: 64,
            access_view: RegistryView::Native64,
            source: LegacyTrayStartupSource::CurrentUserRun64,
        },
        LegacyTrayStartupSource::LocalMachineRun32 => RegistrySource {
            root: RegistryRoot::LocalMachine,
            hive: "HKLM",
            view: 32,
            access_view: RegistryView::Native32,
            source: LegacyTrayStartupSource::LocalMachineRun32,
        },
        LegacyTrayStartupSource::LocalMachineRun64 => RegistrySource {
            root: RegistryRoot::LocalMachine,
            hive: "HKLM",
            view: 64,
            access_view: RegistryView::Native64,
            source: LegacyTrayStartupSource::LocalMachineRun64,
        },
        LegacyTrayStartupSource::CurrentUserStartup => {
            return Err(error(
                "legacy-tray-startup-receipt-locator-untrusted",
                "a Startup folder source cannot name a registry value",
                None,
            ));
        }
    };
    let LegacyTrayStartupLocator::Registry {
        hive, view, subkey, ..
    } = &artifact.locator
    else {
        return Err(error(
            "legacy-tray-startup-receipt-locator-untrusted",
            "a Run source must use a registry locator",
            None,
        ));
    };
    if hive != source.hive || *view != source.view || subkey != RUN_SUBKEY {
        return Err(error(
            "legacy-tray-startup-receipt-locator-untrusted",
            "the Run locator does not match its fixed hive, view, and subkey",
            None,
        ));
    }
    Ok(source)
}

fn open_registry_key(
    source: &RegistrySource,
    writable: bool,
) -> Result<Option<RegistryKey>, StructuredServiceError> {
    let opened = if writable {
        RegistryKey::open_writable(source.root, RUN_SUBKEY, source.access_view)
    } else {
        RegistryKey::open(source.root, RUN_SUBKEY, source.access_view)
    };
    opened.map_err(|io| {
        io_error(
            "legacy-tray-startup-registry-inaccessible",
            &format!(
                "{} {}-bit Run key cannot be opened with the required fixed access",
                source.hive, source.view
            ),
            io,
        )
    })
}

fn read_registry_value(
    key: &RegistryKey,
    value_name: &str,
) -> Result<Option<mactype_service_platform::RawRegistryValue>, StructuredServiceError> {
    key.read_raw(value_name, MAX_REGISTRY_VALUE_BYTES)
        .map_err(|io| {
            if registry_bound_error(&io) {
                error(
                    "legacy-tray-startup-registry-value-oversized",
                    "the Run value exceeds the bounded startup receipt",
                    io.raw_os_error()
                        .and_then(|value| u32::try_from(value).ok()),
                )
            } else {
                io_error(
                    "legacy-tray-startup-registry-read-failed",
                    "the receipt-named Run value cannot be read exactly",
                    io,
                )
            }
        })
}

fn validate_link_locator(
    artifact: &LegacyTrayStartupArtifact,
    path: &Path,
) -> Result<PathBuf, StructuredServiceError> {
    match artifact.entry.source_kind {
        LegacyTrayStartupSource::CurrentUserStartup => {}
        LegacyTrayStartupSource::CurrentUserRun32
        | LegacyTrayStartupSource::CurrentUserRun64
        | LegacyTrayStartupSource::LocalMachineRun32
        | LegacyTrayStartupSource::LocalMachineRun64 => {
            return Err(error(
                "legacy-tray-startup-receipt-locator-untrusted",
                "a Run source cannot name a Startup shortcut",
                None,
            ));
        }
    }
    let folder = known_folder()?;
    require_regular_directory(&folder)?;
    if !matches!(path.parent(), Some(parent) if same_windows_path(parent, &folder))
        || !matches!(
            path.extension().and_then(OsStr::to_str),
            Some(extension) if extension.eq_ignore_ascii_case("lnk")
        )
        || path
            .file_stem()
            .map(|stem| stem.to_string_lossy())
            .as_deref()
            != Some(artifact.entry.display_name.as_str())
    {
        return Err(error(
            "legacy-tray-startup-receipt-locator-untrusted",
            "the Startup shortcut locator is not a direct .lnk child of its fixed folder",
            None,
        ));
    }
    Ok(folder)
}

fn read_link_if_present(
    artifact: &LegacyTrayStartupArtifact,
    path: &Path,
) -> Result<Option<Vec<u8>>, StructuredServiceError> {
    let folder = validate_link_locator(artifact, path)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => {
            require_regular_link_under(&folder, path)?;
            read_bounded_link(path).map(Some)
        }
        Err(io) if io.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(io) => Err(error(
            "legacy-tray-startup-link-inaccessible",
            &format!("the receipt-named Startup shortcut cannot be inspected: {io}"),
            io.raw_os_error().map(|value| value as u32),
        )),
    }
}

fn restore_link_atomically(
    destination: &Path,
    raw_bytes: &[u8],
) -> Result<(), StructuredServiceError> {
    let parent = destination.parent().ok_or_else(|| {
        error(
            "legacy-tray-startup-receipt-locator-untrusted",
            "the Startup shortcut has no fixed parent folder",
            None,
        )
    })?;
    let process_id = std::process::id();
    let mut temporary = None;
    for attempt in 0..32_u32 {
        let candidate = parent.join(format!(
            ".mactype-control-center-restore-{process_id}-{attempt}.tmp"
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                if let Err(io) = file.write_all(raw_bytes).and_then(|_| file.sync_all()) {
                    drop(file);
                    let _ = std::fs::remove_file(&candidate);
                    return Err(error(
                        "legacy-tray-startup-link-restore-failed",
                        &format!("the Startup shortcut restore staging failed: {io}"),
                        io.raw_os_error().map(|value| value as u32),
                    ));
                }
                drop(file);
                temporary = Some(candidate);
                break;
            }
            Err(io) if io.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(io) => {
                return Err(error(
                    "legacy-tray-startup-link-restore-failed",
                    &format!("the Startup shortcut restore cannot be staged: {io}"),
                    io.raw_os_error().map(|value| value as u32),
                ));
            }
        }
    }
    let temporary = temporary.ok_or_else(|| {
        error(
            "legacy-tray-startup-link-restore-failed",
            "a unique bounded Startup shortcut restore path could not be allocated",
            None,
        )
    })?;
    if let Err(io) = std::fs::rename(&temporary, destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error(
            "legacy-tray-startup-link-restore-failed",
            &format!("the original Startup shortcut bytes could not be restored: {io}"),
            io.raw_os_error().map(|value| value as u32),
        ));
    }
    Ok(())
}

fn observe_registry_source(
    source: &RegistrySource,
    context: &RegistryObservationContext<'_>,
) -> Result<Vec<LegacyTrayStartupObservation>, StructuredServiceError> {
    let Some(key) = open_registry_key(source, false)? else {
        return Ok(Vec::new());
    };
    let values = key
        .values_raw(MAX_REGISTRY_NAME_UNITS, MAX_REGISTRY_VALUE_BYTES)
        .map_err(|io| {
            if registry_bound_error(&io) {
                error(
                    "legacy-tray-startup-registry-value-oversized",
                    "a Run value exceeds the bounded startup inventory",
                    io.raw_os_error()
                        .and_then(|value| u32::try_from(value).ok()),
                )
            } else if io.kind() == std::io::ErrorKind::InvalidData
                && io.to_string() == "registry value name is not valid UTF-16"
            {
                error(
                    "legacy-tray-startup-registry-name-invalid",
                    "a Run value name is not valid UTF-16",
                    None,
                )
            } else {
                io_error(
                    "legacy-tray-startup-registry-enumeration-failed",
                    "a Run value could not be read",
                    io,
                )
            }
        })?;
    let mut result = Vec::new();
    for value in values {
        if let Some(observation) =
            classify_registry_value(source, value.name, value.kind, value.bytes, context)?
        {
            result.push(observation);
        }
    }
    Ok(result)
}

fn classify_registry_value(
    source: &RegistrySource,
    display_name: String,
    value_type: u32,
    raw_bytes: Vec<u8>,
    context: &RegistryObservationContext<'_>,
) -> Result<Option<LegacyTrayStartupObservation>, StructuredServiceError> {
    if value_type != REG_SZ && value_type != REG_EXPAND_SZ {
        return if suspicious_tray_name(&display_name) {
            Err(error(
                "legacy-tray-startup-registry-type-untrusted",
                "a MacTray-named Run value is not a supported string type",
                None,
            ))
        } else {
            Ok(None)
        };
    }
    let decoded = decode_registry_string(&raw_bytes)?;
    let command = if value_type == REG_EXPAND_SZ {
        expand_environment(&decoded)?
    } else {
        decoded
    };
    let candidate = is_legacy_tray_startup_candidate(&display_name, &command);
    if !candidate {
        return Ok(None);
    }
    let target = startup_target_hint(&command).unwrap_or_else(|| PathBuf::from(&command));
    let entry = LegacyTrayStartupEntry {
        source_kind: source.source,
        display_name: display_name.clone(),
        target_path: target,
    };
    match classify_startup_command(&command, context.expected) {
        StartupTargetClassification::Owned(normalized_target_path) => Ok(Some(
            LegacyTrayStartupObservation::Owned(LegacyTrayStartupArtifact {
                entry,
                locator: LegacyTrayStartupLocator::Registry {
                    hive: source.hive.to_owned(),
                    view: source.view,
                    subkey: RUN_SUBKEY.to_owned(),
                    value_name: display_name,
                    value_type,
                },
                raw_bytes,
                normalized_target_path,
                user_sid: context.user_sid.to_owned(),
                recorded_at: context.recorded_at,
            }),
        )),
        StartupTargetClassification::Untrusted => {
            Ok(Some(LegacyTrayStartupObservation::Untrusted(entry)))
        }
    }
}

fn observe_startup_folder(
    source: LegacyTrayStartupSource,
    expected: &Path,
    user_sid: &str,
    recorded_at: u64,
) -> Result<Vec<LegacyTrayStartupObservation>, StructuredServiceError> {
    let folder = known_folder()?;
    if !folder.exists() {
        return Ok(Vec::new());
    }
    require_regular_directory(&folder)?;
    let entries = std::fs::read_dir(&folder).map_err(|io| {
        error(
            "legacy-tray-startup-folder-inaccessible",
            &format!("the fixed Startup folder cannot be read: {io}"),
            io.raw_os_error().map(|value| value as u32),
        )
    })?;
    let _apartment =
        ComApartment::initialize_or_borrow(ComThreading::Apartment).map_err(|result| {
            error(
                "legacy-tray-startup-com-unavailable",
                &format!("COM initialization failed with HRESULT {result:#x}"),
                None,
            )
        })?;
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|io| {
            error(
                "legacy-tray-startup-folder-enumeration-failed",
                &format!("a Startup folder entry cannot be read: {io}"),
                io.raw_os_error().map(|value| value as u32),
            )
        })?;
        let path = entry.path();
        if !matches!(
            path.extension().and_then(OsStr::to_str),
            Some(extension) if extension.eq_ignore_ascii_case("lnk")
        ) {
            continue;
        }
        let display_name = path
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Err(problem) = require_regular_link_under(&folder, &path) {
            if suspicious_tray_name(&display_name) {
                result.push(LegacyTrayStartupObservation::Unknown(problem));
            }
            continue;
        }
        let Some(shortcut) = read_shortcut(&path) else {
            if suspicious_tray_name(&display_name) {
                result.push(LegacyTrayStartupObservation::Unknown(error(
                    "legacy-tray-startup-link-resolution-failed",
                    "the Startup shortcut cannot be read",
                    None,
                )));
            }
            continue;
        };
        let target = PathBuf::from(shortcut.path);
        let arguments = shortcut.arguments;
        let is_mactray_target = target
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("MacTray.exe"));
        if !is_mactray_target && !suspicious_tray_name(&display_name) {
            continue;
        }
        let startup_entry = LegacyTrayStartupEntry {
            source_kind: source,
            display_name,
            target_path: target.clone(),
        };
        if same_windows_path(&target, expected) && arguments.is_empty() {
            let raw_bytes = read_bounded_link(&path)?;
            result.push(LegacyTrayStartupObservation::Owned(
                LegacyTrayStartupArtifact {
                    entry: startup_entry,
                    locator: LegacyTrayStartupLocator::File {
                        startup_file_path: path,
                    },
                    raw_bytes,
                    normalized_target_path: expected.to_path_buf(),
                    user_sid: user_sid.to_owned(),
                    recorded_at,
                },
            ));
        } else {
            result.push(LegacyTrayStartupObservation::Untrusted(startup_entry));
        }
    }
    Ok(result)
}

fn decode_registry_string(raw: &[u8]) -> Result<String, StructuredServiceError> {
    if raw.is_empty() || raw.len() % 2 != 0 || raw.len() > MAX_REGISTRY_VALUE_BYTES {
        return Err(error(
            "legacy-tray-startup-registry-string-invalid",
            "a Run string has an invalid bounded UTF-16 byte sequence",
            None,
        ));
    }
    let mut units = raw
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    while units.last() == Some(&0) {
        units.pop();
    }
    if units.is_empty() || units.contains(&0) {
        return Err(error(
            "legacy-tray-startup-registry-string-invalid",
            "a Run string is empty or contains an embedded terminator",
            None,
        ));
    }
    String::from_utf16(&units).map_err(|_| {
        error(
            "legacy-tray-startup-registry-string-invalid",
            "a Run string is not valid UTF-16",
            None,
        )
    })
}

fn expand_environment(value: &str) -> Result<String, StructuredServiceError> {
    expand_environment_strings(value, MAX_WIDE_UNITS).map_err(|io| {
        io_error(
            "legacy-tray-startup-environment-expansion-failed",
            "a Run environment string cannot be expanded within the bound",
            io,
        )
    })
}

fn current_user_sid() -> Result<String, StructuredServiceError> {
    current_user_sid_string().map_err(|io| {
        io_error(
            "legacy-tray-startup-user-sid-unavailable",
            "the current user SID cannot be read",
            io,
        )
    })
}

fn known_folder() -> Result<PathBuf, StructuredServiceError> {
    known_folder_path(KnownFolder::Startup).map_err(|io| {
        io_error(
            "legacy-tray-startup-known-folder-unavailable",
            "a fixed Startup folder is unavailable",
            io,
        )
    })
}

fn require_regular_directory(path: &Path) -> Result<(), StructuredServiceError> {
    let attributes = startup_file_attributes(path)?;
    if attributes & FILE_ATTRIBUTE_DIRECTORY == 0 || attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        Err(error(
            "legacy-tray-startup-folder-untrusted",
            "the fixed Startup folder is not a regular non-reparse directory",
            None,
        ))
    } else {
        Ok(())
    }
}

fn require_regular_link_under(folder: &Path, path: &Path) -> Result<(), StructuredServiceError> {
    if !matches!(path.parent(), Some(parent) if same_windows_path(parent, folder)) {
        return Err(error(
            "legacy-tray-startup-link-untrusted",
            "a Startup shortcut is outside the fixed folder",
            None,
        ));
    }
    let attributes = startup_file_attributes(path)?;
    if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 || attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(error(
            "legacy-tray-startup-link-untrusted",
            "a Startup shortcut is not a regular non-reparse file",
            None,
        ));
    }
    Ok(())
}

fn startup_file_attributes(path: &Path) -> Result<u32, StructuredServiceError> {
    file_attributes(path).map_err(|io| {
        io_error(
            "legacy-tray-startup-path-inaccessible",
            "a fixed Startup path cannot be inspected",
            io,
        )
    })
}

fn read_bounded_link(path: &Path) -> Result<Vec<u8>, StructuredServiceError> {
    let mut file = File::open(path).map_err(|io| {
        error(
            "legacy-tray-startup-link-inaccessible",
            &format!("the Startup shortcut cannot be opened: {io}"),
            io.raw_os_error().map(|value| value as u32),
        )
    })?;
    let before = file.metadata().map_err(|io| {
        error(
            "legacy-tray-startup-link-inaccessible",
            &format!("the Startup shortcut metadata cannot be read: {io}"),
            io.raw_os_error().map(|value| value as u32),
        )
    })?;
    if !before.is_file() || before.len() > MAX_LINK_BYTES {
        return Err(error(
            "legacy-tray-startup-link-oversized",
            "the Startup shortcut is not a bounded regular file",
            None,
        ));
    }
    let mut raw = Vec::with_capacity(before.len() as usize);
    Read::by_ref(&mut file)
        .take(MAX_LINK_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|io| {
            error(
                "legacy-tray-startup-link-read-failed",
                &format!("the Startup shortcut bytes cannot be read: {io}"),
                io.raw_os_error().map(|value| value as u32),
            )
        })?;
    if raw.len() as u64 != before.len() || raw.len() as u64 > MAX_LINK_BYTES {
        return Err(error(
            "legacy-tray-startup-link-changed",
            "the Startup shortcut changed during bounded capture",
            None,
        ));
    }
    Ok(raw)
}

fn startup_target_hint(command: &str) -> Option<PathBuf> {
    if let Some(target) = command.strip_prefix('"') {
        let end = target.find('"')?;
        return Some(PathBuf::from(&target[..end]));
    }
    let lower = command.to_ascii_lowercase();
    let end = lower.find("mactray.exe")? + "mactray.exe".len();
    Some(PathBuf::from(command[..end].trim()))
}

fn suspicious_tray_name(value: &str) -> bool {
    is_legacy_tray_startup_candidate(value, "")
}

fn recorded_at() -> Result<u64, StructuredServiceError> {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
        error(
            "legacy-tray-startup-clock-invalid",
            "the system clock predates the Unix epoch",
            None,
        )
    })?;
    u64::try_from(duration.as_millis()).map_err(|_| {
        error(
            "legacy-tray-startup-clock-invalid",
            "the startup observation timestamp exceeds its receipt range",
            None,
        )
    })
}

fn registry_bound_error(io: &std::io::Error) -> bool {
    io.kind() == std::io::ErrorKind::InvalidData
        && io.to_string() == "registry value exceeds the bound"
}

fn io_error(code: &str, message: &str, io: std::io::Error) -> StructuredServiceError {
    error(
        code,
        message,
        io.raw_os_error()
            .and_then(|value| u32::try_from(value).ok()),
    )
}

fn error(code: &str, message: &str, win32_error: Option<u32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error,
    }
}
