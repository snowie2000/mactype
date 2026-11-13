use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
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

const BROKER_COMMANDS: &[(BrokerCommand, &str)] = &[
    (BrokerCommand::Install, "install"),
    (BrokerCommand::Upgrade, "upgrade"),
    (BrokerCommand::Repair, "repair"),
    (BrokerCommand::Remove, "remove"),
    (BrokerCommand::Start, "start"),
    (BrokerCommand::Stop, "stop"),
    (BrokerCommand::PublishProfile, "publish-profile"),
    (BrokerCommand::MigrateFromLegacy, "migrate-from-legacy"),
    (BrokerCommand::Rollback, "rollback"),
    (BrokerCommand::RestoreRuntime, "restore-runtime"),
];

impl BrokerCommand {
    pub const fn verb(self) -> &'static str {
        BROKER_COMMANDS[self as usize].1
    }

    pub fn parse_verb(verb: &str) -> Option<Self> {
        BROKER_COMMANDS
            .iter()
            .find_map(|(command, fixed_verb)| (*fixed_verb == verb).then_some(*command))
    }
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

    BrokerCommand::parse_verb(verb.as_ref()).ok_or(BrokerCommandError)
}
