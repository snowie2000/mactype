#![forbid(unsafe_code)]

use std::io::Read;

use mactype_service_contract::{BrokerCommand, MAX_PROFILE_BYTES};
use mactype_service_setup::{parse_setup_command, SetupCommand};

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
            std::process::exit(1);
        }
    }
}
