//! Runtime-only capability-scoped NetworkPort resource (ADR 0095).

use crate::capabilities::{CapabilityHandle, CapabilityTable, ResourceId};
#[cfg(any(test, feature = "virtio-net-probe"))]
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};
use pythos_shared::network_port_abi::{
    NETWORK_PORT_FLAG_MAC_ONLY, NETWORK_PORT_FLAG_NO_OFFLOAD, NETWORK_PORT_MAX_FRAME_BYTES,
    NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_RESOURCE_ID_NAMESPACE, NETWORK_PORT_STATE_FAILED,
    NETWORK_PORT_STATE_READY, NETWORK_PORT_STATE_RESET, NETWORK_PORT_STATUS_BAD_REQUEST,
    NETWORK_PORT_STATUS_BUFFER_TOO_SMALL, NETWORK_PORT_STATUS_EMPTY, NETWORK_PORT_STATUS_FAILED,
    NETWORK_PORT_STATUS_NOT_READY, NETWORK_PORT_STATUS_OK, NETWORK_PORT_STATUS_TRANSPORT_ERROR,
    NetworkPortDescriptionV1, NetworkPortResponseV1,
};

static NETWORK_PORT_BOOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(any(test, feature = "virtio-net-probe"))]
struct NetworkPortStorage(UnsafeCell<Option<NetworkPort<crate::virtio_net::VirtioTransport>>>);

// SAFETY:
// 1. Invariant: the current runtime has one CPU and admits one syscall at a time.
// 2. Established by: ADR 0051's retained single-process syscall model.
// 3. Lifetime: the optional port is boot-static and never yields references.
// 4. Pointer ownership: this storage exclusively owns the transport value.
// 5. Alignment: UnsafeCell preserves Option<NetworkPort<VirtioTransport>> alignment.
// 6. Mapped length: exactly one Option value is accessed.
// 7. Concurrency: future SMP must replace this with scheduler-owned synchronization.
// 8. Violation: concurrent access could mutate queue ownership simultaneously.
#[cfg(any(test, feature = "virtio-net-probe"))]
unsafe impl Sync for NetworkPortStorage {}

#[cfg(any(test, feature = "virtio-net-probe"))]
static ACTIVE_NETWORK_PORT: NetworkPortStorage = NetworkPortStorage(UnsafeCell::new(None));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkPortRegistrationError {
    AlreadyRegistered,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransportError {
    Fault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkPortBindingError {
    AlreadyBound,
    NotReady,
}

/// Boundary used by the resource. Implementations retain all device-visible
/// queues, headers, and DMA memory; this boundary accepts Ethernet bytes only.
pub(crate) trait NetworkTransport {
    fn mac(&self) -> [u8; 6];
    fn transmit(&mut self, frame: &[u8]) -> Result<(), TransportError>;
    fn try_receive_into(&mut self, output: &mut [u8]) -> Result<Option<usize>, TransportError>;
    fn reset(&mut self) -> Result<(), TransportError>;
}

#[derive(Clone, Copy)]
struct NetworkPortCapabilityBinding {
    consumer: CapabilityHandle,
    owner: CapabilityHandle,
}

pub(crate) struct NetworkPort<T: NetworkTransport> {
    resource_id: u64,
    transport: Option<T>,
    state: u16,
    receive_scratch: [u8; NETWORK_PORT_MAX_FRAME_BYTES],
    bound_capabilities: Option<NetworkPortCapabilityBinding>,
}

impl<T: NetworkTransport> NetworkPort<T> {
    /// Registers the only port identity available in this boot. The caller
    /// must invoke this only after its `VirtioTransport` is Operational.
    pub(crate) fn register_operational(transport: T) -> Result<Self, NetworkPortRegistrationError> {
        if NETWORK_PORT_BOOT_SEQUENCE
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(NetworkPortRegistrationError::AlreadyRegistered);
        }
        Ok(Self::new(transport, 1))
    }

    fn new(transport: T, sequence: u64) -> Self {
        Self {
            resource_id: NETWORK_PORT_RESOURCE_ID_NAMESPACE | sequence,
            transport: Some(transport),
            state: NETWORK_PORT_STATE_READY,
            receive_scratch: [0; NETWORK_PORT_MAX_FRAME_BYTES],
            bound_capabilities: None,
        }
    }

    pub(crate) const fn resource(&self) -> ResourceId {
        ResourceId::new(self.resource_id)
    }

    pub(crate) const fn state(&self) -> u16 {
        self.state
    }

    pub(crate) fn bind_capabilities(
        &mut self,
        consumer: CapabilityHandle,
        owner: CapabilityHandle,
    ) -> Result<(), NetworkPortBindingError> {
        if self.state != NETWORK_PORT_STATE_READY {
            return Err(NetworkPortBindingError::NotReady);
        }
        if self.bound_capabilities.is_some() {
            return Err(NetworkPortBindingError::AlreadyBound);
        }
        self.bound_capabilities = Some(NetworkPortCapabilityBinding { consumer, owner });
        Ok(())
    }

    pub(crate) fn service_response(&self) -> Option<NetworkPortResponseV1> {
        self.service_state_response()
    }

    pub(crate) fn describe(&self) -> NetworkPortDescriptionV1 {
        let mut description = NetworkPortDescriptionV1::empty();
        description.resource_id = self.resource_id;
        description.mac = self
            .transport
            .as_ref()
            .map_or([0; 6], NetworkTransport::mac);
        description.min_frame_bytes = NETWORK_PORT_MIN_FRAME_BYTES as u32;
        description.max_frame_bytes = NETWORK_PORT_MAX_FRAME_BYTES as u32;
        description.transport_flags = NETWORK_PORT_FLAG_MAC_ONLY | NETWORK_PORT_FLAG_NO_OFFLOAD;
        description.state = u32::from(self.state);
        description
    }

    pub(crate) fn send(
        &mut self,
        frame: &[u8],
        capabilities: &mut CapabilityTable,
    ) -> NetworkPortResponseV1 {
        if let Some(response) = self.service_state_response() {
            return response;
        }
        if !(NETWORK_PORT_MIN_FRAME_BYTES..=NETWORK_PORT_MAX_FRAME_BYTES).contains(&frame.len()) {
            return self.response(NETWORK_PORT_STATUS_BAD_REQUEST);
        }
        match self
            .transport_mut()
            .and_then(|transport| transport.transmit(frame))
        {
            Ok(()) => self.response(NETWORK_PORT_STATUS_OK),
            Err(_) => self.fail_transport(capabilities),
        }
    }

    pub(crate) fn try_receive_into(
        &mut self,
        output: &mut [u8],
        capabilities: &mut CapabilityTable,
    ) -> NetworkPortResponseV1 {
        self.try_receive_impl(output, capabilities)
    }

    fn try_receive_impl(
        &mut self,
        output: &mut [u8],
        capabilities: &mut CapabilityTable,
    ) -> NetworkPortResponseV1 {
        if let Some(response) = self.service_state_response() {
            return response;
        }
        if output.len() < NETWORK_PORT_MAX_FRAME_BYTES {
            let mut response = self.response(NETWORK_PORT_STATUS_BUFFER_TOO_SMALL);
            response.required_len = NETWORK_PORT_MAX_FRAME_BYTES as u64;
            return response;
        }
        if output.len() > NETWORK_PORT_MAX_FRAME_BYTES {
            return self.response(NETWORK_PORT_STATUS_BAD_REQUEST);
        }
        let receive_result = self
            .transport
            .as_mut()
            .ok_or(TransportError::Fault)
            .and_then(|transport| transport.try_receive_into(&mut self.receive_scratch));
        match receive_result {
            Ok(None) => self.response(NETWORK_PORT_STATUS_EMPTY),
            Ok(Some(frame_len))
                if (NETWORK_PORT_MIN_FRAME_BYTES..=NETWORK_PORT_MAX_FRAME_BYTES)
                    .contains(&frame_len) =>
            {
                output[..frame_len].copy_from_slice(&self.receive_scratch[..frame_len]);
                let mut response = self.response(NETWORK_PORT_STATUS_OK);
                response.frame_len = frame_len as u64;
                response
            }
            Ok(Some(_)) | Err(_) => self.fail_transport(capabilities),
        }
    }

    /// RESET is terminal in ABI v1. Dropping the private transport boundary
    /// prevents further queue publication or completion admission; its
    /// boot-local identity is deliberately not released for reuse.
    pub(crate) fn reset(&mut self, capabilities: &mut CapabilityTable) -> NetworkPortResponseV1 {
        let reset_result = self
            .transport
            .as_mut()
            .map_or(Err(TransportError::Fault), NetworkTransport::reset);
        self.transport = None;
        self.state = NETWORK_PORT_STATE_RESET;
        self.revoke_bound_capabilities(capabilities);
        self.response(if reset_result.is_ok() {
            NETWORK_PORT_STATUS_OK
        } else {
            NETWORK_PORT_STATUS_TRANSPORT_ERROR
        })
    }

    fn transport_mut(&mut self) -> Result<&mut T, TransportError> {
        self.transport.as_mut().ok_or(TransportError::Fault)
    }

    fn service_state_response(&self) -> Option<NetworkPortResponseV1> {
        match self.state {
            NETWORK_PORT_STATE_READY => None,
            NETWORK_PORT_STATE_FAILED => Some(self.response(NETWORK_PORT_STATUS_FAILED)),
            NETWORK_PORT_STATE_RESET => Some(self.response(NETWORK_PORT_STATUS_NOT_READY)),
            _ => Some(self.response(NETWORK_PORT_STATUS_NOT_READY)),
        }
    }

    fn fail_transport(&mut self, capabilities: &mut CapabilityTable) -> NetworkPortResponseV1 {
        self.transport = None;
        self.state = NETWORK_PORT_STATE_FAILED;
        self.revoke_bound_capabilities(capabilities);
        self.response(NETWORK_PORT_STATUS_TRANSPORT_ERROR)
    }

    fn revoke_bound_capabilities(&mut self, capabilities: &mut CapabilityTable) {
        let Some(binding) = self.bound_capabilities.take() else {
            return;
        };
        let _ = capabilities.revoke(binding.consumer);
        if binding.owner != binding.consumer {
            let _ = capabilities.revoke(binding.owner);
        }
    }

    fn response(&self, status: u16) -> NetworkPortResponseV1 {
        NetworkPortResponseV1::new(status, self.state)
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(transport: T) -> Self {
        Self::new(transport, 1)
    }
}

#[cfg(any(test, feature = "virtio-net-probe"))]
pub(crate) fn install_operational_transport(
    transport: crate::virtio_net::VirtioTransport,
) -> Result<(), NetworkPortRegistrationError> {
    let port = NetworkPort::register_operational(transport)?;
    // SAFETY: the same single-CPU, non-reentrant ownership invariant documented
    // by NetworkPortStorage applies; this installation happens before consumer
    // capability delivery and the port is never replaced in the boot.
    unsafe { *ACTIVE_NETWORK_PORT.0.get() = Some(port) };
    Ok(())
}

#[cfg(any(test, feature = "virtio-net-probe"))]
pub(crate) fn with_active_port<R>(
    f: impl FnOnce(&mut NetworkPort<crate::virtio_net::VirtioTransport>) -> R,
) -> Option<R> {
    // SAFETY: callers execute under the retained single-CPU syscall model; the
    // mutable borrow is confined to this callback and never retained.
    unsafe { (&mut *ACTIVE_NETWORK_PORT.0.get()).as_mut().map(f) }
}

#[cfg(any(test, feature = "virtio-net-probe"))]
impl NetworkTransport for crate::virtio_net::VirtioTransport {
    fn mac(&self) -> [u8; 6] {
        crate::virtio_net::VirtioTransport::mac(*self).bytes()
    }

    fn transmit(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        self.transmit(frame).map_err(|_| TransportError::Fault)
    }

    fn try_receive_into(&mut self, output: &mut [u8]) -> Result<Option<usize>, TransportError> {
        self.try_receive_into(output)
            .map_err(|_| TransportError::Fault)
    }

    fn reset(&mut self) -> Result<(), TransportError> {
        self.reset_for_teardown().map_err(|_| TransportError::Fault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::CapabilityTable;
    use pythos_shared::network_port_abi::{
        NETWORK_PORT_MAX_FRAME_BYTES, NETWORK_PORT_MIN_FRAME_BYTES, NETWORK_PORT_STATE_FAILED,
        NETWORK_PORT_STATE_READY, NETWORK_PORT_STATE_RESET, NETWORK_PORT_STATUS_BAD_REQUEST,
        NETWORK_PORT_STATUS_BUFFER_TOO_SMALL, NETWORK_PORT_STATUS_EMPTY, NETWORK_PORT_STATUS_OK,
        NETWORK_PORT_STATUS_TRANSPORT_ERROR,
    };

    #[derive(Clone, Copy)]
    struct FakeTransport {
        mac: [u8; 6],
        received: Option<[u8; NETWORK_PORT_MIN_FRAME_BYTES]>,
        invalid_receive_len: Option<usize>,
        transmit_error: bool,
        receive_error: bool,
        reset_error: bool,
        sent_len: usize,
    }

    impl FakeTransport {
        const fn ready() -> Self {
            Self {
                mac: [0x02, 0, 0, 0, 0, 1],
                received: None,
                invalid_receive_len: None,
                transmit_error: false,
                receive_error: false,
                reset_error: false,
                sent_len: 0,
            }
        }
    }

    impl NetworkTransport for FakeTransport {
        fn mac(&self) -> [u8; 6] {
            self.mac
        }

        fn transmit(&mut self, frame: &[u8]) -> Result<(), TransportError> {
            if self.transmit_error {
                return Err(TransportError::Fault);
            }
            self.sent_len = frame.len();
            Ok(())
        }

        fn try_receive_into(&mut self, output: &mut [u8]) -> Result<Option<usize>, TransportError> {
            if self.receive_error {
                return Err(TransportError::Fault);
            }
            if let Some(frame_len) = self.invalid_receive_len.take() {
                let written_len = frame_len.min(output.len());
                output[..written_len].fill(0x5A);
                return Ok(Some(frame_len));
            }
            let Some(frame) = self.received.take() else {
                return Ok(None);
            };
            output[..frame.len()].copy_from_slice(&frame);
            Ok(Some(frame.len()))
        }

        fn reset(&mut self) -> Result<(), TransportError> {
            if self.reset_error {
                Err(TransportError::Fault)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn describe_returns_boot_local_id_and_fixed_metadata() {
        let port = NetworkPort::new_for_test(FakeTransport::ready());

        let description = port.describe();

        assert_eq!(
            description.resource_id & 0xFFFF_0000_0000_0000,
            0x4E50_0000_0000_0000
        );
        assert_eq!(description.mac, [0x02, 0, 0, 0, 0, 1]);
        assert_eq!(
            description.min_frame_bytes as usize,
            NETWORK_PORT_MIN_FRAME_BYTES
        );
        assert_eq!(
            description.max_frame_bytes as usize,
            NETWORK_PORT_MAX_FRAME_BYTES
        );
        assert_eq!(description.state, u32::from(NETWORK_PORT_STATE_READY));
    }

    #[test]
    fn send_enforces_inclusive_ethernet_frame_bounds_before_transport() {
        let mut port = NetworkPort::new_for_test(FakeTransport::ready());
        let mut capabilities = CapabilityTable::new();

        assert_eq!(
            port.send(&[0; NETWORK_PORT_MIN_FRAME_BYTES - 1], &mut capabilities)
                .status,
            NETWORK_PORT_STATUS_BAD_REQUEST
        );
        assert_eq!(
            port.send(&[0; NETWORK_PORT_MAX_FRAME_BYTES + 1], &mut capabilities)
                .status,
            NETWORK_PORT_STATUS_BAD_REQUEST
        );
        assert_eq!(
            port.send(&[0; NETWORK_PORT_MIN_FRAME_BYTES], &mut capabilities)
                .status,
            NETWORK_PORT_STATUS_OK
        );
        assert_eq!(
            port.send(&[0; NETWORK_PORT_MAX_FRAME_BYTES], &mut capabilities)
                .status,
            NETWORK_PORT_STATUS_OK
        );
    }

    #[test]
    fn try_receive_requires_exact_maximum_capacity_before_transport() {
        let mut port = NetworkPort::new_for_test(FakeTransport::ready());
        let mut capabilities = CapabilityTable::new();
        let mut too_small = [0; NETWORK_PORT_MAX_FRAME_BYTES - 1];
        let mut too_large = [0; NETWORK_PORT_MAX_FRAME_BYTES + 1];

        assert_eq!(
            port.try_receive_into(&mut too_small, &mut capabilities)
                .status,
            NETWORK_PORT_STATUS_BUFFER_TOO_SMALL
        );
        assert_eq!(
            port.try_receive_into(&mut too_large, &mut capabilities)
                .status,
            NETWORK_PORT_STATUS_BAD_REQUEST
        );

        let mut output = [0; NETWORK_PORT_MAX_FRAME_BYTES];
        assert_eq!(
            port.try_receive_into(&mut output, &mut capabilities).status,
            NETWORK_PORT_STATUS_EMPTY
        );
    }

    #[test]
    fn try_receive_reports_actual_frame_length_without_exposing_transport_failure() {
        let mut transport = FakeTransport::ready();
        transport.received = Some([0xA5; NETWORK_PORT_MIN_FRAME_BYTES]);
        let mut port = NetworkPort::new_for_test(transport);
        let mut capabilities = CapabilityTable::new();
        let mut output = [0; NETWORK_PORT_MAX_FRAME_BYTES];

        let response = port.try_receive_into(&mut output, &mut capabilities);

        assert_eq!(response.status, NETWORK_PORT_STATUS_OK);
        assert_eq!(response.frame_len, NETWORK_PORT_MIN_FRAME_BYTES as u64);
        assert_eq!(
            &output[..NETWORK_PORT_MIN_FRAME_BYTES],
            &[0xA5; NETWORK_PORT_MIN_FRAME_BYTES]
        );
    }

    #[test]
    fn transport_error_transitions_port_to_failed_without_frame_delivery() {
        let mut transport = FakeTransport::ready();
        transport.receive_error = true;
        let mut port = NetworkPort::new_for_test(transport);
        let mut capabilities = CapabilityTable::new();
        let mut output = [0xCC; NETWORK_PORT_MAX_FRAME_BYTES];

        let response = port.try_receive_into(&mut output, &mut capabilities);

        assert_eq!(response.status, NETWORK_PORT_STATUS_TRANSPORT_ERROR);
        assert_eq!(response.state, NETWORK_PORT_STATE_FAILED);
        assert_eq!(output, [0xCC; NETWORK_PORT_MAX_FRAME_BYTES]);
    }

    #[test]
    fn invalid_receive_length_is_terminal_without_partial_frame_delivery() {
        let mut transport = FakeTransport::ready();
        transport.invalid_receive_len = Some(NETWORK_PORT_MIN_FRAME_BYTES - 1);
        let mut port = NetworkPort::new_for_test(transport);
        let mut capabilities = CapabilityTable::new();
        let mut output = [0xCC; NETWORK_PORT_MAX_FRAME_BYTES];

        let response = port.try_receive_into(&mut output, &mut capabilities);

        assert_eq!(response.status, NETWORK_PORT_STATUS_TRANSPORT_ERROR);
        assert_eq!(response.state, NETWORK_PORT_STATE_FAILED);
        assert_eq!(output, [0xCC; NETWORK_PORT_MAX_FRAME_BYTES]);
    }

    #[test]
    fn reset_is_administrative_and_terminal() {
        let mut port = NetworkPort::new_for_test(FakeTransport::ready());
        let mut capabilities = CapabilityTable::new();

        assert_eq!(
            port.reset(&mut capabilities).state,
            NETWORK_PORT_STATE_RESET
        );
        assert_eq!(
            port.send(&[0; NETWORK_PORT_MIN_FRAME_BYTES], &mut capabilities)
                .state,
            NETWORK_PORT_STATE_RESET
        );
    }

    #[test]
    fn reset_delegates_to_transport_and_remains_terminal_on_reset_error() {
        let mut transport = FakeTransport::ready();
        transport.reset_error = true;
        let mut port = NetworkPort::new_for_test(transport);
        let mut capabilities = CapabilityTable::new();

        let response = port.reset(&mut capabilities);

        assert_eq!(response.status, NETWORK_PORT_STATUS_TRANSPORT_ERROR);
        assert_eq!(response.state, NETWORK_PORT_STATE_RESET);
        assert_eq!(
            port.send(&[0; NETWORK_PORT_MIN_FRAME_BYTES], &mut capabilities)
                .state,
            NETWORK_PORT_STATE_RESET
        );
    }

    #[test]
    fn boot_registration_allocates_only_one_unreused_port_identity() {
        assert!(NetworkPort::register_operational(FakeTransport::ready()).is_ok());
        assert!(matches!(
            NetworkPort::register_operational(FakeTransport::ready()),
            Err(NetworkPortRegistrationError::AlreadyRegistered)
        ));
    }
}
