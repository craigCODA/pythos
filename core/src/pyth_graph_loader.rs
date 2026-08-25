#![cfg_attr(test, allow(dead_code))]
#![cfg_attr(any(feature = "verify", feature = "hardware-probe"), allow(dead_code))]

#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
use crate::{
    package_content_store::{ContentId, PackageContentStore},
    package_registry::PackageRegistryExportRecord,
};
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
use pythos_shared::package_abi::PackageStatus;
use pythos_shared::{
    boot_protocol::PythBootInfo,
    init_bundle, init_pak, pyth_graph_manifest,
    pyth_tig::{
        NO_VALUE,
        opcode::Opcode,
        types::PythType,
        verify::{self, VerifiedGraph, VerifyError},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythGraphLoadError {
    BadInitPak,
    BadInitBundle,
    BadGraphPayload,
    MissingGraphPayload,
    DuplicateGraphName,
    DuplicateGraphPrincipal,
    UnsupportedPhase2Opcode { node: u32, opcode: u16 },
    UnsupportedPhase2ControlFlow { block: u32, target: u32 },
    Verify(VerifyError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadedPythGraph<'a> {
    pub manifest: pyth_graph_manifest::NamedPythGraphManifest<'a>,
    pub verified: VerifiedGraph<'a>,
}

#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadedPackageExportGraph<'a> {
    pub content_id: ContentId,
    pub byte_len: usize,
    pub verified: VerifiedGraph<'a>,
}

#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageExportGraphValidationError {
    pub status: PackageStatus,
    pub verifier: Option<VerifyError>,
}

#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
impl PackageExportGraphValidationError {
    pub const fn from_status(status: PackageStatus) -> Self {
        Self {
            status,
            verifier: None,
        }
    }

    pub const fn from_verifier(verifier: VerifyError) -> Self {
        Self {
            status: PackageStatus::PythTigVerificationFailed,
            verifier: Some(verifier),
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_named_pyth_graph<'a>(
    boot_info: &'a PythBootInfo,
    name: &[u8],
) -> Result<LoadedPythGraph<'a>, PythGraphLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_named_pyth_graph_payload_bytes(bytes, name)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_named_pyth_graph_for_admission<'a>(
    boot_info: &'a PythBootInfo,
    name: &[u8],
) -> Result<LoadedPythGraph<'a>, PythGraphLoadError> {
    let bytes = init_bundle_bytes(boot_info)?;
    validate_named_pyth_graph_payload_bytes_for_admission(bytes, name)
}

pub fn validate_named_pyth_graph_payload_bytes<'a>(
    bytes: &'a [u8],
    name: &[u8],
) -> Result<LoadedPythGraph<'a>, PythGraphLoadError> {
    validate_named_pyth_graph_payload_bytes_with_profile(bytes, name, true)
}

pub fn validate_named_pyth_graph_payload_bytes_for_admission<'a>(
    bytes: &'a [u8],
    name: &[u8],
) -> Result<LoadedPythGraph<'a>, PythGraphLoadError> {
    validate_named_pyth_graph_payload_bytes_with_profile(bytes, name, false)
}

#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
pub fn validate_package_export_graph<'a>(
    content_store: &PackageContentStore<'_>,
    export: PackageRegistryExportRecord,
    package_buffer: &'a mut [u8],
) -> Result<LoadedPackageExportGraph<'a>, PackageExportGraphValidationError> {
    let content_id = ContentId::new(
        export.package_object_id,
        export.release_digest,
        export.content_index,
    );
    let byte_len = content_store
        .read_published(content_id, package_buffer)
        .map_err(PackageExportGraphValidationError::from_status)?;
    let package_bytes =
        package_buffer
            .get(..byte_len)
            .ok_or(PackageExportGraphValidationError::from_status(
                PackageStatus::ContentCorrupt,
            ))?;
    let verified = verify::verify_bytes(package_bytes)
        .map_err(PackageExportGraphValidationError::from_verifier)?;
    Ok(LoadedPackageExportGraph {
        content_id,
        byte_len,
        verified,
    })
}

fn validate_named_pyth_graph_payload_bytes_with_profile<'a>(
    bytes: &'a [u8],
    name: &[u8],
    enforce_runtime_profile: bool,
) -> Result<LoadedPythGraph<'a>, PythGraphLoadError> {
    let payload = init_pak_payload(bytes)?;
    let bundle = init_bundle::validate(payload).map_err(|_| PythGraphLoadError::BadInitBundle)?;
    let mut policy = NamedGraphPolicy::empty();
    let mut selected = None;
    let mut index = 0usize;
    while let Some(record) = bundle.record_at(init_bundle::RecordType::PythGraphPackage, index) {
        let manifest = pyth_graph_manifest::validate_named_pyth_graph(record.bytes())
            .map_err(|_| PythGraphLoadError::BadGraphPayload)?;
        policy.observe(manifest)?;
        if manifest.name() == name {
            selected = Some(manifest);
        }
        index += 1;
    }

    let manifest = selected.ok_or(PythGraphLoadError::MissingGraphPayload)?;
    let verified = verify::verify_bytes(manifest.package()).map_err(PythGraphLoadError::Verify)?;
    if enforce_runtime_profile {
        validate_phase2_runtime_profile(&verified)?;
    }
    Ok(LoadedPythGraph { manifest, verified })
}

fn validate_phase2_runtime_profile(verified: &VerifiedGraph<'_>) -> Result<(), PythGraphLoadError> {
    let package = verified.package();
    for block in package.blocks().iter() {
        let terminator_index = usize::try_from(block.terminator_node).map_err(|_| {
            PythGraphLoadError::UnsupportedPhase2ControlFlow {
                block: block.block_id,
                target: u32::MAX,
            }
        })?;
        let terminator = package.nodes().get(terminator_index).ok_or(
            PythGraphLoadError::UnsupportedPhase2ControlFlow {
                block: block.block_id,
                target: u32::MAX,
            },
        )?;
        if Opcode::try_from(terminator.opcode) == Ok(Opcode::Jump) {
            let target = terminator.auxiliary0;
            let target_index = usize::try_from(target).map_err(|_| {
                PythGraphLoadError::UnsupportedPhase2ControlFlow {
                    block: block.block_id,
                    target,
                }
            })?;
            let target_block = package.blocks().get(target_index).ok_or(
                PythGraphLoadError::UnsupportedPhase2ControlFlow {
                    block: block.block_id,
                    target,
                },
            )?;
            if target_block.parameter_count != 0
                || [
                    terminator.input0,
                    terminator.input1,
                    terminator.input2,
                    terminator.input3,
                ]
                .into_iter()
                .any(|input| input != NO_VALUE)
            {
                return Err(PythGraphLoadError::UnsupportedPhase2ControlFlow {
                    block: block.block_id,
                    target,
                });
            }
        }
    }

    for (node_index, node) in package.nodes().iter().enumerate() {
        let supported = match Opcode::try_from(node.opcode) {
            Ok(Opcode::BlockParam) => {
                PythType::try_from(node.result_type) == Ok(PythType::Capability)
            }
            Ok(Opcode::ConstU64) => matches!(
                PythType::try_from(node.result_type),
                Ok(PythType::ObjectId
                    | PythType::RevisionId
                    | PythType::TaskId
                    | PythType::ProposalId
                    | PythType::ErrorCode
                    | PythType::U64)
            ),
            Ok(
                Opcode::EffectStart
                | Opcode::ConstBytes
                | Opcode::ConstUtf8
                | Opcode::HostResult
                | Opcode::LessThanU64
                | Opcode::SystemLog
                | Opcode::ObjectCreate
                | Opcode::ObjectQuery
                | Opcode::ObjectInspect
                | Opcode::ObjectRevise
                | Opcode::ObjectHistory
                | Opcode::TaskContextRead
                | Opcode::TaskProposalEmit
                | Opcode::CommandRead
                | Opcode::CommandResultEmit
                | Opcode::Branch
                | Opcode::Jump
                | Opcode::Return,
            ) => true,
            _ => false,
        };
        if !supported {
            return Err(PythGraphLoadError::UnsupportedPhase2Opcode {
                node: u32::try_from(node_index).unwrap_or(u32::MAX),
                opcode: node.opcode,
            });
        }
    }
    Ok(())
}

const MAX_NAMED_GRAPH_RECORDS: usize = 8;

struct NamedGraphPolicy<'a> {
    names: [Option<&'a [u8]>; MAX_NAMED_GRAPH_RECORDS],
    principals: [Option<u64>; MAX_NAMED_GRAPH_RECORDS],
    count: usize,
}

impl<'a> NamedGraphPolicy<'a> {
    const fn empty() -> Self {
        Self {
            names: [None; MAX_NAMED_GRAPH_RECORDS],
            principals: [None; MAX_NAMED_GRAPH_RECORDS],
            count: 0,
        }
    }

    fn observe(
        &mut self,
        manifest: pyth_graph_manifest::NamedPythGraphManifest<'a>,
    ) -> Result<(), PythGraphLoadError> {
        let mut index = 0usize;
        while index < self.count {
            if self.names[index] == Some(manifest.name()) {
                return Err(PythGraphLoadError::DuplicateGraphName);
            }
            if self.principals[index] == Some(manifest.principal_id()) {
                return Err(PythGraphLoadError::DuplicateGraphPrincipal);
            }
            index += 1;
        }

        if self.count >= MAX_NAMED_GRAPH_RECORDS {
            return Err(PythGraphLoadError::BadInitBundle);
        }
        self.names[self.count] = Some(manifest.name());
        self.principals[self.count] = Some(manifest.principal_id());
        self.count += 1;
        Ok(())
    }
}

fn init_pak_payload(bytes: &[u8]) -> Result<&[u8], PythGraphLoadError> {
    let header = init_pak::validate(bytes).map_err(|_| PythGraphLoadError::BadInitPak)?;
    let payload_start =
        usize::try_from(header.header_len).map_err(|_| PythGraphLoadError::BadInitPak)?;
    let payload_len =
        usize::try_from(header.payload_len).map_err(|_| PythGraphLoadError::BadInitPak)?;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(PythGraphLoadError::BadInitPak)?;
    bytes
        .get(payload_start..payload_end)
        .ok_or(PythGraphLoadError::BadInitPak)
}

fn init_bundle_bytes(boot_info: &PythBootInfo) -> Result<&[u8], PythGraphLoadError> {
    let len =
        usize::try_from(boot_info.init_bundle_len).map_err(|_| PythGraphLoadError::BadInitPak)?;
    if boot_info.init_bundle_phys == 0 || len == 0 {
        return Err(PythGraphLoadError::BadInitPak);
    }
    // SAFETY:
    // 1. Invariant: `init_bundle_phys` points to the loader-retained INIT.PAK
    //    range mapped read-only by the active PythCore page tables.
    // 2. Established by: VM boot metadata mapping succeeds before graph launch
    //    admission is attempted.
    // 3. Lifetime: the loader allocation is PythCore-owned for the boot.
    // 4. Pointer ownership: graph admission reads the bytes immutably.
    // 5. Alignment: byte slices require only `u8` alignment.
    // 6. Mapped length: `init_bundle_len` bytes were mapped by PythCore.
    // 7. Concurrency: single-core early graph admission races with no writer.
    // 8. Violation: bad boot metadata faults or is rejected by package checks.
    Ok(unsafe { core::slice::from_raw_parts(boot_info.init_bundle_phys as *const u8, len) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_device::{BlockDeviceInfo, SECTOR_SIZE},
        package_candidate_store::{
            PACKAGE_CANDIDATE_STORAGE_TEST_LOCK, reset_package_candidate_storage_for_test,
            write_package_candidate_sector,
        },
        package_content_store::{ContentId, PackageContentStore, PackageContentTransaction},
        package_registry::{PackageRegistry, PackageRegistryExportRecord},
        package_service::validate_package_export_graph as validate_package_export_graph_from_package_service,
    };
    use pythos_shared::{
        init_bundle,
        package_abi::{PACKAGE_CONTENT_BASE_SECTOR, PackageStatus},
        pyth_graph_manifest,
        pyth_tig::{opcode::Opcode, test_support, verify, verify::VerifyError},
        sha256::sha256,
    };
    use std::vec::Vec;

    const HELLO_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0001;
    const PACKAGE_GRAPH_OBJECT_ID: u64 = 0x5059_504B_4752_0001;
    const PACKAGE_GRAPH_SCHEMA_ID: u64 = 0x5059_5343_4852_0001;

    fn build_init_pak(payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; pythos_shared::init_pak::INIT_PAK_HEADER_LEN as usize];
        let total_len = pythos_shared::init_pak::INIT_PAK_HEADER_LEN as usize + payload.len();
        bytes[..pythos_shared::init_pak::INIT_PAK_MAGIC.len()]
            .copy_from_slice(pythos_shared::init_pak::INIT_PAK_MAGIC);
        bytes[18..20].copy_from_slice(&pythos_shared::init_pak::INIT_PAK_MAJOR.to_le_bytes());
        bytes[20..22].copy_from_slice(&pythos_shared::init_pak::INIT_PAK_MINOR.to_le_bytes());
        bytes[22..26].copy_from_slice(&pythos_shared::init_pak::INIT_PAK_HEADER_LEN.to_le_bytes());
        bytes[26..34].copy_from_slice(&(total_len as u64).to_le_bytes());
        bytes[34..42].copy_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes[42..46].copy_from_slice(&pythos_shared::init_pak::checksum(payload).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn build_inner_bundle(records: &[(u32, &[u8])]) -> Vec<u8> {
        let header_len = init_bundle::INIT_BUNDLE_HEADER_LEN as usize;
        let table_len = records.len() * init_bundle::RECORD_ENTRY_LEN;
        let mut bytes = vec![0u8; header_len + table_len];
        bytes[..init_bundle::INIT_BUNDLE_MAGIC.len()]
            .copy_from_slice(init_bundle::INIT_BUNDLE_MAGIC);
        bytes[16..18].copy_from_slice(&init_bundle::INIT_BUNDLE_MAJOR.to_le_bytes());
        bytes[18..20].copy_from_slice(&init_bundle::INIT_BUNDLE_MINOR.to_le_bytes());
        bytes[20..24].copy_from_slice(&init_bundle::INIT_BUNDLE_HEADER_LEN.to_le_bytes());
        bytes[24..26].copy_from_slice(&(records.len() as u16).to_le_bytes());

        let mut cursor = header_len + table_len;
        for (index, (record_type, payload)) in records.iter().enumerate() {
            let entry = header_len + index * init_bundle::RECORD_ENTRY_LEN;
            bytes[entry..entry + 4].copy_from_slice(&record_type.to_le_bytes());
            bytes[entry + 8..entry + 16].copy_from_slice(&(cursor as u64).to_le_bytes());
            bytes[entry + 16..entry + 24].copy_from_slice(&(payload.len() as u64).to_le_bytes());
            bytes[entry + 24..entry + 28]
                .copy_from_slice(&init_bundle::checksum(payload).to_le_bytes());
            cursor += payload.len();
        }

        for (_, payload) in records {
            bytes.extend_from_slice(payload);
        }
        bytes
    }

    fn build_named_graph_bundle(name: &[u8], package: &[u8]) -> Vec<u8> {
        let mut graph =
            vec![
                0u8;
                pyth_graph_manifest::NAMED_PYTH_GRAPH_HEADER_LEN + name.len() + package.len()
            ];
        let graph_len = pyth_graph_manifest::encode_named_pyth_graph(
            &mut graph,
            name,
            HELLO_GRAPH_PRINCIPAL_ID,
            package,
        )
        .unwrap();
        graph.truncate(graph_len);
        build_init_pak(&build_inner_bundle(&[(
            init_bundle::TYPE_PYTH_GRAPH_PACKAGE,
            graph.as_slice(),
        )]))
    }

    fn commit_package_export_content<'a>(
        package_object_id: u64,
        release_digest: [u8; 32],
        bytes: &'a [u8],
    ) -> (PackageContentStore<'a>, ContentId) {
        let mut store = PackageContentStore::empty();
        let mut transaction = PackageContentTransaction::new(package_object_id, release_digest);
        let content_id = store
            .stage_content(&mut transaction, 1, 1, bytes, sha256(bytes))
            .unwrap();
        store.commit(&mut transaction).unwrap();
        (store, content_id)
    }

    fn package_export_record(content_id: ContentId) -> PackageRegistryExportRecord {
        PackageRegistryExportRecord::new(
            0x5059_504B_4E53_0001,
            b"seed",
            b"launch",
            content_id.package_object_id,
            1,
            content_id.release_digest,
            1,
            content_id.content_index,
            0,
            PACKAGE_GRAPH_SCHEMA_ID,
            1,
            sha256(b"schema:package-graph.v0"),
        )
        .unwrap()
    }

    #[test]
    fn package_pythtig_verification_accepts_valid_published_export_content() {
        let package = test_support::system_log_with_import_capability();
        let package_bytes: &[u8] = &package;
        let release_digest = sha256(b"release:package-graph-valid");
        let (store, content_id) =
            commit_package_export_content(PACKAGE_GRAPH_OBJECT_ID, release_digest, package_bytes);
        let export = package_export_record(content_id);
        let mut package_buffer = vec![0u8; package_bytes.len()];

        let loaded_byte_len = {
            let loaded = validate_package_export_graph_from_package_service(
                &store,
                export,
                &mut package_buffer,
            )
            .unwrap();

            assert_eq!(loaded.content_id, content_id);
            assert_eq!(loaded.byte_len, package_bytes.len());
            assert_eq!(loaded.verified.package().header().node_count, 5);
            loaded.byte_len
        };
        assert_eq!(&package_buffer[..loaded_byte_len], package_bytes);
    }

    #[test]
    fn package_pythtig_verification_returns_content_corrupt_before_verifier() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        reset_package_candidate_storage_for_test();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let package = test_support::system_log_with_import_capability();
        let package_bytes: &[u8] = &package;
        let release_digest = sha256(b"release:package-graph-corrupt");
        let mut source_store = PackageContentStore::empty();
        let mut transaction =
            PackageContentTransaction::new(PACKAGE_GRAPH_OBJECT_ID + 1, release_digest);
        let content_id = source_store
            .stage_content(&mut transaction, 1, 1, package_bytes, sha256(package_bytes))
            .unwrap();
        let mut registry = PackageRegistry::empty();
        source_store
            .add_staged_records_to_registry(&transaction, &mut registry)
            .unwrap();
        let record = registry.content_record(0).unwrap();
        let commit = source_store
            .write_candidate_content(device, &transaction)
            .unwrap();
        let restored =
            PackageContentStore::restore_from_validated_registry(device, &registry, commit)
                .unwrap();
        let mut corrupt_sector = [0xA5u8; SECTOR_SIZE];
        corrupt_sector[0] = 0x5A;
        write_package_candidate_sector(
            device,
            PACKAGE_CONTENT_BASE_SECTOR + u64::from(record.extents[0].start_block),
            &corrupt_sector,
        )
        .unwrap();
        let export = package_export_record(content_id);
        let mut package_buffer = vec![0u8; package_bytes.len()];

        let error = validate_package_export_graph_from_package_service(
            &restored,
            export,
            &mut package_buffer,
        )
        .expect_err("corrupt package content must be denied before PythTIG verification");

        assert_eq!(error.status, PackageStatus::ContentCorrupt);
        assert_eq!(error.verifier, None);
    }

    #[test]
    fn package_pythtig_verification_preserves_nested_verifier_identity() {
        let package = test_support::package_with_effect_fork();
        let package_bytes: &[u8] = &package;
        assert_eq!(
            verify::verify_bytes(package_bytes),
            Err(VerifyError::EffectFork { producer: 0 })
        );
        let release_digest = sha256(b"release:package-graph-invalid-pythtig");
        let (store, content_id) = commit_package_export_content(
            PACKAGE_GRAPH_OBJECT_ID + 2,
            release_digest,
            package_bytes,
        );
        let export = package_export_record(content_id);
        let mut package_buffer = vec![0u8; package_bytes.len()];

        let error =
            validate_package_export_graph_from_package_service(&store, export, &mut package_buffer)
                .expect_err("package-layer status must wrap the verifier identity");

        assert_eq!(error.status, PackageStatus::PythTigVerificationFailed);
        assert_eq!(
            error.verifier,
            Some(VerifyError::EffectFork { producer: 0 })
        );
    }

    #[test]
    fn loader_accepts_valid_named_graph_and_rejects_invalid_before_launch() {
        let valid_package = test_support::system_log_with_import_capability();
        let valid = build_named_graph_bundle(b"hello.tig", &valid_package);
        let loaded = validate_named_pyth_graph_payload_bytes(&valid, b"hello.tig").unwrap();

        assert_eq!(loaded.manifest.name(), b"hello.tig");
        assert_eq!(loaded.verified.package().header().node_count, 5);
        assert_eq!(loaded.verified.package().header().block_count, 1);

        let invalid_package = test_support::package_with_effect_fork();
        let invalid = build_named_graph_bundle(b"bad.tig", &invalid_package);
        assert_eq!(
            validate_named_pyth_graph_payload_bytes(&invalid, b"bad.tig"),
            Err(PythGraphLoadError::Verify(VerifyError::EffectFork {
                producer: 0
            }))
        );
    }

    #[test]
    fn loader_rejects_verifier_valid_opcode_outside_phase2_profile() {
        let package = test_support::branch_to_return_package(true);
        verify::verify_bytes(&package).expect("shared v1 verifier must admit frozen opcode");
        let bundle = build_named_graph_bundle(b"unsupported.tig", &package);

        assert_eq!(
            validate_named_pyth_graph_payload_bytes(&bundle, b"unsupported.tig"),
            Err(PythGraphLoadError::UnsupportedPhase2Opcode {
                node: 0,
                opcode: Opcode::ConstBool.code(),
            })
        );
    }

    #[test]
    fn admission_loader_accepts_verifier_valid_graph_without_runtime_profile_claim() {
        let package = test_support::branch_to_return_package(true);
        verify::verify_bytes(&package).expect("shared v1 verifier must admit frozen opcode");
        let bundle = build_named_graph_bundle(b"service.tig", &package);

        let loaded =
            validate_named_pyth_graph_payload_bytes_for_admission(&bundle, b"service.tig").unwrap();

        assert_eq!(loaded.manifest.name(), b"service.tig");
        assert_eq!(loaded.verified.package().header().node_count, 4);
    }

    #[test]
    fn loader_rejects_verifier_valid_parameterized_jump_before_launch() {
        let package = test_support::package_with_parameterized_jump();
        verify::verify_bytes(&package)
            .expect("shared v1 verifier must admit parameterized control flow");
        let bundle = build_named_graph_bundle(b"parameterized.tig", &package);

        assert_eq!(
            validate_named_pyth_graph_payload_bytes(&bundle, b"parameterized.tig"),
            Err(PythGraphLoadError::UnsupportedPhase2ControlFlow {
                block: 0,
                target: 1,
            })
        );
    }

    #[test]
    fn loader_accepts_phase3_object_runtime_profile() {
        let package = test_support::object_note_flow_package();
        verify::verify_bytes(&package).expect("shared v1 verifier must admit Phase 3 object flow");
        let bundle = build_named_graph_bundle(b"object.tig", &package);

        let loaded = validate_named_pyth_graph_payload_bytes(&bundle, b"object.tig").unwrap();

        assert_eq!(loaded.manifest.name(), b"object.tig");
        assert_eq!(loaded.verified.package().header().node_count, 11);
    }

    #[test]
    fn loader_accepts_task_steward_runtime_profile() {
        let package = test_support::task_context_score_with_import_rights(
            pythos_shared::task_abi::TASK_RIGHT_READ_CONTEXT,
        );
        verify::verify_bytes(&package).expect("shared v1 verifier must admit Task Steward graph");
        let bundle = build_named_graph_bundle(b"task-steward.tig", &package);

        let loaded = validate_named_pyth_graph_payload_bytes(&bundle, b"task-steward.tig").unwrap();

        assert_eq!(loaded.manifest.name(), b"task-steward.tig");
        assert_eq!(loaded.verified.package().header().node_count, 5);
    }
}
