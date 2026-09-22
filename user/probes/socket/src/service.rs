//! Private bounded socket-operation state used by the finite proof consumer.

use pythos_shared::capability_abi::PackedCapability;

pub const SOCKET_PAYLOAD_BYTES: usize = 6;
pub const REQUEST_PAYLOAD: &[u8; SOCKET_PAYLOAD_BYTES] = b"PYTCPQ";
pub const RESPONSE_PAYLOAD: &[u8; SOCKET_PAYLOAD_BYTES] = b"PYTCPR";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub local_ipv4: [u8; 4],
    pub peer_ipv4: [u8; 4],
    pub local_port: u16,
    pub peer_port: u16,
}

pub const ACCEPTED_ENDPOINT: Endpoint = Endpoint {
    local_ipv4: [192, 168, 14, 2],
    peer_ipv4: [192, 168, 14, 1],
    local_port: 0x1505,
    peer_port: 0x1506,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityAuthority {
    pub capability: PackedCapability,
    pub operational: bool,
}

impl CapabilityAuthority {
    pub const fn new(capability: PackedCapability, operational: bool) -> Self {
        Self {
            capability,
            operational,
        }
    }

    pub const fn is_valid(self) -> bool {
        self.capability.raw() != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketHandle {
    pub slot: u16,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketState {
    Closed,
    Opening,
    Established,
    Closing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenError {
    MissingAuthority,
    InvalidAuthority,
    WrongEndpoint,
    AlreadyOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationError {
    BadHandle,
    BadPayload,
    BufferTooSmall,
    WrongState,
}

pub struct SocketService {
    state: SocketState,
    handle: Option<SocketHandle>,
    authority: Option<CapabilityAuthority>,
    next_generation: u32,
}

impl SocketService {
    pub const fn new() -> Self {
        Self {
            state: SocketState::Closed,
            handle: None,
            authority: None,
            next_generation: 1,
        }
    }

    pub const fn state(&self) -> SocketState {
        self.state
    }

    pub const fn handle(&self) -> Option<SocketHandle> {
        self.handle
    }

    pub fn open(
        &mut self,
        authority: Option<CapabilityAuthority>,
        endpoint: Endpoint,
    ) -> Result<SocketHandle, OpenError> {
        let authority = authority.ok_or(OpenError::MissingAuthority)?;
        if !authority.is_valid() || !authority.operational {
            return Err(OpenError::InvalidAuthority);
        }
        if endpoint != ACCEPTED_ENDPOINT {
            return Err(OpenError::WrongEndpoint);
        }
        if self.handle.is_some() {
            return Err(OpenError::AlreadyOpen);
        }
        let handle = SocketHandle {
            slot: 0,
            generation: self.next_generation,
        };
        self.handle = Some(handle);
        self.authority = Some(authority);
        self.state = SocketState::Opening;
        Ok(handle)
    }

    pub fn establish(
        &mut self,
        handle: SocketHandle,
        authority: CapabilityAuthority,
    ) -> Result<(), OperationError> {
        self.validate(handle, authority)?;
        if self.state != SocketState::Opening {
            return Err(OperationError::WrongState);
        }
        self.state = SocketState::Established;
        Ok(())
    }

    pub fn send(
        &self,
        handle: SocketHandle,
        authority: CapabilityAuthority,
        payload: &[u8],
    ) -> Result<(), OperationError> {
        self.validate(handle, authority)?;
        if self.state != SocketState::Established {
            return Err(OperationError::WrongState);
        }
        if payload != REQUEST_PAYLOAD {
            return Err(OperationError::BadPayload);
        }
        Ok(())
    }

    pub fn receive(
        &self,
        handle: SocketHandle,
        authority: CapabilityAuthority,
        peer_payload: &[u8],
        output: &mut [u8],
    ) -> Result<(), OperationError> {
        self.validate(handle, authority)?;
        if self.state != SocketState::Established {
            return Err(OperationError::WrongState);
        }
        if peer_payload != RESPONSE_PAYLOAD {
            return Err(OperationError::BadPayload);
        }
        if output.len() < SOCKET_PAYLOAD_BYTES {
            return Err(OperationError::BufferTooSmall);
        }
        output[..SOCKET_PAYLOAD_BYTES].copy_from_slice(RESPONSE_PAYLOAD);
        Ok(())
    }

    pub fn close(
        &mut self,
        handle: SocketHandle,
        authority: CapabilityAuthority,
    ) -> Result<(), OperationError> {
        self.validate(handle, authority)?;
        if self.state != SocketState::Established {
            return Err(OperationError::WrongState);
        }
        self.state = SocketState::Closing;
        self.invalidate();
        self.state = SocketState::Closed;
        Ok(())
    }

    pub fn revoke(&mut self) {
        self.invalidate();
        self.state = SocketState::Closed;
    }

    fn invalidate(&mut self) {
        self.next_generation = self.next_generation.wrapping_add(1);
        self.handle = None;
        self.authority = None;
    }

    fn validate(
        &self,
        handle: SocketHandle,
        authority: CapabilityAuthority,
    ) -> Result<(), OperationError> {
        if self.handle != Some(handle) || self.authority != Some(authority) || !authority.is_valid()
        {
            return Err(OperationError::BadHandle);
        }
        Ok(())
    }
}

impl Default for SocketService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHORITY: CapabilityAuthority = CapabilityAuthority {
        capability: PackedCapability::from_raw(1),
        operational: true,
    };

    fn open_service() -> (SocketService, SocketHandle) {
        let mut service = SocketService::new();
        let handle = service.open(Some(AUTHORITY), ACCEPTED_ENDPOINT).unwrap();
        (service, handle)
    }

    #[test]
    fn denied_open_allocates_no_handle() {
        let mut service = SocketService::new();
        assert_eq!(
            service.open(None, ACCEPTED_ENDPOINT),
            Err(OpenError::MissingAuthority)
        );
        assert_eq!(service.handle(), None);
    }

    #[test]
    fn open_requires_operational_authority_and_exact_endpoint() {
        let mut service = SocketService::new();
        assert_eq!(
            service.open(
                Some(CapabilityAuthority {
                    capability: PackedCapability::from_raw(1),
                    operational: false,
                }),
                ACCEPTED_ENDPOINT,
            ),
            Err(OpenError::InvalidAuthority)
        );
        let mut endpoint = ACCEPTED_ENDPOINT;
        endpoint.peer_port += 1;
        assert_eq!(
            service.open(Some(AUTHORITY), endpoint),
            Err(OpenError::WrongEndpoint)
        );
    }

    #[test]
    fn granted_operations_are_bounded_and_close_invalidates_handle() {
        let (mut service, handle) = open_service();
        service.establish(handle, AUTHORITY).unwrap();
        assert_eq!(
            service.send(handle, AUTHORITY, b"WRONG!"),
            Err(OperationError::BadPayload)
        );
        service.send(handle, AUTHORITY, REQUEST_PAYLOAD).unwrap();
        let mut output = [0u8; SOCKET_PAYLOAD_BYTES];
        service
            .receive(handle, AUTHORITY, RESPONSE_PAYLOAD, &mut output)
            .unwrap();
        assert_eq!(&output, RESPONSE_PAYLOAD);
        service.close(handle, AUTHORITY).unwrap();
        assert_eq!(service.handle(), None);
        assert_eq!(
            service.send(handle, AUTHORITY, REQUEST_PAYLOAD),
            Err(OperationError::BadHandle)
        );
        let reopened = service.open(Some(AUTHORITY), ACCEPTED_ENDPOINT).unwrap();
        assert_eq!(reopened.generation, 2);
        assert_ne!(reopened, handle);
    }

    #[test]
    fn revoke_invalidates_handle_and_rejects_wrong_peer_data() {
        let (mut service, handle) = open_service();
        service.establish(handle, AUTHORITY).unwrap();
        let mut output = [0u8; SOCKET_PAYLOAD_BYTES];
        assert_eq!(
            service.receive(handle, AUTHORITY, b"BAD!!!", &mut output),
            Err(OperationError::BadPayload)
        );
        service.revoke();
        assert_eq!(
            service.send(handle, AUTHORITY, REQUEST_PAYLOAD),
            Err(OperationError::BadHandle)
        );
    }
}
