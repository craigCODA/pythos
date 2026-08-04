use pythos_shared::pyth_tig::{
    opcode::{RESOURCE_OBJECT, RIGHTS_READ},
    types::PythType,
    verify::verify_bytes,
};

use crate::encode;

pub(crate) fn run_mutation_suite(valid_fixture: &[u8]) -> Result<(), String> {
    expect_ok("input fixture", valid_fixture)?;

    let mut cases: Vec<(&'static str, Vec<u8>, &'static str)> = Vec::new();

    let mut bad_magic = valid_fixture.to_vec();
    bad_magic[0] = b'X';
    cases.push(("bad magic", bad_magic, "Decode(BadMagic)"));

    let mut unknown_major = valid_fixture.to_vec();
    encode::write_u16(&mut unknown_major, encode::MAJOR_OFFSET, 2);
    encode::refresh_checksum(&mut unknown_major);
    cases.push(("unknown major", unknown_major, "Decode(UnsupportedMajor)"));

    let mut section_overlap = valid_fixture.to_vec();
    let blocks_offset = read_u32(&section_overlap, encode::HEADER_BLOCKS_OFFSET_OFFSET);
    encode::write_u32(
        &mut section_overlap,
        encode::HEADER_NODES_OFFSET_OFFSET,
        blocks_offset,
    );
    encode::refresh_checksum(&mut section_overlap);
    cases.push(("section overlap", section_overlap, "Decode(SectionOverlap)"));

    let mut checksum_mismatch = valid_fixture.to_vec();
    checksum_mismatch[encode::CHECKSUM_OFFSET] ^= 0x01;
    cases.push((
        "checksum mismatch",
        checksum_mismatch,
        "Decode(ChecksumMismatch)",
    ));

    let missing_terminator = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::import_capability(),
            encode::const_utf8(5),
            encode::system_log(0),
            encode::const_utf8(5),
        ],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        pythos_shared::pyth_tig::opcode::RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "missing terminator",
        missing_terminator,
        "MissingTerminator { block: 0 }",
    ));

    let bad_control_target = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::import_capability(),
            encode::const_utf8(5),
            encode::system_log(0),
            encode::jump(9),
        ],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        pythos_shared::pyth_tig::opcode::RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "bad control target",
        bad_control_target,
        "InvalidControlTarget { block: 0, target: 9 }",
    ));

    let type_mismatch = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::import_capability(),
            encode::const_bool(),
            encode::add_u64(2, 2),
            encode::ret(),
        ],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        pythos_shared::pyth_tig::opcode::RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "type mismatch",
        type_mismatch,
        "TypeMismatch { node: 3, input: 0, expected: U64, actual: Bool }",
    ));

    let effect_fork = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::import_capability(),
            encode::const_utf8(5),
            encode::system_log(0),
            encode::system_log(0),
            encode::ret(),
        ],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        pythos_shared::pyth_tig::opcode::RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push(("effect fork", effect_fork, "EffectFork { producer: 0 }"));

    let capability_constant = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::const_capability(),
            encode::const_utf8(5),
            encode::system_log(0),
            encode::ret(),
        ],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        pythos_shared::pyth_tig::opcode::RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "capability constant",
        capability_constant,
        "CapabilityOriginInvalid { node: 1 }",
    ));

    let insufficient_rights = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::import_capability(),
            encode::const_utf8(5),
            encode::system_log(0),
            encode::ret(),
        ],
        RIGHTS_READ,
        RESOURCE_OBJECT,
        PythType::Capability.code(),
    );
    cases.push((
        "insufficient rights",
        insufficient_rights,
        "ImportRightsInsufficient { node: 3, import_slot: 0 }",
    ));

    let mut node_budget_exceeded = valid_fixture.to_vec();
    encode::write_u32(
        &mut node_budget_exceeded,
        encode::HEADER_NODE_COUNT_OFFSET,
        1025,
    );
    encode::refresh_checksum(&mut node_budget_exceeded);
    cases.push((
        "node budget exceeded",
        node_budget_exceeded,
        "Decode(CountLimit)",
    ));

    for (name, bytes, expected) in cases {
        let error = match verify_bytes(&bytes) {
            Ok(_) => return Err(format!("{name} unexpectedly verified")),
            Err(error) => error,
        };
        let actual = format!("{error:?}");
        if actual != expected {
            return Err(format!("{name} produced {actual}, expected {expected}"));
        }
        println!("PYTH_TIG_MUTATION_OK {name}: {actual}");
    }

    println!("PYTH_TIG_MUTATION_SUITE_OK");
    Ok(())
}

fn expect_ok(name: &str, bytes: &[u8]) -> Result<(), String> {
    verify_bytes(bytes)
        .map(|_| ())
        .map_err(|error| format!("{name} did not verify before mutation: {error:?}"))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
