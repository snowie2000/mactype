#![forbid(unsafe_code)]

use mactype_service_contract::{
    event_log::{EventArea, EventLogWriter, EventSeverity, EventSource},
    BrokerCommand,
};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{SetupCommand, SetupError};

static WRITE_ERROR_REPORTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn command_verb(command: SetupCommand) -> &'static str {
    match command {
        SetupCommand::Broker(command) => broker_verb(command),
        SetupCommand::BootstrapInstall => "bootstrap-install",
        SetupCommand::UninstallOwned => "uninstall-owned",
    }
}

pub(crate) fn command_failed(command: SetupCommand, error: &SetupError, profile: Option<&[u8]>) {
    let result = writer().and_then(|writer| command_failed_with(&writer, command, error, profile));
    report_write_error(result);
}

fn command_failed_with(
    writer: &EventLogWriter,
    command: SetupCommand,
    error: &SetupError,
    profile: Option<&[u8]>,
) -> Result<(), String> {
    let text = error.to_string();
    let stage = text
        .split_once(':')
        .map_or(text.as_str(), |(stage, _)| stage);
    let profile = profile.map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    writer
        .record(
            EventSource::ServiceSetup,
            EventSeverity::Error,
            EventArea::Setup,
            "setup-command-failed",
            &[("command", command_verb(command)), ("stage", stage)],
            Some(&text),
            profile
                .as_deref()
                .into_iter()
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .map_err(|error| error.to_string())
}

pub(crate) fn rollback_completed(command: &str) {
    write(
        EventSeverity::Notice,
        "setup-rollback-completed",
        BTreeMap::from([("command".to_owned(), command.to_owned())]),
        None,
        &[],
    );
}

pub(crate) fn service_configuration_repaired(fields: &[&str]) {
    write(
        EventSeverity::Notice,
        "service-configuration-repaired",
        BTreeMap::from([("fields".to_owned(), fields.join(","))]),
        None,
        &[],
    );
}

fn write(
    severity: EventSeverity,
    code: &str,
    params: BTreeMap<String, String>,
    detail: Option<String>,
    redactions: &[&str],
) {
    let result = writer().and_then(|writer| {
        let params = params
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        writer
            .record(
                EventSource::ServiceSetup,
                severity,
                EventArea::Setup,
                code,
                &params,
                detail.as_deref(),
                redactions,
            )
            .map_err(|error| error.to_string())
    });
    report_write_error(result);
}

fn report_write_error(result: Result<(), String>) {
    if let Err(error) = result {
        if !WRITE_ERROR_REPORTED.swap(true, Ordering::AcqRel) {
            eprintln!(
                "recording the setup event log failed: {}",
                error.replace(['\r', '\n'], " ")
            );
        }
    }
}

fn writer() -> Result<EventLogWriter, String> {
    #[cfg(windows)]
    {
        crate::windows::event_log_path()
            .map(EventLogWriter::new)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("setup event logging requires Windows machine paths".to_owned())
    }
}

fn broker_verb(command: BrokerCommand) -> &'static str {
    match command {
        BrokerCommand::Install => "install",
        BrokerCommand::Upgrade => "upgrade",
        BrokerCommand::Repair => "repair",
        BrokerCommand::Remove => "remove",
        BrokerCommand::Start => "start",
        BrokerCommand::Stop => "stop",
        BrokerCommand::PublishProfile => "publish-profile",
        BrokerCommand::MigrateFromLegacy => "migrate-from-legacy",
        BrokerCommand::Rollback => "rollback",
        BrokerCommand::RestoreRuntime => "restore-runtime",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mactype_service_contract::event_log::read_events;

    #[test]
    fn setup_failure_keeps_stage_and_redacts_profile_bytes() {
        let root = std::env::temp_dir().join(format!("setup-event-log-{}", std::process::id()));
        let path = root.join("setup.log");
        let profile = b"[General]\r\nSecret=private";
        let error = SetupError::Runtime(format!(
            "publish profile: rejected {} 00112233445566778899aabbccddeeff",
            String::from_utf8_lossy(profile)
        ));
        command_failed_with(
            &EventLogWriter::new(path.clone()),
            SetupCommand::Broker(BrokerCommand::PublishProfile),
            &error,
            Some(profile),
        )
        .unwrap();
        let event = read_events(std::slice::from_ref(&path), 1).pop().unwrap();
        assert_eq!(event.params["command"], "publish-profile");
        assert_eq!(event.params["stage"], "machine runtime operation failed");
        assert!(event
            .detail
            .as_deref()
            .unwrap()
            .contains("[redacted-profile]"));
        let disk = std::fs::read_to_string(path).unwrap();
        assert!(!disk.contains("Secret=private"));
        assert!(!disk.contains("00112233445566778899aabbccddeeff"));
        let _ = std::fs::remove_dir_all(root);
    }
}
