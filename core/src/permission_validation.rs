//! Capability validation wrappers for privileged Phase 3 operations.
#![cfg_attr(test, allow(dead_code))]

use crate::capabilities::{
    CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask,
};
use crate::ipc_channels::{IpcChannel, IpcError, IpcMessage};
#[cfg(not(test))]
use crate::serial;
use crate::service_identity::{ServiceId, ServiceIdentityTable};
use crate::tasks::TaskId;

const IPC_SEND_RESOURCE: ResourceId = ResourceId::new(0x1FC0_0001);
const PERMISSION_MESSAGE_TYPE: u16 = 0x70;
const PERMISSION_PAYLOAD: [u8; 4] = [0x50, 0x45, 0x52, 0x4D];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionError {
    Capability(CapabilityError),
    Ipc(IpcError),
}

impl From<CapabilityError> for PermissionError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

impl From<IpcError> for PermissionError {
    fn from(error: IpcError) -> Self {
        Self::Ipc(error)
    }
}

pub fn send_with_capability(
    table: &CapabilityTable,
    caller: ServiceId,
    handle: CapabilityHandle,
    resource: ResourceId,
    channel: &mut IpcChannel,
    to: ServiceId,
    message: IpcMessage,
) -> Result<(), PermissionError> {
    table.validate(caller, handle, resource, RightsMask::new(RightsMask::SEND))?;
    channel.send(caller, to, message)?;
    Ok(())
}

pub fn run_permission_validation_self_test() -> Result<(), PermissionError> {
    let mut identities = ServiceIdentityTable::new();
    let sender = identities
        .register_task(TaskId::new(50))
        .map_err(|_| CapabilityError::InvalidHandle)?;
    let receiver = identities
        .register_task(TaskId::new(51))
        .map_err(|_| CapabilityError::InvalidHandle)?;
    let mut table = CapabilityTable::new();
    let allowed = table.grant(sender, IPC_SEND_RESOURCE, RightsMask::new(RightsMask::SEND))?;
    let denied = table.grant(sender, IPC_SEND_RESOURCE, RightsMask::new(RightsMask::READ))?;
    let mut channel = IpcChannel::new(sender, receiver);
    let message = IpcMessage::new(PERMISSION_MESSAGE_TYPE, &PERMISSION_PAYLOAD)?;

    send_with_capability(
        &table,
        sender,
        allowed,
        IPC_SEND_RESOURCE,
        &mut channel,
        receiver,
        message,
    )?;
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:PERMISSION:IPC_ALLOWED");

    let denied_message = IpcMessage::new(PERMISSION_MESSAGE_TYPE, &PERMISSION_PAYLOAD)?;
    if send_with_capability(
        &table,
        sender,
        denied,
        IPC_SEND_RESOURCE,
        &mut channel,
        receiver,
        denied_message,
    ) != Err(PermissionError::Capability(CapabilityError::MissingRights))
    {
        return Err(PermissionError::Capability(CapabilityError::MissingRights));
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:PERMISSION:IPC_DENIED");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privileged_ipc_send_requires_send_capability() {
        let mut identities = ServiceIdentityTable::new();
        let sender = identities.register_task(TaskId::new(50)).unwrap();
        let receiver = identities.register_task(TaskId::new(51)).unwrap();
        let mut table = CapabilityTable::new();
        let allowed = table
            .grant(sender, IPC_SEND_RESOURCE, RightsMask::new(RightsMask::SEND))
            .unwrap();
        let denied = table
            .grant(sender, IPC_SEND_RESOURCE, RightsMask::new(RightsMask::READ))
            .unwrap();
        let mut channel = IpcChannel::new(sender, receiver);
        let allowed_message =
            IpcMessage::new(PERMISSION_MESSAGE_TYPE, &PERMISSION_PAYLOAD).unwrap();
        let denied_message = IpcMessage::new(PERMISSION_MESSAGE_TYPE, &PERMISSION_PAYLOAD).unwrap();

        assert_eq!(
            send_with_capability(
                &table,
                sender,
                allowed,
                IPC_SEND_RESOURCE,
                &mut channel,
                receiver,
                allowed_message,
            ),
            Ok(())
        );
        assert_eq!(
            send_with_capability(
                &table,
                sender,
                denied,
                IPC_SEND_RESOURCE,
                &mut channel,
                receiver,
                denied_message,
            ),
            Err(PermissionError::Capability(CapabilityError::MissingRights))
        );
        assert_eq!(channel.receive(receiver), Ok(allowed_message));
        assert_eq!(channel.receive(receiver), Err(IpcError::QueueEmpty));
    }
}
