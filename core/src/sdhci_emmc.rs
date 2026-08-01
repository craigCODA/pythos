//! Production SDHCI/eMMC polling PIO block-backend adapter.
//!
//! This module owns production controller selection and maps the shared SDHCI
//! command engine onto a retained, already-initialized eMMC card context. It
//! does not enable interrupts, DMA, ADMA, or multi-block transfers.
#![cfg_attr(
    any(test, all(not(test), feature = "sdhci-emmc-backend")),
    allow(dead_code)
)]

use crate::sdhci::{self, EMMC_LOGICAL_BLOCK_SIZE, EmmcBlockError, EmmcCard, MmioRegisterIo};
#[cfg(not(test))]
use crate::serial;
#[cfg(not(test))]
use crate::storage_probe;
use crate::storage_probe::{MemoryBar, StorageController, StorageControllerKind};

pub const SDHCI_EMMC_MMIO_VIRT: u64 = 0xFFFF_C000_1002_0000;
pub const SDHCI_EMMC_MMIO_LEN: u64 = 0x1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciEmmcController {
    pub controller: StorageController,
    pub physical_mmio_base: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciEmmcBlockDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub mmio_base: u64,
    pub card: EmmcCard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SdhciEmmcBackendError {
    DeviceAbsent,
    InvalidController,
    InvalidBar,
    MmioWindowOverflow,
    Block(EmmcBlockError),
}

impl From<EmmcBlockError> for SdhciEmmcBackendError {
    fn from(error: EmmcBlockError) -> Self {
        Self::Block(error)
    }
}

#[cfg(not(test))]
pub fn probe_controller() -> Result<SdhciEmmcController, SdhciEmmcBackendError> {
    let report = storage_probe::run_probe();
    let controller = report
        .first_of_kind(StorageControllerKind::SdhciEmmcCandidate)
        .ok_or(SdhciEmmcBackendError::DeviceAbsent)?;
    let selected = controller_from_storage_controller(controller)?;
    storage_probe::enable_memory_space(controller);
    serial::write_line("PYTHOS:CORE:BLOCK:SDHCI_EMMC_CONTROLLER_FOUND");
    Ok(selected)
}

pub fn controller_from_storage_controller(
    controller: StorageController,
) -> Result<SdhciEmmcController, SdhciEmmcBackendError> {
    if controller.kind != StorageControllerKind::SdhciEmmcCandidate {
        return Err(SdhciEmmcBackendError::InvalidController);
    }
    let physical_mmio_base = match controller.bar0 {
        Some(MemoryBar::Memory32(base)) | Some(MemoryBar::Memory64(base)) => base,
        None => return Err(SdhciEmmcBackendError::InvalidBar),
    };
    validate_mmio_window(physical_mmio_base)?;
    Ok(SdhciEmmcController {
        controller,
        physical_mmio_base,
    })
}

#[cfg(not(test))]
pub fn initialize_device(
    controller: SdhciEmmcController,
) -> Result<SdhciEmmcBlockDevice, SdhciEmmcBackendError> {
    let mut io = MmioRegisterIo::new(SDHCI_EMMC_MMIO_VIRT);
    let card = sdhci::initialize_emmc_card(&mut io)?;
    serial::write_line("PYTHOS:CORE:BLOCK:SDHCI_EMMC_CARD_READY");
    Ok(SdhciEmmcBlockDevice {
        bus: controller.controller.bus,
        device: controller.controller.device,
        function: controller.controller.function,
        mmio_base: SDHCI_EMMC_MMIO_VIRT,
        card,
    })
}

pub fn read_sector(
    device: SdhciEmmcBlockDevice,
    lba: u64,
    out: &mut [u8; EMMC_LOGICAL_BLOCK_SIZE],
) -> Result<(), SdhciEmmcBackendError> {
    let mut io = MmioRegisterIo::new(device.mmio_base);
    sdhci::read_single_block(&mut io, device.card, lba, out).map_err(Into::into)
}

pub fn write_sector(
    device: SdhciEmmcBlockDevice,
    lba: u64,
    bytes: &[u8; EMMC_LOGICAL_BLOCK_SIZE],
) -> Result<(), SdhciEmmcBackendError> {
    let mut io = MmioRegisterIo::new(device.mmio_base);
    sdhci::write_single_block(&mut io, device.card, lba, bytes).map_err(Into::into)
}

pub const fn validate_mmio_window(base: u64) -> Result<(), SdhciEmmcBackendError> {
    if base == 0 {
        return Err(SdhciEmmcBackendError::InvalidBar);
    }
    match base.checked_add(SDHCI_EMMC_MMIO_LEN) {
        Some(_) => Ok(()),
        None => Err(SdhciEmmcBackendError::MmioWindowOverflow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SDHCI_BAR0: u64 = 0x0000_0000_E3B0_1000;

    fn controller(kind: StorageControllerKind, bar0: Option<MemoryBar>) -> StorageController {
        StorageController {
            kind,
            bus: 1,
            device: 0,
            function: 0,
            vendor_id: 0x1217,
            device_id: 0x8620,
            class_code: 0x08,
            subclass: 0x05,
            prog_if: 0x01,
            bar0,
            bar5: None,
        }
    }

    #[test]
    fn accepts_sdhci_memory_bar_and_records_physical_mmio_base() {
        let storage_controller = controller(
            StorageControllerKind::SdhciEmmcCandidate,
            Some(MemoryBar::Memory64(TEST_SDHCI_BAR0)),
        );

        let selected = controller_from_storage_controller(storage_controller).unwrap();

        assert_eq!(selected.controller, storage_controller);
        assert_eq!(selected.physical_mmio_base, TEST_SDHCI_BAR0);
    }

    #[test]
    fn rejects_non_sdhci_missing_bar_and_zero_mmio_base() {
        assert_eq!(
            controller_from_storage_controller(controller(
                StorageControllerKind::Ahci,
                Some(MemoryBar::Memory64(TEST_SDHCI_BAR0)),
            )),
            Err(SdhciEmmcBackendError::InvalidController)
        );
        assert_eq!(
            controller_from_storage_controller(controller(
                StorageControllerKind::SdhciEmmcCandidate,
                None
            )),
            Err(SdhciEmmcBackendError::InvalidBar)
        );
        assert_eq!(
            controller_from_storage_controller(controller(
                StorageControllerKind::SdhciEmmcCandidate,
                Some(MemoryBar::Memory32(0)),
            )),
            Err(SdhciEmmcBackendError::InvalidBar)
        );
    }

    #[test]
    fn rejects_sdhci_mmio_window_overflow() {
        assert_eq!(
            validate_mmio_window(u64::MAX - (SDHCI_EMMC_MMIO_LEN / 2)),
            Err(SdhciEmmcBackendError::MmioWindowOverflow)
        );
    }
}
