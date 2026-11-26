//! Build-time identity and polling for a selected native NVMe target.

use core::arch::asm;

use crate::architecture::x86_64::timer;

use super::{StorageController, read_config_u32};
use crate::block_device::write_pci_config_word;

pub(super) struct Target {
    pub vendor: u16,
    pub device: u16,
    pub subsystem: Option<u32>,
    pub serial: [u8; 20],
    pub namespace_blocks: u64,
    pub disk_guid: [u8; 16],
    pub partition_guid: [u8; 16],
    pub first_lba: u64,
    pub last_lba: u64,
    pub emulated: bool,
}

pub(super) const TARGET: Target = include!(env!("PYTHOS_NVME_WORLD_TARGET"));

const PCI_STATUS_HAS_CAPABILITIES: u32 = 1 << 20;
const PCI_CAPABILITY_POINTER: u8 = 0x34;
const PCI_SUBSYSTEM_ID: u8 = 0x2c;
const CAP_MSI: u8 = 0x05;
const CAP_MSIX: u8 = 0x11;
const MSI_ENABLE: u16 = 1;
const MSIX_ENABLE: u16 = 1 << 15;
const STALLED_TIMER_SPINS: usize = 20_000_000;

pub(super) fn matches_pci(controller: StorageController) -> bool {
    controller.vendor_id == TARGET.vendor
        && controller.device_id == TARGET.device
        && TARGET
            .subsystem
            .is_none_or(|expected| read_pci(controller, PCI_SUBSYSTEM_ID) == expected)
}

pub(super) fn disable_message_interrupts(
    controller: StorageController,
) -> Result<(), &'static str> {
    disable_pci_message_interrupts(controller.bus, controller.device, controller.function)
}

pub(crate) fn disable_pci_message_interrupts(
    bus: u8,
    device: u8,
    function: u8,
) -> Result<(), &'static str> {
    let read = |offset| read_config_u32(bus, device, function, offset);
    if read(4) & PCI_STATUS_HAS_CAPABILITIES == 0 {
        return Ok(());
    }
    let mut current = read(PCI_CAPABILITY_POINTER) as u8;
    let mut seen = 0u64;
    while current != 0 {
        if current < 0x40 || !current.is_multiple_of(4) {
            return Err("PCI_CAPABILITY_LAYOUT");
        }
        let bit = 1u64 << (current / 4);
        if seen & bit != 0 {
            return Err("PCI_CAPABILITY_LAYOUT");
        }
        seen |= bit;
        let value = read(current);
        let enable = match value as u8 {
            CAP_MSI => MSI_ENABLE,
            CAP_MSIX => MSIX_ENABLE,
            _ => 0,
        };
        let control = (value >> 16) as u16;
        if control & enable != 0 {
            write_pci_config_word(bus, device, function, current + 2, control & !enable);
            if (read(current) >> 16) as u16 & enable != 0 {
                return Err("PCI_MESSAGE_INTERRUPT");
            }
        }
        current = (value >> 8) as u8;
    }
    Ok(())
}

fn read_pci(controller: StorageController, offset: u8) -> u32 {
    read_config_u32(
        controller.bus,
        controller.device,
        controller.function,
        offset,
    )
}

pub(super) fn start_clock() -> Result<(), &'static str> {
    with_timer_interrupts(timer::initialize).map_err(|_| "NVME_TIMER_START")
}

pub(super) fn poll<T>(
    milliseconds: u64,
    timeout: &'static str,
    mut step: impl FnMut() -> Result<Option<T>, &'static str>,
) -> Result<T, &'static str> {
    with_timer_interrupts(|| {
        let start = timer::ticks();
        let limit = (milliseconds * u64::from(timer::TIMER_HZ)).div_ceil(1000) + 1;
        let mut last_tick = start;
        let mut spins = 0;
        loop {
            if let Some(value) = step()? {
                return Ok(value);
            }
            let tick = timer::ticks();
            if tick.wrapping_sub(start) >= limit {
                return Err(timeout);
            }
            if tick != last_tick {
                last_tick = tick;
                spins = 0;
            } else {
                spins += 1;
                if spins == STALLED_TIMER_SPINS {
                    return Err("NVME_TIMER_STALLED");
                }
            }
            core::hint::spin_loop();
        }
    })
}

fn with_timer_interrupts<T>(run: impl FnOnce() -> T) -> T {
    const INTERRUPT_FLAG: u64 = 1 << 9;
    let flags: u64;
    // SAFETY: the world boot has installed its IDT and masked the PIC. Each
    // call runs on the single CPU's retained mapped kernel stack. PUSHFQ uses
    // one aligned 8-byte stack slot owned by this call. STI permits the PIT
    // and, after input initialization, the PS/2 handlers. Neither can touch
    // the NVMe queue. Enabling IRQs under another root could fault or let a
    // scheduler interrupt a DMA request.
    unsafe { asm!("pushfq", "pop {flags}", "sti", flags = out(reg) flags) };
    let result = run();
    if flags & INTERRUPT_FLAG == 0 {
        // SAFETY: restore the same CPU's original interrupt state on the
        // retained kernel stack. CLI uses no pointer or mapped span and
        // completes before the world switches address spaces. Omitting it
        // could let an IRQ observe a partially switched root.
        unsafe { asm!("cli", options(nostack)) };
    }
    result
}
