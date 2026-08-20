//! Phase 4 initial runtime-bundle loading.

#![cfg_attr(test, allow(dead_code))]

use pythos_shared::{
    boot_protocol::PythBootInfo, init_bundle, init_pak, pyth_native_binding, runtime_payload,
    user_program_manifest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLoadError {
    BadInitPak,
    BadInitBundle,
    BadRuntimePayload,
    BadUserElfPayload,
    MissingRuntimePayload,
    MissingUserElfPayload,
    MissingPythNativeBinding,
    DuplicateProgramName,
    DuplicateProgramPrincipal,
    BadPythNativeBinding,
}

pub fn load_init_payload(
    boot_info: &PythBootInfo,
) -> Result<runtime_payload::RuntimePayload<'_>, RuntimeLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_init_payload_bytes(bytes)
}

pub fn validate_init_payload_bytes(
    bytes: &[u8],
) -> Result<runtime_payload::RuntimePayload<'_>, RuntimeLoadError> {
    let payload = init_pak_payload(bytes)?;
    match init_bundle::validate(payload) {
        Ok(bundle) => {
            let record = bundle
                .record(init_bundle::RecordType::RuntimePayload)
                .ok_or(RuntimeLoadError::MissingRuntimePayload)?;
            runtime_payload::validate(record.bytes())
                .map_err(|_| RuntimeLoadError::BadRuntimePayload)
        }
        Err(init_bundle::InitBundleError::BadMagic) => {
            runtime_payload::validate(payload).map_err(|_| RuntimeLoadError::BadRuntimePayload)
        }
        Err(_) => Err(RuntimeLoadError::BadInitBundle),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_user_elf_payload(boot_info: &PythBootInfo) -> Result<&[u8], RuntimeLoadError> {
    load_user_elf_payload_at(boot_info, 0)
}

pub fn load_user_elf_payload_at(
    boot_info: &PythBootInfo,
    ordinal: usize,
) -> Result<&[u8], RuntimeLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_user_elf_payload_at_bytes(bytes, ordinal)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_named_user_program<'a>(
    boot_info: &'a PythBootInfo,
    name: &[u8],
) -> Result<user_program_manifest::NamedUserProgramManifest<'a>, RuntimeLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_named_user_program_payload_bytes(bytes, name)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn validate_user_elf_payload_bytes(bytes: &[u8]) -> Result<&[u8], RuntimeLoadError> {
    validate_user_elf_payload_at_bytes(bytes, 0)
}

pub fn validate_user_elf_payload_at_bytes(
    bytes: &[u8],
    ordinal: usize,
) -> Result<&[u8], RuntimeLoadError> {
    let payload = init_pak_payload(bytes)?;
    let bundle = init_bundle::validate(payload).map_err(|_| RuntimeLoadError::BadInitBundle)?;
    let record = bundle
        .record_at(init_bundle::RecordType::UserElf, ordinal)
        .ok_or(RuntimeLoadError::MissingUserElfPayload)?;
    Ok(record.bytes())
}

pub fn validate_named_user_program_payload_bytes<'a>(
    bytes: &'a [u8],
    name: &[u8],
) -> Result<user_program_manifest::NamedUserProgramManifest<'a>, RuntimeLoadError> {
    let payload = init_pak_payload(bytes)?;
    let bundle = init_bundle::validate(payload).map_err(|_| RuntimeLoadError::BadInitBundle)?;
    let mut policy = NamedProgramPolicy::empty();
    let mut selected = None;
    let mut index = 0usize;
    while let Some(record) = bundle.record_at(init_bundle::RecordType::NamedUserElf, index) {
        let manifest = user_program_manifest::validate_named_user_program(record.bytes())
            .map_err(|_| RuntimeLoadError::BadUserElfPayload)?;
        policy.observe(manifest)?;
        if manifest.name() == name {
            selected = Some(manifest);
        }
        index += 1;
    }
    let manifest = selected.ok_or(RuntimeLoadError::MissingUserElfPayload)?;
    enforce_kernel_identity_policy(manifest)?;
    Ok(manifest)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_pyth_native_binding<'a>(
    boot_info: &'a PythBootInfo,
    graph_name: &[u8],
    elf_name: &[u8],
    principal_id: u64,
    graph: &[u8],
    elf: &[u8],
) -> Result<pyth_native_binding::PythNativeBinding<'a>, RuntimeLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_pyth_native_binding_payload_bytes(
        bytes,
        graph_name,
        elf_name,
        principal_id,
        graph,
        elf,
    )
}

pub fn validate_pyth_native_binding_payload_bytes<'a>(
    bytes: &'a [u8],
    graph_name: &[u8],
    elf_name: &[u8],
    principal_id: u64,
    graph: &[u8],
    elf: &[u8],
) -> Result<pyth_native_binding::PythNativeBinding<'a>, RuntimeLoadError> {
    let payload = init_pak_payload(bytes)?;
    let bundle = init_bundle::validate(payload).map_err(|_| RuntimeLoadError::BadInitBundle)?;
    let mut selected = None;
    let mut index = 0usize;
    while let Some(record) = bundle.record_at(init_bundle::RecordType::PythNativeBinding, index) {
        let binding = pyth_native_binding::validate_pyth_native_binding(record.bytes())
            .map_err(|_| RuntimeLoadError::BadPythNativeBinding)?;
        if binding.graph_name() == graph_name && binding.elf_name() == elf_name {
            if selected.is_some() {
                return Err(RuntimeLoadError::BadInitBundle);
            }
            selected = Some(record.bytes());
        }
        index += 1;
    }
    let binding_bytes = selected.ok_or(RuntimeLoadError::MissingPythNativeBinding)?;
    pyth_native_binding::validate_pyth_native_binding_for_artifacts(
        binding_bytes,
        graph_name,
        elf_name,
        principal_id,
        graph,
        elf,
    )
    .map_err(|_| RuntimeLoadError::BadPythNativeBinding)
}

const MAX_NAMED_PROGRAM_RECORDS: usize = 8;
const SHELL_PROGRAM_NAME: &[u8] = b"shell.elf";

struct NamedProgramPolicy<'a> {
    names: [Option<&'a [u8]>; MAX_NAMED_PROGRAM_RECORDS],
    principals: [Option<u64>; MAX_NAMED_PROGRAM_RECORDS],
    count: usize,
}

impl<'a> NamedProgramPolicy<'a> {
    const fn empty() -> Self {
        Self {
            names: [None; MAX_NAMED_PROGRAM_RECORDS],
            principals: [None; MAX_NAMED_PROGRAM_RECORDS],
            count: 0,
        }
    }

    fn observe(
        &mut self,
        manifest: user_program_manifest::NamedUserProgramManifest<'a>,
    ) -> Result<(), RuntimeLoadError> {
        if manifest.name() != SHELL_PROGRAM_NAME
            && manifest.principal_id() == user_program_manifest::SHELL_PRINCIPAL_ID
        {
            return Err(RuntimeLoadError::DuplicateProgramPrincipal);
        }

        let mut index = 0usize;
        while index < self.count {
            if self.names[index] == Some(manifest.name()) {
                return Err(RuntimeLoadError::DuplicateProgramName);
            }
            if self.principals[index] == Some(manifest.principal_id()) {
                return Err(RuntimeLoadError::DuplicateProgramPrincipal);
            }
            index += 1;
        }

        if self.count >= MAX_NAMED_PROGRAM_RECORDS {
            return Err(RuntimeLoadError::BadInitBundle);
        }
        self.names[self.count] = Some(manifest.name());
        self.principals[self.count] = Some(manifest.principal_id());
        self.count += 1;
        Ok(())
    }
}

fn enforce_kernel_identity_policy(
    manifest: user_program_manifest::NamedUserProgramManifest<'_>,
) -> Result<(), RuntimeLoadError> {
    if manifest.name() == SHELL_PROGRAM_NAME
        && manifest.principal_id() != user_program_manifest::SHELL_PRINCIPAL_ID
    {
        return Err(RuntimeLoadError::BadUserElfPayload);
    }
    Ok(())
}

fn init_pak_payload(bytes: &[u8]) -> Result<&[u8], RuntimeLoadError> {
    let header = init_pak::validate(bytes).map_err(|_| RuntimeLoadError::BadInitPak)?;
    let payload_start =
        usize::try_from(header.header_len).map_err(|_| RuntimeLoadError::BadInitPak)?;
    let payload_len =
        usize::try_from(header.payload_len).map_err(|_| RuntimeLoadError::BadInitPak)?;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(RuntimeLoadError::BadInitPak)?;
    let payload = bytes
        .get(payload_start..payload_end)
        .ok_or(RuntimeLoadError::BadInitPak)?;
    Ok(payload)
}

fn init_bundle_bytes(boot_info: &PythBootInfo) -> Result<&[u8], RuntimeLoadError> {
    let len =
        usize::try_from(boot_info.init_bundle_len).map_err(|_| RuntimeLoadError::BadInitPak)?;
    if boot_info.init_bundle_phys == 0 || len == 0 {
        return Err(RuntimeLoadError::BadInitPak);
    }
    // SAFETY:
    // 1. Invariant: `init_bundle_phys` is mapped readable by the PythCore-owned
    //    address space before Phase 4 runtime-bundle validation runs.
    // 2. Established by: `KernelAddressSpace::build()` mapping the exact
    //    `INIT.PAK` range and `validate_active()` succeeding before this call.
    // 3. Lifetime: the loader allocation is owned by PythCore for early boot
    //    and remains live through runtime bootstrap.
    // 4. Pointer ownership: PythCore owns the buffer and reads it immutably in
    //    this slice.
    // 5. Alignment: a byte slice has no alignment requirement beyond `u8`.
    // 6. Mapped length: `init_bundle_len` bytes were mapped by the VM slice.
    // 7. Concurrency: single-core early boot; this validation does not mutate
    //    the bundle and races with no writer.
    // 8. Violation: an invalid pointer or length faults through exception
    //    diagnostics or causes bundle validation to reject the data.
    Ok(unsafe { core::slice::from_raw_parts(boot_info.init_bundle_phys as *const u8, len) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::user_program_manifest::{self, INTRUDER_PRINCIPAL_ID, SHELL_PRINCIPAL_ID};
    use std::vec::Vec;

    const HELLO_SERVICE: &[u8] = b"class HelloService(Service):\n    async def start(self):\n        system.log(\"PythOS [HISS] We Are Woken\")\n        self.ready()\n";

    fn build_runtime_payload(source: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; runtime_payload::RUNTIME_PAYLOAD_HEADER_LEN as usize];
        bytes[..runtime_payload::RUNTIME_PAYLOAD_MAGIC.len()]
            .copy_from_slice(runtime_payload::RUNTIME_PAYLOAD_MAGIC);
        bytes[16..18].copy_from_slice(&runtime_payload::RUNTIME_PAYLOAD_MAJOR.to_le_bytes());
        bytes[18..20].copy_from_slice(&runtime_payload::RUNTIME_PAYLOAD_MINOR.to_le_bytes());
        bytes[20..24].copy_from_slice(&runtime_payload::RUNTIME_PAYLOAD_HEADER_LEN.to_le_bytes());
        bytes[24..28].copy_from_slice(&(source.len() as u32).to_le_bytes());
        bytes[28..32].copy_from_slice(&runtime_payload::checksum(source).to_le_bytes());
        bytes.extend_from_slice(source);
        bytes
    }

    fn build_init_pak(payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; init_pak::INIT_PAK_HEADER_LEN as usize];
        let total_len = init_pak::INIT_PAK_HEADER_LEN as usize + payload.len();
        bytes[..init_pak::INIT_PAK_MAGIC.len()].copy_from_slice(init_pak::INIT_PAK_MAGIC);
        bytes[18..20].copy_from_slice(&init_pak::INIT_PAK_MAJOR.to_le_bytes());
        bytes[20..22].copy_from_slice(&init_pak::INIT_PAK_MINOR.to_le_bytes());
        bytes[22..26].copy_from_slice(&init_pak::INIT_PAK_HEADER_LEN.to_le_bytes());
        bytes[26..34].copy_from_slice(&(total_len as u64).to_le_bytes());
        bytes[34..42].copy_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes[42..46].copy_from_slice(&init_pak::checksum(payload).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn build_inner_bundle(records: &[(u32, &[u8])]) -> Vec<u8> {
        let header_len = pythos_shared::init_bundle::INIT_BUNDLE_HEADER_LEN as usize;
        let table_len = records.len() * pythos_shared::init_bundle::RECORD_ENTRY_LEN;
        let mut bytes = vec![0u8; header_len + table_len];
        bytes[..pythos_shared::init_bundle::INIT_BUNDLE_MAGIC.len()]
            .copy_from_slice(pythos_shared::init_bundle::INIT_BUNDLE_MAGIC);
        bytes[16..18].copy_from_slice(&pythos_shared::init_bundle::INIT_BUNDLE_MAJOR.to_le_bytes());
        bytes[18..20].copy_from_slice(&pythos_shared::init_bundle::INIT_BUNDLE_MINOR.to_le_bytes());
        bytes[20..24]
            .copy_from_slice(&pythos_shared::init_bundle::INIT_BUNDLE_HEADER_LEN.to_le_bytes());
        bytes[24..26].copy_from_slice(&(records.len() as u16).to_le_bytes());

        let mut cursor = header_len + table_len;
        for (index, (record_type, payload)) in records.iter().enumerate() {
            let entry = header_len + index * pythos_shared::init_bundle::RECORD_ENTRY_LEN;
            bytes[entry..entry + 4].copy_from_slice(&record_type.to_le_bytes());
            bytes[entry + 8..entry + 16].copy_from_slice(&(cursor as u64).to_le_bytes());
            bytes[entry + 16..entry + 24].copy_from_slice(&(payload.len() as u64).to_le_bytes());
            bytes[entry + 24..entry + 28]
                .copy_from_slice(&pythos_shared::init_bundle::checksum(payload).to_le_bytes());
            bytes.extend_from_slice(payload);
            cursor += payload.len();
        }
        bytes
    }

    fn build_named_user_program(name: &[u8], principal_id: u64, elf: &[u8]) -> Vec<u8> {
        let mut bytes =
            vec![
                0u8;
                user_program_manifest::NAMED_USER_PROGRAM_HEADER_LEN + name.len() + elf.len()
            ];
        let len =
            user_program_manifest::encode_named_user_program(&mut bytes, name, principal_id, elf)
                .unwrap();
        bytes.truncate(len);
        bytes
    }

    fn build_native_binding(
        graph_name: &[u8],
        elf_name: &[u8],
        principal_id: u64,
        graph: &[u8],
        elf: &[u8],
    ) -> Vec<u8> {
        let mut bytes = vec![
            0u8;
            pyth_native_binding::PYTH_NATIVE_BINDING_HEADER_LEN
                + graph_name.len()
                + elf_name.len()
        ];
        let len = pyth_native_binding::encode_pyth_native_binding(
            &mut bytes,
            graph_name,
            elf_name,
            principal_id,
            graph,
            elf,
        )
        .unwrap();
        bytes.truncate(len);
        bytes
    }

    #[test]
    fn valid_init_pak_runtime_payload_passes_without_execution() {
        let payload = build_runtime_payload(HELLO_SERVICE);
        let bundle = build_init_pak(&payload);

        let runtime = validate_init_payload_bytes(&bundle).unwrap();

        assert!(runtime.source.contains("system.log"));
    }

    #[test]
    fn inner_bundle_runtime_payload_passes_without_execution() {
        let payload = build_runtime_payload(HELLO_SERVICE);
        let user_elf = b"\x7FELFuser";
        let inner = build_inner_bundle(&[
            (
                pythos_shared::init_bundle::TYPE_RUNTIME_PAYLOAD,
                payload.as_slice(),
            ),
            (pythos_shared::init_bundle::TYPE_USER_ELF, user_elf),
        ]);
        let bundle = build_init_pak(&inner);

        let runtime = validate_init_payload_bytes(&bundle).unwrap();

        assert!(runtime.source.contains("system.log"));
    }

    #[test]
    fn inner_bundle_user_elf_record_is_exposed() {
        let payload = build_runtime_payload(HELLO_SERVICE);
        let user_elf = b"\x7FELFuser";
        let inner = build_inner_bundle(&[
            (
                pythos_shared::init_bundle::TYPE_RUNTIME_PAYLOAD,
                payload.as_slice(),
            ),
            (pythos_shared::init_bundle::TYPE_USER_ELF, user_elf),
        ]);
        let bundle = build_init_pak(&inner);

        let exposed = validate_user_elf_payload_bytes(&bundle).unwrap();

        assert_eq!(exposed, user_elf);
    }

    #[test]
    fn duplicate_inner_bundle_user_elf_records_are_addressable_by_ordinal() {
        let payload = build_runtime_payload(HELLO_SERVICE);
        let first_user_elf = b"\x7FELFfirst";
        let second_user_elf = b"\x7FELFsecond";
        let inner = build_inner_bundle(&[
            (
                pythos_shared::init_bundle::TYPE_RUNTIME_PAYLOAD,
                payload.as_slice(),
            ),
            (pythos_shared::init_bundle::TYPE_USER_ELF, first_user_elf),
            (pythos_shared::init_bundle::TYPE_USER_ELF, second_user_elf),
        ]);
        let bundle = build_init_pak(&inner);

        let first = validate_user_elf_payload_at_bytes(&bundle, 0).unwrap();
        let second = validate_user_elf_payload_at_bytes(&bundle, 1).unwrap();

        assert_eq!(first, first_user_elf);
        assert_eq!(second, second_user_elf);
        assert_eq!(
            validate_user_elf_payload_at_bytes(&bundle, 2),
            Err(RuntimeLoadError::MissingUserElfPayload)
        );
    }

    #[test]
    fn named_user_program_loader_binds_shell_identity_to_manifest() {
        let runtime_payload = build_runtime_payload(HELLO_SERVICE);
        let shell = build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, b"\x7FELFpayload");
        let inner = build_inner_bundle(&[
            (
                pythos_shared::init_bundle::TYPE_RUNTIME_PAYLOAD,
                runtime_payload.as_slice(),
            ),
            (
                pythos_shared::init_bundle::TYPE_NAMED_USER_ELF,
                shell.as_slice(),
            ),
        ]);
        let bundle = build_init_pak(&inner);

        let loaded = validate_named_user_program_payload_bytes(&bundle, b"shell.elf").unwrap();

        assert_eq!(loaded.name(), b"shell.elf");
        assert_eq!(loaded.principal_id(), SHELL_PRINCIPAL_ID);
        assert_eq!(loaded.elf(), b"\x7FELFpayload");
    }

    #[test]
    fn named_user_program_loader_rejects_duplicate_shell_principal_claim() {
        let shell = build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, b"\x7FELFpayload");
        let impostor =
            build_named_user_program(b"other.elf", SHELL_PRINCIPAL_ID, b"\x7FELFimpostor");
        let inner = build_inner_bundle(&[
            (
                pythos_shared::init_bundle::TYPE_NAMED_USER_ELF,
                shell.as_slice(),
            ),
            (
                pythos_shared::init_bundle::TYPE_NAMED_USER_ELF,
                impostor.as_slice(),
            ),
        ]);
        let bundle = build_init_pak(&inner);

        assert_eq!(
            validate_named_user_program_payload_bytes(&bundle, b"shell.elf"),
            Err(RuntimeLoadError::DuplicateProgramPrincipal)
        );
    }

    #[test]
    fn named_user_program_loader_rejects_duplicate_shell_name() {
        let shell = build_named_user_program(b"shell.elf", SHELL_PRINCIPAL_ID, b"\x7FELFpayload");
        let duplicate =
            build_named_user_program(b"shell.elf", INTRUDER_PRINCIPAL_ID, b"\x7FELFother");
        let inner = build_inner_bundle(&[
            (
                pythos_shared::init_bundle::TYPE_NAMED_USER_ELF,
                shell.as_slice(),
            ),
            (
                pythos_shared::init_bundle::TYPE_NAMED_USER_ELF,
                duplicate.as_slice(),
            ),
        ]);
        let bundle = build_init_pak(&inner);

        assert_eq!(
            validate_named_user_program_payload_bytes(&bundle, b"shell.elf"),
            Err(RuntimeLoadError::DuplicateProgramName)
        );
    }

    #[test]
    fn pyth_native_binding_loader_matches_graph_and_elf_manifests() {
        let graph = b"PYTHTIG1graph";
        let elf = b"\x7FELFnative";
        let principal_id = 0x5059_5448_4752_0001;
        let binding = build_native_binding(b"hello.tig", b"hello.elf", principal_id, graph, elf);
        let inner = build_inner_bundle(&[(
            pythos_shared::init_bundle::TYPE_PYTH_NATIVE_BINDING,
            binding.as_slice(),
        )]);
        let bundle = build_init_pak(&inner);

        let loaded = validate_pyth_native_binding_payload_bytes(
            &bundle,
            b"hello.tig",
            b"hello.elf",
            principal_id,
            graph,
            elf,
        )
        .unwrap();

        assert_eq!(loaded.graph_name(), b"hello.tig");
        assert_eq!(loaded.elf_name(), b"hello.elf");
        assert_eq!(loaded.principal_id(), principal_id);
    }

    #[test]
    fn pyth_native_binding_loader_rejects_missing_or_mismatched_binding() {
        let graph = b"PYTHTIG1graph";
        let elf = b"\x7FELFnative";
        let principal_id = 0x5059_5448_4752_0001;
        let empty = build_init_pak(&build_inner_bundle(&[]));
        assert_eq!(
            validate_pyth_native_binding_payload_bytes(
                &empty,
                b"hello.tig",
                b"hello.elf",
                principal_id,
                graph,
                elf,
            ),
            Err(RuntimeLoadError::BadInitBundle)
        );

        let binding = build_native_binding(b"hello.tig", b"hello.elf", principal_id, graph, elf);
        let inner = build_inner_bundle(&[(
            pythos_shared::init_bundle::TYPE_PYTH_NATIVE_BINDING,
            binding.as_slice(),
        )]);
        let bundle = build_init_pak(&inner);

        assert_eq!(
            validate_pyth_native_binding_payload_bytes(
                &bundle,
                b"hello.tig",
                b"hello.elf",
                principal_id,
                graph,
                b"\x7FELFother",
            ),
            Err(RuntimeLoadError::BadPythNativeBinding)
        );
        assert_eq!(
            validate_pyth_native_binding_payload_bytes(
                &bundle,
                b"budget.tig",
                b"budget.elf",
                principal_id,
                graph,
                elf,
            ),
            Err(RuntimeLoadError::MissingPythNativeBinding)
        );
    }

    #[test]
    fn bad_outer_init_pak_is_rejected() {
        let payload = build_runtime_payload(HELLO_SERVICE);
        let mut bundle = build_init_pak(&payload);
        bundle[0] = 0;

        assert_eq!(
            validate_init_payload_bytes(&bundle),
            Err(RuntimeLoadError::BadInitPak)
        );
    }

    #[test]
    fn bad_inner_runtime_payload_is_rejected() {
        let payload = build_runtime_payload(&[]);
        let bundle = build_init_pak(&payload);

        assert_eq!(
            validate_init_payload_bytes(&bundle),
            Err(RuntimeLoadError::BadRuntimePayload)
        );
    }
}
