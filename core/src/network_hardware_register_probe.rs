//! Pure policy and bounded hardware access for the network register probe.
//!
//! The policy selects one fixed status location for a small, explicit set of
//! controller IDs. It never enables PCI memory space, claims a resource, or
//! performs an MMIO write.

use crate::network_hardware_bar_probe::{NetworkBarLayout, PciBarKind, decode_bar_layout};
use crate::network_hardware_probe::{NetworkController, NetworkControllerKind};

pub const PCI_MEMORY_SPACE_ENABLE: u32 = 1 << 1;
pub const NETWORK_REGISTER_WINDOW_LEN: u64 = 0x1000;
pub const NETWORK_REGISTER_MMIO_VIRT: u64 = 0xFFFF_C000_1005_0000;
pub const INTEL_STATUS_REGISTER_OFFSET: u64 = 0x08;
pub const REALTEK_SYS_STATUS1_REGISTER_OFFSET: u64 = 0x00F4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterTargetKind {
    IntelDeviceStatus,
    RealtekSysStatus1,
}

impl RegisterTargetKind {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::IntelDeviceStatus => "INTEL_DEVICE_STATUS",
            Self::RealtekSysStatus1 => "REALTEK_SYS_STATUS1",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterTarget {
    pub kind: RegisterTargetKind,
    pub bar_slot: usize,
    pub register_offset: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterProbePlan {
    pub target: RegisterTarget,
    pub physical_base: u64,
    pub virtual_base: u64,
    pub mapping_len: u64,
}

impl RegisterProbePlan {
    pub const fn mapping(self) -> (u64, u64, u64) {
        (self.physical_base, self.virtual_base, self.mapping_len)
    }

    pub const fn register_virtual_address(self) -> u64 {
        self.virtual_base + self.target.register_offset
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterProbeSkip {
    UnsupportedController,
    PciMemorySpaceDisabled,
    MalformedBarLayout,
    TargetBarMissing,
    TargetBarIsIo,
    TargetBarMisaligned,
    TargetBarOverflow,
    RegisterOutsideWindow,
}

impl RegisterProbeSkip {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::UnsupportedController => "UNSUPPORTED_CONTROLLER",
            Self::PciMemorySpaceDisabled => "PCI_MEMORY_SPACE_DISABLED",
            Self::MalformedBarLayout => "MMIO_TARGET_INVALID",
            Self::TargetBarMissing => "MMIO_TARGET_INVALID",
            Self::TargetBarIsIo => "MMIO_TARGET_INVALID",
            Self::TargetBarMisaligned => "MMIO_TARGET_INVALID",
            Self::TargetBarOverflow => "MMIO_TARGET_INVALID",
            Self::RegisterOutsideWindow => "MMIO_TARGET_INVALID",
        }
    }
}

pub fn select_target(controller: NetworkController) -> Option<RegisterTarget> {
    match (controller.vendor_id, controller.device_id) {
        (0x8086, 0x100E) | (0x8086, 0x10D3) => Some(RegisterTarget {
            kind: RegisterTargetKind::IntelDeviceStatus,
            bar_slot: 0,
            register_offset: INTEL_STATUS_REGISTER_OFFSET,
        }),
        (0x10EC, 0xC82F) if controller.kind == NetworkControllerKind::OtherNetwork => {
            Some(RegisterTarget {
                kind: RegisterTargetKind::RealtekSysStatus1,
                bar_slot: 2,
                register_offset: REALTEK_SYS_STATUS1_REGISTER_OFFSET,
            })
        }
        _ => None,
    }
}

pub fn prepare_plan(
    controller: NetworkController,
    command_status: u32,
    layout: NetworkBarLayout,
) -> Result<RegisterProbePlan, RegisterProbeSkip> {
    let target = select_target(controller).ok_or(RegisterProbeSkip::UnsupportedController)?;
    if command_status & PCI_MEMORY_SPACE_ENABLE == 0 {
        return Err(RegisterProbeSkip::PciMemorySpaceDisabled);
    }
    prepare_target_plan(target, layout)
}

/// Prepare the same fixed target for the enable experiment. Unlike the
/// read-only profile, this deliberately permits a clear MSE bit so the caller
/// can perform the bounded Command-register transition before the MMIO read.
pub fn prepare_enable_plan(
    controller: NetworkController,
    _command_status: u32,
    layout: NetworkBarLayout,
) -> Result<RegisterProbePlan, RegisterProbeSkip> {
    let target = select_target(controller).ok_or(RegisterProbeSkip::UnsupportedController)?;
    prepare_target_plan(target, layout)
}

fn prepare_target_plan(
    target: RegisterTarget,
    layout: NetworkBarLayout,
) -> Result<RegisterProbePlan, RegisterProbeSkip> {
    if layout.malformed {
        return Err(RegisterProbeSkip::MalformedBarLayout);
    }
    let bar = layout.bars[target.bar_slot].ok_or(RegisterProbeSkip::TargetBarMissing)?;
    if matches!(bar.kind, PciBarKind::Io) {
        return Err(RegisterProbeSkip::TargetBarIsIo);
    }
    if bar.base & (NETWORK_REGISTER_WINDOW_LEN - 1) != 0 {
        return Err(RegisterProbeSkip::TargetBarMisaligned);
    }
    bar.base
        .checked_add(NETWORK_REGISTER_WINDOW_LEN)
        .ok_or(RegisterProbeSkip::TargetBarOverflow)?;
    target
        .register_offset
        .checked_add(core::mem::size_of::<u32>() as u64)
        .filter(|end| *end <= NETWORK_REGISTER_WINDOW_LEN)
        .ok_or(RegisterProbeSkip::RegisterOutsideWindow)?;

    Ok(RegisterProbePlan {
        target,
        physical_base: bar.base,
        virtual_base: NETWORK_REGISTER_MMIO_VIRT,
        mapping_len: NETWORK_REGISTER_WINDOW_LEN,
    })
}

#[cfg(not(test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterProbeSetup {
    Ready {
        controller: NetworkController,
        command_status: u32,
        plan: RegisterProbePlan,
    },
    Skipped {
        controller: Option<NetworkController>,
        command_status: Option<u32>,
        reason: RegisterProbeSkip,
    },
}

#[cfg(not(test))]
pub fn discover_setup() -> RegisterProbeSetup {
    let report = crate::network_hardware_probe::run_probe();
    let Some(controller) = report.preferred_controller() else {
        return RegisterProbeSetup::Skipped {
            controller: None,
            command_status: None,
            reason: RegisterProbeSkip::UnsupportedController,
        };
    };
    let command_status = crate::network_hardware_probe::read_controller_command_status(controller);
    let layout = decode_bar_layout(crate::network_hardware_probe::read_controller_bar_dwords(
        controller,
    ));
    match prepare_plan(controller, command_status, layout) {
        Ok(plan) => RegisterProbeSetup::Ready {
            controller,
            command_status,
            plan,
        },
        Err(reason) => RegisterProbeSetup::Skipped {
            controller: Some(controller),
            command_status: Some(command_status),
            reason,
        },
    }
}

#[cfg(all(not(test), feature = "network-hardware-register-enable-probe"))]
pub fn discover_enable_setup() -> RegisterProbeSetup {
    let report = crate::network_hardware_probe::run_probe();
    let Some(controller) = report.preferred_controller() else {
        return RegisterProbeSetup::Skipped {
            controller: None,
            command_status: None,
            reason: RegisterProbeSkip::UnsupportedController,
        };
    };
    let command_status = crate::network_hardware_probe::read_controller_command_status(controller);
    let layout = decode_bar_layout(crate::network_hardware_probe::read_controller_bar_dwords(
        controller,
    ));
    match prepare_enable_plan(controller, command_status, layout) {
        Ok(plan) => RegisterProbeSetup::Ready {
            controller,
            command_status,
            plan,
        },
        Err(reason) => RegisterProbeSetup::Skipped {
            controller: Some(controller),
            command_status: Some(command_status),
            reason,
        },
    }
}

#[cfg(not(test))]
pub unsafe fn read_register(plan: RegisterProbePlan) -> u32 {
    // SAFETY: the caller has activated the root containing the dedicated
    // cache-disabled mapping, and `prepare_plan` bounds the fixed offset to
    // the mapped 4 KiB window. This function performs one volatile load only.
    unsafe { core::ptr::read_volatile(plan.register_virtual_address() as *const u32) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller(
        vendor_id: u16,
        device_id: u16,
        kind: NetworkControllerKind,
    ) -> NetworkController {
        NetworkController {
            kind,
            bus: 2,
            device: 0,
            function: 0,
            vendor_id,
            device_id,
            subsystem_vendor_id: 0,
            subsystem_device_id: 0,
            class_code: 2,
            subclass: 0,
            prog_if: 0,
        }
    }

    fn layout_with_bar(slot: usize, raw_low: u32, raw_high: u32) -> NetworkBarLayout {
        let mut raw = [0; 6];
        raw[slot] = raw_low;
        if ((raw_low >> 1) & 0b11) == 2 && slot < 5 {
            raw[slot + 1] = raw_high;
        }
        decode_bar_layout(raw)
    }

    #[test]
    fn selects_fixed_qemu_status_target() {
        let target = select_target(controller(0x8086, 0x100E, NetworkControllerKind::Ethernet));
        assert_eq!(
            target.unwrap().register_offset,
            INTEL_STATUS_REGISTER_OFFSET
        );
        assert_eq!(target.unwrap().bar_slot, 0);
    }

    #[test]
    fn selects_lenovo_realtek_status_target() {
        let target = select_target(controller(
            0x10EC,
            0xC82F,
            NetworkControllerKind::OtherNetwork,
        ));
        assert_eq!(
            target.unwrap().register_offset,
            REALTEK_SYS_STATUS1_REGISTER_OFFSET
        );
        assert_eq!(target.unwrap().bar_slot, 2);
    }

    #[test]
    fn rejects_unknown_controller_and_disabled_memory_space() {
        let unknown = controller(0x1234, 0x5678, NetworkControllerKind::Ethernet);
        assert_eq!(
            prepare_plan(
                unknown,
                PCI_MEMORY_SPACE_ENABLE,
                NetworkBarLayout {
                    raw: [0; 6],
                    bars: [None; 6],
                    malformed: false,
                }
            ),
            Err(RegisterProbeSkip::UnsupportedController)
        );
        let qemu = controller(0x8086, 0x100E, NetworkControllerKind::Ethernet);
        assert_eq!(
            prepare_plan(qemu, 0, layout_with_bar(0, 0x8104_0000, 0)),
            Err(RegisterProbeSkip::PciMemorySpaceDisabled)
        );
    }

    #[test]
    fn accepts_qemu_memory_bar_and_bounds_status_register() {
        let qemu = controller(0x8086, 0x100E, NetworkControllerKind::Ethernet);
        let plan = prepare_plan(
            qemu,
            PCI_MEMORY_SPACE_ENABLE,
            layout_with_bar(0, 0x8104_0000, 0),
        )
        .unwrap();
        assert_eq!(
            plan.mapping(),
            (0x8104_0000, NETWORK_REGISTER_MMIO_VIRT, 0x1000)
        );
        assert_eq!(
            plan.register_virtual_address(),
            NETWORK_REGISTER_MMIO_VIRT + 0x08
        );
    }

    #[test]
    fn enable_profile_accepts_target_before_memory_space_is_enabled() {
        let qemu = controller(0x8086, 0x100E, NetworkControllerKind::Ethernet);
        let plan = prepare_enable_plan(qemu, 0, layout_with_bar(0, 0x8104_0000, 0)).unwrap();
        assert_eq!(plan.physical_base, 0x8104_0000);
        assert_eq!(plan.target.register_offset, INTEL_STATUS_REGISTER_OFFSET);
    }

    #[test]
    fn accepts_lenovo_memory64_bar_and_bounds_status_register() {
        let lenovo = controller(0x10EC, 0xC82F, NetworkControllerKind::OtherNetwork);
        let plan = prepare_plan(
            lenovo,
            PCI_MEMORY_SPACE_ENABLE,
            layout_with_bar(2, 0xE8A0_0004, 0),
        )
        .unwrap();
        assert_eq!(plan.physical_base, 0xE8A0_0000);
        assert_eq!(
            plan.target.register_offset,
            REALTEK_SYS_STATUS1_REGISTER_OFFSET
        );
    }

    #[test]
    fn rejects_io_missing_misaligned_overflowing_and_malformed_targets() {
        let qemu = controller(0x8086, 0x100E, NetworkControllerKind::Ethernet);
        assert_eq!(
            prepare_plan(qemu, PCI_MEMORY_SPACE_ENABLE, layout_with_bar(0, 0x1001, 0)),
            Err(RegisterProbeSkip::TargetBarIsIo)
        );
        assert_eq!(
            prepare_plan(
                qemu,
                PCI_MEMORY_SPACE_ENABLE,
                NetworkBarLayout {
                    raw: [0; 6],
                    bars: [None; 6],
                    malformed: false,
                }
            ),
            Err(RegisterProbeSkip::TargetBarMissing)
        );
        assert_eq!(
            prepare_plan(
                qemu,
                PCI_MEMORY_SPACE_ENABLE,
                layout_with_bar(0, 0x8104_0100, 0)
            ),
            Err(RegisterProbeSkip::TargetBarMisaligned)
        );
        assert_eq!(
            prepare_plan(
                controller(0x10EC, 0xC82F, NetworkControllerKind::OtherNetwork),
                PCI_MEMORY_SPACE_ENABLE,
                layout_with_bar(2, 0xFFFF_F004, 0xFFFF_FFFF),
            ),
            Err(RegisterProbeSkip::TargetBarOverflow)
        );
        assert_eq!(
            prepare_plan(
                qemu,
                PCI_MEMORY_SPACE_ENABLE,
                NetworkBarLayout {
                    raw: [0; 6],
                    bars: [None; 6],
                    malformed: true,
                }
            ),
            Err(RegisterProbeSkip::MalformedBarLayout)
        );
    }
}
