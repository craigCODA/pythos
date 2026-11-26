//! Persistent polling of one addressed HID boot keyboard.

use crate::{
    architecture::x86_64::timer,
    usb_xhci_probe::{self, XhciRegisterSnapshot},
};

use super::{
    Ordering, USB_REQUEST_GET_DESCRIPTOR_DEVICE, XHCI_COMPLETION_SUCCESS, XHCI_EVENT_RING_TRBS,
    XHCI_SETUP_TRANSFER_TYPE_IN, XHCI_SETUP_TRANSFER_TYPE_SHIFT, XHCI_TRB_IMMEDIATE_DATA,
    XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT, XHCI_TRB_TYPE_SETUP_STAGE, XhciAddressProbeResult,
    XhciCommandProbeState, XhciDmaState, XhciDriverError, XhciEventRingConsumer,
    XhciInterruptTransferProducer, XhciTrb, accept_event_at_consumer, ack_event,
    capture_interrupt_report_prefix, command_trb_control, compiler_fence,
    configuration_from_command_state, configure_interrupt_endpoint, device_descriptor_data_trb,
    doorbell_offset, event_ring_ptr, initialize_command_probe, input_context_ptr,
    prepare_interrupt_transfer_at, read_device_descriptor_buffer, read_mmio_u32, read_trb,
    stop_controller, submit_command_trb, submit_ep0_control_read, submit_ep0_no_data_request,
    validate_interrupt_transfer_event, write_dma_u32, write_mmio_u32, zero_dma_page,
};

const BOOT_KEYBOARD_PROTOCOL: u8 = 1;
const BOOT_REPORT_BYTES: u16 = 8;
const PORT_STATUS_CHANGE: u8 = 34;
const EVALUATE_CONTEXT: u8 = 13;
const BIOS_OWNED: u32 = 1 << 16;
const OS_OWNED: u32 = 1 << 24;
// USBLEGCTLSTS reserves these fields. All SMI enable and W1C status bits
// receive zero, leaving the status bits unchanged and SMI delivery disabled.
const LEGACY_CONTROL_PRESERVE: u32 = (0x7 << 1) | (0xff << 5) | (0x7 << 17);

pub(crate) fn claim_keyboard_controller(
    registers: XhciRegisterSnapshot,
) -> Result<(), &'static str> {
    let status = usb_xhci_probe::read_port_status_from_mapped_window(
        usb_xhci_probe::XHCI_MMIO_VIRT,
        registers,
    )
    .map_err(|_| "USB_LEGACY_CAPABILITIES")?;
    let Some(legacy) = status.legacy_support else {
        return Ok(());
    };
    write_mmio_u32(legacy.byte_offset, legacy.header | OS_OWNED).map_err(driver_error)?;
    let start = timer::ticks();
    for _ in 0..20_000_000 {
        let owned = read_mmio_u32(legacy.byte_offset).map_err(driver_error)?;
        if owned & (BIOS_OWNED | OS_OWNED) == OS_OWNED {
            let control = read_mmio_u32(legacy.byte_offset + 4).map_err(driver_error)?;
            return write_mmio_u32(legacy.byte_offset + 4, control & LEGACY_CONTROL_PRESERVE)
                .map_err(driver_error);
        }
        if timer::ticks().wrapping_sub(start) >= u64::from(timer::TIMER_HZ) {
            break;
        }
        core::hint::spin_loop();
    }
    Err("USB_FIRMWARE_OWNERSHIP")
}

pub(crate) fn stop_keyboard_controller(
    registers: XhciRegisterSnapshot,
) -> Result<(), &'static str> {
    stop_controller(registers).map_err(driver_error)
}

pub(crate) struct XhciWorldKeyboard {
    registers: XhciRegisterSnapshot,
    dma: XhciDmaState,
    events: XhciEventRingConsumer,
    transfers: XhciInterruptTransferProducer,
    pending: Option<u64>,
    slot: u8,
    endpoint: u8,
}

impl XhciWorldKeyboard {
    pub(crate) fn start(
        registers: XhciRegisterSnapshot,
        port: u8,
        vendor: u16,
        product: u16,
    ) -> Result<Self, &'static str> {
        let mut state = initialize_command_probe(registers, port).map_err(driver_error)?;
        let configuration = configuration_from_command_state(
            registers,
            port,
            &mut state,
            Some(BOOT_KEYBOARD_PROTOCOL),
        )
        .map_err(driver_error)?;
        let descriptor = configuration.descriptor.descriptor;
        if (descriptor.vendor_id, descriptor.product_id) != (vendor, product) {
            return Err("USB_KEYBOARD_IDENTITY");
        }
        let interface = configuration.configuration;
        if interface.interrupt_in_max_packet_size & 0x07ff < BOOT_REPORT_BYTES {
            return Err("USB_KEYBOARD_PACKET_SIZE");
        }
        let endpoint = configure_interrupt_endpoint(registers, &mut state, configuration)
            .map_err(driver_error)?;
        // HID SET_PROTOCOL with wValue 0 selects the boot keyboard report.
        let boot_protocol = XhciTrb::new(
            0x0b21 | (u64::from(interface.interface_number) << 32),
            0,
            8,
            command_trb_control(XHCI_TRB_TYPE_SETUP_STAGE, true) | XHCI_TRB_IMMEDIATE_DATA,
        );
        let completion = submit_ep0_no_data_request(
            registers,
            state.dma,
            state.result.slot_id,
            &mut state.event_consumer,
            &mut state.control_index,
            boot_protocol,
        )
        .map_err(driver_error)?;
        if completion.completion_code() != XHCI_COMPLETION_SUCCESS {
            return Err("USB_KEYBOARD_BOOT_PROTOCOL");
        }
        Ok(Self {
            registers,
            dma: state.dma,
            events: state.event_consumer,
            transfers: XhciInterruptTransferProducer::new(),
            pending: None,
            slot: state.result.slot_id,
            endpoint: endpoint.endpoint_id,
        })
    }

    pub(crate) fn poll(&mut self) -> Result<Option<[u8; 8]>, &'static str> {
        let pending = match self.pending {
            Some(pointer) => pointer,
            None => self.arm()?,
        };
        for _ in 0..XHCI_EVENT_RING_TRBS {
            let index = self.events.index();
            if !self.events.accepts(read_trb(event_ring_ptr(), index)) {
                return Ok(None);
            }
            // xHCI publishes the event Cycle bit after the other TRB words.
            // Read the payload after observing the new cycle before ownership
            // is returned by updating the event dequeue pointer.
            compiler_fence(Ordering::Acquire);
            let event = read_trb(event_ring_ptr(), index);
            let Some(event) = accept_event_at_consumer(event, &mut self.events) else {
                return Ok(None);
            };
            ack_event(self.registers, self.dma, self.events.index()).map_err(driver_error)?;
            if event.trb_type() == PORT_STATUS_CHANGE {
                continue;
            }
            let report = validate_interrupt_transfer_event(
                event,
                pending,
                self.slot,
                self.endpoint,
                BOOT_REPORT_BYTES,
            )
            .map_err(driver_error)?;
            if report.actual_length != BOOT_REPORT_BYTES {
                return Err("USB_KEYBOARD_REPORT_LENGTH");
            }
            let (bytes, _) = capture_interrupt_report_prefix(report.actual_length);
            if self.transfers.complete().map_err(driver_error)? {
                crate::serial::write_line("PYTHOS:WORLD:USB_RING_WRAP");
            }
            self.pending = None;
            // Capture the next reply while the world handles and draws this
            // one. The completed bytes are now in a CPU-owned stack value.
            self.arm()?;
            return Ok(Some(bytes));
        }
        Ok(None)
    }

    fn arm(&mut self) -> Result<u64, &'static str> {
        let cursor = self.transfers.arm().map_err(driver_error)?;
        let trb = prepare_interrupt_transfer_at(self.dma, BOOT_REPORT_BYTES, cursor)
            .map_err(driver_error)?;
        let doorbell = doorbell_offset(self.registers, self.slot).map_err(driver_error)?;
        write_mmio_u32(doorbell, u32::from(self.endpoint)).map_err(driver_error)?;
        self.pending = Some(trb);
        Ok(trb)
    }
}

pub(super) fn apply_device_control_packet_size(
    registers: XhciRegisterSnapshot,
    state: &mut XhciCommandProbeState,
    address: XhciAddressProbeResult,
) -> Result<(), XhciDriverError> {
    // Full-speed devices can declare a larger endpoint-zero packet size.
    // A read of the first eight descriptor bytes is valid with the default.
    if address.port_speed != 1 {
        return Ok(());
    }
    let setup = XhciTrb::new(
        (USB_REQUEST_GET_DESCRIPTOR_DEVICE & 0x0000_ffff_ffff_ffff) | (8 << 48),
        0,
        8,
        command_trb_control(XHCI_TRB_TYPE_SETUP_STAGE, true)
            | XHCI_TRB_IMMEDIATE_DATA
            | (XHCI_SETUP_TRANSFER_TYPE_IN << XHCI_SETUP_TRANSFER_TYPE_SHIFT),
    );
    let data = device_descriptor_data_trb(state.dma.descriptor_buffer_phys, true);
    let prefix_data = XhciTrb::new(data.parameter(), 0, 8, data.control());
    let completion = submit_ep0_control_read(
        registers,
        state.dma,
        state.result.slot_id,
        &mut state.event_consumer,
        &mut state.control_index,
        setup,
        prefix_data,
    )?;
    let descriptor = read_device_descriptor_buffer();
    let max_packet = descriptor[7];
    if completion.completion_code() != XHCI_COMPLETION_SUCCESS
        || descriptor[0] != 18
        || descriptor[1] != 1
        || !matches!(max_packet, 8 | 16 | 32 | 64)
    {
        return Err(XhciDriverError::DeviceDescriptorNonSuccess);
    }
    if max_packet == 8 {
        return Ok(());
    }
    zero_dma_page(input_context_ptr());
    write_dma_u32(input_context_ptr(), 4, 1 << 1);
    let ep0 = usize::from(address.context_size) * 2;
    write_dma_u32(input_context_ptr(), ep0 + 4, u32::from(max_packet) << 16);
    let command = XhciTrb::new(
        state.dma.input_context_phys,
        0,
        0,
        command_trb_control(EVALUATE_CONTEXT, true) | (u32::from(state.result.slot_id) << 24),
    );
    submit_command_trb(
        registers,
        state.dma,
        state.next_command_index,
        command,
        XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT,
        &mut state.event_consumer,
    )?;
    state.next_command_index += 1;
    Ok(())
}

pub(crate) fn driver_error(error: XhciDriverError) -> &'static str {
    let marker = error.marker();
    crate::serial::write_line(marker);
    marker.rsplit(':').next().unwrap_or("USB_DRIVER")
}
