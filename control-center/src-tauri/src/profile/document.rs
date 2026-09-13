use super::{
    codec::{decode, detect_line_ending, original_legacy_lines, split_lines},
    identity::identify_profile,
    AdvancedProfile, IndividualSetting, IniNode, LineEnding, ProfileDocument, ProfileLists,
    ProfileSnapshot, ShadowSetting,
};
use crate::{
    bounded_io::read_bounded_file,
    generated_settings::{SettingDefinition, SETTINGS},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, path::PathBuf};

pub(super) fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn parse_nodes(text: &str) -> Vec<IniNode> {
    let mut section = String::new();
    split_lines(text)
        .into_iter()
        .map(|line| {
            let body = line.trim_end_matches(['\r', '\n']);
            let trimmed = body.trim();
            if trimmed.is_empty() {
                return IniNode::Blank {
                    raw: line.to_owned(),
                };
            }
            if trimmed.starts_with(';') || trimmed.starts_with('#') {
                return IniNode::Comment {
                    raw: line.to_owned(),
                };
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = trimmed[1..trimmed.len() - 1].trim().to_owned();
                return IniNode::Section {
                    name: section.clone(),
                    raw: line.to_owned(),
                };
            }
            let Some(separator) = body.find('=') else {
                return IniNode::Unknown {
                    section: section.clone(),
                    raw: line.to_owned(),
                };
            };
            let key = body[..separator].trim();
            if key.is_empty() {
                return IniNode::Unknown {
                    section: section.clone(),
                    raw: line.to_owned(),
                };
            }
            let after_separator = &body[separator + 1..];
            let leading = after_separator.len() - after_separator.trim_start().len();
            let value_with_suffix = &after_separator[leading..];
            let trailing = value_with_suffix.len() - value_with_suffix.trim_end().len();
            let value_end = value_with_suffix.len() - trailing;
            let newline = &line[body.len()..];
            IniNode::KeyValue {
                section: section.clone(),
                key: key.to_owned(),
                value: value_with_suffix[..value_end].to_owned(),
                prefix: body[..separator].to_owned(),
                separator: format!("={}", &after_separator[..leading]),
                suffix: format!("{}{}", &value_with_suffix[value_end..], newline),
                raw: line.to_owned(),
            }
        })
        .collect()
}

pub(super) fn validate_entry(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value
            .chars()
            .any(|character| matches!(character, '\r' | '\n'))
        || value.starts_with(['[', ';', '#'])
    {
        return Err(format!(
            "{label} is empty or contains an unsupported character"
        ));
    }
    Ok(())
}

impl ProfileDocument {
    pub(super) fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let bytes = read_bounded_file(
            &path,
            mactype_service_contract::MAX_PROFILE_BYTES,
            "profile",
        )?;
        Self::from_bytes(path, &bytes)
    }

    pub(super) fn from_bytes(path: PathBuf, bytes: &[u8]) -> Result<Self, String> {
        let original_hash = hash(bytes);
        let (text, encoding, bom) = decode(bytes)?;
        let mut document = Self {
            path,
            encoding,
            bom,
            line_ending: detect_line_ending(&text),
            nodes: parse_nodes(&text),
            original_hash,
            original_legacy_lines: original_legacy_lines(bytes, &text, encoding),
            saved_values: Default::default(),
            dirty_keys: Default::default(),
            undo_history: Default::default(),
            redo_history: Default::default(),
        };
        document.saved_values = document.setting_values();
        Ok(document)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn setting_values(&self) -> BTreeMap<String, f64> {
        SETTINGS
            .iter()
            .map(|setting| {
                let value = self
                    .value(setting)
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(setting.default);
                (setting.id.to_owned(), value)
            })
            .collect()
    }

    pub(super) fn snapshot(&self) -> ProfileSnapshot {
        let identity = identify_profile(&self.path);
        let values = self.setting_values();

        ProfileSnapshot {
            path: self.path.to_string_lossy().into_owned(),
            display_path: identity.display_path,
            location: identity.location,
            can_save: identity.can_save,
            encoding: self.encoding,
            bom: self.bom,
            line_ending: self.line_ending,
            original_hash: self
                .original_hash
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            values,
            saved_values: self.saved_values.clone(),
            dirty_keys: self.dirty_keys.iter().cloned().collect(),
            can_undo: !self.undo_history.is_empty(),
            can_redo: !self.redo_history.is_empty(),
            individuals: self.individuals(),
            lists: ProfileLists {
                exclude_fonts: self.list_entries("Exclude"),
                include_fonts: self.list_entries("Include"),
                exclude_modules: self.list_entries("ExcludeModule"),
                include_modules: self.list_entries("IncludeModule"),
                unload_dlls: self.list_entries("UnloadDLL"),
                exclude_substitution_modules: self.list_entries("ExcludeSub"),
            },
            advanced: self.advanced(),
        }
    }

    pub(super) fn is_dirty(&self) -> bool {
        !self.dirty_keys.is_empty()
    }

    pub(super) fn raw_value(&self, section_name: &str, key_name: &str) -> Option<&str> {
        let sections: &[&str] = if section_name.eq_ignore_ascii_case("General") {
            &["FreeType", "General"]
        } else {
            std::slice::from_ref(&section_name)
        };
        sections.iter().find_map(|target| {
            self.nodes.iter().rev().find_map(|node| match node {
                IniNode::KeyValue {
                    section,
                    key,
                    value,
                    ..
                } if section.eq_ignore_ascii_case(target) && key.eq_ignore_ascii_case(key_name) => {
                    Some(value.as_str())
                }
                _ => None,
            })
        })
    }

    fn parse_vector(&self, section: &str, key: &str, length: usize) -> Option<Vec<i32>> {
        let values = self
            .raw_value(section, key)?
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|part| !part.is_empty())
            .map(|part| part.trim().parse::<i32>().ok())
            .collect::<Option<Vec<_>>>()?;
        (values.len() == length).then_some(values)
    }

    fn shadow(&self) -> Option<ShadowSetting> {
        let raw = self.raw_value("General", "Shadow")?;
        let parts = raw
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 3 {
            return None;
        }
        let dark_alpha = parts[2].parse().ok()?;
        Some(ShadowSetting {
            offset_x: parts[0].parse().ok()?,
            offset_y: parts[1].parse().ok()?,
            dark_alpha,
            dark_color: parts
                .get(3)
                .and_then(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
            light_alpha: parts
                .get(4)
                .and_then(|value| value.parse().ok())
                .unwrap_or(dark_alpha),
            light_color: parts
                .get(5)
                .and_then(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0),
        })
    }

    fn advanced(&self) -> AdvancedProfile {
        AdvancedProfile {
            shadow: self.shadow(),
            lcd_filter_weight: self.parse_vector("General", "LcdFilterWeight", 5),
            pixel_layout: self.parse_vector("General", "PixelLayout", 6),
            font_substitutes: self.list_entries("FontSubstitutes"),
        }
    }

    fn individuals(&self) -> Vec<IndividualSetting> {
        self.nodes
            .iter()
            .filter_map(|node| match node {
                IniNode::KeyValue {
                    section,
                    key,
                    value,
                    ..
                } if section.eq_ignore_ascii_case("Individual") => Some(IndividualSetting {
                    font_face: key.to_owned(),
                    values: value
                        .split(',')
                        .take(6)
                        .map(|part| {
                            let trimmed = part.trim();
                            (!trimmed.is_empty())
                                .then(|| trimmed.parse::<i32>().ok())
                                .flatten()
                        })
                        .chain(std::iter::repeat(None))
                        .take(6)
                        .collect(),
                }),
                _ => None,
            })
            .collect()
    }

    fn list_entries(&self, target: &str) -> Vec<String> {
        self.nodes
            .iter()
            .filter_map(|node| match node {
                IniNode::Unknown { section, raw } if section.eq_ignore_ascii_case(target) => {
                    let value = raw.trim();
                    (!value.is_empty()).then(|| value.to_owned())
                }
                IniNode::KeyValue {
                    section,
                    key,
                    value,
                    ..
                } if section.eq_ignore_ascii_case(target) => Some(format!("{key}={value}")),
                _ => None,
            })
            .collect()
    }

    pub(super) fn ending(&self) -> &'static str {
        match self.line_ending {
            LineEnding::CrLf => "\r\n",
            LineEnding::Lf => "\n",
            LineEnding::Cr => "\r",
        }
    }

    fn value(&self, setting: &SettingDefinition) -> Option<&str> {
        self.raw_value(setting.section, setting.key)
    }
}
