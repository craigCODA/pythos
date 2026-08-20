use pythos_shared::{pyth_native_binding, user_program_manifest::digest64};

#[test]
fn native_binding_round_trips_graph_and_elf_identity() {
    let graph = b"PYTHTIG1graph";
    let elf = b"\x7fELFnative";
    let mut bytes = [0u8; 128];

    let len = pyth_native_binding::encode_pyth_native_binding(
        &mut bytes,
        b"hello.tig",
        b"hello.elf",
        0x5059_5448_4752_0001,
        graph,
        elf,
    )
    .unwrap();
    let binding = pyth_native_binding::validate_pyth_native_binding(&bytes[..len]).unwrap();

    assert_eq!(binding.graph_name(), b"hello.tig");
    assert_eq!(binding.elf_name(), b"hello.elf");
    assert_eq!(binding.principal_id(), 0x5059_5448_4752_0001);
    assert_eq!(binding.graph_digest(), digest64(graph));
    assert_eq!(binding.elf_digest(), digest64(elf));
}

#[test]
fn native_binding_rejects_mismatched_digest_and_name_lengths() {
    let mut bytes = [0u8; 128];
    let len = pyth_native_binding::encode_pyth_native_binding(
        &mut bytes,
        b"hello.tig",
        b"hello.elf",
        0x5059_5448_4752_0001,
        b"graph",
        b"elf",
    )
    .unwrap();

    let mut bad_digest = bytes;
    bad_digest[24] ^= 0x55;
    assert_eq!(
        pyth_native_binding::validate_pyth_native_binding_for_artifacts(
            &bad_digest[..len],
            b"hello.tig",
            b"hello.elf",
            0x5059_5448_4752_0001,
            b"graph",
            b"elf",
        ),
        Err(pyth_native_binding::PythNativeBindingError::BadGraphDigest)
    );

    let mut truncated = bytes;
    truncated[12..14].copy_from_slice(&32u16.to_le_bytes());
    assert_eq!(
        pyth_native_binding::validate_pyth_native_binding(&truncated[..len]),
        Err(pyth_native_binding::PythNativeBindingError::TooShort)
    );
}
