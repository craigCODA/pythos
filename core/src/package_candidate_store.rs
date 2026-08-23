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
    let first_sector = registry_slot_sector(expected.generation);
    ensure_sector_range(
        device,
        first_sector,
        PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS,
    )
    .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
    let mut bytes = [0u8; PACKAGE_REGISTRY_SNAPSHOT_MAX_BYTES];
    let mut sector_index = 0usize;
    while sector_index < PACKAGE_CANDIDATE_REGISTRY_SLOT_SECTORS {
        let sector = read_package_candidate_sector(device, first_sector + sector_index as u64)
            .map_err(|_| PackageStatus::RegistryRecoveryDenied)?;
        let start = sector_index * SECTOR_SIZE;
        bytes[start..start + SECTOR_SIZE].copy_from_slice(&sector);
        sector_index += 1;
    }
    let encoded_len = PackageRegistry::encoded_len_from_snapshot_header(&bytes)?;
    if encoded_len > bytes.len() {
        return Err(PackageStatus::BoundsExceeded);
    }
    let registry = PackageRegistry::decode_snapshot(&bytes[..encoded_len])?;
    if registry.generation() != expected.generation
        || registry.root_digest() != expected.root_digest
    {
        return Err(PackageStatus::TransactionAnchorMismatch);
    }
    Ok(registry)
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
