use std::env;

const REQUESTED_VERSION_ENV: &str = "MACTYPE_SERVICE_RUNTIME_VERSION";
const COMPILED_VERSION_ENV: &str = "MACTYPE_COMPILED_SERVICE_RUNTIME_VERSION";
const MAX_VERSION_LENGTH: usize = 64;

fn main() {
    println!("cargo:rerun-if-env-changed=MACTYPE_SERVICE_RUNTIME_VERSION");

    let package_version = env::var("CARGO_PKG_VERSION")
        .expect("Cargo must provide CARGO_PKG_VERSION to the service host build");
    let service_version = env::var(REQUESTED_VERSION_ENV).unwrap_or(package_version);
    assert_valid_service_version(&service_version);

    println!("cargo:rustc-env={COMPILED_VERSION_ENV}={service_version}");
}

fn assert_valid_service_version(version: &str) {
    assert!(
        !version.is_empty()
            && version.len() <= MAX_VERSION_LENGTH
            && version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
            && !matches!(version, "." | ".."),
        "{REQUESTED_VERSION_ENV} must be a bounded ASCII SemVer-compatible generation"
    );
}
