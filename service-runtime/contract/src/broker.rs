use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerCommand {
    Install,
    Upgrade,
    Repair,
    Remove,
    Start,
    Stop,
    PublishProfile,
    MigrateFromLegacy,
    Rollback,
    RestoreRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerCommandError;

impl fmt::Display for BrokerCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected one fixed broker verb without arguments")
    }
}

impl std::error::Error for BrokerCommandError {}

pub fn parse_broker_command<I, S>(arguments: I) -> Result<BrokerCommand, BrokerCommandError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut arguments = arguments.into_iter();
    let verb = arguments.next().ok_or(BrokerCommandError)?;
    if arguments.next().is_some() {
        return Err(BrokerCommandError);
    }

    match verb.as_ref() {
        "install" => Ok(BrokerCommand::Install),
        "upgrade" => Ok(BrokerCommand::Upgrade),
        "repair" => Ok(BrokerCommand::Repair),
        "remove" => Ok(BrokerCommand::Remove),
        "start" => Ok(BrokerCommand::Start),
        "stop" => Ok(BrokerCommand::Stop),
        "publish-profile" => Ok(BrokerCommand::PublishProfile),
        "migrate-from-legacy" => Ok(BrokerCommand::MigrateFromLegacy),
        "rollback" => Ok(BrokerCommand::Rollback),
        "restore-runtime" => Ok(BrokerCommand::RestoreRuntime),
        _ => Err(BrokerCommandError),
    }
}
