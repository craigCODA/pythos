//! Versioned, capability-scoped `NetworkPort` ABI (ADR 0095).

use crate::capability_abi::PackedCapability;

pub const NETWORK_PORT_ABI_MAJOR: u16 = 1;
pub const NETWORK_PORT_ABI_MINOR: u16 = 0;
pub const SYSCALL_NETWORK_PORT_REQUEST: u64 = 0x5059_0160;

pub const NETWORK_PORT_OP_DESCRIBE: u16 = 1;
pub const NETWORK_PORT_OP_SEND: u16 = 2;
pub const NETWORK_PORT_OP_TRY_RECEIVE: u16 = 3;
pub const NETWORK_PORT_OP_RESET: u16 = 4;

pub const NETWORK_PORT_STATUS_OK: u16 = 0;
pub const NETWORK_PORT_STATUS_EMPTY: u16 = 1;
pub const NETWORK_PORT_STATUS_DENIED: u16 = 2;
pub const NETWORK_PORT_STATUS_BAD_REQUEST: u16 = 3;
pub const NETWORK_PORT_STATUS_BUFFER_TOO_SMALL: u16 = 4;
pub const NETWORK_PORT_STATUS_NOT_READY: u16 = 5;
pub const NETWORK_PORT_STATUS_FAILED: u16 = 6;
pub const NETWORK_PORT_STATUS_TRANSPORT_ERROR: u16 = 7;

pub const NETWORK_PORT_STATE_READY: u16 = 1;
pub const NETWORK_PORT_STATE_FAILED: u16 = 2;
pub const NETWORK_PORT_STATE_RESET: u16 = 3;

pub const NETWORK_PORT_FLAG_MAC_ONLY: u32 = 1 << 0;
pub const NETWORK_PORT_FLAG_NO_OFFLOAD: u32 = 1 << 1;

pub const NETWORK_PORT_RESOURCE_ID_NAMESPACE: u64 = 0x4E50_0000_0000_0000;
pub const NETWORK_PORT_BOOTSTRAP_MAGIC: u64 = 0x3154_524F_5054_5950;
pub const NETWORK_PORT_MIN_FRAME_BYTES: usize = 60;
pub const NETWORK_PORT_MAX_FRAME_BYTES: usize = 1514;

pub const NETWORK_PORT_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:NETWORK_PORT:BOOTSTRAPPED";
pub const NETWORK_PORT_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:NETWORK_PORT:DESCRIBE_OK";
pub const NETWORK_PORT_TX_OK_MARKER: &str = "PYTHOS:CORE:NETWORK_PORT:TX_OK";
pub const NETWORK_PORT_RX_OK_MARKER: &str = "PYTHOS:CORE:NETWORK_PORT:RX_OK";
pub const NETWORK_PORT_FORGED_DENIED_MARKER: &str = "PYTHOS:CORE:NETWORK_PORT:FORGED_DENIED";
pub const NETWORK_PORT_WRONG_HOLDER_DENIED_MARKER: &str =
    "PYTHOS:CORE:NETWORK_PORT:WRONG_HOLDER_DENIED";
pub const NETWORK_PORT_BAD_BUFFER_DENIED_MARKER: &str =
    "PYTHOS:CORE:NETWORK_PORT:BAD_BUFFER_DENIED";
pub const NETWORK_PORT_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:NETWORK_PORT:TEARDOWN_REVOKED";
pub const NETWORK_PORT_READY_MARKER: &str = "PYTHOS:CORE:NETWORK_PORT_READY";

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkPortRequestV1 {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub operation: u16,
    pub flags: u16,
    pub authority: PackedCapability,
    pub input_ptr: u64,
    pub input_len: u64,
    pub output_ptr: u64,
    pub output_len: u64,
    pub reserved0: u64,
    pub reserved1: u64,
    pub reserved2: u64,
    pub reserved3: u64,
}

impl NetworkPortRequestV1 {
    pub const fn new(operation: u16, authority: PackedCapability) -> Self {
        Self {
            abi_major: NETWORK_PORT_ABI_MAJOR,
            abi_minor: NETWORK_PORT_ABI_MINOR,
            operation,
            flags: 0,
            authority,
            input_ptr: 0,
            input_len: 0,
            output_ptr: 0,
            output_len: 0,
            reserved0: 0,
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkPortResponseV1 {
    pub status: u16,
    pub state: u16,
    pub reserved0: u32,
    pub resource_id: u64,
    pub frame_len: u64,
    pub required_len: u64,
    pub reserved1: u64,
    pub reserved2: u64,
    pub reserved3: u64,
    pub reserved4: u64,
}

impl NetworkPortResponseV1 {
    pub const fn new(status: u16, state: u16) -> Self {
        Self {
            status,
            state,
            reserved0: 0,
            resource_id: 0,
            frame_len: 0,
            required_len: 0,
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
            reserved4: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkPortDescriptionV1 {
    pub resource_id: u64,
    pub mac: [u8; 6],
    pub reserved0: [u8; 2],
    pub min_frame_bytes: u32,
    pub max_frame_bytes: u32,
    pub transport_flags: u32,
    pub state: u32,
    pub reserved1: u64,
}

impl NetworkPortDescriptionV1 {
    pub const fn empty() -> Self {
        Self {
            resource_id: 0,
            mac: [0; 6],
            reserved0: [0; 2],
            min_frame_bytes: 0,
            max_frame_bytes: 0,
            transport_flags: 0,
            state: 0,
            reserved1: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkPortBootstrapV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub reserved0: u32,
    pub port_capability: PackedCapability,
    pub reserved: [u64; 5],
}

impl NetworkPortBootstrapV1 {
    pub const fn new(port_capability: PackedCapability) -> Self {
        Self {
            magic: NETWORK_PORT_BOOTSTRAP_MAGIC,
            abi_major: NETWORK_PORT_ABI_MAJOR,
            abi_minor: NETWORK_PORT_ABI_MINOR,
            reserved0: 0,
            port_capability,
            reserved: [0; 5],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        capability_abi::PackedCapability,
        user_program_manifest::{
            INTRUDER_PRINCIPAL_ID, NETWORK_PORT_PROBE_PRINCIPAL_ID,
            NETWORK_PORT_PROBE_PROGRAM_NAME, NORMAL_SESSION_PROGRAM_NAME,
            SESSION_INPUT_PROBE_PRINCIPAL_ID, SESSION_INPUT_PROBE_PROGRAM_NAME,
            SESSION_RUNTIME_PRINCIPAL_ID, SESSION_RUNTIME_PROGRAM_NAME, SHELL_PRINCIPAL_ID,
        },
    };
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn request_response_description_and_bootstrap_layouts_are_frozen() {
        assert_eq!(size_of::<NetworkPortRequestV1>(), 80);
        assert_eq!(align_of::<NetworkPortRequestV1>(), 8);
        assert_eq!(offset_of!(NetworkPortRequestV1, abi_major), 0);
        assert_eq!(offset_of!(NetworkPortRequestV1, abi_minor), 2);
        assert_eq!(offset_of!(NetworkPortRequestV1, operation), 4);
        assert_eq!(offset_of!(NetworkPortRequestV1, flags), 6);
        assert_eq!(offset_of!(NetworkPortRequestV1, authority), 8);
        assert_eq!(offset_of!(NetworkPortRequestV1, input_ptr), 16);
        assert_eq!(offset_of!(NetworkPortRequestV1, input_len), 24);
        assert_eq!(offset_of!(NetworkPortRequestV1, output_ptr), 32);
        assert_eq!(offset_of!(NetworkPortRequestV1, output_len), 40);
        assert_eq!(offset_of!(NetworkPortRequestV1, reserved0), 48);
        assert_eq!(offset_of!(NetworkPortRequestV1, reserved1), 56);
        assert_eq!(offset_of!(NetworkPortRequestV1, reserved2), 64);
        assert_eq!(offset_of!(NetworkPortRequestV1, reserved3), 72);

        assert_eq!(size_of::<NetworkPortResponseV1>(), 64);
        assert_eq!(align_of::<NetworkPortResponseV1>(), 8);
        assert_eq!(offset_of!(NetworkPortResponseV1, status), 0);
        assert_eq!(offset_of!(NetworkPortResponseV1, state), 2);
        assert_eq!(offset_of!(NetworkPortResponseV1, reserved0), 4);
        assert_eq!(offset_of!(NetworkPortResponseV1, resource_id), 8);
        assert_eq!(offset_of!(NetworkPortResponseV1, frame_len), 16);
        assert_eq!(offset_of!(NetworkPortResponseV1, required_len), 24);
        assert_eq!(offset_of!(NetworkPortResponseV1, reserved1), 32);
        assert_eq!(offset_of!(NetworkPortResponseV1, reserved2), 40);
        assert_eq!(offset_of!(NetworkPortResponseV1, reserved3), 48);
        assert_eq!(offset_of!(NetworkPortResponseV1, reserved4), 56);

        assert_eq!(size_of::<NetworkPortDescriptionV1>(), 40);
        assert_eq!(align_of::<NetworkPortDescriptionV1>(), 8);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, resource_id), 0);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, mac), 8);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, reserved0), 14);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, min_frame_bytes), 16);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, max_frame_bytes), 20);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, transport_flags), 24);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, state), 28);
        assert_eq!(offset_of!(NetworkPortDescriptionV1, reserved1), 32);

        assert_eq!(size_of::<NetworkPortBootstrapV1>(), 64);
        assert_eq!(align_of::<NetworkPortBootstrapV1>(), 8);
        assert_eq!(offset_of!(NetworkPortBootstrapV1, magic), 0);
        assert_eq!(offset_of!(NetworkPortBootstrapV1, abi_major), 8);
        assert_eq!(offset_of!(NetworkPortBootstrapV1, abi_minor), 10);
        assert_eq!(offset_of!(NetworkPortBootstrapV1, reserved0), 12);
        assert_eq!(offset_of!(NetworkPortBootstrapV1, port_capability), 16);
        assert_eq!(offset_of!(NetworkPortBootstrapV1, reserved), 24);
    }

    #[test]
    fn constructors_set_identity_and_leave_reserved_fields_zero() {
        let authority = PackedCapability::from_parts(7, 11);
        let request = NetworkPortRequestV1::new(NETWORK_PORT_OP_SEND, authority);
        assert_eq!(request.abi_major, 1);
        assert_eq!(request.abi_minor, 0);
        assert_eq!(request.operation, 2);
        assert_eq!(request.flags, 0);
        assert_eq!(request.authority, authority);
        assert_eq!(
            [
                request.reserved0,
                request.reserved1,
                request.reserved2,
                request.reserved3
            ],
            [0; 4]
        );

        let response =
            NetworkPortResponseV1::new(NETWORK_PORT_STATUS_EMPTY, NETWORK_PORT_STATE_READY);
        assert_eq!(response.status, 1);
        assert_eq!(response.state, 1);
        assert_eq!(response.reserved0, 0);
        assert_eq!(
            [
                response.reserved1,
                response.reserved2,
                response.reserved3,
                response.reserved4
            ],
            [0; 4]
        );

        let description = NetworkPortDescriptionV1::empty();
        assert_eq!(description.reserved0, [0; 2]);
        assert_eq!(description.reserved1, 0);

        let bootstrap = NetworkPortBootstrapV1::new(authority);
        assert_eq!(bootstrap.magic, 0x3154_524F_5054_5950);
        assert_eq!(bootstrap.abi_major, 1);
        assert_eq!(bootstrap.abi_minor, 0);
        assert_eq!(bootstrap.port_capability, authority);
        assert_eq!(bootstrap.reserved0, 0);
        assert_eq!(bootstrap.reserved, [0; 5]);
    }

    #[test]
    fn constants_and_acceptance_markers_match_adr_0095() {
        assert_eq!(NETWORK_PORT_ABI_MAJOR, 1);
        assert_eq!(NETWORK_PORT_ABI_MINOR, 0);
        assert_eq!(SYSCALL_NETWORK_PORT_REQUEST, 0x5059_0160);
        assert_eq!(
            [
                NETWORK_PORT_OP_DESCRIBE,
                NETWORK_PORT_OP_SEND,
                NETWORK_PORT_OP_TRY_RECEIVE,
                NETWORK_PORT_OP_RESET
            ],
            [1, 2, 3, 4]
        );
        assert_eq!(
            [
                NETWORK_PORT_STATUS_OK,
                NETWORK_PORT_STATUS_EMPTY,
                NETWORK_PORT_STATUS_DENIED,
                NETWORK_PORT_STATUS_BAD_REQUEST,
                NETWORK_PORT_STATUS_BUFFER_TOO_SMALL,
                NETWORK_PORT_STATUS_NOT_READY,
                NETWORK_PORT_STATUS_FAILED,
                NETWORK_PORT_STATUS_TRANSPORT_ERROR
            ],
            [0, 1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(
            [
                NETWORK_PORT_STATE_READY,
                NETWORK_PORT_STATE_FAILED,
                NETWORK_PORT_STATE_RESET
            ],
            [1, 2, 3]
        );
        assert_eq!(NETWORK_PORT_FLAG_MAC_ONLY, 1 << 0);
        assert_eq!(NETWORK_PORT_FLAG_NO_OFFLOAD, 1 << 1);
        assert_eq!(NETWORK_PORT_RESOURCE_ID_NAMESPACE, 0x4E50_0000_0000_0000);
        assert_eq!(NETWORK_PORT_BOOTSTRAP_MAGIC, 0x3154_524F_5054_5950);
        assert_eq!(NETWORK_PORT_MIN_FRAME_BYTES, 60);
        assert_eq!(NETWORK_PORT_MAX_FRAME_BYTES, 1514);
        assert_eq!(
            NETWORK_PORT_BOOTSTRAPPED_MARKER,
            "PYTHOS:CORE:NETWORK_PORT:BOOTSTRAPPED"
        );
        assert_eq!(
            NETWORK_PORT_DESCRIBE_OK_MARKER,
            "PYTHOS:CORE:NETWORK_PORT:DESCRIBE_OK"
        );
        assert_eq!(NETWORK_PORT_TX_OK_MARKER, "PYTHOS:CORE:NETWORK_PORT:TX_OK");
        assert_eq!(NETWORK_PORT_RX_OK_MARKER, "PYTHOS:CORE:NETWORK_PORT:RX_OK");
        assert_eq!(
            NETWORK_PORT_FORGED_DENIED_MARKER,
            "PYTHOS:CORE:NETWORK_PORT:FORGED_DENIED"
        );
        assert_eq!(
            NETWORK_PORT_WRONG_HOLDER_DENIED_MARKER,
            "PYTHOS:CORE:NETWORK_PORT:WRONG_HOLDER_DENIED"
        );
        assert_eq!(
            NETWORK_PORT_BAD_BUFFER_DENIED_MARKER,
            "PYTHOS:CORE:NETWORK_PORT:BAD_BUFFER_DENIED"
        );
        assert_eq!(
            NETWORK_PORT_TEARDOWN_REVOKED_MARKER,
            "PYTHOS:CORE:NETWORK_PORT:TEARDOWN_REVOKED"
        );
        assert_eq!(NETWORK_PORT_READY_MARKER, "PYTHOS:CORE:NETWORK_PORT_READY");
    }

    #[test]
    fn network_probe_identity_is_additive_and_existing_identities_are_unchanged() {
        assert_eq!(NETWORK_PORT_PROBE_PROGRAM_NAME, b"network-port-probe.elf");
        assert_eq!(NETWORK_PORT_PROBE_PRINCIPAL_ID, 0x5059_4E50_5254_0001);
        assert_eq!(SHELL_PRINCIPAL_ID, 0x5059_5348_454C_4C01);
        assert_eq!(INTRUDER_PRINCIPAL_ID, 0x5059_494E_5452_4401);
        assert_eq!(SESSION_INPUT_PROBE_PROGRAM_NAME, b"session-input-probe.elf");
        assert_eq!(SESSION_INPUT_PROBE_PRINCIPAL_ID, 0x5059_5349_4E50_0001);
        assert_eq!(SESSION_RUNTIME_PROGRAM_NAME, b"session-runtime.elf");
        assert_eq!(SESSION_RUNTIME_PRINCIPAL_ID, 0x5059_5352_544D_0001);
        assert_eq!(NORMAL_SESSION_PROGRAM_NAME, b"normal-session.elf");
    }
}
