use crate::{
    block_device::{BlockDeviceInfo, SECTOR_SIZE},
    package_registry::{
        PACKAGE_TRANSACTION_COMMIT_V0_LEN, PackageRegistry, PackageRegistryGeneration,
        PackageTransactionCommitV0,
    },
};
use pythos_shared::package_abi::PackageStatus;

#[cfg(not(test))]
use crate::block_device;
#[cfg(not(test))]
use core::cell::UnsafeCell;

pub const PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES: usize = 32 * 1024;
pub const PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS: usize = 64;
pub const PACKAGE_CANDIDATE_REGISTRY_SLOT_A_SECTOR: u64 = 8500;
pub const PACKAGE_CANDIDATE_REGISTRY_SLOT_B_SECTOR: u64 = 8564;
pub const PACKAGE_PUBLICATION_ANCHOR_SLOT_A_SECTOR: u64 = 8628;
pub const PACKAGE_PUBLICATION_ANCHOR_SLOT_B_SECTOR: u64 = 8629;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackagePublicationAnchorSlot {
    A,
    B,
}

pub fn write_candidate_registry_generation(
    device: BlockDeviceInfo,
    registry: &PackageRegistry,
) -> Result<PackageRegistryGeneration, PackageStatus> {
    let mut bytes = [0u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
    let generation = registry.encode_snapshot(&mut bytes)?;
    let first_sector = registry_slot_sector(generation.generation);
    ensure_sector_range(
        device,
        first_sector,
        PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS,
    )
    .map_err(|_| PackageStatus::RegistryWriteDenied)?;
    let mut sector_index = 0usize;
    while sector_index < PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS {
        let start = sector_index * SECTOR_SIZE;
        let mut sector = [0u8; SECTOR_SIZE];
        sector.copy_from_slice(&bytes[start..start + SECTOR_SIZE]);
        write_package_candidate_sector(device, first_sector + sector_index as u64, &sector)
            .map_err(|_| PackageStatus::RegistryWriteDenied)?;
        sector_index += 1;
    }

    let loaded = read_candidate_registry_generation(device, generation)?;
    if loaded.generation() != generation.generation
        || loaded.root_digest() != generation.root_digest
    {
        return Err(PackageStatus::RegistryRecoveryDenied);
    }
    Ok(generation)
}

pub fn read_candidate_registry_generation(
    device: BlockDeviceInfo,
    expected: PackageRegistryGeneration,
) -> Result<PackageRegistry, PackageStatus> {
    let mut registry = PackageRegistry::empty();
    read_candidate_registry_generation_into(device, expected, &mut registry)?;
    Ok(registry)
}

pub fn read_candidate_registry_generation_into(
    device: BlockDeviceInfo,
    expected: PackageRegistryGeneration,
    registry: &mut PackageRegistry,
) -> Result<(), PackageStatus> {
    #[cfg(not(test))]
    {
        return with_registry_snapshot_scratch(|bytes| {
            read_candidate_registry_generation_into_bytes(device, expected, registry, bytes)
        });
    }

    #[cfg(test)]
    {
        let mut bytes = [0u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
        read_candidate_registry_generation_into_bytes(device, expected, registry, &mut bytes)
    }
}

fn read_candidate_registry_generation_into_bytes(
    device: BlockDeviceInfo,
    expected: PackageRegistryGeneration,
    registry: &mut PackageRegistry,
    bytes: &mut [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES],
) -> Result<(), PackageStatus> {
    let first_sector = registry_slot_sector(expected.generation);
    ensure_sector_range(
        device,
        first_sector,
        PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS,
    )
    .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
    bytes.fill(0);
    let mut sector_index = 0usize;
    while sector_index < PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS {
        let sector = read_package_candidate_sector(device, first_sector + sector_index as u64)
            .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
        let start = sector_index * SECTOR_SIZE;
        bytes[start..start + SECTOR_SIZE].copy_from_slice(&sector);
        sector_index += 1;
    }
    let encoded_len = PackageRegistry::encoded_len_from_snapshot_header(bytes)?;
    if encoded_len > bytes.len() {
        return Err(PackageStatus::BoundsExceeded);
    }
    PackageRegistry::decode_snapshot_into(&bytes[..encoded_len], registry)?;
    if registry.generation() != expected.generation
        || registry.root_digest() != expected.root_digest
    {
        return Err(PackageStatus::TransactionAnchorMismatch);
    }
    Ok(())
}

#[cfg(not(test))]
struct RegistrySnapshotScratch(UnsafeCell<[u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES]>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: candidate-registry reads borrow this buffer synchronously.
// 2. Established by: Phase 13 package hydration is single-core and non-reentrant.
// 3. Lifetime: this scratch storage is static for the full boot.
// 4. Pointer ownership: this module exclusively accesses the buffer.
// 5. Alignment: `UnsafeCell` preserves the byte-array alignment.
// 6. Mapped length: exactly `PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES` bytes are used.
// 7. Concurrency: no concurrent package registry operation is authorized.
// 8. Violation: reentry could mix candidate snapshot bytes and corrupt validation.
unsafe impl Sync for RegistrySnapshotScratch {}

#[cfg(not(test))]
static REGISTRY_SNAPSHOT_SCRATCH: RegistrySnapshotScratch =
    RegistrySnapshotScratch(UnsafeCell::new([0; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES]));

#[cfg(not(test))]
fn with_registry_snapshot_scratch<R>(
    f: impl FnOnce(&mut [u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES]) -> R,
) -> R {
    // SAFETY:
    // 1. Invariant: the closure does not retain the mutable buffer reference.
    // 2. Established by: this helper accepts a synchronous `FnOnce`.
    // 3. Lifetime: the static buffer remains initialized for the whole boot.
    // 4. Pointer ownership: this module creates the only mutable reference.
    // 5. Alignment: `UnsafeCell` preserves the byte-array alignment.
    // 6. Mapped length: exactly one complete scratch buffer is borrowed.
    // 7. Concurrency: package hydration is single-core and non-reentrant.
    // 8. Violation: overlapping borrows could mix registry generations.
    unsafe { f(&mut *REGISTRY_SNAPSHOT_SCRATCH.0.get()) }
}

pub fn write_publication_anchor(
    device: BlockDeviceInfo,
    anchor: PackageTransactionCommitV0,
) -> Result<(), PackageStatus> {
    let sector_number = match anchor.package_registry_generation & 1 {
        0 => PACKAGE_PUBLICATION_ANCHOR_SLOT_A_SECTOR,
        _ => PACKAGE_PUBLICATION_ANCHOR_SLOT_B_SECTOR,
    };
    ensure_sector_range(device, sector_number, 1)
        .map_err(|_| PackageStatus::RegistryWriteDenied)?;
    let mut sector = [0u8; SECTOR_SIZE];
    anchor.encode(&mut sector[..PACKAGE_TRANSACTION_COMMIT_V0_LEN])?;
    write_package_candidate_sector(device, sector_number, &sector)
        .map_err(|_| PackageStatus::RegistryWriteDenied)
}

pub fn read_publication_anchor_slot(
    device: BlockDeviceInfo,
    slot: PackagePublicationAnchorSlot,
) -> Result<Option<PackageTransactionCommitV0>, PackageStatus> {
    let sector_number = match slot {
        PackagePublicationAnchorSlot::A => PACKAGE_PUBLICATION_ANCHOR_SLOT_A_SECTOR,
        PackagePublicationAnchorSlot::B => PACKAGE_PUBLICATION_ANCHOR_SLOT_B_SECTOR,
    };
    ensure_sector_range(device, sector_number, 1)
        .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
    let sector = read_package_candidate_sector(device, sector_number)
        .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
    if sector.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    if sector[PACKAGE_TRANSACTION_COMMIT_V0_LEN..]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Ok(None);
    }
    Ok(
        PackageTransactionCommitV0::decode_stored(&sector[..PACKAGE_TRANSACTION_COMMIT_V0_LEN])
            .ok(),
    )
}

pub(crate) fn read_package_candidate_sector(
    device: BlockDeviceInfo,
    sector: u64,
) -> Result<[u8; SECTOR_SIZE], ()> {
    #[cfg(not(test))]
    {
        block_device::read_sector(device, sector).map_err(|_| ())
    }
    #[cfg(test)]
    {
        let _ = device;
        let index = sector as usize;
        if index >= TEST_PACKAGE_CANDIDATE_SECTOR_COUNT {
            return Err(());
        }
        Ok(TEST_PACKAGE_CANDIDATE_SECTORS.lock().unwrap()[index])
    }
}

pub(crate) fn write_package_candidate_sector(
    device: BlockDeviceInfo,
    sector: u64,
    bytes: &[u8; SECTOR_SIZE],
) -> Result<(), ()> {
    #[cfg(not(test))]
    {
        block_device::write_sector(device, sector, bytes).map_err(|_| ())
    }
    #[cfg(test)]
    {
        let _ = device;
        let index = sector as usize;
        if index >= TEST_PACKAGE_CANDIDATE_SECTOR_COUNT {
            return Err(());
        }
        TEST_PACKAGE_CANDIDATE_SECTORS.lock().unwrap()[index] = *bytes;
        Ok(())
    }
}

fn registry_slot_sector(generation: u64) -> u64 {
    match generation & 1 {
        0 => PACKAGE_CANDIDATE_REGISTRY_SLOT_A_SECTOR,
        _ => PACKAGE_CANDIDATE_REGISTRY_SLOT_B_SECTOR,
    }
}

fn ensure_sector_range(
    device: BlockDeviceInfo,
    first_sector: u64,
    sector_count: usize,
) -> Result<(), ()> {
    let end = first_sector.checked_add(sector_count as u64).ok_or(())?;
    if end > device.capacity_sectors() {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
const TEST_PACKAGE_CANDIDATE_SECTOR_COUNT: usize =
    PACKAGE_PUBLICATION_ANCHOR_SLOT_B_SECTOR as usize + 1;

#[cfg(test)]
static TEST_PACKAGE_CANDIDATE_SECTORS: Mutex<
    [[u8; SECTOR_SIZE]; TEST_PACKAGE_CANDIDATE_SECTOR_COUNT],
> = Mutex::new([[0; SECTOR_SIZE]; TEST_PACKAGE_CANDIDATE_SECTOR_COUNT]);

#[cfg(test)]
pub(crate) static PACKAGE_CANDIDATE_STORAGE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn reset_package_candidate_storage_for_test() {
    *TEST_PACKAGE_CANDIDATE_SECTORS.lock().unwrap() =
        [[0; SECTOR_SIZE]; TEST_PACKAGE_CANDIDATE_SECTOR_COUNT];
}

#[cfg(test)]
pub(crate) fn reset_package_persistence_storage_for_test() {
    reset_package_candidate_storage_for_test();
    crate::object_service_checkpoint::reset_checkpoint_storage_for_test();
}

#[cfg(test)]
mod tests {
    use super::{
        PACKAGE_CANDIDATE_STORAGE_TEST_LOCK, read_candidate_registry_generation,
        reset_package_persistence_storage_for_test, write_candidate_registry_generation,
    };
    use crate::{block_device::BlockDeviceInfo, package_registry::PackageRegistry};

    #[test]
    fn package_persistence_test_reset_uses_common_storage_boundary() {
        let _guard = PACKAGE_CANDIDATE_STORAGE_TEST_LOCK.lock().unwrap();
        let device = BlockDeviceInfo::new_for_test(9000, 8);
        let registry = PackageRegistry::empty();
        let generation = write_candidate_registry_generation(device, &registry).unwrap();

        reset_package_persistence_storage_for_test();

        assert!(read_candidate_registry_generation(device, generation).is_err());
    }
}
