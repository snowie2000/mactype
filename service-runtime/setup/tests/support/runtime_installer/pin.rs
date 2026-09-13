use std::collections::BTreeMap;
use std::fs;

use mactype_service_contract::{
    sha256_digest, MachinePaths, MigrationPinnedRuntime, MigrationRuntimePin,
    IMMUTABLE_RUNTIME_FILES,
};

pub(crate) fn pin_runtime_generation(paths: &MachinePaths, nonce: &str, version: &str) {
    let runtime_root = paths.runtime_versions().join(version);
    let files = IMMUTABLE_RUNTIME_FILES
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                sha256_digest(&fs::read(runtime_root.join(name)).unwrap()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let generated_profile = runtime_root
        .join("MacType.ini")
        .is_file()
        .then(|| sha256_digest(&fs::read(runtime_root.join("MacType.ini")).unwrap()));
    let pin = MigrationRuntimePin::new(
        nonce.to_owned(),
        vec![MigrationPinnedRuntime::new(version.to_owned(), files, generated_profile).unwrap()],
    )
    .unwrap();
    let root = paths.service_root().join("migration-runtime-pins");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(format!("{nonce}.json")),
        serde_json::to_vec(&pin).unwrap(),
    )
    .unwrap();
}
