use pythos_shared::{
    object_shell_abi::PackedCapability,
    pyth_command_abi::{
        COMMAND_RESULT_STATUS_OK, PythCommand, PythCommandResult, command_kind_is_known,
    },
    pyth_runtime_abi::{HostCallResult, MAX_HOST_RESULT_BYTES},
};
use pythos_user_pyth_runtime::interpreter::{Host, HostError};

pub struct SessionCommandHost<'a> {
    command_handle: PackedCapability,
    command: &'a PythCommand,
    payload: &'a [u8],
    command_read: bool,
    result: Option<PythCommandResult>,
}

impl<'a> SessionCommandHost<'a> {
    pub fn new(
        command_handle: PackedCapability,
        command: &'a PythCommand,
        payload: &'a [u8],
    ) -> Result<Self, HostError> {
        if command_handle.raw() == 0
            || !command_kind_is_known(command.kind)
            || usize::try_from(command.payload_len).ok() != Some(payload.len())
            || payload.len() > MAX_HOST_RESULT_BYTES
            || core::str::from_utf8(payload).is_err()
        {
            return Err(HostError::Denied);
        }
        Ok(Self {
            command_handle,
            command,
            payload,
            command_read: false,
            result: None,
        })
    }

    pub const fn result(&self) -> Option<PythCommandResult> {
        self.result
    }

    fn accepts_handle(&self, capability: PackedCapability) -> bool {
        capability == self.command_handle && capability.raw() != 0
    }
}

impl Host for SessionCommandHost<'_> {
    fn system_log(&mut self, _capability: PackedCapability, _text: &[u8]) -> Result<(), HostError> {
        Err(HostError::Denied)
    }

    fn object_create(
        &mut self,
        _capability: PackedCapability,
        _kind: &[u8],
    ) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn object_query(
        &mut self,
        _capability: PackedCapability,
        _kind: &[u8],
    ) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn object_inspect(
        &mut self,
        _capability: PackedCapability,
        _object_id: u64,
    ) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn object_revise(
        &mut self,
        _capability: PackedCapability,
        _object_id: u64,
        _text: &[u8],
    ) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn object_history(
        &mut self,
        _capability: PackedCapability,
        _object_id: u64,
    ) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn task_context(&mut self, _capability: PackedCapability) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn task_propose(
        &mut self,
        _capability: PackedCapability,
        _candidate_task_id: u64,
        _score: u64,
    ) -> Result<(), HostError> {
        Err(HostError::Denied)
    }

    fn command_read(&mut self, capability: PackedCapability) -> Result<HostCallResult, HostError> {
        if !self.accepts_handle(capability) || self.command_read {
            return Err(HostError::Denied);
        }
        self.command_read = true;
        let mut result = HostCallResult::empty(0);
        result.reserved0 = u32::from(self.command.kind);
        result.object_id = self.command.object_id;
        result.revision = self.command.task_id;
        result.capability = PackedCapability::from_raw(self.command.proposal_id);
        result.bytes_len = self.payload.len() as u16;
        result.bytes[..self.payload.len()].copy_from_slice(self.payload);
        Ok(result)
    }

    fn command_result_emit(
        &mut self,
        capability: PackedCapability,
        status: u16,
        text: &[u8],
    ) -> Result<(), HostError> {
        if !self.accepts_handle(capability)
            || !self.command_read
            || self.result.is_some()
            || status != COMMAND_RESULT_STATUS_OK
            || text != self.payload
        {
            return Err(HostError::Denied);
        }
        self.result = Some(PythCommandResult {
            status,
            kind: self.command.kind,
            reserved0: 0,
            object_id: self.command.object_id,
            task_id: self.command.task_id,
            proposal_id: self.command.proposal_id,
            bytes_written: self.payload.len() as u64,
            reserved1: 0,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::{
        object_shell_abi::PackedCapability,
        pyth_command_abi::{COMMAND_KIND_CREATE_NOTE, COMMAND_RESULT_STATUS_OK, PythCommand},
        pyth_runtime_abi::HostCallResult,
    };
    use pythos_user_pyth_runtime::interpreter::{Host, HostError};

    const COMMAND_HANDLE: PackedCapability = PackedCapability::from_parts(9, 2);

    fn command() -> PythCommand {
        PythCommand {
            object_id: 11,
            task_id: 12,
            proposal_id: 13,
            payload_len: 10,
            ..PythCommand::empty(COMMAND_KIND_CREATE_NOTE)
        }
    }

    #[test]
    fn command_host_accepts_only_its_import_and_exactly_one_read_then_matching_emit() {
        // Catches authority widening, duplicate access, or emitting a result for a different command.
        let command = command();
        let mut host = SessionCommandHost::new(COMMAND_HANDLE, &command, b"slice2-one").unwrap();
        let read = host.command_read(COMMAND_HANDLE).unwrap();
        assert_eq!(read.reserved0, u32::from(COMMAND_KIND_CREATE_NOTE));
        assert_eq!(read.object_id, 11);
        assert_eq!(read.revision, 12);
        assert_eq!(read.capability.raw(), 13);
        assert_eq!(&read.bytes[..read.bytes_len.into()], b"slice2-one");
        assert_eq!(host.command_read(COMMAND_HANDLE), Err(HostError::Denied));
        assert_eq!(
            host.command_result_emit(COMMAND_HANDLE, COMMAND_RESULT_STATUS_OK, b"slice2-one"),
            Ok(())
        );
        assert_eq!(
            host.command_result_emit(COMMAND_HANDLE, COMMAND_RESULT_STATUS_OK, b"slice2-one"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.result().unwrap(),
            PythCommandResult {
                status: COMMAND_RESULT_STATUS_OK,
                kind: COMMAND_KIND_CREATE_NOTE,
                reserved0: 0,
                object_id: 11,
                task_id: 12,
                proposal_id: 13,
                bytes_written: 10,
                reserved1: 0,
            }
        );
    }

    #[test]
    fn command_host_rejects_invalid_operation_order_authority_and_result() {
        // Catches emit-before-read, wrong handle, status, or payload bypasses.
        let command = command();
        let wrong = PackedCapability::from_parts(10, 2);
        let mut host = SessionCommandHost::new(COMMAND_HANDLE, &command, b"slice2-one").unwrap();
        assert_eq!(host.command_read(wrong), Err(HostError::Denied));
        assert_eq!(
            host.command_result_emit(COMMAND_HANDLE, COMMAND_RESULT_STATUS_OK, b"slice2-one"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.command_read(COMMAND_HANDLE),
            Ok(command_result(&command, b"slice2-one"))
        );
        assert_eq!(
            host.command_result_emit(wrong, COMMAND_RESULT_STATUS_OK, b"slice2-one"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.command_result_emit(COMMAND_HANDLE, 1, b"slice2-one"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.command_result_emit(COMMAND_HANDLE, COMMAND_RESULT_STATUS_OK, b"wrong"),
            Err(HostError::Denied)
        );
    }

    #[test]
    fn command_host_rejects_a_zero_imported_handle() {
        // Catches treating the absence of command authority as a valid imported handle.
        let command = command();
        assert!(matches!(
            SessionCommandHost::new(PackedCapability::from_raw(0), &command, b"slice2-one"),
            Err(HostError::Denied)
        ));
    }

    #[test]
    fn command_host_denies_every_unrelated_host_operation() {
        // Catches exposing generic runtime authority through the session command adapter.
        let command = command();
        let mut host = SessionCommandHost::new(COMMAND_HANDLE, &command, b"slice2-one").unwrap();
        assert_eq!(
            host.system_log(COMMAND_HANDLE, b"no"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.object_create(COMMAND_HANDLE, b"note"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.object_query(COMMAND_HANDLE, b"note"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.object_inspect(COMMAND_HANDLE, 1),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.object_revise(COMMAND_HANDLE, 1, b"no"),
            Err(HostError::Denied)
        );
        assert_eq!(
            host.object_history(COMMAND_HANDLE, 1),
            Err(HostError::Denied)
        );
        assert_eq!(host.task_context(COMMAND_HANDLE), Err(HostError::Denied));
        assert_eq!(
            host.task_propose(COMMAND_HANDLE, 1, 1),
            Err(HostError::Denied)
        );
    }

    fn command_result(command: &PythCommand, text: &[u8]) -> HostCallResult {
        let mut result = HostCallResult::empty(0);
        result.reserved0 = u32::from(command.kind);
        result.object_id = command.object_id;
        result.revision = command.task_id;
        result.capability = PackedCapability::from_raw(command.proposal_id);
        result.bytes_len = text.len() as u16;
        result.bytes[..text.len()].copy_from_slice(text);
        result
    }
}
