use mactype_service_contract::{parse_broker_command, BrokerCommand};

#[test]
fn broker_accepts_only_fixed_verbs_without_arguments() {
    let allowed = [
        ("install", BrokerCommand::Install),
        ("upgrade", BrokerCommand::Upgrade),
        ("repair", BrokerCommand::Repair),
        ("remove", BrokerCommand::Remove),
        ("start", BrokerCommand::Start),
        ("stop", BrokerCommand::Stop),
        ("publish-profile", BrokerCommand::PublishProfile),
        ("migrate-from-legacy", BrokerCommand::MigrateFromLegacy),
        ("rollback", BrokerCommand::Rollback),
        ("restore-runtime", BrokerCommand::RestoreRuntime),
    ];

    for (verb, expected) in allowed {
        assert_eq!(parse_broker_command([verb]).unwrap(), expected);
    }

    for rejected in [
        vec![],
        vec!["INSTALL"],
        vec!["health"],
        vec!["install", "AnotherService"],
        vec!["publish-profile", r"C:\Users\person\profile.ini"],
        vec!["start", "--service-name", "Other"],
    ] {
        assert!(parse_broker_command(rejected).is_err());
    }
}
