use super::helper::PreviewManager;
use crate::installation_root;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct Finding {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) ok: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallationStatus {
    pub(crate) state: String,
    pub(crate) root: Option<String>,
    pub(crate) core_version: Option<String>,
    pub(crate) findings: Vec<Finding>,
}

pub(super) fn format_core_version(version: u32) -> String {
    let raw = version.to_string();
    if raw.len() == 8 && raw.starts_with("20") {
        let month = raw[4..6].parse::<u8>().unwrap_or_default();
        let day = raw[6..8].parse::<u8>().unwrap_or_default();
        if (1..=12).contains(&month) && (1..=31).contains(&day) {
            return format!("{}.{}.{}", &raw[..4], month, day);
        }
    }
    raw
}

pub(super) fn collect_installation(
    manager: &mut PreviewManager,
    reconnect: bool,
) -> InstallationStatus {
    let root = installation_root();
    let finding = |label: &str, file: &str| {
        let ok = root.as_ref().is_some_and(|path| path.join(file).is_file());
        Finding {
            label: label.to_owned(),
            value: file.to_owned(),
            ok,
        }
    };

    let mut findings = vec![
        finding("32비트 코어", "MacType.dll"),
        finding("64비트 코어", "MacType64.dll"),
        finding("수동 실행 로더", "MacLoader.exe"),
    ];
    let core_version = root.as_deref().and_then(|path| {
        let result = if reconnect {
            manager.reconnect(path)
        } else {
            manager.probe_core_version(path)
        };
        match result {
            Ok(version) => {
                findings.push(Finding {
                    label: "preview".to_owned(),
                    value: "connected".to_owned(),
                    ok: true,
                });
                Some(format_core_version(version))
            }
            Err(error) => {
                findings.push(Finding {
                    label: "preview".to_owned(),
                    value: error,
                    ok: false,
                });
                None
            }
        }
    });
    let ready = !findings.is_empty() && findings.iter().all(|item| item.ok);

    InstallationStatus {
        state: if root.is_none() {
            "not-found"
        } else if ready {
            "ready"
        } else {
            "incomplete"
        }
        .to_owned(),
        root: root.map(|path| path.to_string_lossy().into_owned()),
        core_version,
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_build_date_is_displayed_as_a_version() {
        assert_eq!(format_core_version(20220712), "2022.7.12");
        assert_eq!(format_core_version(42), "42");
    }
}
