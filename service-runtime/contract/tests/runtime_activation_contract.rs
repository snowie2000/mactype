use mactype_service_contract::{
    parse_runtime_activation_receipt, ParsedRuntimeActivationReceipt, RuntimeActivationPhase,
    RuntimeActivationReceipt, RuntimeGenerationPointer, MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES,
    MAX_RUNTIME_POINTER_BYTES,
};

#[test]
fn current_receipt_round_trip_preserves_the_durable_phase_and_exact_pointers() {
    let previous = RuntimeGenerationPointer::new("0.1.0").unwrap();
    let activated = RuntimeGenerationPointer::new("0.2.0").unwrap();
    let committed = RuntimeActivationReceipt::candidate(Some(previous.clone()), activated.clone())
        .with_phase(RuntimeActivationPhase::Committed);
    let bytes = committed.to_bytes().unwrap();

    let parsed = parse_runtime_activation_receipt(&bytes).unwrap();

    assert_eq!(parsed, ParsedRuntimeActivationReceipt::Current(committed));
    assert_eq!(parsed.previous(), Some(&previous));
    assert_eq!(parsed.activated(), Some(&activated));
    assert_eq!(parsed.phase(), Some(RuntimeActivationPhase::Committed));
}

#[test]
fn legacy_receipts_remain_parseable_but_have_no_durable_commit_phase() {
    for bytes in [
        br#"{"schema":1,"previous":{"schema":1,"version":"0.1.0"}}"#.as_slice(),
        br#"{"schema":2,"previous":null,"activated":{"schema":1,"version":"0.2.0"}}"#.as_slice(),
    ] {
        let parsed = parse_runtime_activation_receipt(bytes).unwrap();
        assert_eq!(parsed.phase(), None);
    }
}

#[test]
fn activation_contract_rejects_unsafe_versions_unknown_fields_and_oversized_input() {
    assert!(RuntimeGenerationPointer::new("..").is_err());
    assert!(parse_runtime_activation_receipt(
        br#"{"schema":3,"phase":"committed","previous":null,"activated":{"schema":1,"version":"0.2.0"},"foreign":true}"#
    )
    .is_err());
    assert!(parse_runtime_activation_receipt(&vec![
        b'x';
        MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES as usize
            + 1
    ])
    .is_err());
    assert!(
        RuntimeGenerationPointer::parse(&vec![b'x'; MAX_RUNTIME_POINTER_BYTES as usize + 1])
            .is_err()
    );
}
