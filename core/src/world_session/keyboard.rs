//! Owns the selected xHCI controller for one native world session.

use crate::{
    block_device::{
        nvme_probe::{EMULATED_TARGET, disable_pci_message_interrupts},
        read_config_u32, write_pci_config_word,
    },
    memory::r#virtual::{PhysicalDeviceMapping, translate_active_address},
    serial, session_input,
    usb_boot_keyboard::BootKeyboard,
    usb_xhci_driver::{
        XHCI_DRIVER_MMIO_LEN, XhciWorldKeyboard, claim_keyboard_controller,
        stop_keyboard_controller,
    },
    usb_xhci_probe::{self, UsbController, UsbControllerKind},
};

const PCI_COMMAND: u8 = 4;
const MEMORY_SPACE: u16 = 1 << 1;
const BUS_MASTER: u16 = 1 << 2;
const DISABLE_INTX: u16 = 1 << 10;
const CONTROL_BITS: u16 = MEMORY_SPACE | BUS_MASTER | DISABLE_INTX;

pub(super) struct UsbLocation {
    controller: UsbController,
    mapping: PhysicalDeviceMapping,
}

impl UsbLocation {
    pub(super) fn discover() -> Result<Self, &'static str> {
        let report = usb_xhci_probe::run_probe();
        let mut candidates = (0..report.count())
            .filter_map(|index| report.controller_at(index))
            .filter(|controller| {
                controller.kind == UsbControllerKind::Xhci && matches(*controller)
            });
        let controller = candidates.next().ok_or("USB_KEYBOARD_CONTROLLER_ABSENT")?;
        if report.overflowed() || candidates.next().is_some() {
            return Err("USB_KEYBOARD_CONTROLLER_AMBIGUOUS");
        }
        let mapping =
            usb_xhci_probe::controller_mmio_mapping_with_len(controller, XHCI_DRIVER_MMIO_LEN)
                .map_err(|_| "USB_BAR0_MAPPING")?;
        Ok(Self {
            controller,
            mapping,
        })
    }

    pub(super) fn mapping(&self) -> PhysicalDeviceMapping {
        self.mapping
    }

    pub(super) fn with_keyboard(
        self,
        run: impl FnOnce(&mut WorldKeyboard) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        let (physical, virtual_base, length) = self.mapping;
        if translate_active_address(virtual_base).ok() != Some(physical)
            || translate_active_address(virtual_base + length - 1).ok()
                != Some(physical + length - 1)
        {
            return Err("USB_BAR0_MAPPING");
        }
        let controller = self.controller;
        let stopped = read_pci(controller, PCI_COMMAND) as u16 & !BUS_MASTER;
        let registers_only = stopped | MEMORY_SPACE | DISABLE_INTX;
        if !set_command(controller, registers_only) {
            return Err(self.restore(stopped, "USB_PCI_ENABLE"));
        }
        let registers = match usb_xhci_probe::snapshot_controller(controller) {
            Ok(registers) => registers,
            Err(_) => return Err(self.restore(stopped, "USB_REGISTER_LAYOUT")),
        };
        let startup = super::with_input_interrupts(|| {
            disable_pci_message_interrupts(controller.bus, controller.device, controller.function)?;
            claim_keyboard_controller(registers)?;
            let port = if EMULATED_TARGET {
                let ports =
                    usb_xhci_probe::read_port_status_from_mapped_window(virtual_base, registers)
                        .map_err(|_| "USB_PORT_LAYOUT")?;
                (0..usize::from(ports.captured_ports))
                    .filter_map(|index| ports.port_at(index))
                    .find(|port| port.portsc & 1 != 0)
                    .map(|port| port.port_number)
                    .ok_or("USB_KEYBOARD_PORT_EMPTY")?
            } else {
                3
            };
            let (vendor, product) = if EMULATED_TARGET {
                (0x0627, 0x0001)
            } else {
                (0x062a, 0x261b)
            };
            if !set_command(controller, registers_only | BUS_MASTER) {
                return Err("USB_PCI_BUS_MASTER");
            }
            let driver = XhciWorldKeyboard::start(registers, port, vendor, product)?;
            serial::write_line("PYTHOS:WORLD:USB_KEYBOARD_READY");
            Ok(WorldKeyboard {
                driver,
                decoder: BootKeyboard::new(),
            })
        });
        let result = startup.and_then(|mut keyboard| run(&mut keyboard));
        let halted = if read_pci(controller, PCI_COMMAND) as u16 & BUS_MASTER != 0 {
            super::with_input_interrupts(|| stop_keyboard_controller(registers))
        } else {
            Ok(())
        };
        if !set_command(controller, stopped) {
            return Err("USB_PCI_QUIESCE");
        }
        halted?;
        serial::write_line("PYTHOS:WORLD:USB_QUIESCED");
        result
    }

    fn restore(&self, command: u16, failure: &'static str) -> &'static str {
        if set_command(self.controller, command) {
            failure
        } else {
            "USB_PCI_QUIESCE"
        }
    }
}

pub(super) struct WorldKeyboard {
    driver: XhciWorldKeyboard,
    decoder: BootKeyboard,
}

impl WorldKeyboard {
    pub(super) fn poll(&mut self) -> Result<(), &'static str> {
        if let Some(report) = self.driver.poll()? {
            self.decoder.update(&report, |event| {
                session_input::publish(event);
            });
        }
        Ok(())
    }
}

fn matches(controller: UsbController) -> bool {
    if EMULATED_TARGET {
        controller.vendor_id == 0x1b36 && controller.device_id == 0x000d
    } else {
        // The receiver is on HS03 of the AMD controller at function 3.
        controller.vendor_id == 0x1022
            && controller.device_id == 0x15e0
            && controller.function == 3
            && read_pci(controller, 0x2c) == 0x8619_103c
    }
}

fn read_pci(controller: UsbController, offset: u8) -> u32 {
    read_config_u32(
        controller.bus,
        controller.device,
        controller.function,
        offset,
    )
}

fn set_command(controller: UsbController, command: u16) -> bool {
    write_pci_config_word(
        controller.bus,
        controller.device,
        controller.function,
        PCI_COMMAND,
        command,
    );
    read_pci(controller, PCI_COMMAND) as u16 & CONTROL_BITS == command & CONTROL_BITS
}
