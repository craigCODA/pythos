//! Read-only SDHCI/eMMC controller register snapshot for hardware bring-up.
//!
//! This module is not a block driver. It validates an already-discovered SDHCI
//! PCI function and reads a fixed set of host-controller registers without
//! programming power, clocks, commands, interrupts, reset, DMA, or media state.

use crate::storage_probe::{MemoryBar, StorageController, StorageControllerKind};

pub const SDHCI_PRESENT_STATE_OFFSET: u64 = 0x24;
pub const SDHCI_CAPABILITIES_LOW_OFFSET: u64 = 0x40;
pub const SDHCI_CAPABILITIES_HIGH_OFFSET: u64 = 0x44;
pub const SDHCI_MAX_CURRENT_CAPABILITIES_OFFSET: u64 = 0x48;
pub const SDHCI_SLOT_INTERRUPT_STATUS_OFFSET: u64 = 0xFC;
pub const SDHCI_REGISTER_WINDOW_LEN: u64 = 0x100;

const ONE_GIB: u64 = 1024 * 1024 * 1024;
const LOADER_IDENTITY_LOWER_BOUND: u64 = 0x0020_0000;
const LOADER_IDENTITY_UPPER_BOUND_EXCLUSIVE: u64 = 512 * ONE_GIB;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SdhciProbeError {
    NotSdhci,
    MissingBar0,
    Bar0OutsideLoaderIdentityMap,
    RegisterWindowOverflow,
}

impl SdhciProbeError {
    #[cfg_attr(test, allow(dead_code))]
    pub const fn marker(self) -> &'static str {
        match self {
            Self::NotSdhci => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:NOT_SDHCI",
            Self::MissingBar0 => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:MISSING_BAR0",
            Self::Bar0OutsideLoaderIdentityMap => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:BAR0_OUTSIDE_LOADER_IDENTITY_MAP"
            }
            Self::RegisterWindowOverflow => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:REGISTER_WINDOW_OVERFLOW"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciRegisterWindow {
    bar0_base: u64,
    length: u64,
}

impl SdhciRegisterWindow {
    pub const fn bar0_base(self) -> u64 {
        self.bar0_base
    }

    pub const fn length(self) -> u64 {
        self.length
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciRegisterSnapshot {
    pub bar0_base: u64,
    pub present_state: u32,
    pub capabilities_low: u32,
    pub capabilities_high: u32,
    pub max_current_capabilities: u32,
    pub slot_interrupt_status: u16,
    pub host_controller_version: u16,
}

#[cfg_attr(test, allow(dead_code))]
pub fn snapshot_controller(
    controller: StorageController,
) -> Result<SdhciRegisterSnapshot, SdhciProbeError> {
    let window = prepare_register_window(controller)?;
    let mut snapshot = read_snapshot_from_mapped_window(window.bar0_base())?;
    snapshot.bar0_base = window.bar0_base();
    Ok(snapshot)
}

pub fn prepare_register_window(
    controller: StorageController,
) -> Result<SdhciRegisterWindow, SdhciProbeError> {
    if controller.kind != StorageControllerKind::SdhciEmmcCandidate {
        return Err(SdhciProbeError::NotSdhci);
    }
    let bar0_base = match controller.bar0 {
        Some(MemoryBar::Memory32(base)) | Some(MemoryBar::Memory64(base)) => base,
        None => return Err(SdhciProbeError::MissingBar0),
    };
    let Some(end) = bar0_base.checked_add(SDHCI_REGISTER_WINDOW_LEN) else {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    };
    if bar0_base < LOADER_IDENTITY_LOWER_BOUND || end > LOADER_IDENTITY_UPPER_BOUND_EXCLUSIVE {
        return Err(SdhciProbeError::Bar0OutsideLoaderIdentityMap);
    }
    Ok(SdhciRegisterWindow {
        bar0_base,
        length: SDHCI_REGISTER_WINDOW_LEN,
    })
}

pub fn read_snapshot_from_mapped_window(
    mapped_base: u64,
) -> Result<SdhciRegisterSnapshot, SdhciProbeError> {
    let present_state = read_u32(mapped_base, SDHCI_PRESENT_STATE_OFFSET)?;
    let capabilities_low = read_u32(mapped_base, SDHCI_CAPABILITIES_LOW_OFFSET)?;
    let capabilities_high = read_u32(mapped_base, SDHCI_CAPABILITIES_HIGH_OFFSET)?;
    let max_current_capabilities = read_u32(mapped_base, SDHCI_MAX_CURRENT_CAPABILITIES_OFFSET)?;
    let slot_and_version = read_u32(mapped_base, SDHCI_SLOT_INTERRUPT_STATUS_OFFSET)?;

    Ok(SdhciRegisterSnapshot {
        bar0_base: mapped_base,
        present_state,
        capabilities_low,
        capabilities_high,
        max_current_capabilities,
        slot_interrupt_status: (slot_and_version & 0xFFFF) as u16,
        host_controller_version: (slot_and_version >> 16) as u16,
    })
}

fn read_u32(mapped_base: u64, offset: u64) -> Result<u32, SdhciProbeError> {
    let Some(end) = offset.checked_add(4) else {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    };
    if end > SDHCI_REGISTER_WINDOW_LEN {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    }
    let Some(address) = mapped_base.checked_add(offset) else {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    };

    // SAFETY:
    // 1. Invariant: `address` names a 32-bit SDHCI register inside the fixed
    //    0x100-byte snapshot window, and the caller supplies a mapped MMIO
    //    base for the SDHCI BAR0 window.
    // 2. Established by: hardware-probe calls `prepare_register_window`, which
    //    accepts only an SDHCI/eMMC candidate BAR0 in the loader identity range;
    //    tests pass a host-owned backing array to this helper directly.
    // 3. Lifetime: the loader-built identity mapping remains active for the
    //    whole hardware-probe path, which halts before VM replacement.
    // 4. Pointer ownership: the register block is device-owned; volatile reads
    //    observe it without taking ownership or mutating controller state.
    // 5. Alignment: every fixed offset used by this module is 4-byte aligned.
    // 6. Mapped length: `offset + 4 <= SDHCI_REGISTER_WINDOW_LEN`, checked
    //    above, and the BAR0 window is at least that span for this probe.
    // 7. Concurrency: hardware-probe boot is single-core, pre-userspace, and
    //    does not enable SDHCI interrupts or DMA.
    // 8. Violation: a bad mapping or bogus BAR can fault before the screen
    //    fallback; range validation narrows that risk before MMIO access.
    Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn validates_sdhci_bar0_window_in_loader_identity_range() {
        let window = prepare_register_window(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            Some(MemoryBar::Memory64(0x0000_0000_E3B0_1000)),
        ))
        .unwrap();

        assert_eq!(window.bar0_base(), 0x0000_0000_E3B0_1000);
        assert_eq!(window.length(), SDHCI_REGISTER_WINDOW_LEN);

        let high_window = prepare_register_window(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            Some(MemoryBar::Memory64(0x0000_0001_E3B0_1000)),
        ))
        .unwrap();
        assert_eq!(high_window.bar0_base(), 0x0000_0001_E3B0_1000);
    }

    #[test]
    fn rejects_non_sdhci_or_missing_bar0_before_mmio_read() {
        assert_eq!(
            prepare_register_window(controller(
                StorageControllerKind::Nvme,
                Some(MemoryBar::Memory64(0x0000_0000_E3B0_1000)),
            )),
            Err(SdhciProbeError::NotSdhci)
        );
        assert_eq!(
            prepare_register_window(controller(StorageControllerKind::SdhciEmmcCandidate, None,)),
            Err(SdhciProbeError::MissingBar0)
        );
    }

    #[test]
    fn rejects_bar0_windows_outside_loader_identity_mapping_or_overflowing() {
        assert_eq!(
            prepare_register_window(controller(
                StorageControllerKind::SdhciEmmcCandidate,
                Some(MemoryBar::Memory32(0x0000_0000_001F_F000)),
            )),
            Err(SdhciProbeError::Bar0OutsideLoaderIdentityMap)
        );
        assert_eq!(
            prepare_register_window(controller(
                StorageControllerKind::SdhciEmmcCandidate,
                Some(MemoryBar::Memory64(0xFFFF_FFFF_FFFF_FF80)),
            )),
            Err(SdhciProbeError::RegisterWindowOverflow)
        );
    }

    #[test]
    fn reads_fixed_snapshot_registers_without_mutating_the_backing_window() {
        let mut registers = [0u32; 64];
        registers[(SDHCI_PRESENT_STATE_OFFSET / 4) as usize] = 0x1122_3344;
        registers[(SDHCI_CAPABILITIES_LOW_OFFSET / 4) as usize] = 0x5566_7788;
        registers[(SDHCI_CAPABILITIES_HIGH_OFFSET / 4) as usize] = 0x99AA_BBCC;
        registers[(SDHCI_MAX_CURRENT_CAPABILITIES_OFFSET / 4) as usize] = 0xDDEE_F001;
        registers[(SDHCI_SLOT_INTERRUPT_STATUS_OFFSET / 4) as usize] = 0x0203_0001;
        let before = registers;

        let snapshot = read_snapshot_from_mapped_window(registers.as_ptr() as u64).unwrap();

        assert_eq!(registers, before);
        assert_eq!(snapshot.present_state, 0x1122_3344);
        assert_eq!(snapshot.capabilities_low, 0x5566_7788);
        assert_eq!(snapshot.capabilities_high, 0x99AA_BBCC);
        assert_eq!(snapshot.max_current_capabilities, 0xDDEE_F001);
        assert_eq!(snapshot.slot_interrupt_status, 0x0001);
        assert_eq!(snapshot.host_controller_version, 0x0203);
    }
}
