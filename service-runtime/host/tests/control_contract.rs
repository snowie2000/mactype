use mactype_service_host::{
    validate_host_arguments, ServiceControl, ACCEPTED_CONTROL_MASK, SERVICE_STOP_WAIT_HINT_MS,
};

const _: () = assert!(SERVICE_STOP_WAIT_HINT_MS >= 25_000);

#[test]
fn service_control_accepts_stop_shutdown_and_session_change_only() {
    assert_eq!(ServiceControl::from_raw(1, 0), Some(ServiceControl::Stop));
    assert_eq!(
        ServiceControl::from_raw(5, 0),
        Some(ServiceControl::Shutdown)
    );
    assert_eq!(
        ServiceControl::from_raw(14, 5),
        Some(ServiceControl::SessionChange {
            event_type: 5,
            session_id: 0,
        })
    );
    assert_eq!(
        ServiceControl::from_session_change(6, 7),
        ServiceControl::SessionChange {
            event_type: 6,
            session_id: 7,
        }
    );
    assert_eq!(ServiceControl::from_raw(2, 0), None);
    assert_eq!(ServiceControl::from_raw(99, 0), None);

    assert_eq!(
        ACCEPTED_CONTROL_MASK,
        0x0000_0001 | 0x0000_0004 | 0x0000_0080
    );
}

#[test]
fn service_host_rejects_arbitrary_command_line_inputs() {
    assert!(validate_host_arguments(Vec::<String>::new()).is_ok());
    assert!(validate_host_arguments(["--service"]).is_ok());
    assert!(validate_host_arguments(["--service", "OtherService"]).is_err());
    assert!(validate_host_arguments(["--dll", r"C:\Temp\arbitrary.dll"]).is_err());
}

#[test]
fn service_host_rejects_after_two_arguments_without_collecting_the_rest() {
    let arguments = ["--service", "OtherService"]
        .into_iter()
        .chain(std::iter::once_with(|| {
            panic!("argument validation consumed beyond its fixed grammar")
        }));

    assert!(validate_host_arguments(arguments).is_err());
}
