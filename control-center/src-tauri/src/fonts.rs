#[cfg(windows)]
use std::collections::BTreeSet;

#[cfg(windows)]
fn filter_face_name(face: String) -> Option<String> {
    let name = face.trim().to_owned();
    (!name.is_empty() && !name.starts_with('@')).then_some(name)
}

#[cfg(windows)]
pub fn installed_families() -> Result<Vec<String>, String> {
    let families = mactype_service_platform::installed_font_families()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(filter_face_name)
        .collect::<BTreeSet<_>>();
    if families.is_empty() {
        Err("Windows did not report any installed font families".to_owned())
    } else {
        Ok(families.into_iter().collect())
    }
}

#[cfg(not(windows))]
pub fn installed_families() -> Result<Vec<String>, String> {
    Err("installed font discovery is supported only on Windows".to_owned())
}

#[tauri::command]
pub(crate) fn installed_font_families() -> Result<Vec<String>, String> {
    installed_families()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn vertical_and_empty_faces_are_not_exposed() {
        assert_eq!(filter_face_name("@Vertical".to_owned()), None);
        assert_eq!(filter_face_name(String::new()), None);
    }

    #[test]
    fn installed_families_are_detected_and_deduplicated() {
        let families = installed_families().unwrap();
        assert!(!families.is_empty());
        assert!(families
            .iter()
            .all(|name| !name.is_empty() && !name.starts_with('@')));
        assert!(families.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
