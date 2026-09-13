use super::super::{document::validate_entry, AdvancedProfile, IndividualSetting, ShadowSetting};
use crate::generated_settings::{SettingDefinition, SettingValueType, SETTINGS};
use std::collections::BTreeSet;

pub(super) struct RenderedSettingValue {
    pub(super) section: &'static str,
    pub(super) key: &'static str,
    pub(super) value: String,
}

pub(super) struct RenderedAdvancedProfile {
    pub(super) shadow: Option<String>,
    pub(super) lcd_filter_weight: Option<String>,
    pub(super) pixel_layout: Option<String>,
    pub(super) font_substitutes: Vec<String>,
}

pub(super) fn render_setting_value(
    setting_id: &str,
    value: f64,
) -> Result<RenderedSettingValue, String> {
    let setting = schema(setting_id)?;
    if !value.is_finite() || value < setting.min || value > setting.max {
        return Err(format!(
            "{setting_id} must be between {} and {}",
            setting.min, setting.max
        ));
    }
    let value = if matches!(setting.value_type, SettingValueType::Integer) {
        if value.fract() != 0.0 {
            return Err(format!("{setting_id} requires an integer"));
        }
        format!("{}", value as i64)
    } else {
        render_float(value)
    };
    Ok(RenderedSettingValue {
        section: setting.section,
        key: setting.key,
        value,
    })
}

pub(super) fn render_advanced_profile(
    profile: AdvancedProfile,
) -> Result<RenderedAdvancedProfile, String> {
    let AdvancedProfile {
        shadow,
        lcd_filter_weight,
        pixel_layout,
        font_substitutes,
    } = profile;
    let shadow = shadow.map(render_shadow).transpose()?;
    let lcd_filter_weight = lcd_filter_weight
        .as_deref()
        .map(|values| render_vector(values, 5, 0, 255, "LCD filter weight", ","))
        .transpose()?;
    let pixel_layout = pixel_layout
        .as_deref()
        .map(|values| render_vector(values, 6, -128, 127, "pixel layout", ","))
        .transpose()?;
    let font_substitutes = normalize_list_entries(font_substitutes)?;
    validate_font_substitutions(&font_substitutes)?;
    Ok(RenderedAdvancedProfile {
        shadow,
        lcd_filter_weight,
        pixel_layout,
        font_substitutes,
    })
}

pub(super) fn normalize_list_entries(entries: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        let entry = entry.trim().to_owned();
        validate_entry(&entry, "list entry")?;
        if seen.insert(entry.to_lowercase()) {
            normalized.push(entry);
        }
    }
    Ok(normalized)
}

pub(super) fn validate_individuals(entries: &[IndividualSetting]) -> Result<(), String> {
    let bounds = [(0, 2), (-1, 6), (-64, 64), (-32, 32), (-32, 32), (0, 1)];
    let mut seen = BTreeSet::new();
    for entry in entries {
        validate_entry(&entry.font_face, "font face")?;
        if entry.font_face.contains('=') {
            return Err("font face cannot contain '='".to_owned());
        }
        if !seen.insert(entry.font_face.to_lowercase()) {
            return Err(format!("duplicate font face: {}", entry.font_face));
        }
        if entry.values.len() != 6 {
            return Err("individual font settings require exactly six values".to_owned());
        }
        for (index, value) in entry.values.iter().enumerate() {
            if let Some(value) = value {
                let (minimum, maximum) = bounds[index];
                if *value < minimum || *value > maximum {
                    return Err(format!(
                        "{} value {} must be between {minimum} and {maximum}",
                        entry.font_face,
                        index + 1
                    ));
                }
            }
        }
    }
    Ok(())
}

fn schema(setting_id: &str) -> Result<&'static SettingDefinition, String> {
    SETTINGS
        .iter()
        .find(|item| item.id == setting_id)
        .ok_or_else(|| format!("unknown setting id: {setting_id}"))
}

fn render_float(value: f64) -> String {
    let mut result = format!("{value:.6}");
    while result.contains('.') && result.ends_with('0') {
        result.pop();
    }
    if result.ends_with('.') {
        result.push('0');
    }
    result
}

fn render_vector(
    values: &[i32],
    length: usize,
    minimum: i32,
    maximum: i32,
    name: &str,
    separator: &str,
) -> Result<String, String> {
    if values.len() != length
        || values
            .iter()
            .any(|value| *value < minimum || *value > maximum)
    {
        return Err(format!(
            "{name} requires {length} values between {minimum} and {maximum}"
        ));
    }
    Ok(values
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(separator))
}

fn render_shadow(value: ShadowSetting) -> Result<String, String> {
    if !(0..=255).contains(&value.dark_alpha)
        || !(0..=255).contains(&value.light_alpha)
        || value.dark_color > 0xFFFFFF
        || value.light_color > 0xFFFFFF
    {
        return Err("shadow alpha or color is outside its supported range".to_owned());
    }
    Ok(format!(
        "{},{},{},{:06X},{},{:06X}",
        value.offset_x,
        value.offset_y,
        value.dark_alpha,
        value.dark_color,
        value.light_alpha,
        value.light_color
    ))
}

fn validate_font_substitutions(entries: &[String]) -> Result<(), String> {
    for mapping in entries {
        let Some((source, replacement)) = mapping.split_once('=') else {
            return Err("font substitutions must use Source font=Replacement font".to_owned());
        };
        if source.trim().is_empty() || replacement.trim().is_empty() {
            return Err("font substitutions require both source and replacement fonts".to_owned());
        }
    }
    Ok(())
}
