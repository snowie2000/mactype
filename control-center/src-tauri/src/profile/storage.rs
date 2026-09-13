use super::{
    codec::{encode, encode_preserving_legacy_lines, original_legacy_lines},
    document::{hash, validate_entry},
    IniNode, ProfileDocument,
};
use crate::bounded_io::read_bounded_file;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

impl ProfileDocument {
    pub(super) fn duplicate_in(&self, directory: &Path, name: &str) -> Result<Self, String> {
        let stem = name.trim().trim_end_matches(".ini");
        validate_entry(stem, "profile name")?;
        if stem
            .chars()
            .any(|character| "<>:\"/\\|?*".contains(character))
        {
            return Err("profile name contains a Windows-reserved character".to_owned());
        }
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let destination = directory.join(format!("{stem}.ini"));
        if destination.exists() {
            return Err("a profile with that name already exists".to_owned());
        }
        let bytes = self.encoded()?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())?;
        Self::open(destination)
    }

    pub(super) fn encoded(&self) -> Result<Vec<u8>, String> {
        let bytes = if let Some(original_lines) = &self.original_legacy_lines {
            encode_preserving_legacy_lines(
                self.nodes.iter().map(IniNode::raw),
                original_lines,
                self.encoding,
            )?
        } else {
            let text = self.nodes.iter().map(IniNode::raw).collect::<String>();
            encode(&text, self.encoding, self.bom)?
        };
        if bytes.len() > mactype_service_contract::MAX_PROFILE_BYTES {
            return Err(format!(
                "profile exceeds its {}-byte limit",
                mactype_service_contract::MAX_PROFILE_BYTES
            ));
        }
        Ok(bytes)
    }

    pub(super) fn export_to(&self, destination: &Path) -> Result<(), String> {
        let parent = destination
            .parent()
            .filter(|path| path.is_dir())
            .ok_or_else(|| "export destination directory does not exist".to_owned())?;
        if destination.file_name().is_none() || parent == destination {
            return Err("export destination must be an INI file".to_owned());
        }
        let bytes = self.encoded()?;
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(destination)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())
    }

    pub(super) fn save(&mut self) -> Result<(), String> {
        let disk = read_bounded_file(
            &self.path,
            mactype_service_contract::MAX_PROFILE_BYTES,
            "profile on disk",
        )?;
        if hash(&disk) != self.original_hash {
            return Err("profile changed on disk; reload before saving".to_owned());
        }
        let bytes = self.encoded()?;
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("profile.ini");
        let temporary = self
            .path
            .with_file_name(format!(".{file_name}.mactype-{}.tmp", std::process::id()));
        let backup = self.path.with_extension("ini.bak");
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        if let Err(error) = replace_file(&self.path, &temporary, &backup) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        self.original_hash = hash(&bytes);
        let text = self.nodes.iter().map(IniNode::raw).collect::<String>();
        self.original_legacy_lines = original_legacy_lines(&bytes, &text, self.encoding);
        self.saved_values = self.setting_values();
        self.dirty_keys.clear();
        self.undo_history.clear();
        self.redo_history.clear();
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn replace_file(
    destination: &Path,
    replacement: &Path,
    backup: &Path,
) -> Result<(), String> {
    mactype_service_platform::replace_file_preserving_attributes(
        destination,
        replacement,
        Some(backup),
    )
    .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub(super) fn replace_file(
    destination: &Path,
    replacement: &Path,
    backup: &Path,
) -> Result<(), String> {
    fs::rename(replacement, destination).map_err(|error| error.to_string())
}
