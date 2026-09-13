mod validated_input;

use self::validated_input::{
    normalize_list_entries, render_advanced_profile, render_setting_value, validate_individuals,
};
use super::{AdvancedProfile, IndividualSetting, IniNode, ProfileDocument, ProfileRevision};
use crate::generated_settings::SETTINGS;

impl ProfileDocument {
    fn revision(&self) -> ProfileRevision {
        ProfileRevision {
            nodes: self.nodes.clone(),
            dirty_keys: self.dirty_keys.clone(),
        }
    }

    fn restore_revision(&mut self, revision: ProfileRevision) {
        self.nodes = revision.nodes;
        self.dirty_keys = revision.dirty_keys;
    }

    fn record_edit(
        &mut self,
        edit: impl FnOnce(&mut ProfileDocument) -> Result<(), String>,
    ) -> Result<(), String> {
        const HISTORY_LIMIT: usize = 100;
        let previous = self.revision();
        edit(self)?;
        if self.revision() == previous {
            return Ok(());
        }
        if self.undo_history.len() == HISTORY_LIMIT {
            self.undo_history.pop_front();
        }
        self.undo_history.push_back(previous);
        self.redo_history.clear();
        Ok(())
    }

    pub(super) fn update_value(&mut self, setting_id: &str, value: f64) -> Result<(), String> {
        self.record_edit(|document| document.set_value(setting_id, value))
    }

    pub(super) fn update_individuals(
        &mut self,
        entries: Vec<IndividualSetting>,
    ) -> Result<(), String> {
        self.record_edit(|document| document.set_individuals(entries))
    }

    pub(super) fn update_list(&mut self, kind: &str, entries: Vec<String>) -> Result<(), String> {
        self.record_edit(|document| document.set_list(kind, entries))
    }

    pub(super) fn update_advanced(&mut self, advanced: AdvancedProfile) -> Result<(), String> {
        self.record_edit(|document| document.set_advanced(advanced))
    }

    /// Writes the factory value (the official Default.ini profile, falling back
    /// to the engine default where that profile ships no key) for every scalar
    /// setting, as one undoable revision. Lists, per-font entries, and the
    /// structured advanced settings are intentionally left untouched.
    pub(super) fn reset_settings_to_defaults(&mut self) -> Result<(), String> {
        self.record_edit(|document| {
            for setting in SETTINGS {
                let rendered = render_setting_value(setting.id, setting.factory)?;
                let unchanged = {
                    let current = document.raw_value(rendered.section, rendered.key);
                    match current {
                        Some(value) => value == rendered.value,
                        None => setting.factory == setting.default,
                    }
                };
                if unchanged {
                    continue;
                }
                document.set_raw_value(
                    rendered.section,
                    rendered.key,
                    Some(rendered.value),
                    setting.id,
                );
            }
            Ok(())
        })
    }

    pub(super) fn undo(&mut self) -> bool {
        let Some(previous) = self.undo_history.pop_back() else {
            return false;
        };
        self.redo_history.push_back(self.revision());
        self.restore_revision(previous);
        true
    }

    pub(super) fn redo(&mut self) -> bool {
        let Some(next) = self.redo_history.pop_back() else {
            return false;
        };
        self.undo_history.push_back(self.revision());
        self.restore_revision(next);
        true
    }

    pub(super) fn set_raw_value(
        &mut self,
        section_name: &str,
        key_name: &str,
        value: Option<String>,
        dirty_key: &str,
    ) {
        let sections: &[&str] = if section_name.eq_ignore_ascii_case("General") {
            &["FreeType", "General"]
        } else {
            std::slice::from_ref(&section_name)
        };
        let Some(rendered) = value else {
            self.nodes.retain(|node| {
                !matches!(
                    node,
                    IniNode::KeyValue { section, key, .. }
                        if sections
                            .iter()
                            .any(|target| section.eq_ignore_ascii_case(target))
                            && key.eq_ignore_ascii_case(key_name)
                )
            });
            self.dirty_keys.insert(dirty_key.to_owned());
            return;
        };
        let target_section = sections
            .iter()
            .find(|target| {
                self.nodes.iter().any(|node| {
                    matches!(
                        node,
                        IniNode::KeyValue { section, key, .. }
                            if section.eq_ignore_ascii_case(target)
                                && key.eq_ignore_ascii_case(key_name)
                    )
                })
            })
            .copied()
            .unwrap_or(section_name);
        for node in self.nodes.iter_mut().rev() {
            if let IniNode::KeyValue {
                section,
                key,
                value,
                prefix,
                separator,
                suffix,
                raw,
            } = node
            {
                if section.eq_ignore_ascii_case(target_section)
                    && key.eq_ignore_ascii_case(key_name)
                {
                    *value = rendered.clone();
                    *raw = format!("{prefix}{separator}{rendered}{suffix}");
                    self.dirty_keys.insert(dirty_key.to_owned());
                    return;
                }
            }
        }
        let ending = self.ending();
        if !self.nodes.iter().any(|node| {
            matches!(
                node,
                IniNode::Section { name, .. } if name.eq_ignore_ascii_case(target_section)
            )
        }) {
            self.nodes.push(IniNode::Section {
                name: target_section.to_owned(),
                raw: format!("[{target_section}]{ending}"),
            });
        }
        let insert_at = self
            .nodes
            .iter()
            .rposition(|node| match node {
                IniNode::Section { name, .. } => name.eq_ignore_ascii_case(target_section),
                IniNode::KeyValue { section, .. } | IniNode::Unknown { section, .. } => {
                    section.eq_ignore_ascii_case(target_section)
                }
                _ => false,
            })
            .map_or(self.nodes.len(), |index| index + 1);
        self.nodes.insert(
            insert_at,
            IniNode::KeyValue {
                section: target_section.to_owned(),
                key: key_name.to_owned(),
                value: rendered.clone(),
                prefix: key_name.to_owned(),
                separator: "=".to_owned(),
                suffix: ending.to_owned(),
                raw: format!("{key_name}={rendered}{ending}"),
            },
        );
        self.dirty_keys.insert(dirty_key.to_owned());
    }

    pub(super) fn set_value(&mut self, setting_id: &str, value: f64) -> Result<(), String> {
        let rendered = render_setting_value(setting_id, value)?;
        self.set_raw_value(
            rendered.section,
            rendered.key,
            Some(rendered.value),
            setting_id,
        );
        Ok(())
    }

    pub(super) fn set_advanced(&mut self, advanced: AdvancedProfile) -> Result<(), String> {
        let rendered = render_advanced_profile(advanced)?;

        // Composite edits must be fully rendered before touching nodes or a late error corrupts undo state.
        self.set_raw_value("General", "Shadow", rendered.shadow, "advanced:shadow");
        self.set_raw_value(
            "General",
            "LcdFilterWeight",
            rendered.lcd_filter_weight,
            "advanced:lcdFilterWeight",
        );
        self.set_raw_value(
            "General",
            "PixelLayout",
            rendered.pixel_layout,
            "advanced:pixelLayout",
        );
        self.replace_list_section("FontSubstitutes", rendered.font_substitutes);
        Ok(())
    }

    fn section_range(&mut self, section: &str) -> std::ops::Range<usize> {
        let start = self
            .nodes
            .iter()
            .position(|node| {
                matches!(
                    node,
                    IniNode::Section { name, .. } if name.eq_ignore_ascii_case(section)
                )
            })
            .unwrap_or_else(|| {
                let ending = self.ending();
                self.nodes.push(IniNode::Section {
                    name: section.to_owned(),
                    raw: format!("[{section}]{ending}"),
                });
                self.nodes.len() - 1
            });
        let end = self.nodes[start + 1..]
            .iter()
            .position(|node| matches!(node, IniNode::Section { .. }))
            .map_or(self.nodes.len(), |offset| start + 1 + offset);
        start + 1..end
    }

    pub(super) fn set_individuals(
        &mut self,
        entries: Vec<IndividualSetting>,
    ) -> Result<(), String> {
        validate_individuals(&entries)?;
        let range = self.section_range("Individual");
        let insert_at = range.start;
        let mut replacement = self
            .nodes
            .drain(range)
            .filter(|node| {
                !matches!(
                    node,
                    IniNode::KeyValue { section, .. }
                        if section.eq_ignore_ascii_case("Individual")
                )
            })
            .collect::<Vec<_>>();
        let ending = self.ending();
        replacement.extend(entries.into_iter().map(|entry| {
            let value = entry
                .values
                .iter()
                .map(|value| value.map_or_else(String::new, |value| value.to_string()))
                .collect::<Vec<_>>()
                .join(",");
            IniNode::KeyValue {
                section: "Individual".to_owned(),
                key: entry.font_face.clone(),
                value: value.clone(),
                prefix: entry.font_face.clone(),
                separator: "=".to_owned(),
                suffix: ending.to_owned(),
                raw: format!("{}={value}{ending}", entry.font_face),
            }
        }));
        self.nodes.splice(insert_at..insert_at, replacement);
        self.dirty_keys.insert("section:Individual".to_owned());
        Ok(())
    }

    pub(super) fn set_list(&mut self, kind: &str, entries: Vec<String>) -> Result<(), String> {
        let section = match kind {
            "excludeFonts" => "Exclude",
            "includeFonts" => "Include",
            "excludeModules" => "ExcludeModule",
            "includeModules" => "IncludeModule",
            "unloadDlls" => "UnloadDLL",
            "excludeSubstitutionModules" => "ExcludeSub",
            "fontSubstitutes" => "FontSubstitutes",
            _ => return Err(format!("unknown profile list: {kind}")),
        };
        let normalized = normalize_list_entries(entries)?;
        self.replace_list_section(section, normalized);
        Ok(())
    }

    fn replace_list_section(&mut self, section: &str, normalized: Vec<String>) {
        let range = self.section_range(section);
        let insert_at = range.start;
        let mut replacement = self
            .nodes
            .drain(range)
            .filter(|node| match node {
                IniNode::Unknown {
                    section: item_section,
                    ..
                }
                | IniNode::KeyValue {
                    section: item_section,
                    ..
                } => !item_section.eq_ignore_ascii_case(section),
                _ => true,
            })
            .collect::<Vec<_>>();
        let ending = self.ending();
        replacement.extend(normalized.into_iter().map(|entry| IniNode::Unknown {
            section: section.to_owned(),
            raw: format!("{entry}{ending}"),
        }));
        self.nodes.splice(insert_at..insert_at, replacement);
        self.dirty_keys.insert(format!("section:{section}"));
    }
}
