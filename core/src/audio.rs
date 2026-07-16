//! Phase 6 AC97 audio device selection.
#![cfg_attr(test, allow(dead_code))]

use crate::serial;
#[cfg(not(test))]
use core::arch::asm;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_BUS: u8 = 0;
const PCI_DEVICE_COUNT: u8 = 32;
const PCI_FUNCTION_COUNT: u8 = 8;
const PCI_VENDOR_INVALID: u16 = 0xFFFF;
const PCI_COMMAND_OFFSET: u8 = 0x04;
const PCI_CLASS_REVISION_OFFSET: u8 = 0x08;
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_BAR1_OFFSET: u8 = 0x14;
const PCI_COMMAND_IO_SPACE: u16 = 1 << 0;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;
const PCI_CLASS_MULTIMEDIA: u8 = 0x04;
const PCI_SUBCLASS_AUDIO: u8 = 0x01;
const IO_BAR_FLAG: u32 = 1;
const IO_BAR_MASK: u32 = !0x3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioError {
    InvalidBar,
    CommandRejected,
    PortRangeOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioDeviceSelection {
    Ac97(Ac97Device),
    Absent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ac97Device {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub mixer_base: u16,
    pub bus_master_base: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioDriver {
    Ac97(Ac97Driver),
    Silent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ac97Driver {
    device: Ac97Device,
    sample_rate_hz: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PciFunction {
    bus: u8,
    device: u8,
    function: u8,
    vendor_device: u32,
    command_status: u32,
    class_revision: u32,
    bar0: u32,
    bar1: u32,
}

const AC97_SAMPLE_RATE_HZ: u32 = 48_000;
const AC97_MIXER_RESET: u16 = 0x00;
const AC97_MIXER_MASTER_VOLUME: u16 = 0x02;
const AC97_MIXER_PCM_OUT_VOLUME: u16 = 0x18;
const AC97_MIXER_EXT_AUDIO_ID: u16 = 0x28;
const AC97_MIXER_EXT_AUDIO_CTRL: u16 = 0x2A;
const AC97_MIXER_PCM_FRONT_DAC_RATE: u16 = 0x2C;
const AC97_EXT_AUDIO_VARIABLE_RATE: u16 = 1 << 0;
const AC97_BUS_GLOBAL_CONTROL: u16 = 0x2C;
const AC97_GLOBAL_CONTROL_COLD_RESET: u32 = 1 << 1;

pub fn select_device() -> Result<AudioDeviceSelection, AudioError> {
    match scan_primary_bus()? {
        AudioDeviceSelection::Ac97(device) => {
            serial::write_line("PYTHOS:CORE:AUDIO:DEVICE_SELECTED");
            Ok(AudioDeviceSelection::Ac97(device))
        }
        AudioDeviceSelection::Absent => {
            serial::write_line("PYTHOS:CORE:AUDIO:DEVICE_ABSENT");
            Ok(AudioDeviceSelection::Absent)
        }
    }
}

pub fn initialize_driver(selection: AudioDeviceSelection) -> Result<AudioDriver, AudioError> {
    let driver = driver_from_selection(selection);
    match driver {
        AudioDriver::Ac97(ac97) => {
            configure_ac97(ac97.device)?;
            serial::write_line("PYTHOS:CORE:AUDIO:DRIVER");
        }
        AudioDriver::Silent => {
            serial::write_line("PYTHOS:CORE:AUDIO:DRIVER_SKIPPED");
        }
    }
    Ok(driver)
}

fn driver_from_selection(selection: AudioDeviceSelection) -> AudioDriver {
    match selection {
        AudioDeviceSelection::Ac97(device) => AudioDriver::Ac97(Ac97Driver {
            device,
            sample_rate_hz: AC97_SAMPLE_RATE_HZ,
        }),
        AudioDeviceSelection::Absent => AudioDriver::Silent,
    }
}

fn scan_primary_bus() -> Result<AudioDeviceSelection, AudioError> {
    for device in 0..PCI_DEVICE_COUNT {
        for function in 0..PCI_FUNCTION_COUNT {
            let candidate = read_function(PCI_BUS, device, function);
            if vendor(candidate.vendor_device) == PCI_VENDOR_INVALID {
                continue;
            }
            if let Some(ac97) = classify_ac97(candidate)? {
                enable_io_bus_master(candidate)?;
                return Ok(AudioDeviceSelection::Ac97(ac97));
            }
        }
    }
    Ok(AudioDeviceSelection::Absent)
}

fn classify_ac97(function: PciFunction) -> Result<Option<Ac97Device>, AudioError> {
    if class_code(function.class_revision) != PCI_CLASS_MULTIMEDIA
        || subclass(function.class_revision) != PCI_SUBCLASS_AUDIO
    {
        return Ok(None);
    }

    let Some(mixer_base) = io_bar_base(function.bar0) else {
        return Err(AudioError::InvalidBar);
    };
    let Some(bus_master_base) = io_bar_base(function.bar1) else {
        return Err(AudioError::InvalidBar);
    };
    Ok(Some(Ac97Device {
        bus: function.bus,
        device: function.device,
        function: function.function,
        mixer_base,
        bus_master_base,
    }))
}

fn enable_io_bus_master(function: PciFunction) -> Result<(), AudioError> {
    let command = (function.command_status & 0xFFFF) as u16;
    let updated = command | PCI_COMMAND_IO_SPACE | PCI_COMMAND_BUS_MASTER;
    let value = (function.command_status & 0xFFFF_0000) | u32::from(updated);
    write_config_u32(
        function.bus,
        function.device,
        function.function,
        PCI_COMMAND_OFFSET,
        value,
    );
    let readback = read_config_u32(
        function.bus,
        function.device,
        function.function,
        PCI_COMMAND_OFFSET,
    );
    let readback_command = (readback & 0xFFFF) as u16;
    if readback_command & (PCI_COMMAND_IO_SPACE | PCI_COMMAND_BUS_MASTER)
        != (PCI_COMMAND_IO_SPACE | PCI_COMMAND_BUS_MASTER)
    {
        return Err(AudioError::CommandRejected);
    }
    Ok(())
}

fn read_function(bus: u8, device: u8, function: u8) -> PciFunction {
    PciFunction {
        bus,
        device,
        function,
        vendor_device: read_config_u32(bus, device, function, 0x00),
        command_status: read_config_u32(bus, device, function, PCI_COMMAND_OFFSET),
        class_revision: read_config_u32(bus, device, function, PCI_CLASS_REVISION_OFFSET),
        bar0: read_config_u32(bus, device, function, PCI_BAR0_OFFSET),
        bar1: read_config_u32(bus, device, function, PCI_BAR1_OFFSET),
    }
}

fn vendor(vendor_device: u32) -> u16 {
    (vendor_device & 0xFFFF) as u16
}

fn class_code(class_revision: u32) -> u8 {
    (class_revision >> 24) as u8
}

fn subclass(class_revision: u32) -> u8 {
    (class_revision >> 16) as u8
}

fn io_bar_base(bar: u32) -> Option<u16> {
    if bar & IO_BAR_FLAG == 0 {
        return None;
    }
    let base = bar & IO_BAR_MASK;
    if base == 0 || base > u32::from(u16::MAX) {
        return None;
    }
    Some(base as u16)
}

fn config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xFC)
}

#[cfg(not(test))]
fn configure_ac97(device: Ac97Device) -> Result<(), AudioError> {
    let global_control = ac97_bus_port(device, AC97_BUS_GLOBAL_CONTROL)?;
    let current = inl(global_control);
    outl(global_control, current | AC97_GLOBAL_CONTROL_COLD_RESET);
    io_delay();

    outw(ac97_mixer_port(device, AC97_MIXER_RESET)?, 0);
    io_delay();
    outw(ac97_mixer_port(device, AC97_MIXER_MASTER_VOLUME)?, 0);
    outw(ac97_mixer_port(device, AC97_MIXER_PCM_OUT_VOLUME)?, 0);

    let ext_audio_id = inw(ac97_mixer_port(device, AC97_MIXER_EXT_AUDIO_ID)?);
    if ext_audio_id & AC97_EXT_AUDIO_VARIABLE_RATE != 0 {
        let ext_audio_ctrl = inw(ac97_mixer_port(device, AC97_MIXER_EXT_AUDIO_CTRL)?);
        outw(
            ac97_mixer_port(device, AC97_MIXER_EXT_AUDIO_CTRL)?,
            ext_audio_ctrl | AC97_EXT_AUDIO_VARIABLE_RATE,
        );
        outw(
            ac97_mixer_port(device, AC97_MIXER_PCM_FRONT_DAC_RATE)?,
            AC97_SAMPLE_RATE_HZ as u16,
        );
    }
    Ok(())
}

#[cfg(test)]
fn configure_ac97(_device: Ac97Device) -> Result<(), AudioError> {
    Ok(())
}

fn ac97_mixer_port(device: Ac97Device, offset: u16) -> Result<u16, AudioError> {
    device
        .mixer_base
        .checked_add(offset)
        .ok_or(AudioError::PortRangeOverflow)
}

fn ac97_bus_port(device: Ac97Device, offset: u16) -> Result<u16, AudioError> {
    device
        .bus_master_base
        .checked_add(offset)
        .ok_or(AudioError::PortRangeOverflow)
}

#[cfg(not(test))]
fn read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    outl(
        PCI_CONFIG_ADDRESS,
        config_address(bus, device, function, offset),
    );
    inl(PCI_CONFIG_DATA)
}

#[cfg(test)]
fn read_config_u32(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u32 {
    0xFFFF_FFFF
}

#[cfg(not(test))]
fn write_config_u32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    outl(
        PCI_CONFIG_ADDRESS,
        config_address(bus, device, function, offset),
    );
    outl(PCI_CONFIG_DATA, value);
}

#[cfg(test)]
fn write_config_u32(_bus: u8, _device: u8, _function: u8, _offset: u8, _value: u32) {}

#[cfg(not(test))]
fn outl(port: u16, value: u32) {
    // SAFETY:
    // 1. Invariant: `port` is either a PCI configuration I/O port or a
    //    validated AC97 bus-master register.
    // 2. Established by: private callers pass PCI config constants or derive
    //    the port from the selected AC97 I/O BAR plus a fixed register offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: boot remains single-core for this hardware scan.
    // 8. Violation: writing a wrong port could reconfigure unrelated hardware.
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(not(test))]
fn inl(port: u16) -> u32 {
    let value: u32;
    // SAFETY:
    // 1. Invariant: `port` is either the PCI configuration data port after an
    //    address selection or a validated AC97 bus-master register.
    // 2. Established by: private callers pass PCI config data after a bounded
    //    config-address write or derive the port from the selected AC97 I/O BAR.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: boot remains single-core for this hardware scan.
    // 8. Violation: reading a wrong port could observe unrelated hardware.
    unsafe {
        asm!(
            "in eax, dx",
            out("eax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(not(test))]
fn outw(port: u16, value: u16) {
    // SAFETY:
    // 1. Invariant: `port` names a validated AC97 mixer I/O register.
    // 2. Established by: callers derive it from the selected AC97 I/O BAR plus
    //    a fixed Phase 6 mixer-register offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 6 boot audio setup is single-core.
    // 8. Violation: writing the wrong port could reconfigure unrelated hardware.
    unsafe {
        asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(test)]
fn outw(_port: u16, _value: u16) {}

#[cfg(not(test))]
fn inw(port: u16) -> u16 {
    let value: u16;
    // SAFETY:
    // 1. Invariant: `port` names a validated AC97 mixer I/O register.
    // 2. Established by: callers derive it from the selected AC97 I/O BAR plus
    //    a fixed Phase 6 mixer-register offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 6 boot audio setup is single-core.
    // 8. Violation: reading the wrong port could observe unrelated hardware.
    unsafe {
        asm!(
            "in ax, dx",
            out("ax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(test)]
fn inw(_port: u16) -> u16 {
    0
}

#[cfg(not(test))]
fn io_delay() {
    for _ in 0..10_000 {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
fn io_delay() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_address_targets_primary_bus_function() {
        assert_eq!(config_address(0, 3, 2, 0x13), 0x8000_1A10);
    }

    #[test]
    fn classifies_valid_ac97_function() {
        let function = PciFunction {
            bus: 0,
            device: 5,
            function: 0,
            vendor_device: 0x2415_8086,
            command_status: 0,
            class_revision: 0x0401_0000,
            bar0: 0x1001,
            bar1: 0x2001,
        };

        assert_eq!(
            classify_ac97(function),
            Ok(Some(Ac97Device {
                bus: 0,
                device: 5,
                function: 0,
                mixer_base: 0x1000,
                bus_master_base: 0x2000,
            }))
        );
    }

    #[test]
    fn rejects_memory_bars_for_ac97_io_target() {
        let function = PciFunction {
            bus: 0,
            device: 5,
            function: 0,
            vendor_device: 0x2415_8086,
            command_status: 0,
            class_revision: 0x0401_0000,
            bar0: 0x1000,
            bar1: 0x2001,
        };

        assert_eq!(classify_ac97(function), Err(AudioError::InvalidBar));
    }

    #[test]
    fn ignores_non_audio_pci_functions() {
        let function = PciFunction {
            bus: 0,
            device: 1,
            function: 0,
            vendor_device: 0x1234_8086,
            command_status: 0,
            class_revision: 0x0101_0000,
            bar0: 0x1001,
            bar1: 0x2001,
        };

        assert_eq!(classify_ac97(function), Ok(None));
    }

    #[test]
    fn selected_ac97_device_becomes_fixed_rate_driver() {
        let device = Ac97Device {
            bus: 0,
            device: 5,
            function: 0,
            mixer_base: 0x1000,
            bus_master_base: 0x2000,
        };

        assert_eq!(
            driver_from_selection(AudioDeviceSelection::Ac97(device)),
            AudioDriver::Ac97(Ac97Driver {
                device,
                sample_rate_hz: AC97_SAMPLE_RATE_HZ,
            })
        );
    }

    #[test]
    fn absent_device_becomes_silent_driver() {
        assert_eq!(
            driver_from_selection(AudioDeviceSelection::Absent),
            AudioDriver::Silent
        );
    }
}
