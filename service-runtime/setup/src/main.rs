#![forbid(unsafe_code)]

use std::io::Read;

use mactype_service_contract::{BrokerCommand, MAX_PROFILE_BYTES};
use mactype_service_setup::{SetupCommand, SetupError, parse_setup_command};

const SETUP_FAILURE_EXIT_CODE: i32 = 1;
const SETUP_ROLLBACK_FAILURE_EXIT_CODE: i32 = 3;

fn exit_code_for(error: &SetupError) -> i32 {
    match error {
        SetupError::RollbackFailed { .. } => SETUP_ROLLBACK_FAILURE_EXIT_CODE,
        SetupError::MachineOperation { source, .. } => exit_code_for(source),
        _ => SETUP_FAILURE_EXIT_CODE,
    }
}

fn main() {
    let command = match parse_setup_command(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let mut profile = Vec::new();
    let input = if command == SetupCommand::Broker(BrokerCommand::PublishProfile) {
        if let Err(error) = std::io::stdin()
            .take(MAX_PROFILE_BYTES as u64 + 1)
            .read_to_end(&mut profile)
        {
            eprintln!("could not read profile from stdin: {error}");
            std::process::exit(1);
        }
        Some(profile.as_slice())
    } else {
        None
    };

    match mactype_service_setup::run_setup_command(command, input) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(exit_code_for(&error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_failure_has_a_distinct_exit_code() {
        let rollback = SetupError::RollbackFailed {
            operation: "runtime activation failed (operation)".to_owned(),
            restoration: "pointer restoration failed: restoration".to_owned(),
        };
        assert_eq!(
            rollback.to_string(),
            "machine cleanup state is unknown: runtime activation failed (operation); pointer restoration failed: restoration"
        );
        assert_eq!(exit_code_for(&rollback), SETUP_ROLLBACK_FAILURE_EXIT_CODE);
        let wrapped = SetupError::MachineOperation {
            operation: "repair protected runtime transaction",
            path: std::path::PathBuf::from(r"C:\Service"),
            source: Box::new(rollback),
        };
        assert_eq!(exit_code_for(&wrapped), SETUP_ROLLBACK_FAILURE_EXIT_CODE);
        assert_eq!(exit_code_for(&SetupError::Runtime("failure".to_owned())), 1);
    }
}
