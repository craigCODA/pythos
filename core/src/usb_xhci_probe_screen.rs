//! Framebuffer-readable USB/xHCI probe identity panel.
//!
//! This module formats already-collected USB controller and xHCI register
//! snapshots into fixed ASCII lines for machines without serial capture.

#[cfg(feature = "usb-xhci-probe")]
use crate::framebuffer;
#[cfg(any(test, feature = "usb-xhci-command-probe"))]
use crate::usb_xhci_driver::{
    XhciAddressProbeResult, XhciCommandProbeResult, XhciConfigurationProbeResult,
    XhciDescriptorProbeResult, XhciDriverError,
};
use crate::usb_xhci_probe::{
    UsbController, UsbControllerKind, UsbMemoryBar, UsbProbeReport, XhciPortChange,
    XhciPortStatusSnapshot, XhciProbeError, XhciRegisterSnapshot,
};
#[cfg(feature = "usb-xhci-probe")]
use pythos_shared::boot_protocol::PythFramebufferInfo;

const PROBE_SCREEN_MAX_LINES: usize = 14;
const PROBE_LINE_MAX_BYTES: usize = 32;

#[derive(Clone, Copy)]
pub struct ProbeLine {
    bytes: [u8; PROBE_LINE_MAX_BYTES],
    len: usize,
}

impl ProbeLine {
    pub const fn new() -> Self {
        Self {
            bytes: [0; PROBE_LINE_MAX_BYTES],
            len: 0,
        }
    }

    fn push_byte(&mut self, byte: u8) {
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
    }

    fn push_str(&mut self, text: &str) {
        for byte in text.bytes() {
            self.push_byte(byte);
        }
    }

    fn push_hex(&mut self, value: u64, digits: usize) {
        let mut shift = digits.saturating_mul(4);
        while shift > 0 {
            shift -= 4;
            let nibble = ((value >> shift) & 0xF) as u8;
            self.push_byte(hex_digit(nibble));
        }
    }

    fn push_dec(&mut self, value: u64, digits: usize) {
        let mut divisor = 1u64;
        let mut scale = 1usize;
        while scale < digits {
            divisor = divisor.saturating_mul(10);
            scale += 1;
        }
        while divisor > 0 {
            let digit = ((value / divisor) % 10) as u8;
            self.push_byte(b'0' + digit);
            divisor /= 10;
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.bytes[..self.len]).ok()
    }
}

#[derive(Clone, Copy)]
pub struct ProbeScreen {
    lines: [ProbeLine; PROBE_SCREEN_MAX_LINES],
    count: usize,
}

impl ProbeScreen {
    pub const fn new() -> Self {
        Self {
            lines: [ProbeLine::new(); PROBE_SCREEN_MAX_LINES],
            count: 0,
        }
    }

    pub const fn line_count(&self) -> usize {
        self.count
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        if index >= self.count {
            return None;
        }
        self.lines[index].as_str()
    }

    fn push(&mut self, line: ProbeLine) {
        if self.count < self.lines.len() {
            self.lines[self.count] = line;
            self.count += 1;
        }
    }
}

pub fn build_screen(
    report: &UsbProbeReport,
    snapshot: Option<XhciRegisterSnapshot>,
    port_status: Option<XhciPortStatusSnapshot>,
    register_error: Option<XhciProbeError>,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");

    let selected = select_controller(report);
    match selected {
        Some(controller) => {
            if port_status.is_some() {
                push_text(&mut screen, "xhci ports");
            } else if register_error.is_some() {
                push_text(&mut screen, "xhci err");
            } else if snapshot.is_some() {
                push_text(&mut screen, "xhci regs");
            } else if controller.kind == UsbControllerKind::Xhci {
                push_text(&mut screen, "usb xhci");
            } else {
                push_text(&mut screen, "usb other");
            }
            push_text(&mut screen, "no disk writes");
            push_count(&mut screen, report.count() as u64);
            push_bdf(&mut screen, controller);
            push_vid_did(&mut screen, controller);
            if let Some(port_status) = port_status {
                push_ports(&mut screen, port_status);
            } else if let Some(error) = register_error {
                push_class(&mut screen, controller);
                push_bar(&mut screen, "bar0 ", controller.bar0);
                push_u32(&mut screen, "err ", error.screen_code());
            } else if let Some(snapshot) = snapshot {
                push_class(&mut screen, controller);
                push_bar(&mut screen, "bar0 ", controller.bar0);
                push_u8(&mut screen, "caplen ", snapshot.capability_length);
                push_u16(&mut screen, "hciver ", snapshot.hci_version);
                push_u32(&mut screen, "hcs1 ", snapshot.hcsparams1);
                push_u32(&mut screen, "hcc1 ", snapshot.hccparams1);
                push_u32(&mut screen, "sts ", snapshot.usbsts);
            } else {
                push_class(&mut screen, controller);
                push_bar(&mut screen, "bar0 ", controller.bar0);
            }
        }
        None => {
            push_text(&mut screen, "no xhci");
            push_text(&mut screen, "no disk writes");
            push_count(&mut screen, report.count() as u64);
        }
    }

    screen
}

pub fn build_swap_screen(
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "swap mouse now");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    push_ports(&mut screen, port_status);
    screen
}

pub fn build_swap_change_screen(
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci swap");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    push_change(&mut screen, change);
    push_ports_summary(&mut screen, port_status);
    push_xecp(&mut screen, port_status);
    push_legacy(&mut screen, port_status);
    screen
}

#[cfg(any(test, feature = "usb-xhci-command-probe"))]
pub fn build_command_probe_screen(
    report: &UsbProbeReport,
    result: XhciCommandProbeResult,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci cmd");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    push_u8(&mut screen, "port ", result.port_number);
    push_u8(&mut screen, "slot ", result.slot_id);
    push_u8(&mut screen, "noop cc ", result.noop_completion_code);
    push_u8(
        &mut screen,
        "enslot cc ",
        result.enable_slot_completion_code,
    );
    push_u32(&mut screen, "usbsts ", result.usbsts_after_start);
    push_u32(&mut screen, "portsc ", result.portsc_after_reset);
    push_u8(&mut screen, "scratch ", result.scratchpad_count as u8);
    screen
}

#[cfg(any(test, feature = "usb-xhci-command-probe"))]
pub fn build_command_error_screen(
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci cmd err");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    push_driver_error(&mut screen, error);
    push_change(&mut screen, change);
    push_ports_summary(&mut screen, port_status);
    push_xecp(&mut screen, port_status);
    push_legacy(&mut screen, port_status);
    screen
}

#[cfg(any(test, feature = "usb-xhci-address-probe"))]
pub fn build_address_probe_screen(
    report: &UsbProbeReport,
    result: XhciAddressProbeResult,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci addr");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    let mut port = ProbeLine::new();
    port.push_str("port ");
    port.push_hex(u64::from(result.command.port_number), 2);
    port.push_str(" slot ");
    port.push_hex(u64::from(result.command.slot_id), 2);
    screen.push(port);
    let mut command = ProbeLine::new();
    command.push_str("noop cc ");
    command.push_hex(u64::from(result.command.noop_completion_code), 2);
    command.push_str(" en cc ");
    command.push_hex(u64::from(result.command.enable_slot_completion_code), 2);
    screen.push(command);
    push_u8(
        &mut screen,
        "addr cc ",
        result.address_device_completion_code,
    );
    let mut address = ProbeLine::new();
    address.push_str("dev addr ");
    address.push_hex(u64::from(result.device_address), 2);
    address.push_str(" state ");
    address.push_hex(u64::from(result.slot_state), 2);
    screen.push(address);
    let mut ep0 = ProbeLine::new();
    ep0.push_str("ep0 st ");
    ep0.push_hex(u64::from(result.ep0_state), 2);
    ep0.push_str(" speed ");
    ep0.push_hex(u64::from(result.port_speed), 2);
    screen.push(ep0);
    let mut context = ProbeLine::new();
    context.push_str("ctx ");
    context.push_dec(u64::from(result.context_size), 2);
    context.push_str(" mps ");
    context.push_dec(u64::from(result.default_control_max_packet_size), 4);
    screen.push(context);
    push_u32(&mut screen, "portsc ", result.command.portsc_after_reset);
    push_u8(
        &mut screen,
        "scratch ",
        result.command.scratchpad_count as u8,
    );
    screen
}

#[cfg(any(test, feature = "usb-xhci-descriptor-probe"))]
pub fn build_descriptor_probe_screen(
    report: &UsbProbeReport,
    result: XhciDescriptorProbeResult,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci desc");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    let mut port = ProbeLine::new();
    port.push_str("port ");
    port.push_hex(u64::from(result.address.command.port_number), 2);
    port.push_str(" slot ");
    port.push_hex(u64::from(result.address.command.slot_id), 2);
    screen.push(port);
    let mut command = ProbeLine::new();
    command.push_str("addr cc ");
    command.push_hex(u64::from(result.address.address_device_completion_code), 2);
    command.push_str(" desc cc ");
    command.push_hex(u64::from(result.descriptor_completion_code), 2);
    screen.push(command);
    let mut length = ProbeLine::new();
    length.push_str("len ");
    length.push_hex(u64::from(result.descriptor.length), 2);
    length.push_str(" type ");
    length.push_hex(u64::from(result.descriptor.descriptor_type), 2);
    screen.push(length);
    let mut version = ProbeLine::new();
    version.push_str("usb ");
    version.push_hex(u64::from(result.descriptor.usb_bcd), 4);
    version.push_str(" dev ");
    version.push_hex(u64::from(result.descriptor.device_bcd), 4);
    screen.push(version);
    let mut class = ProbeLine::new();
    class.push_str("cls ");
    class.push_hex(u64::from(result.descriptor.device_class), 2);
    class.push_str(" sub ");
    class.push_hex(u64::from(result.descriptor.device_subclass), 2);
    class.push_str(" pr ");
    class.push_hex(u64::from(result.descriptor.device_protocol), 2);
    screen.push(class);
    let mut mps = ProbeLine::new();
    mps.push_str("mps ");
    mps.push_dec(u64::from(result.descriptor.max_packet_size0), 3);
    mps.push_str(" cfg ");
    mps.push_hex(u64::from(result.descriptor.configuration_count), 2);
    screen.push(mps);
    let mut identity = ProbeLine::new();
    identity.push_str("vid pid ");
    identity.push_hex(u64::from(result.descriptor.vendor_id), 4);
    identity.push_str(" ");
    identity.push_hex(u64::from(result.descriptor.product_id), 4);
    screen.push(identity);
    push_u8(
        &mut screen,
        "scratch ",
        result.address.command.scratchpad_count as u8,
    );
    screen
}

#[cfg(any(test, feature = "usb-xhci-configuration-probe"))]
pub fn build_configuration_probe_screen(
    report: &UsbProbeReport,
    result: XhciConfigurationProbeResult,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci cfg");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }

    let mut port = ProbeLine::new();
    port.push_str("port ");
    port.push_hex(u64::from(result.descriptor.address.command.port_number), 2);
    port.push_str(" slot ");
    port.push_hex(u64::from(result.descriptor.address.command.slot_id), 2);
    screen.push(port);

    let mut device = ProbeLine::new();
    device.push_str("addr cc ");
    device.push_hex(
        u64::from(result.descriptor.address.address_device_completion_code),
        2,
    );
    device.push_str(" desc cc ");
    device.push_hex(u64::from(result.descriptor.descriptor_completion_code), 2);
    screen.push(device);

    let mut transfers = ProbeLine::new();
    transfers.push_str("hdr cc ");
    transfers.push_hex(u64::from(result.configuration_header_completion_code), 2);
    transfers.push_str(" cfg cc ");
    transfers.push_hex(u64::from(result.configuration_completion_code), 2);
    screen.push(transfers);

    let mut total = ProbeLine::new();
    total.push_str("total ");
    total.push_dec(u64::from(result.configuration.header.total_length), 4);
    total.push_str(" val ");
    total.push_hex(
        u64::from(result.configuration.header.configuration_value),
        2,
    );
    screen.push(total);

    let mut counts = ProbeLine::new();
    counts.push_str("cfgs ");
    counts.push_hex(
        u64::from(result.descriptor.descriptor.configuration_count),
        2,
    );
    counts.push_str(" ifs ");
    counts.push_hex(u64::from(result.configuration.header.interface_count), 2);
    screen.push(counts);

    let mut interface = ProbeLine::new();
    interface.push_str("if ");
    interface.push_hex(u64::from(result.configuration.interface_class), 2);
    interface.push_str(" ");
    interface.push_hex(u64::from(result.configuration.interface_subclass), 2);
    interface.push_str(" ");
    interface.push_hex(u64::from(result.configuration.interface_protocol), 2);
    interface.push_str(" ep ");
    interface.push_hex(
        u64::from(result.configuration.interrupt_in_endpoint_address),
        2,
    );
    screen.push(interface);

    let mut endpoint = ProbeLine::new();
    endpoint.push_str("attr ");
    endpoint.push_hex(u64::from(result.configuration.interrupt_in_attributes), 2);
    endpoint.push_str(" mps ");
    endpoint.push_dec(
        u64::from(result.configuration.interrupt_in_max_packet_size),
        4,
    );
    screen.push(endpoint);

    let mut interval = ProbeLine::new();
    interval.push_str("int ");
    interval.push_dec(u64::from(result.configuration.interrupt_in_interval), 3);
    interval.push_str(" scratch ");
    interval.push_hex(
        u64::from(result.descriptor.address.command.scratchpad_count),
        2,
    );
    screen.push(interval);
    screen
}

#[cfg(any(test, feature = "usb-xhci-configuration-probe"))]
pub fn build_configuration_error_screen(
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci cfg err");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    push_driver_error(&mut screen, error);
    push_change(&mut screen, change);
    push_ports_summary(&mut screen, port_status);
    push_xecp(&mut screen, port_status);
    push_legacy(&mut screen, port_status);
    screen
}

#[cfg(any(test, feature = "usb-xhci-descriptor-probe"))]
pub fn build_descriptor_error_screen(
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci desc err");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    push_driver_error(&mut screen, error);
    push_change(&mut screen, change);
    push_ports_summary(&mut screen, port_status);
    push_xecp(&mut screen, port_status);
    push_legacy(&mut screen, port_status);
    screen
}

#[cfg(any(test, feature = "usb-xhci-address-probe"))]
pub fn build_address_error_screen(
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");
    push_text(&mut screen, "xhci addr err");
    push_text(&mut screen, "no disk writes");
    push_count(&mut screen, report.count() as u64);
    if let Some(controller) = select_controller(report) {
        push_bdf(&mut screen, controller);
        push_vid_did(&mut screen, controller);
    }
    push_driver_error(&mut screen, error);
    push_change(&mut screen, change);
    push_ports_summary(&mut screen, port_status);
    push_xecp(&mut screen, port_status);
    push_legacy(&mut screen, port_status);
    screen
}

#[cfg(feature = "usb-xhci-probe")]
pub fn render(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    snapshot: Option<XhciRegisterSnapshot>,
    port_status: Option<XhciPortStatusSnapshot>,
    register_error: Option<XhciProbeError>,
) -> Result<(), ()> {
    let screen = build_screen(report, snapshot, port_status, register_error);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-probe")]
pub fn render_swap_prompt(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
) -> Result<(), ()> {
    let screen = build_swap_screen(report, port_status);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-probe")]
pub fn render_swap_change(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
) -> Result<(), ()> {
    let screen = build_swap_change_screen(report, port_status, change);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-command-probe")]
pub fn render_command_probe(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    result: XhciCommandProbeResult,
) -> Result<(), ()> {
    let screen = build_command_probe_screen(report, result);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-command-probe")]
pub fn render_command_error(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> Result<(), ()> {
    let screen = build_command_error_screen(report, port_status, change, error);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-address-probe")]
pub fn render_address_probe(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    result: XhciAddressProbeResult,
) -> Result<(), ()> {
    let screen = build_address_probe_screen(report, result);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-descriptor-probe")]
pub fn render_descriptor_probe(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    result: XhciDescriptorProbeResult,
) -> Result<(), ()> {
    let screen = build_descriptor_probe_screen(report, result);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-configuration-probe")]
pub fn render_configuration_probe(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    result: XhciConfigurationProbeResult,
) -> Result<(), ()> {
    let screen = build_configuration_probe_screen(report, result);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-configuration-probe")]
pub fn render_configuration_error(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> Result<(), ()> {
    let screen = build_configuration_error_screen(report, port_status, change, error);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-descriptor-probe")]
pub fn render_descriptor_error(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> Result<(), ()> {
    let screen = build_descriptor_error_screen(report, port_status, change, error);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-address-probe")]
pub fn render_address_error(
    framebuffer_info: &PythFramebufferInfo,
    report: &UsbProbeReport,
    port_status: XhciPortStatusSnapshot,
    change: XhciPortChange,
    error: XhciDriverError,
) -> Result<(), ()> {
    let screen = build_address_error_screen(report, port_status, change, error);
    render_screen(framebuffer_info, screen)
}

#[cfg(feature = "usb-xhci-probe")]
fn render_screen(framebuffer_info: &PythFramebufferInfo, screen: ProbeScreen) -> Result<(), ()> {
    let mut lines = [""; PROBE_SCREEN_MAX_LINES];
    let mut index = 0;
    while index < screen.line_count() {
        lines[index] = screen.line(index).ok_or(())?;
        index += 1;
    }
    framebuffer::render_hardware_probe_lines(framebuffer_info, &lines[..screen.line_count()])
}

fn select_controller(report: &UsbProbeReport) -> Option<UsbController> {
    if let Some(xhci) = report.first_xhci() {
        return Some(xhci);
    }
    report.controller_at(0)
}

fn push_text(screen: &mut ProbeScreen, text: &str) {
    let mut line = ProbeLine::new();
    line.push_str(text);
    screen.push(line);
}

fn push_count(screen: &mut ProbeScreen, count: u64) {
    let mut line = ProbeLine::new();
    line.push_str("count ");
    line.push_hex(count, 16);
    screen.push(line);
}

fn push_bdf(screen: &mut ProbeScreen, controller: UsbController) {
    let mut line = ProbeLine::new();
    line.push_str("bdf ");
    line.push_hex(u64::from(controller.bus), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.device), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.function), 2);
    screen.push(line);
}

fn push_vid_did(screen: &mut ProbeScreen, controller: UsbController) {
    let mut line = ProbeLine::new();
    line.push_str("vid did ");
    line.push_hex(u64::from(controller.vendor_id), 4);
    line.push_str(" ");
    line.push_hex(u64::from(controller.device_id), 4);
    screen.push(line);
}

fn push_class(screen: &mut ProbeScreen, controller: UsbController) {
    let mut line = ProbeLine::new();
    line.push_str("class sub if ");
    line.push_hex(u64::from(controller.class_code), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.subclass), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.prog_if), 2);
    screen.push(line);
}

fn push_bar(screen: &mut ProbeScreen, label: &str, bar: Option<UsbMemoryBar>) {
    push_bar_base(screen, label, bar_base(bar));
}

fn push_ports(screen: &mut ProbeScreen, port_status: XhciPortStatusSnapshot) {
    push_ports_summary(screen, port_status);
    push_xecp(screen, port_status);
    push_legacy(screen, port_status);

    let mut index = 0usize;
    while index < 4 {
        if let Some(port) = port_status.port_at(index) {
            let mut line = ProbeLine::new();
            line.push_str("p");
            line.push_hex(u64::from(port.port_number), 1);
            line.push_str(" sc ");
            line.push_hex(u64::from(port.portsc), 8);
            screen.push(line);
        }
        index += 1;
    }
}

fn push_ports_summary(screen: &mut ProbeScreen, port_status: XhciPortStatusSnapshot) {
    let mut ports = ProbeLine::new();
    ports.push_str("ports total ");
    ports.push_hex(u64::from(port_status.max_ports), 2);
    ports.push_str(" snap ");
    ports.push_hex(u64::from(port_status.captured_ports), 2);
    screen.push(ports);
}

fn push_xecp(screen: &mut ProbeScreen, port_status: XhciPortStatusSnapshot) {
    let mut xecp = ProbeLine::new();
    xecp.push_str("xecp ");
    xecp.push_hex(port_status.extended_capability_byte_offset, 4);
    screen.push(xecp);
}

fn push_legacy(screen: &mut ProbeScreen, port_status: XhciPortStatusSnapshot) {
    match port_status.legacy_support {
        Some(legacy) => {
            let mut line = ProbeLine::new();
            line.push_str("leg ");
            line.push_hex(legacy.byte_offset, 4);
            line.push_str(" bo");
            line.push_byte(bool_digit(legacy.bios_owned));
            line.push_str(" oo");
            line.push_byte(bool_digit(legacy.os_owned));
            screen.push(line);
        }
        None => push_text(screen, "leg none"),
    }
}

fn push_change(screen: &mut ProbeScreen, change: XhciPortChange) {
    let mut port = ProbeLine::new();
    port.push_str("chg p");
    port.push_hex(u64::from(change.port_number), 1);
    screen.push(port);

    let mut before = ProbeLine::new();
    before.push_str("was sc ");
    before.push_hex(u64::from(change.before_portsc), 8);
    screen.push(before);

    let mut after = ProbeLine::new();
    after.push_str("now sc ");
    after.push_hex(u64::from(change.after_portsc), 8);
    screen.push(after);
}

fn push_bar_base(screen: &mut ProbeScreen, label: &str, base: u64) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(base, 16);
    screen.push(line);
}

fn push_u8(screen: &mut ProbeScreen, label: &str, value: u8) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(u64::from(value), 2);
    screen.push(line);
}

fn push_u16(screen: &mut ProbeScreen, label: &str, value: u16) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(u64::from(value), 4);
    screen.push(line);
}

fn push_u32(screen: &mut ProbeScreen, label: &str, value: u32) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(u64::from(value), 8);
    screen.push(line);
}

fn push_driver_error(screen: &mut ProbeScreen, error: XhciDriverError) {
    push_u32(screen, "err ", error.screen_code());
    if let Some(stage) = error.screen_stage() {
        push_text(screen, stage);
    }
}

fn bar_base(bar: Option<UsbMemoryBar>) -> u64 {
    match bar {
        Some(UsbMemoryBar::Memory32(base)) | Some(UsbMemoryBar::Memory64(base)) => base,
        None => 0,
    }
}

fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        10..=15 => b'A' + (nibble - 10),
        _ => b'0',
    }
}

fn bool_digit(value: bool) -> u8 {
    if value { b'1' } else { b'0' }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font;

    fn controller(kind: UsbControllerKind, bus: u8, device: u8) -> UsbController {
        UsbController {
            kind,
            bus,
            device,
            function: 0,
            vendor_id: 0x1022,
            device_id: if kind == UsbControllerKind::Xhci {
                0x7914
            } else {
                0x7908
            },
            class_code: 0x0C,
            subclass: 0x03,
            prog_if: if kind == UsbControllerKind::Xhci {
                0x30
            } else {
                0x20
            },
            command_status: 0,
            bar0: Some(UsbMemoryBar::Memory64(0x0000_0000_E8C6_8000)),
        }
    }

    #[test]
    fn formats_xhci_register_snapshot_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Ehci, 0, 18)));
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let snapshot = XhciRegisterSnapshot {
            bar0_base: 0x0000_0000_E8C6_8000,
            capability_length: 0x40,
            hci_version: 0x0110,
            hcsparams1: 0x0200_1004,
            hcsparams2: 0x0000_00F0,
            hcsparams3: 0x0000_0002,
            hccparams1: 0x0000_08F1,
            dboff: 0x0000_1000,
            rtsoff: 0x0000_1800,
            usbcmd: 0x0000_0000,
            usbsts: 0x0000_0001,
            pagesize: 0x0000_0001,
        };

        let screen = build_screen(&report, Some(snapshot), None, None);

        assert_eq!(screen.line_count(), 13);
        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci regs"));
        assert_eq!(screen.line(2), Some("no disk writes"));
        assert_eq!(screen.line(3), Some("count 0000000000000002"));
        assert_eq!(screen.line(4), Some("bdf 00 10 00"));
        assert_eq!(screen.line(5), Some("vid did 1022 7914"));
        assert_eq!(screen.line(6), Some("class sub if 0C 03 30"));
        assert_eq!(screen.line(7), Some("bar0 00000000E8C68000"));
        assert_eq!(screen.line(8), Some("caplen 40"));
        assert_eq!(screen.line(9), Some("hciver 0110"));
        assert_eq!(screen.line(10), Some("hcs1 02001004"));
        assert_eq!(screen.line(11), Some("hcc1 000008F1"));
        assert_eq!(screen.line(12), Some("sts 00000001"));
    }

    #[test]
    fn formats_xhci_probe_error_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));

        let screen = build_screen(&report, None, None, Some(XhciProbeError::MmioMappingFailed));

        assert_eq!(screen.line(1), Some("xhci err"));
        assert_eq!(screen.line(8), Some("err 00000005"));
    }

    #[test]
    fn formats_xhci_port_status_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let mut ports = [None; crate::usb_xhci_probe::XHCI_PORT_SNAPSHOT_LIMIT];
        ports[0] = Some(crate::usb_xhci_probe::XhciPortRegisterSnapshot {
            port_number: 1,
            portsc: 0x0000_0203,
            portpmsc: 0,
        });
        ports[1] = Some(crate::usb_xhci_probe::XhciPortRegisterSnapshot {
            port_number: 2,
            portsc: 0x0000_02A0,
            portpmsc: 1,
        });
        let port_status = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 2,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: Some(crate::usb_xhci_probe::XhciLegacySupportSnapshot {
                byte_offset: 0x20,
                header: 0x0101,
                control_status: 0,
                bios_owned: false,
                os_owned: true,
            }),
            ports,
        };

        let screen = build_screen(&report, None, Some(port_status), None);

        assert_eq!(screen.line_count(), 11);
        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci ports"));
        assert_eq!(screen.line(2), Some("no disk writes"));
        assert_eq!(screen.line(3), Some("count 0000000000000001"));
        assert_eq!(screen.line(4), Some("bdf 00 10 00"));
        assert_eq!(screen.line(5), Some("vid did 1022 7914"));
        assert_eq!(screen.line(6), Some("ports total 08 snap 02"));
        assert_eq!(screen.line(7), Some("xecp 0020"));
        assert_eq!(screen.line(8), Some("leg 0020 bo0 oo1"));
        assert_eq!(screen.line(9), Some("p1 sc 00000203"));
        assert_eq!(screen.line(10), Some("p2 sc 000002A0"));
    }

    #[test]
    fn formats_swap_mouse_prompt_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let mut ports = [None; crate::usb_xhci_probe::XHCI_PORT_SNAPSHOT_LIMIT];
        ports[0] = Some(crate::usb_xhci_probe::XhciPortRegisterSnapshot {
            port_number: 1,
            portsc: 0x0000_02A0,
            portpmsc: 0,
        });
        let port_status = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 1,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports,
        };

        let screen = build_swap_screen(&report, port_status);

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("swap mouse now"));
        assert_eq!(screen.line(2), Some("no disk writes"));
        assert_eq!(screen.line(6), Some("ports total 08 snap 01"));
    }

    #[test]
    fn formats_swap_change_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let mut ports = [None; crate::usb_xhci_probe::XHCI_PORT_SNAPSHOT_LIMIT];
        ports[4] = Some(crate::usb_xhci_probe::XhciPortRegisterSnapshot {
            port_number: 5,
            portsc: 0x0002_0EE1,
            portpmsc: 0,
        });
        let port_status = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 8,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports,
        };
        let change = crate::usb_xhci_probe::XhciPortChange {
            port_number: 5,
            before_portsc: 0x0000_02A0,
            after_portsc: 0x0002_0EE1,
            before_portpmsc: 0,
            after_portpmsc: 0,
        };

        let screen = build_swap_change_screen(&report, port_status, change);

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci swap"));
        assert_eq!(screen.line(6), Some("chg p5"));
        assert_eq!(screen.line(7), Some("was sc 000002A0"));
        assert_eq!(screen.line(8), Some("now sc 00020EE1"));
        assert_eq!(screen.line(9), Some("ports total 08 snap 08"));
    }

    #[test]
    fn formats_command_probe_result_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let result = crate::usb_xhci_driver::XhciCommandProbeResult {
            port_number: 5,
            noop_completion_code: 1,
            enable_slot_completion_code: 1,
            slot_id: 7,
            scratchpad_count: 1,
            usbsts_after_start: 0,
            portsc_after_reset: 0x0000_0E03,
        };

        let screen = build_command_probe_screen(&report, result);

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci cmd"));
        assert_eq!(screen.line(6), Some("port 05"));
        assert_eq!(screen.line(7), Some("slot 07"));
        assert_eq!(screen.line(8), Some("noop cc 01"));
        assert_eq!(screen.line(9), Some("enslot cc 01"));
        assert_eq!(screen.line(10), Some("usbsts 00000000"));
        assert_eq!(screen.line(11), Some("portsc 00000E03"));
        assert_eq!(screen.line(12), Some("scratch 01"));
    }

    #[test]
    fn formats_address_probe_result_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let result = crate::usb_xhci_driver::XhciAddressProbeResult {
            command: crate::usb_xhci_driver::XhciCommandProbeResult {
                port_number: 6,
                noop_completion_code: 1,
                enable_slot_completion_code: 1,
                slot_id: 1,
                scratchpad_count: 8,
                usbsts_after_start: 0,
                portsc_after_reset: 0x0022_0603,
            },
            address_device_completion_code: 1,
            device_address: 4,
            slot_state: 2,
            ep0_state: 1,
            port_speed: 1,
            context_size: 32,
            default_control_max_packet_size: 8,
        };

        let screen = build_address_probe_screen(&report, result);

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci addr"));
        assert_eq!(screen.line(6), Some("port 06 slot 01"));
        assert_eq!(screen.line(7), Some("noop cc 01 en cc 01"));
        assert_eq!(screen.line(8), Some("addr cc 01"));
        assert_eq!(screen.line(9), Some("dev addr 04 state 02"));
        assert_eq!(screen.line(10), Some("ep0 st 01 speed 01"));
        assert_eq!(screen.line(11), Some("ctx 32 mps 0008"));
        assert_eq!(screen.line(12), Some("portsc 00220603"));
        assert_eq!(screen.line(13), Some("scratch 08"));
    }

    #[test]
    fn formats_descriptor_probe_result_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let result = crate::usb_xhci_driver::XhciDescriptorProbeResult {
            address: crate::usb_xhci_driver::XhciAddressProbeResult {
                command: crate::usb_xhci_driver::XhciCommandProbeResult {
                    port_number: 6,
                    noop_completion_code: 1,
                    enable_slot_completion_code: 1,
                    slot_id: 1,
                    scratchpad_count: 8,
                    usbsts_after_start: 0,
                    portsc_after_reset: 0x0022_0603,
                },
                address_device_completion_code: 1,
                device_address: 4,
                slot_state: 2,
                ep0_state: 1,
                port_speed: 1,
                context_size: 32,
                default_control_max_packet_size: 8,
            },
            descriptor_completion_code: 1,
            descriptor: crate::usb_xhci_driver::XhciDeviceDescriptorSnapshot {
                length: 18,
                descriptor_type: 1,
                usb_bcd: 0x0200,
                device_class: 0,
                device_subclass: 0,
                device_protocol: 0,
                max_packet_size0: 8,
                vendor_id: 0x413c,
                product_id: 0x301a,
                device_bcd: 0x0100,
                manufacturer_index: 1,
                product_index: 2,
                serial_index: 0,
                configuration_count: 1,
            },
        };

        let screen = build_descriptor_probe_screen(&report, result);

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci desc"));
        assert_eq!(screen.line(2), Some("no disk writes"));
        assert_eq!(screen.line(6), Some("port 06 slot 01"));
        assert_eq!(screen.line(7), Some("addr cc 01 desc cc 01"));
        assert_eq!(screen.line(8), Some("len 12 type 01"));
        assert_eq!(screen.line(9), Some("usb 0200 dev 0100"));
        assert_eq!(screen.line(10), Some("cls 00 sub 00 pr 00"));
        assert_eq!(screen.line(11), Some("mps 008 cfg 01"));
        assert_eq!(screen.line(12), Some("vid pid 413C 301A"));
        assert_eq!(screen.line(13), Some("scratch 08"));
    }

    #[test]
    fn formats_descriptor_probe_error_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let mut ports = [None; crate::usb_xhci_probe::XHCI_PORT_SNAPSHOT_LIMIT];
        ports[5] = Some(crate::usb_xhci_probe::XhciPortRegisterSnapshot {
            port_number: 6,
            portsc: 0x0022_0603,
            portpmsc: 0,
        });
        let port_status = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 8,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports,
        };
        let change = crate::usb_xhci_probe::XhciPortChange {
            port_number: 6,
            before_portsc: 0x0000_02A0,
            after_portsc: 0x0022_0603,
            before_portpmsc: 0,
            after_portpmsc: 0,
        };

        let screen = build_descriptor_error_screen(
            &report,
            port_status,
            change,
            crate::usb_xhci_driver::XhciDriverError::UnexpectedTransferPointer,
        );

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci desc err"));
        assert_eq!(screen.line(6), Some("err 00000021"));
        assert_eq!(screen.line(7), Some("chg p6"));
        assert_eq!(screen.line(8), Some("was sc 000002A0"));
        assert_eq!(screen.line(9), Some("now sc 00220603"));
    }

    #[test]
    fn formats_configuration_probe_result_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let result = crate::usb_xhci_driver::XhciConfigurationProbeResult {
            descriptor: crate::usb_xhci_driver::XhciDescriptorProbeResult {
                address: crate::usb_xhci_driver::XhciAddressProbeResult {
                    command: crate::usb_xhci_driver::XhciCommandProbeResult {
                        port_number: 6,
                        noop_completion_code: 1,
                        enable_slot_completion_code: 1,
                        slot_id: 1,
                        scratchpad_count: 8,
                        usbsts_after_start: 0,
                        portsc_after_reset: 0x0022_0603,
                    },
                    address_device_completion_code: 1,
                    device_address: 4,
                    slot_state: 2,
                    ep0_state: 1,
                    port_speed: 1,
                    context_size: 32,
                    default_control_max_packet_size: 8,
                },
                descriptor_completion_code: 1,
                descriptor: crate::usb_xhci_driver::XhciDeviceDescriptorSnapshot {
                    length: 18,
                    descriptor_type: 1,
                    usb_bcd: 0x0200,
                    device_class: 0,
                    device_subclass: 0,
                    device_protocol: 0,
                    max_packet_size0: 8,
                    vendor_id: 0x0627,
                    product_id: 0x0001,
                    device_bcd: 0x0000,
                    manufacturer_index: 1,
                    product_index: 2,
                    serial_index: 0,
                    configuration_count: 1,
                },
            },
            configuration_header_completion_code: 1,
            configuration_completion_code: 1,
            configuration: crate::usb_xhci_driver::XhciConfigurationDescriptorSnapshot {
                header: crate::usb_xhci_driver::XhciConfigurationDescriptorHeader {
                    length: 9,
                    descriptor_type: 2,
                    total_length: 34,
                    interface_count: 1,
                    configuration_value: 1,
                    configuration_index: 0,
                    attributes: 0xA0,
                    max_power: 50,
                },
                interface_number: 0,
                alternate_setting: 0,
                endpoint_count: 1,
                interface_class: 3,
                interface_subclass: 1,
                interface_protocol: 2,
                interrupt_in_endpoint_address: 0x81,
                interrupt_in_attributes: 0x03,
                interrupt_in_max_packet_size: 4,
                interrupt_in_interval: 10,
            },
        };

        let screen = build_configuration_probe_screen(&report, result);

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci cfg"));
        assert_eq!(screen.line(2), Some("no disk writes"));
        assert_eq!(screen.line(6), Some("port 06 slot 01"));
        assert_eq!(screen.line(7), Some("addr cc 01 desc cc 01"));
        assert_eq!(screen.line(8), Some("hdr cc 01 cfg cc 01"));
        assert_eq!(screen.line(9), Some("total 0034 val 01"));
        assert_eq!(screen.line(10), Some("cfgs 01 ifs 01"));
        assert_eq!(screen.line(11), Some("if 03 01 02 ep 81"));
        assert_eq!(screen.line(12), Some("attr 03 mps 0004"));
        assert_eq!(screen.line(13), Some("int 010 scratch 08"));
    }

    #[test]
    fn formats_configuration_probe_error_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let port_status = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 0,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports: [None; crate::usb_xhci_probe::XHCI_PORT_SNAPSHOT_LIMIT],
        };
        let change = crate::usb_xhci_probe::XhciPortChange {
            port_number: 6,
            before_portsc: 0x0000_02A0,
            after_portsc: 0x0022_0603,
            before_portpmsc: 0,
            after_portpmsc: 0,
        };

        let screen = build_configuration_error_screen(
            &report,
            port_status,
            change,
            crate::usb_xhci_driver::XhciDriverError::ConfigurationHeaderTransferTimeout,
        );

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci cfg err"));
        assert_eq!(screen.line(2), Some("no disk writes"));
        assert_eq!(screen.line(6), Some("err 00000030"));
        assert_eq!(screen.line(7), Some("stage config header"));
        assert_eq!(screen.line(8), Some("chg p6"));
    }

    #[test]
    fn formats_command_probe_error_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let mut ports = [None; crate::usb_xhci_probe::XHCI_PORT_SNAPSHOT_LIMIT];
        ports[4] = Some(crate::usb_xhci_probe::XhciPortRegisterSnapshot {
            port_number: 5,
            portsc: 0x0002_02E1,
            portpmsc: 0,
        });
        let port_status = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 8,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports,
        };
        let change = crate::usb_xhci_probe::XhciPortChange {
            port_number: 5,
            before_portsc: 0x0000_02A0,
            after_portsc: 0x0002_02E1,
            before_portpmsc: 0,
            after_portpmsc: 0,
        };

        let screen = build_command_error_screen(
            &report,
            port_status,
            change,
            crate::usb_xhci_driver::XhciDriverError::MmioWindowTooLarge,
        );

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci cmd err"));
        assert_eq!(screen.line(6), Some("err 00000002"));
        assert_eq!(screen.line(7), Some("chg p5"));
        assert_eq!(screen.line(8), Some("was sc 000002A0"));
        assert_eq!(screen.line(9), Some("now sc 000202E1"));
        assert_eq!(screen.line(10), Some("ports total 08 snap 08"));
    }

    #[test]
    fn formats_address_probe_error_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let mut ports = [None; crate::usb_xhci_probe::XHCI_PORT_SNAPSHOT_LIMIT];
        ports[5] = Some(crate::usb_xhci_probe::XhciPortRegisterSnapshot {
            port_number: 6,
            portsc: 0x0022_0603,
            portpmsc: 0,
        });
        let port_status = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 8,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports,
        };
        let change = crate::usb_xhci_probe::XhciPortChange {
            port_number: 6,
            before_portsc: 0x0000_02A0,
            after_portsc: 0x0022_0603,
            before_portpmsc: 0,
            after_portpmsc: 0,
        };

        let screen = build_address_error_screen(
            &report,
            port_status,
            change,
            crate::usb_xhci_driver::XhciDriverError::UnsupportedPortSpeed,
        );

        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("xhci addr err"));
        assert_eq!(screen.line(6), Some("err 00000020"));
        assert_eq!(screen.line(7), Some("chg p6"));
        assert_eq!(screen.line(8), Some("was sc 000002A0"));
        assert_eq!(screen.line(9), Some("now sc 00220603"));
    }

    #[test]
    fn formats_no_xhci_state_for_no_serial_capture() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Ehci, 0, 18)));

        let screen = build_screen(&report, None, None, None);

        assert_eq!(screen.line(1), Some("usb other"));
        assert_eq!(screen.line(6), Some("class sub if 0C 03 20"));
    }

    #[test]
    fn renders_only_fixed_boot_glyphs() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(UsbControllerKind::Xhci, 0, 16)));
        let snapshot = XhciRegisterSnapshot {
            bar0_base: 0x0000_0000_E8C6_8000,
            capability_length: 0x40,
            hci_version: 0x0110,
            hcsparams1: 0x0200_1004,
            hcsparams2: 0x0000_00F0,
            hcsparams3: 0x0000_0002,
            hccparams1: 0x0000_08F1,
            dboff: 0x0000_1000,
            rtsoff: 0x0000_1800,
            usbcmd: 0x0000_0000,
            usbsts: 0x0000_0001,
            pagesize: 0x0000_0001,
        };

        let screen = build_screen(&report, Some(snapshot), None, None);

        for line_index in 0..screen.line_count() {
            let line = screen.line(line_index).unwrap();
            for byte in line.bytes() {
                assert!(
                    font::glyph(byte).is_some(),
                    "missing glyph for byte {byte:?} in line {line:?}"
                );
            }
        }
    }
}
