//! Bounded SDHCI/eMMC controller probing for hardware bring-up.
//!
//! This module is not a block driver. The snapshot path validates an
//! already-discovered SDHCI PCI function and reads a fixed set of
//! host-controller registers. The initialization path performs only host reset,
//! internal clock enable, and bus-power selection. It never writes command,
//! argument, transfer, data, DMA, ADMA, or block-count registers.

use crate::storage_probe::{MemoryBar, StorageController, StorageControllerKind};

pub const SDHCI_DMA_ADDRESS_OFFSET: u64 = 0x00;
pub const SDHCI_BLOCK_SIZE_OFFSET: u64 = 0x04;
pub const SDHCI_BLOCK_COUNT_OFFSET: u64 = 0x06;
pub const SDHCI_ARGUMENT_OFFSET: u64 = 0x08;
pub const SDHCI_TRANSFER_MODE_OFFSET: u64 = 0x0C;
pub const SDHCI_COMMAND_OFFSET: u64 = 0x0E;
pub const SDHCI_RESPONSE_OFFSET: u64 = 0x10;
pub const SDHCI_BUFFER_DATA_PORT_OFFSET: u64 = 0x20;
pub const SDHCI_PRESENT_STATE_OFFSET: u64 = 0x24;
pub const SDHCI_POWER_CONTROL_OFFSET: u64 = 0x29;
pub const SDHCI_CLOCK_CONTROL_OFFSET: u64 = 0x2C;
pub const SDHCI_SOFTWARE_RESET_OFFSET: u64 = 0x2F;
pub const SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET: u64 = 0x30;
pub const SDHCI_ERROR_INTERRUPT_STATUS_OFFSET: u64 = 0x32;
pub const SDHCI_CAPABILITIES_LOW_OFFSET: u64 = 0x40;
pub const SDHCI_CAPABILITIES_HIGH_OFFSET: u64 = 0x44;
pub const SDHCI_MAX_CURRENT_CAPABILITIES_OFFSET: u64 = 0x48;
pub const SDHCI_ADMA_SYSTEM_ADDRESS_OFFSET: u64 = 0x58;
pub const SDHCI_SLOT_INTERRUPT_STATUS_OFFSET: u64 = 0xFC;
pub const SDHCI_REGISTER_WINDOW_LEN: u64 = 0x100;

pub const SDHCI_CAPABILITIES_VOLTAGE_33: u32 = 1 << 24;
pub const SDHCI_CAPABILITIES_VOLTAGE_30: u32 = 1 << 25;
pub const SDHCI_CAPABILITIES_VOLTAGE_18: u32 = 1 << 26;

pub const SDHCI_POWER_BUS_ON: u8 = 0x01;
pub const SDHCI_POWER_VOLTAGE_33: u8 = 0x0E;
pub const SDHCI_POWER_VOLTAGE_30: u8 = 0x0C;
pub const SDHCI_POWER_VOLTAGE_18: u8 = 0x0A;

pub const SDHCI_CLOCK_INTERNAL_ENABLE: u16 = 1 << 0;
pub const SDHCI_CLOCK_INTERNAL_STABLE: u16 = 1 << 1;

pub const SDHCI_SOFTWARE_RESET_ALL: u8 = 1 << 0;
pub const SDHCI_INIT_POLL_LIMIT: usize = 100_000;

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
pub enum SdhciInitializationError {
    Probe(SdhciProbeError),
    ResetTimeout,
    ClockStableTimeout,
    UnsupportedVoltage,
    RegisterWindowOverflow,
}

impl SdhciInitializationError {
    #[cfg_attr(test, allow(dead_code))]
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Probe(error) => error.marker(),
            Self::ResetTimeout => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:RESET_TIMEOUT",
            Self::ClockStableTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:CLOCK_STABLE_TIMEOUT"
            }
            Self::UnsupportedVoltage => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:UNSUPPORTED_VOLTAGE",
            Self::RegisterWindowOverflow => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:REGISTER_WINDOW_OVERFLOW"
            }
        }
    }
}

impl From<SdhciProbeError> for SdhciInitializationError {
    fn from(error: SdhciProbeError) -> Self {
        Self::Probe(error)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciInitializationReport {
    pub bar0_base: u64,
    pub reset_control: u8,
    pub clock_control: u16,
    pub power_control: u8,
    pub present_state: u32,
    pub normal_interrupt_status: u16,
    pub error_interrupt_status: u16,
}

pub trait SdhciRegisterIo {
    fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError>;
    fn read_u16(&mut self, offset: u64) -> Result<u16, SdhciInitializationError>;
    fn read_u32(&mut self, offset: u64) -> Result<u32, SdhciInitializationError>;
    fn write_u8(&mut self, offset: u64, value: u8) -> Result<(), SdhciInitializationError>;
    fn write_u16(&mut self, offset: u64, value: u16) -> Result<(), SdhciInitializationError>;
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

#[cfg_attr(test, allow(dead_code))]
pub fn initialize_controller(
    controller: StorageController,
) -> Result<SdhciInitializationReport, SdhciInitializationError> {
    let window = prepare_register_window(controller)?;
    let mut io = SdhciMmioWindow {
        mapped_base: window.bar0_base(),
    };
    let mut report = initialize_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
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

pub fn initialize_with_io(
    io: &mut impl SdhciRegisterIo,
) -> Result<SdhciInitializationReport, SdhciInitializationError> {
    io.write_u8(SDHCI_SOFTWARE_RESET_OFFSET, SDHCI_SOFTWARE_RESET_ALL)?;
    let mut reset_control = SDHCI_SOFTWARE_RESET_ALL;
    for _ in 0..SDHCI_INIT_POLL_LIMIT {
        reset_control = io.read_u8(SDHCI_SOFTWARE_RESET_OFFSET)?;
        if reset_control & SDHCI_SOFTWARE_RESET_ALL == 0 {
            break;
        }
    }
    if reset_control & SDHCI_SOFTWARE_RESET_ALL != 0 {
        return Err(SdhciInitializationError::ResetTimeout);
    }

    io.write_u16(SDHCI_CLOCK_CONTROL_OFFSET, SDHCI_CLOCK_INTERNAL_ENABLE)?;
    let mut clock_control = SDHCI_CLOCK_INTERNAL_ENABLE;
    for _ in 0..SDHCI_INIT_POLL_LIMIT {
        clock_control = io.read_u16(SDHCI_CLOCK_CONTROL_OFFSET)?;
        if clock_control & SDHCI_CLOCK_INTERNAL_STABLE != 0 {
            break;
        }
    }
    if clock_control & SDHCI_CLOCK_INTERNAL_STABLE == 0 {
        return Err(SdhciInitializationError::ClockStableTimeout);
    }

    let capabilities_low = io.read_u32(SDHCI_CAPABILITIES_LOW_OFFSET)?;
    let power_control = select_power_control_value(capabilities_low)?;
    io.write_u8(SDHCI_POWER_CONTROL_OFFSET, power_control)?;

    Ok(SdhciInitializationReport {
        bar0_base: 0,
        reset_control: io.read_u8(SDHCI_SOFTWARE_RESET_OFFSET)?,
        clock_control: io.read_u16(SDHCI_CLOCK_CONTROL_OFFSET)?,
        power_control: io.read_u8(SDHCI_POWER_CONTROL_OFFSET)?,
        present_state: io.read_u32(SDHCI_PRESENT_STATE_OFFSET)?,
        normal_interrupt_status: io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?,
        error_interrupt_status: io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?,
    })
}

pub const fn select_power_control_value(
    capabilities_low: u32,
) -> Result<u8, SdhciInitializationError> {
    if capabilities_low & SDHCI_CAPABILITIES_VOLTAGE_33 != 0 {
        Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33)
    } else if capabilities_low & SDHCI_CAPABILITIES_VOLTAGE_30 != 0 {
        Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_30)
    } else if capabilities_low & SDHCI_CAPABILITIES_VOLTAGE_18 != 0 {
        Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_18)
    } else {
        Err(SdhciInitializationError::UnsupportedVoltage)
    }
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

struct SdhciMmioWindow {
    mapped_base: u64,
}

impl SdhciRegisterIo for SdhciMmioWindow {
    fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 1)?;
        // SAFETY:
        // 1. Invariant: `address` is a byte-wide register within the fixed
        //    SDHCI BAR0 register window.
        // 2. Established by: `initialize_controller` accepts only an
        //    SDHCI/eMMC candidate whose BAR0 passed `prepare_register_window`;
        //    `mmio_address` bounds-checks this exact offset and width.
        // 3. Lifetime: hardware-probe halts before replacing the loader's
        //    current mappings, so the BAR0 mapping remains live for the read.
        // 4. Pointer ownership: the controller owns the register; volatile
        //    access observes device state without aliasing Rust-owned memory.
        // 5. Alignment: byte accesses have no stricter alignment requirement.
        // 6. Mapped length: `offset + 1 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: this boot path is single-core and SDHCI interrupts
        //    and DMA remain disabled.
        // 8. Violation: a bad BAR or mapping can fault; typed BAR validation
        //    and bounded offsets narrow the MMIO surface before access.
        // SAFETY: the detailed invariant above applies to this volatile byte read.
        Ok(unsafe { core::ptr::read_volatile(address as *const u8) })
    }

    fn read_u16(&mut self, offset: u64) -> Result<u16, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 2)?;
        // SAFETY:
        // 1. Invariant: `address` is a 16-bit SDHCI register inside BAR0.
        // 2. Established by: `prepare_register_window` validates the BAR0 span
        //    and `mmio_address` bounds-checks the requested register.
        // 3. Lifetime: the loader identity mapping remains active until this
        //    hardware-probe path halts.
        // 4. Pointer ownership: volatile access reads a device-owned register
        //    and does not create a Rust reference to MMIO memory.
        // 5. Alignment: all 16-bit offsets used by this init path are
        //    specification-aligned.
        // 6. Mapped length: `offset + 2 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: no SDHCI interrupt, DMA, or second CPU can race this
        //    probe path.
        // 8. Violation: invalid firmware enumeration could fault; the typed
        //    checks prevent out-of-window volatile reads.
        // SAFETY: the detailed invariant above applies to this volatile word read.
        Ok(unsafe { core::ptr::read_volatile(address as *const u16) })
    }

    fn read_u32(&mut self, offset: u64) -> Result<u32, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 4)?;
        // SAFETY:
        // 1. Invariant: `address` is a 32-bit SDHCI register inside BAR0.
        // 2. Established by: `prepare_register_window` validates BAR0 and
        //    `mmio_address` checks that the requested register fits the fixed
        //    window.
        // 3. Lifetime: the loader identity mapping remains active through this
        //    halt-only hardware-probe path.
        // 4. Pointer ownership: the device owns the register block; volatile
        //    access does not borrow or mutate Rust-owned memory.
        // 5. Alignment: all 32-bit offsets used by this init path are
        //    specification-aligned.
        // 6. Mapped length: `offset + 4 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: SDHCI interrupts, DMA, and media commands are not
        //    enabled, and boot remains single-core here.
        // 8. Violation: malformed BAR data could fault; range validation
        //    narrows this before the volatile read.
        // SAFETY: the detailed invariant above applies to this volatile dword read.
        Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
    }

    fn write_u8(&mut self, offset: u64, value: u8) -> Result<(), SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 1)?;
        // SAFETY:
        // 1. Invariant: `address` is a byte-wide SDHCI control register inside
        //    BAR0 and this init path writes only software-reset or bus-power
        //    controls.
        // 2. Established by: callers use fixed offsets checked by
        //    `mmio_address`; media-path offsets are never passed by
        //    `initialize_with_io`.
        // 3. Lifetime: the loader identity mapping remains valid until the
        //    probe halts.
        // 4. Pointer ownership: volatile write programs a device-owned
        //    register and never aliases Rust-owned memory.
        // 5. Alignment: byte accesses have no stricter alignment requirement.
        // 6. Mapped length: `offset + 1 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: boot is single-core and this path enables neither
        //    SDHCI interrupts nor DMA.
        // 8. Violation: an invalid BAR could fault or program the wrong
        //    device; controller classification and BAR validation reduce that
        //    risk before writes.
        // SAFETY: the detailed invariant above applies to this volatile byte write.
        unsafe { core::ptr::write_volatile(address as *mut u8, value) };
        Ok(())
    }

    fn write_u16(&mut self, offset: u64, value: u16) -> Result<(), SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 2)?;
        // SAFETY:
        // 1. Invariant: `address` is a 16-bit SDHCI control register inside
        //    BAR0 and this init path writes only the internal-clock enable bit.
        // 2. Established by: callers use the fixed clock-control offset, and
        //    `mmio_address` bounds-checks the register access.
        // 3. Lifetime: the loader identity mapping remains valid through the
        //    halt-only hardware-probe boot.
        // 4. Pointer ownership: volatile write targets device-owned MMIO, not
        //    Rust-owned memory.
        // 5. Alignment: the clock-control offset is 16-bit aligned.
        // 6. Mapped length: `offset + 2 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: SDHCI interrupts and DMA stay disabled.
        // 8. Violation: invalid enumeration could fault or program the wrong
        //    register; typed PCI/BAR filtering narrows the write surface.
        // SAFETY: the detailed invariant above applies to this volatile word write.
        unsafe { core::ptr::write_volatile(address as *mut u16, value) };
        Ok(())
    }
}

fn mmio_address(
    mapped_base: u64,
    offset: u64,
    width: u64,
) -> Result<u64, SdhciInitializationError> {
    let Some(end) = offset.checked_add(width) else {
        return Err(SdhciInitializationError::RegisterWindowOverflow);
    };
    if end > SDHCI_REGISTER_WINDOW_LEN {
        return Err(SdhciInitializationError::RegisterWindowOverflow);
    }
    mapped_base
        .checked_add(offset)
        .ok_or(SdhciInitializationError::RegisterWindowOverflow)
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
    // SAFETY: the detailed invariant above applies to this volatile dword read.
    Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SDHCI_BAR0: u64 = 0x0000_0000_E3B0_1000;
    const SAMPLE_HIGH_SDHCI_BAR0: u64 = 0x0000_0001_E3B0_1000;

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
            Some(MemoryBar::Memory64(SAMPLE_SDHCI_BAR0)),
        ))
        .unwrap();

        assert_eq!(window.bar0_base(), SAMPLE_SDHCI_BAR0);
        assert_eq!(window.length(), SDHCI_REGISTER_WINDOW_LEN);

        let high_window = prepare_register_window(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            Some(MemoryBar::Memory64(SAMPLE_HIGH_SDHCI_BAR0)),
        ))
        .unwrap();
        assert_eq!(high_window.bar0_base(), SAMPLE_HIGH_SDHCI_BAR0);
    }

    #[test]
    fn rejects_non_sdhci_or_missing_bar0_before_mmio_read() {
        assert_eq!(
            prepare_register_window(controller(
                StorageControllerKind::Nvme,
                Some(MemoryBar::Memory64(SAMPLE_SDHCI_BAR0)),
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

    #[test]
    fn selects_supported_voltage_without_touching_media_registers() {
        assert_eq!(
            select_power_control_value(SDHCI_CAPABILITIES_VOLTAGE_33),
            Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33)
        );
        assert_eq!(
            select_power_control_value(SDHCI_CAPABILITIES_VOLTAGE_30),
            Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_30)
        );
        assert_eq!(
            select_power_control_value(SDHCI_CAPABILITIES_VOLTAGE_18),
            Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_18)
        );
        assert_eq!(
            select_power_control_value(0),
            Err(SdhciInitializationError::UnsupportedVoltage)
        );
    }

    #[test]
    fn initializes_reset_internal_clock_and_power_with_ordered_writes() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33);
        io.reset_reads_before_clear = 2;
        io.clock_reads_before_stable = 2;

        let report = initialize_with_io(&mut io).unwrap();

        assert_eq!(report.reset_control, 0);
        assert_eq!(
            report.clock_control,
            SDHCI_CLOCK_INTERNAL_ENABLE | SDHCI_CLOCK_INTERNAL_STABLE
        );
        assert_eq!(
            report.power_control,
            SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33
        );
        assert_eq!(
            io.writes,
            [
                FakeWrite::U8(SDHCI_SOFTWARE_RESET_OFFSET, SDHCI_SOFTWARE_RESET_ALL),
                FakeWrite::U16(SDHCI_CLOCK_CONTROL_OFFSET, SDHCI_CLOCK_INTERNAL_ENABLE),
                FakeWrite::U8(
                    SDHCI_POWER_CONTROL_OFFSET,
                    SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33,
                ),
            ]
        );
        assert!(io.writes.iter().all(|write| !write.touches_media_path()));
    }

    #[test]
    fn reset_and_clock_polling_have_typed_timeouts() {
        let mut reset_timeout = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33);
        reset_timeout.reset_reads_before_clear = SDHCI_INIT_POLL_LIMIT + 1;
        assert_eq!(
            initialize_with_io(&mut reset_timeout),
            Err(SdhciInitializationError::ResetTimeout)
        );

        let mut clock_timeout = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33);
        clock_timeout.clock_reads_before_stable = SDHCI_INIT_POLL_LIMIT + 1;
        assert_eq!(
            initialize_with_io(&mut clock_timeout),
            Err(SdhciInitializationError::ClockStableTimeout)
        );
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FakeWrite {
        U8(u64, u8),
        U16(u64, u16),
    }

    impl FakeWrite {
        fn touches_media_path(self) -> bool {
            let offset = match self {
                Self::U8(offset, _) | Self::U16(offset, _) => offset,
            };
            matches!(
                offset,
                SDHCI_ARGUMENT_OFFSET
                    | SDHCI_TRANSFER_MODE_OFFSET
                    | SDHCI_COMMAND_OFFSET
                    | SDHCI_RESPONSE_OFFSET
                    | SDHCI_BUFFER_DATA_PORT_OFFSET
                    | SDHCI_BLOCK_SIZE_OFFSET
                    | SDHCI_BLOCK_COUNT_OFFSET
                    | SDHCI_DMA_ADDRESS_OFFSET
                    | SDHCI_ADMA_SYSTEM_ADDRESS_OFFSET
            )
        }
    }

    struct FakeSdhciIo {
        capabilities_low: u32,
        reset_control: u8,
        clock_control: u16,
        power_control: u8,
        reset_reads_before_clear: usize,
        clock_reads_before_stable: usize,
        writes: [FakeWrite; 3],
        write_count: usize,
    }

    impl FakeSdhciIo {
        fn new(capabilities_low: u32) -> Self {
            Self {
                capabilities_low,
                reset_control: 0,
                clock_control: 0,
                power_control: 0,
                reset_reads_before_clear: 0,
                clock_reads_before_stable: 0,
                writes: [FakeWrite::U8(0, 0); 3],
                write_count: 0,
            }
        }

        fn push_write(&mut self, write: FakeWrite) {
            self.writes[self.write_count] = write;
            self.write_count += 1;
        }
    }

    impl SdhciRegisterIo for FakeSdhciIo {
        fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_SOFTWARE_RESET_OFFSET => {
                    if self.reset_reads_before_clear > 0 {
                        self.reset_reads_before_clear -= 1;
                        self.reset_control
                    } else {
                        self.reset_control = 0;
                        0
                    }
                }
                SDHCI_POWER_CONTROL_OFFSET => self.power_control,
                _ => 0,
            })
        }

        fn read_u16(&mut self, offset: u64) -> Result<u16, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_CLOCK_CONTROL_OFFSET => {
                    if self.clock_reads_before_stable > 0 {
                        self.clock_reads_before_stable -= 1;
                        self.clock_control
                    } else {
                        self.clock_control |= SDHCI_CLOCK_INTERNAL_STABLE;
                        self.clock_control
                    }
                }
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET | SDHCI_ERROR_INTERRUPT_STATUS_OFFSET => 0,
                _ => 0,
            })
        }

        fn read_u32(&mut self, offset: u64) -> Result<u32, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_PRESENT_STATE_OFFSET => 0x01FF_00F0,
                SDHCI_CAPABILITIES_LOW_OFFSET => self.capabilities_low,
                _ => 0,
            })
        }

        fn write_u8(&mut self, offset: u64, value: u8) -> Result<(), SdhciInitializationError> {
            self.push_write(FakeWrite::U8(offset, value));
            match offset {
                SDHCI_SOFTWARE_RESET_OFFSET => self.reset_control = value,
                SDHCI_POWER_CONTROL_OFFSET => self.power_control = value,
                _ => {}
            }
            Ok(())
        }

        fn write_u16(&mut self, offset: u64, value: u16) -> Result<(), SdhciInitializationError> {
            self.push_write(FakeWrite::U16(offset, value));
            if offset == SDHCI_CLOCK_CONTROL_OFFSET {
                self.clock_control = value;
            }
            Ok(())
        }
    }
}
