//! Independently versioned terminal evidence for the bounded Viewing profile.

use crate::{pyth_command_abi::PythCommandResult, pyth_runtime_abi::GraphExitRecord};

pub const SESSION_VIEWING_RESULT_OFFSET: u64 = 2048;
pub const SESSION_VIEWING_RESULT_MAGIC: u64 = 0x3152_5745_4956_5950;
pub const SESSION_VIEWING_RESULT_COMPLETE: u16 = 1;
pub const SESSION_VIEWING_RESULT_RECOVERY: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewingSnapshotRecord {
    pub revision: u64,
    pub flags: u32,
    pub x: u16,
    pub y: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionViewingResultV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub terminal_status: u16,
    pub reserved0: u16,
    pub session_service_id: u64,
    pub runtime_principal_id: u64,
    pub graph_principal_id: u64,
    pub input_event_count: u64,
    pub invocation_count: u64,
    pub traversal_count: u64,
    pub activation_count: u64,
    pub graph_checkpoint_events: [u64; 2],
    pub snapshots: [ViewingSnapshotRecord; 8],
    pub command_results: [PythCommandResult; 2],
    pub graph_exits: [GraphExitRecord; 2],
    pub reserved1: [u64; 7],
}

impl SessionViewingResultV1 {
    pub const fn new(service: u64, runtime: u64, graph: u64) -> Self {
        Self {
            magic: SESSION_VIEWING_RESULT_MAGIC,
            abi_major: 1,
            abi_minor: 0,
            terminal_status: 0,
            reserved0: 0,
            session_service_id: service,
            runtime_principal_id: runtime,
            graph_principal_id: graph,
            input_event_count: 0,
            invocation_count: 0,
            traversal_count: 0,
            activation_count: 0,
            graph_checkpoint_events: [0; 2],
            snapshots: [ViewingSnapshotRecord {
                revision: 0,
                flags: 0,
                x: 0,
                y: 0,
            }; 8],
            command_results: [PythCommandResult::empty(0, 0); 2],
            graph_exits: [GraphExitRecord {
                status: 0,
                error_code: 0,
                last_node: 0,
                executed_nodes: 0,
                result_type: crate::pyth_runtime_abi::GRAPH_RESULT_UNIT,
                reserved0: 0,
                reserved1: 0,
                result_raw: 0,
            }; 2],
            reserved1: [0; 7],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewingResultError {
    Header,
    Identity,
    Status,
    Counters,
    Snapshot,
}

/// Validate completion evidence; graph outputs are independently checked against
/// the immutable fixture and admitted instruction budget by the kernel caller.
pub fn validate_session_viewing_result(
    result: &SessionViewingResultV1,
    identities: [u64; 3],
) -> Result<(), ViewingResultError> {
    if result.magic != SESSION_VIEWING_RESULT_MAGIC
        || result.abi_major != 1
        || result.abi_minor != 0
        || result.reserved0 != 0
        || result.reserved1 != [0; 7]
    {
        return Err(ViewingResultError::Header);
    }
    if identities.contains(&0)
        || identities[0] == identities[1]
        || identities[0] == identities[2]
        || identities[1] == identities[2]
        || identities
            != [
                result.session_service_id,
                result.runtime_principal_id,
                result.graph_principal_id,
            ]
    {
        return Err(ViewingResultError::Identity);
    }
    if result.terminal_status != SESSION_VIEWING_RESULT_COMPLETE {
        return Err(ViewingResultError::Status);
    }
    if result.input_event_count != 7
        || result.invocation_count != 2
        || result.traversal_count != 1
        || result.activation_count != 1
        || result.graph_checkpoint_events != [3, 6]
    {
        return Err(ViewingResultError::Counters);
    }
    for (index, snapshot) in result.snapshots.iter().enumerate() {
        let (flags, x, y) = match index {
            0..=4 => (0, 0, 0),
            5 => (1, 320, 240),
            _ => (1, 327, 233),
        };
        if snapshot.revision != index as u64
            || (snapshot.flags, snapshot.x, snapshot.y) != (flags, x, y)
        {
            return Err(ViewingResultError::Snapshot);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_is_independent_and_fits_the_reserved_page_subrange() {
        assert_eq!(core::mem::size_of::<SessionViewingResultV1>(), 432);
        assert_eq!(core::mem::align_of::<SessionViewingResultV1>(), 8);
        assert_eq!(core::mem::offset_of!(SessionViewingResultV1, snapshots), 88);
        assert_eq!(
            core::mem::offset_of!(SessionViewingResultV1, command_results),
            216
        );
        assert_eq!(
            core::mem::offset_of!(SessionViewingResultV1, graph_exits),
            312
        );
        assert_eq!(
            core::mem::offset_of!(SessionViewingResultV1, reserved1),
            376
        );
        let result = SessionViewingResultV1::new(11, 12, 13);
        assert_eq!(
            validate_session_viewing_result(&result, [11, 12, 13]),
            Err(ViewingResultError::Status)
        );
    }

    #[test]
    fn malformed_identity_version_reserved_and_snapshot_are_rejected() {
        let mut result = complete_result();
        assert_eq!(
            validate_session_viewing_result(&result, [11, 12, 13]),
            Ok(())
        );
        result.abi_major = 2;
        assert_eq!(
            validate_session_viewing_result(&result, [11, 12, 13]),
            Err(ViewingResultError::Header)
        );
        result = complete_result();
        result.reserved1[6] = 1;
        assert_eq!(
            validate_session_viewing_result(&result, [11, 12, 13]),
            Err(ViewingResultError::Header)
        );
        result = complete_result();
        assert_eq!(
            validate_session_viewing_result(&result, [11, 12, 14]),
            Err(ViewingResultError::Identity)
        );
        result.snapshots[5].x = 319;
        assert_eq!(
            validate_session_viewing_result(&result, [11, 12, 13]),
            Err(ViewingResultError::Snapshot)
        );
    }

    #[test]
    fn recovery_unknown_status_and_incomplete_retention_cannot_claim_completion() {
        for status in [0, 2, 3, u16::MAX] {
            let mut result = complete_result();
            result.terminal_status = status;
            assert_eq!(
                validate_session_viewing_result(&result, [11, 12, 13]),
                Err(ViewingResultError::Status)
            );
        }
        let mut result = complete_result();
        result.graph_checkpoint_events = [3, 5];
        assert_eq!(
            validate_session_viewing_result(&result, [11, 12, 13]),
            Err(ViewingResultError::Counters)
        );
    }

    fn complete_result() -> SessionViewingResultV1 {
        let mut result = SessionViewingResultV1::new(11, 12, 13);
        result.terminal_status = 1;
        result.input_event_count = 7;
        result.invocation_count = 2;
        result.traversal_count = 1;
        result.activation_count = 1;
        result.graph_checkpoint_events = [3, 6];
        for index in 0..8 {
            result.snapshots[index] = ViewingSnapshotRecord {
                revision: index as u64,
                flags: u32::from(index >= 5),
                x: if index < 5 {
                    0
                } else if index == 5 {
                    320
                } else {
                    327
                },
                y: if index < 5 {
                    0
                } else if index == 5 {
                    240
                } else {
                    233
                },
            };
        }
        result
    }
}
