use std::fs;

pub(crate) fn active_version(path: &std::path::Path) -> String {
    serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()["version"]
        .as_str()
        .unwrap()
        .to_owned()
}
