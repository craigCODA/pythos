//! Private finite policy contract for the Phase 14 socket proof.
//!
//! This is deliberately not a public ABI, syscall, or kernel socket object.
//! Task 2 will adapt this bounded policy to the existing NetworkPort consumer
//! path.

use pythos_shared::network_port_abi::NETWORK_PORT_RESOURCE_ID_NAMESPACE;
use pythos_shared::socket_markers::{SOCKET_DENIED_MARKERS, SOCKET_GRANTED_MARKERS};

pub(crate) const SOCKET_PAYLOAD_BYTES: usize = 6;
pub(crate) const NETWORK_PORT_READ_RIGHT: u32 = 1 << 0;
pub(crate) const NETWORK_PORT_SEND_RIGHT: u32 = 1 << 2;
pub(crate) const NETWORK_PORT_REQUIRED_RIGHTS: u32 =
    NETWORK_PORT_READ_RIGHT | NETWORK_PORT_SEND_RIGHT;
pub(crate) const SOCKET_NETWORK_RESOURCE_ID: u64 = NETWORK_PORT_RESOURCE_ID_NAMESPACE | 1;
pub(crate) const SOCKET_CONSUMER_HOLDER_ID: u64 = 0x5059_534F_4353_0001;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SocketEndpoint {
    pub(crate) local_ipv4: [u8; 4],
    pub(crate) peer_ipv4: [u8; 4],
    pub(crate) local_port: u16,
    pub(crate) peer_port: u16,
}

pub(crate) const ACCEPTED_SOCKET_ENDPOINT: SocketEndpoint = SocketEndpoint {
    local_ipv4: [192, 168, 14, 2],
    peer_ipv4: [192, 168, 14, 1],
    local_port: 0x1505,
    peer_port: 0x1506,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NetworkPortAuthority {
    pub(crate) resource_id: u64,
    pub(crate) holder_id: u64,
    pub(crate) rights: u32,
    pub(crate) valid: bool,
}

impl NetworkPortAuthority {
    pub(crate) const fn valid_for_socket() -> Self {
        Self {
            resource_id: SOCKET_NETWORK_RESOURCE_ID,
            holder_id: SOCKET_CONSUMER_HOLDER_ID,
            rights: NETWORK_PORT_REQUIRED_RIGHTS,
            valid: true,
        }
    }

    pub(crate) const fn forged() -> Self {
        Self {
            resource_id: SOCKET_NETWORK_RESOURCE_ID ^ 1,
            holder_id: SOCKET_CONSUMER_HOLDER_ID,
            rights: NETWORK_PORT_REQUIRED_RIGHTS,
            valid: true,
        }
    }

    pub(crate) const fn with_holder(holder_id: u64) -> Self {
        Self {
            resource_id: SOCKET_NETWORK_RESOURCE_ID,
            holder_id,
            rights: NETWORK_PORT_REQUIRED_RIGHTS,
            valid: true,
        }
    }

    pub(crate) const fn with_rights(rights: u32) -> Self {
        Self {
            resource_id: SOCKET_NETWORK_RESOURCE_ID,
            holder_id: SOCKET_CONSUMER_HOLDER_ID,
            rights,
            valid: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkPortAdmission {
    NotOperational,
    Operational,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SocketHandle {
    pub(crate) slot: u16,
    pub(crate) generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SocketState {
    Closed,
    Opening,
    Established,
    Closing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenError {
    MissingAuthority,
    InvalidAuthority,
    WrongEndpoint,
    NetworkPortNotOperational,
    AlreadyOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SocketError {
    BadHandle,
    BadPayload,
    BufferTooSmall,
    WrongState,
}

pub(crate) struct SocketPolicy {
    state: SocketState,
    authority: Option<NetworkPortAuthority>,
    handle: Option<SocketHandle>,
    emitted_frames: usize,
}

impl SocketPolicy {
    pub(crate) const fn new() -> Self {
        Self {
            state: SocketState::Closed,
            authority: None,
            handle: None,
            emitted_frames: 0,
        }
    }

    pub(crate) const fn state(&self) -> SocketState {
        self.state
    }

    pub(crate) const fn handle(&self) -> Option<SocketHandle> {
        self.handle
    }

    pub(crate) const fn emitted_frames(&self) -> usize {
        self.emitted_frames
    }

    pub(crate) fn open(
        &mut self,
        authority: Option<NetworkPortAuthority>,
        endpoint: SocketEndpoint,
        admission: NetworkPortAdmission,
    ) -> Result<SocketHandle, OpenError> {
        if self.handle.is_some() {
            return Err(OpenError::AlreadyOpen);
        }
        let authority = authority.ok_or(OpenError::MissingAuthority)?;
        if !self.authority_is_valid(authority) {
            return Err(OpenError::InvalidAuthority);
        }
        if endpoint != ACCEPTED_SOCKET_ENDPOINT {
            return Err(OpenError::WrongEndpoint);
        }
        if admission != NetworkPortAdmission::Operational {
            return Err(OpenError::NetworkPortNotOperational);
        }
        let handle = SocketHandle {
            slot: 0,
            generation: 1,
        };
        self.authority = Some(authority);
        self.handle = Some(handle);
        self.state = SocketState::Opening;
        Ok(handle)
    }

    pub(crate) fn mark_established(
        &mut self,
        handle: SocketHandle,
        authority: NetworkPortAuthority,
    ) -> Result<(), SocketError> {
        self.validate_handle(handle, authority)?;
        if self.state != SocketState::Opening {
            return Err(SocketError::WrongState);
        }
        self.state = SocketState::Established;
        Ok(())
    }

    pub(crate) fn send(
        &mut self,
        handle: SocketHandle,
        authority: NetworkPortAuthority,
        payload: &[u8],
    ) -> Result<(), SocketError> {
        self.validate_handle(handle, authority)?;
        if self.state != SocketState::Established {
            return Err(SocketError::WrongState);
        }
        if payload.len() != SOCKET_PAYLOAD_BYTES {
            return Err(SocketError::BadPayload);
        }
        self.emitted_frames += 1;
        Ok(())
    }

    pub(crate) fn receive(
        &self,
        handle: SocketHandle,
        authority: NetworkPortAuthority,
        output: &mut [u8],
    ) -> Result<(), SocketError> {
        self.validate_handle(handle, authority)?;
        if self.state != SocketState::Established {
            return Err(SocketError::WrongState);
        }
        if output.len() < SOCKET_PAYLOAD_BYTES {
            return Err(SocketError::BufferTooSmall);
        }
        output[..SOCKET_PAYLOAD_BYTES].copy_from_slice(b"PYTCPR");
        Ok(())
    }

    pub(crate) fn close(
        &mut self,
        handle: SocketHandle,
        authority: NetworkPortAuthority,
    ) -> Result<(), SocketError> {
        self.validate_handle(handle, authority)?;
        if self.state != SocketState::Established {
            return Err(SocketError::WrongState);
        }
        self.state = SocketState::Closing;
        self.handle = None;
        self.authority = None;
        self.state = SocketState::Closed;
        Ok(())
    }

    pub(crate) fn revoke(&mut self) {
        self.state = SocketState::Closed;
        self.handle = None;
        self.authority = None;
    }

    fn authority_is_valid(&self, authority: NetworkPortAuthority) -> bool {
        authority.valid
            && authority.resource_id == SOCKET_NETWORK_RESOURCE_ID
            && authority.holder_id == SOCKET_CONSUMER_HOLDER_ID
            && authority.rights == NETWORK_PORT_REQUIRED_RIGHTS
    }

    fn validate_handle(
        &self,
        handle: SocketHandle,
        authority: NetworkPortAuthority,
    ) -> Result<(), SocketError> {
        if self.handle != Some(handle)
            || self.authority != Some(authority)
            || !self.authority_is_valid(authority)
        {
            return Err(SocketError::BadHandle);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::socket_markers::{
        SOCKET_DENIED_BOOTSTRAPPED_MARKER, SOCKET_DENIED_READY_MARKER,
        SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER, SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER,
    };

    fn operational_open() -> (SocketPolicy, NetworkPortAuthority, SocketHandle) {
        let authority = NetworkPortAuthority::valid_for_socket();
        let mut policy = SocketPolicy::new();
        let handle = policy
            .open(
                Some(authority),
                ACCEPTED_SOCKET_ENDPOINT,
                NetworkPortAdmission::Operational,
            )
            .unwrap();
        (policy, authority, handle)
    }

    #[test]
    fn open_requires_authority_before_endpoint_and_network_use() {
        let mut policy = SocketPolicy::new();
        assert_eq!(
            policy.open(
                None,
                ACCEPTED_SOCKET_ENDPOINT,
                NetworkPortAdmission::Operational,
            ),
            Err(OpenError::MissingAuthority)
        );
        assert_eq!(policy.handle(), None);
        assert_eq!(policy.emitted_frames(), 0);
    }

    #[test]
    fn open_requires_exact_endpoint_and_operational_network_port() {
        let authority = NetworkPortAuthority::valid_for_socket();
        let mut policy = SocketPolicy::new();
        let mut wrong = ACCEPTED_SOCKET_ENDPOINT;
        wrong.peer_port += 1;
        assert_eq!(
            policy.open(Some(authority), wrong, NetworkPortAdmission::Operational,),
            Err(OpenError::WrongEndpoint)
        );
        assert_eq!(
            policy.open(
                Some(authority),
                ACCEPTED_SOCKET_ENDPOINT,
                NetworkPortAdmission::NotOperational,
            ),
            Err(OpenError::NetworkPortNotOperational)
        );
    }

    #[test]
    fn valid_open_allocates_only_slot_zero_generation_one() {
        let (policy, _, handle) = operational_open();
        assert_eq!(
            handle,
            SocketHandle {
                slot: 0,
                generation: 1
            }
        );
        assert_eq!(policy.state(), SocketState::Opening);
        assert_eq!(policy.handle(), Some(handle));
        assert_eq!(policy.emitted_frames(), 0);
    }

    #[test]
    fn bounded_lifecycle_rejects_bad_payload_and_stale_handles() {
        let (mut policy, authority, handle) = operational_open();
        policy.mark_established(handle, authority).unwrap();
        assert_eq!(
            policy.send(handle, authority, b"short"),
            Err(SocketError::BadPayload)
        );
        policy.send(handle, authority, b"PYTCPQ").unwrap();
        let mut output = [0u8; SOCKET_PAYLOAD_BYTES];
        policy.receive(handle, authority, &mut output).unwrap();
        assert_eq!(&output, b"PYTCPR");
        policy.close(handle, authority).unwrap();
        assert_eq!(policy.state(), SocketState::Closed);
        assert_eq!(
            policy.send(handle, authority, b"PYTCPQ"),
            Err(SocketError::BadHandle)
        );
        assert_eq!(policy.emitted_frames(), 1);
    }

    #[test]
    fn wrong_or_revoked_authority_cannot_use_handle() {
        let (mut policy, authority, handle) = operational_open();
        assert_eq!(
            policy.mark_established(handle, NetworkPortAuthority::forged()),
            Err(SocketError::BadHandle)
        );
        assert_eq!(
            policy.mark_established(handle, NetworkPortAuthority::with_holder(0xBAD)),
            Err(SocketError::BadHandle)
        );
        policy.mark_established(handle, authority).unwrap();
        policy.revoke();
        assert_eq!(policy.handle(), None);
        assert_eq!(
            policy.send(handle, authority, b"PYTCPQ"),
            Err(SocketError::BadHandle)
        );
    }

    #[test]
    fn denied_case_has_no_handle_or_frame() {
        let mut policy = SocketPolicy::new();
        assert!(
            policy
                .open(
                    Some(NetworkPortAuthority::with_rights(NETWORK_PORT_READ_RIGHT)),
                    ACCEPTED_SOCKET_ENDPOINT,
                    NetworkPortAdmission::Operational,
                )
                .is_err()
        );
        assert_eq!(policy.handle(), None);
        assert_eq!(policy.emitted_frames(), 0);
    }

    #[test]
    fn denied_markers_are_exactly_ordered() {
        assert_eq!(
            [
                SOCKET_DENIED_BOOTSTRAPPED_MARKER,
                SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER,
                SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER,
                SOCKET_DENIED_READY_MARKER,
            ],
            SOCKET_DENIED_MARKERS
        );
        assert_eq!(SOCKET_DENIED_MARKERS.len(), 4);
        assert_eq!(SOCKET_GRANTED_MARKERS.len(), 8);
    }
}
