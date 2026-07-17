//! Phase 4 initial runtime-bundle loading.

#![cfg_attr(test, allow(dead_code))]

use pythos_shared::{boot_protocol::PythBootInfo, init_bundle, init_pak, runtime_payload};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLoadError {
    BadInitPak,
    BadInitBundle,
    BadRuntimePayload,
    MissingRuntimePayload,
    MissingUserElfPayload,
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

pub fn load_user_elf_payload(boot_info: &PythBootInfo) -> Result<&[u8], RuntimeLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_user_elf_payload_bytes(bytes)
}

pub fn validate_user_elf_payload_bytes(bytes: &[u8]) -> Result<&[u8], RuntimeLoadError> {
    let payload = init_pak_payload(bytes)?;
    let bundle = init_bundle::validate(payload).map_err(|_| RuntimeLoadError::BadInitBundle)?;
    let record = bundle
        .record(init_bundle::RecordType::UserElf)
        .ok_or(RuntimeLoadError::MissingUserElfPayload)?;
    Ok(record.bytes())
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
