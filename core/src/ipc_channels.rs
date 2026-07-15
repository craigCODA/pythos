//! Fixed in-kernel IPC channels for Phase 3 bootstrap.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

const MAX_MESSAGE_SIZE: usize = 16;
const CHANNEL_QUEUE_DEPTH: usize = 2;
const IPC_MESSAGE_TYPE_BOOT_PROOF: u16 = 1;
const IPC_MESSAGE_TYPE_QUEUE_PROOF: u16 = 2;
const IPC_PROOF_PAYLOAD: [u8; 8] = [0x50, 0x59, 0x54, 0x48, 0x49, 0x50, 0x43, 0x31];
const QUEUE_PROOF_A: [u8; 4] = [0x51, 0x41, 0x30, 0x31];
const QUEUE_PROOF_B: [u8; 4] = [0x51, 0x42, 0x30, 0x32];
const QUEUE_PROOF_C: [u8; 4] = [0x51, 0x43, 0x30, 0x33];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpcError {
    InvalidEndpoint,
    MessageTooLarge,
    QueueFull,
    QueueEmpty,
    PayloadCorrupt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpcMessage {
    type_id: u16,
    length: usize,
    payload: [u8; MAX_MESSAGE_SIZE],
    checksum: u32,
}

impl IpcMessage {
    pub fn new(type_id: u16, bytes: &[u8]) -> Result<Self, IpcError> {
        if bytes.len() > MAX_MESSAGE_SIZE {
            return Err(IpcError::MessageTooLarge);
        }
        let mut payload = [0; MAX_MESSAGE_SIZE];
        payload[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            type_id,
            length: bytes.len(),
            payload,
            checksum: checksum(bytes),
        })
    }

    fn verify(&self) -> Result<(), IpcError> {
        if self.length > MAX_MESSAGE_SIZE {
            return Err(IpcError::PayloadCorrupt);
        }
        if checksum(&self.payload[..self.length]) != self.checksum {
            return Err(IpcError::PayloadCorrupt);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpcChannel {
    owner: ServiceId,
    peer: ServiceId,
    queue: [Option<QueuedMessage>; CHANNEL_QUEUE_DEPTH],
    head: usize,
    length: usize,
}

impl IpcChannel {
    pub const fn new(owner: ServiceId, peer: ServiceId) -> Self {
        Self {
            owner,
            peer,
            queue: [None; CHANNEL_QUEUE_DEPTH],
            head: 0,
            length: 0,
        }
    }

    pub fn send(
        &mut self,
        from: ServiceId,
        to: ServiceId,
        message: IpcMessage,
    ) -> Result<(), IpcError> {
        if !self.has_endpoint_pair(from, to) {
            return Err(IpcError::InvalidEndpoint);
        }
        message.verify()?;
        if self.length == CHANNEL_QUEUE_DEPTH {
            return Err(IpcError::QueueFull);
        }
        let tail = (self.head + self.length) % CHANNEL_QUEUE_DEPTH;
        self.queue[tail] = Some(QueuedMessage { from, to, message });
        self.length += 1;
        Ok(())
    }

    pub fn receive(&mut self, receiver: ServiceId) -> Result<IpcMessage, IpcError> {
        if receiver != self.owner && receiver != self.peer {
            return Err(IpcError::InvalidEndpoint);
        }
        let queued = self.queue[self.head].ok_or(IpcError::QueueEmpty)?;
        if queued.to != receiver {
            return Err(IpcError::QueueEmpty);
        }
        self.queue[self.head] = None;
        self.head = (self.head + 1) % CHANNEL_QUEUE_DEPTH;
        self.length -= 1;
        queued.message.verify()?;
        Ok(queued.message)
    }

    fn has_endpoint_pair(&self, from: ServiceId, to: ServiceId) -> bool {
        (from == self.owner && to == self.peer) || (from == self.peer && to == self.owner)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct QueuedMessage {
    from: ServiceId,
    to: ServiceId,
    message: IpcMessage,
}

pub fn run_self_test() -> Result<(), IpcError> {
    let mut identities = ServiceIdentityTable::new();
    let sender = identities
        .register_task(TaskId::new(20))
        .map_err(|_| IpcError::InvalidEndpoint)?;
    let receiver = identities
        .register_task(TaskId::new(21))
        .map_err(|_| IpcError::InvalidEndpoint)?;
    let mut channel = IpcChannel::new(sender, receiver);
    let message = IpcMessage::new(IPC_MESSAGE_TYPE_BOOT_PROOF, &IPC_PROOF_PAYLOAD)?;
    channel.send(sender, receiver, message)?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:IPC:SEND");
    let received = channel.receive(receiver)?;
    if received.type_id != IPC_MESSAGE_TYPE_BOOT_PROOF
        || received.length != IPC_PROOF_PAYLOAD.len()
        || received.payload[..received.length] != IPC_PROOF_PAYLOAD
        || received.checksum != checksum(&IPC_PROOF_PAYLOAD)
    {
        return Err(IpcError::PayloadCorrupt);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:IPC:RECV");
    Ok(())
}

pub fn run_bounded_queue_self_test() -> Result<(), IpcError> {
    let mut identities = ServiceIdentityTable::new();
    let sender = identities
        .register_task(TaskId::new(22))
        .map_err(|_| IpcError::InvalidEndpoint)?;
    let receiver = identities
        .register_task(TaskId::new(23))
        .map_err(|_| IpcError::InvalidEndpoint)?;
    let mut channel = IpcChannel::new(sender, receiver);
    let first = IpcMessage::new(IPC_MESSAGE_TYPE_QUEUE_PROOF, &QUEUE_PROOF_A)?;
    let second = IpcMessage::new(IPC_MESSAGE_TYPE_QUEUE_PROOF, &QUEUE_PROOF_B)?;
    let overflow = IpcMessage::new(IPC_MESSAGE_TYPE_QUEUE_PROOF, &QUEUE_PROOF_C)?;

    channel.send(sender, receiver, first)?;
    channel.send(sender, receiver, second)?;
    if channel.send(sender, receiver, overflow) != Err(IpcError::QueueFull) {
        return Err(IpcError::PayloadCorrupt);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:IPC:QUEUE_FULL");

    if channel.receive(receiver)? != first || channel.receive(receiver)? != second {
        return Err(IpcError::PayloadCorrupt);
    }
    if channel.receive(receiver) != Err(IpcError::QueueEmpty) {
        return Err(IpcError::PayloadCorrupt);
    }
    Ok(())
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |sum, byte| sum.wrapping_add(u32::from(*byte)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_message_round_trips_between_known_service_identities() {
        let mut identities = ServiceIdentityTable::new();
        let sender = identities.register_task(TaskId::new(20)).unwrap();
        let receiver = identities.register_task(TaskId::new(21)).unwrap();
        let mut channel = IpcChannel::new(sender, receiver);
        let message = IpcMessage::new(IPC_MESSAGE_TYPE_BOOT_PROOF, &IPC_PROOF_PAYLOAD).unwrap();

        assert_eq!(channel.send(sender, receiver, message), Ok(()));
        assert_eq!(channel.receive(receiver), Ok(message));
    }

    #[test]
    fn channel_rejects_unknown_service_identity() {
        let mut identities = ServiceIdentityTable::new();
        let sender = identities.register_task(TaskId::new(20)).unwrap();
        let receiver = identities.register_task(TaskId::new(21)).unwrap();
        let stranger = identities.register_task(TaskId::new(22)).unwrap();
        let mut channel = IpcChannel::new(sender, receiver);
        let message = IpcMessage::new(IPC_MESSAGE_TYPE_BOOT_PROOF, &IPC_PROOF_PAYLOAD).unwrap();

        assert_eq!(
            channel.send(stranger, receiver, message),
            Err(IpcError::InvalidEndpoint)
        );
    }

    #[test]
    fn message_payload_size_is_fixed() {
        let too_large = [0xAA; MAX_MESSAGE_SIZE + 1];

        assert_eq!(
            IpcMessage::new(IPC_MESSAGE_TYPE_BOOT_PROOF, &too_large),
            Err(IpcError::MessageTooLarge)
        );
    }

    #[test]
    fn full_queue_returns_explicit_error_without_dropping_messages() {
        let mut identities = ServiceIdentityTable::new();
        let sender = identities.register_task(TaskId::new(22)).unwrap();
        let receiver = identities.register_task(TaskId::new(23)).unwrap();
        let mut channel = IpcChannel::new(sender, receiver);
        let first = IpcMessage::new(IPC_MESSAGE_TYPE_QUEUE_PROOF, &QUEUE_PROOF_A).unwrap();
        let second = IpcMessage::new(IPC_MESSAGE_TYPE_QUEUE_PROOF, &QUEUE_PROOF_B).unwrap();
        let overflow = IpcMessage::new(IPC_MESSAGE_TYPE_QUEUE_PROOF, &QUEUE_PROOF_C).unwrap();

        assert_eq!(channel.send(sender, receiver, first), Ok(()));
        assert_eq!(channel.send(sender, receiver, second), Ok(()));
        assert_eq!(
            channel.send(sender, receiver, overflow),
            Err(IpcError::QueueFull)
        );
        assert_eq!(channel.receive(receiver), Ok(first));
        assert_eq!(channel.receive(receiver), Ok(second));
        assert_eq!(channel.receive(receiver), Err(IpcError::QueueEmpty));
    }
}
