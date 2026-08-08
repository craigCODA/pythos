use pythos_shared::pyth_tig::{
    opcode::{Opcode, RESOURCE_SYSTEM_LOG},
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

    let mut unknown_minor = valid_fixture.to_vec();
    encode::write_u16(&mut unknown_minor, encode::MINOR_OFFSET, 1);
    encode::refresh_checksum(&mut unknown_minor);
    cases.push(("unknown minor", unknown_minor, "Decode(UnsupportedMinor)"));

    let mut nonzero_reserved = valid_fixture.to_vec();
    encode::write_u32(&mut nonzero_reserved, encode::HEADER_RESERVED_OFFSET, 1);
    encode::refresh_checksum(&mut nonzero_reserved);
    cases.push((
        "nonzero reserved field",
        nonzero_reserved,
        "Decode(NonZeroReserved)",
    ));

    cases.push((
        "truncated header",
        valid_fixture[..encode::HEADER_SIZE - 1].to_vec(),
        "Decode(HeaderTooShort)",
    ));

    let mut count_multiplication_overflow = valid_fixture.to_vec();
    encode::write_u32(
        &mut count_multiplication_overflow,
        encode::HEADER_NODE_COUNT_OFFSET,
        u32::MAX,
    );
    encode::refresh_checksum(&mut count_multiplication_overflow);
    cases.push((
        "count multiplication overflow",
        count_multiplication_overflow,
        "Decode(CountLimit)",
    ));

    let mut offset_addition_overflow = valid_fixture.to_vec();
    encode::write_u32(
        &mut offset_addition_overflow,
        encode::HEADER_STRING_TABLE_OFFSET_OFFSET,
        u32::MAX,
    );
    encode::refresh_checksum(&mut offset_addition_overflow);
    cases.push((
        "offset addition overflow",
        offset_addition_overflow,
        "Decode(SectionOutOfBounds)",
    ));

    let mut section_overlap = valid_fixture.to_vec();
    let blocks_offset = read_u32(&section_overlap, encode::HEADER_BLOCKS_OFFSET_OFFSET);
    encode::write_u32(
        &mut section_overlap,
        encode::HEADER_NODES_OFFSET_OFFSET,
        blocks_offset,
    );
    encode::refresh_checksum(&mut section_overlap);
    cases.push(("section overlap", section_overlap, "Decode(SectionOverlap)"));

    let mut unaligned_section = valid_fixture.to_vec();
    let nodes_offset = read_u32(&unaligned_section, encode::HEADER_NODES_OFFSET_OFFSET);
    encode::write_u32(
        &mut unaligned_section,
        encode::HEADER_NODES_OFFSET_OFFSET,
        nodes_offset + 2,
    );
    encode::refresh_checksum(&mut unaligned_section);
    cases.push((
        "unaligned section",
        unaligned_section,
        "Decode(SectionUnaligned)",
    ));

    let nodes_offset = read_u32(valid_fixture, encode::HEADER_NODES_OFFSET_OFFSET) as usize;
    let mut unknown_type = valid_fixture.to_vec();
    encode::write_u16(&mut unknown_type, nodes_offset + 2, u16::MAX);
    encode::refresh_checksum(&mut unknown_type);
    cases.push(("unknown type", unknown_type, "UnknownType { code: 65535 }"));

    let mut unknown_opcode = valid_fixture.to_vec();
    encode::write_u16(&mut unknown_opcode, nodes_offset, u16::MAX);
    encode::refresh_checksum(&mut unknown_opcode);
    cases.push((
        "unknown opcode",
        unknown_opcode,
        "UnknownOpcode { code: 65535 }",
    ));

    let mut wrong_block_owner = valid_fixture.to_vec();
    encode::write_u16(&mut wrong_block_owner, nodes_offset + 6, 1);
    encode::refresh_checksum(&mut wrong_block_owner);
    cases.push((
        "node assigned to wrong block",
        wrong_block_owner,
        "InvalidBlockRange { block: 0 }",
    ));

    let mut multiple_terminators = valid_fixture.to_vec();
    encode::write_u16(
        &mut multiple_terminators,
        nodes_offset + 3 * encode::NODE_RECORD_SIZE,
        Opcode::Return.code(),
    );
    encode::refresh_checksum(&mut multiple_terminators);
    cases.push((
        "multiple terminators",
        multiple_terminators,
        "MultipleTerminators { block: 0 }",
    ));

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

    let wrong_block_argument_count = encode::build_log_package(
        &[encode::const_u64(), encode::jump_with_input(0, 0)],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "wrong block argument count",
        wrong_block_argument_count,
        "BlockArgumentCountMismatch { source: 0, target: 0 }",
    ));

    let use_before_dominance = encode::build_log_package(
        &[encode::add_u64(1, 1), encode::const_u64(), encode::ret()],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "use before dominance",
        use_before_dominance,
        "ValueNotAvailable { node: 0, input: 0 }",
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

    let mut result_type_mismatch = valid_fixture.to_vec();
    encode::write_u16(
        &mut result_type_mismatch,
        nodes_offset + 2 * encode::NODE_RECORD_SIZE + 2,
        PythType::Bool.code(),
    );
    encode::refresh_checksum(&mut result_type_mismatch);
    cases.push((
        "result type mismatch",
        result_type_mismatch,
        "ResultTypeForbidden { node: 2 }",
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

    let import_type_mismatch = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::import_capability(),
            encode::const_utf8(5),
            encode::system_log(0),
            encode::ret(),
        ],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Unit.code(),
    );
    cases.push((
        "import type mismatch",
        import_type_mismatch,
        "ImportTypeMismatch { import_slot: 0 }",
    ));

    let insufficient_rights = encode::build_log_package(
        &[
            encode::effect_start(),
            encode::import_capability(),
            encode::const_utf8(5),
            encode::system_log(0),
            encode::ret(),
        ],
        0,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "insufficient rights",
        insufficient_rights,
        "ImportRightsInsufficient { node: 3, import_slot: 0 }",
    ));

    let mut string_range_violation = valid_fixture.to_vec();
    encode::write_u32(
        &mut string_range_violation,
        nodes_offset + 2 * encode::NODE_RECORD_SIZE + 28,
        6,
    );
    encode::refresh_checksum(&mut string_range_violation);
    cases.push((
        "string range violation",
        string_range_violation,
        "NonCanonicalEncoding",
    ));

    let constant_range_violation = encode::build_log_package(
        &[encode::const_bytes(1), encode::ret()],
        pythos_shared::pyth_tig::opcode::RIGHTS_READ,
        RESOURCE_SYSTEM_LOG,
        PythType::Capability.code(),
    );
    cases.push((
        "constant range violation",
        constant_range_violation,
        "NonCanonicalEncoding",
    ));

    let mut package_limit = valid_fixture.to_vec();
    package_limit.resize(131_073, 0);
    cases.push((
        "package size limit",
        package_limit,
        "Decode(PackageTooLarge)",
    ));

    let mut node_count_limit = valid_fixture.to_vec();
    encode::write_u32(
        &mut node_count_limit,
        encode::HEADER_NODE_COUNT_OFFSET,
        1025,
    );
    encode::refresh_checksum(&mut node_count_limit);
    cases.push(("node count limit", node_count_limit, "Decode(CountLimit)"));

    let mut block_count_limit = valid_fixture.to_vec();
    encode::write_u32(
        &mut block_count_limit,
        encode::HEADER_BLOCK_COUNT_OFFSET,
        129,
    );
    encode::refresh_checksum(&mut block_count_limit);
    cases.push(("block count limit", block_count_limit, "Decode(CountLimit)"));

    let mut import_count_limit = valid_fixture.to_vec();
    encode::write_u32(
        &mut import_count_limit,
        encode::HEADER_IMPORT_COUNT_OFFSET,
        33,
    );
    encode::refresh_checksum(&mut import_count_limit);
    cases.push((
        "import count limit",
        import_count_limit,
        "Decode(CountLimit)",
    ));

    let mut noncanonical_encoding = valid_fixture.to_vec();
    encode::write_u16(
        &mut noncanonical_encoding,
        nodes_offset + 2 * encode::NODE_RECORD_SIZE + 4,
        1,
    );
    encode::refresh_checksum(&mut noncanonical_encoding);
    cases.push((
        "noncanonical encoding",
        noncanonical_encoding,
        "NonCanonicalEncoding",
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
