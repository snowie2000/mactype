pub(crate) const fn service_runtime_version() -> &'static str {
    env!("MACTYPE_COMPILED_SERVICE_RUNTIME_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_reports_the_requested_payload_generation() {
        let expected = std::env::var("MACTYPE_SERVICE_RUNTIME_VERSION")
            .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned());

        assert_eq!(service_runtime_version(), expected);
    }
}
