//! Stable identity and acceptance markers for the Phase 14 socket proof.

#[cfg(test)]
use crate::user_program_manifest::{SOCKET_PROBE_PRINCIPAL_ID, SOCKET_PROBE_PROGRAM_NAME};

pub const SOCKET_CONSUMER_SERVICE_ID: u64 = 0x5059_534F_4353_0001;
pub const SOCKET_OWNER_SERVICE_ID: u64 = 0x5059_534F_4F57_0001;

pub const SOCKET_DENIED_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:SOCKET:DENIED_BOOTSTRAPPED";
pub const SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER: &str =
    "PYTHOS:CORE:SOCKET:OPEN_WITHOUT_CAP_DENIED";
pub const SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER: &str =
    "PYTHOS:CORE:SOCKET:DENIED_TEARDOWN_COMPLETE";
pub const SOCKET_DENIED_READY_MARKER: &str = "PYTHOS:CORE:SOCKET_DENIED_READY";

pub const SOCKET_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:SOCKET:BOOTSTRAPPED";
pub const SOCKET_OPEN_GRANTED_MARKER: &str = "PYTHOS:CORE:SOCKET:OPEN_GRANTED";
pub const SOCKET_HANDSHAKE_OK_MARKER: &str = "PYTHOS:CORE:SOCKET:HANDSHAKE_OK";
pub const SOCKET_REQUEST_OK_MARKER: &str = "PYTHOS:CORE:SOCKET:REQUEST_OK";
pub const SOCKET_RESPONSE_OK_MARKER: &str = "PYTHOS:CORE:SOCKET:RESPONSE_OK";
pub const SOCKET_CLOSE_OK_MARKER: &str = "PYTHOS:CORE:SOCKET:CLOSE_OK";
pub const SOCKET_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:SOCKET:TEARDOWN_REVOKED";
pub const SOCKET_READY_MARKER: &str = "PYTHOS:CORE:SOCKET_READY";

pub const SOCKET_DENIED_MARKERS: [&str; 4] = [
    SOCKET_DENIED_BOOTSTRAPPED_MARKER,
    SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER,
    SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER,
    SOCKET_DENIED_READY_MARKER,
];

pub const SOCKET_GRANTED_MARKERS: [&str; 8] = [
    SOCKET_BOOTSTRAPPED_MARKER,
    SOCKET_OPEN_GRANTED_MARKER,
    SOCKET_HANDSHAKE_OK_MARKER,
    SOCKET_REQUEST_OK_MARKER,
    SOCKET_RESPONSE_OK_MARKER,
    SOCKET_CLOSE_OK_MARKER,
    SOCKET_TEARDOWN_REVOKED_MARKER,
    SOCKET_READY_MARKER,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_identity_is_private_and_unique() {
        assert_eq!(SOCKET_PROBE_PROGRAM_NAME, b"socket-probe.elf");
        assert_eq!(SOCKET_PROBE_PRINCIPAL_ID, 0x5059_534F_4300_0001);
        assert_eq!(SOCKET_CONSUMER_SERVICE_ID, 0x5059_534F_4353_0001);
        assert_eq!(SOCKET_OWNER_SERVICE_ID, 0x5059_534F_4F57_0001);
        assert_ne!(SOCKET_PROBE_PRINCIPAL_ID, SOCKET_CONSUMER_SERVICE_ID);
        assert_ne!(SOCKET_PROBE_PRINCIPAL_ID, SOCKET_OWNER_SERVICE_ID);
        assert_ne!(SOCKET_CONSUMER_SERVICE_ID, SOCKET_OWNER_SERVICE_ID);
    }

    #[test]
    fn socket_markers_are_exact_and_ordered() {
        assert_eq!(
            SOCKET_DENIED_MARKERS,
            [
                "PYTHOS:CORE:SOCKET:DENIED_BOOTSTRAPPED",
                "PYTHOS:CORE:SOCKET:OPEN_WITHOUT_CAP_DENIED",
                "PYTHOS:CORE:SOCKET:DENIED_TEARDOWN_COMPLETE",
                "PYTHOS:CORE:SOCKET_DENIED_READY",
            ]
        );
        assert_eq!(
            SOCKET_GRANTED_MARKERS,
            [
                "PYTHOS:CORE:SOCKET:BOOTSTRAPPED",
                "PYTHOS:CORE:SOCKET:OPEN_GRANTED",
                "PYTHOS:CORE:SOCKET:HANDSHAKE_OK",
                "PYTHOS:CORE:SOCKET:REQUEST_OK",
                "PYTHOS:CORE:SOCKET:RESPONSE_OK",
                "PYTHOS:CORE:SOCKET:CLOSE_OK",
                "PYTHOS:CORE:SOCKET:TEARDOWN_REVOKED",
                "PYTHOS:CORE:SOCKET_READY",
            ]
        );
        for markers in [
            SOCKET_DENIED_MARKERS.as_slice(),
            SOCKET_GRANTED_MARKERS.as_slice(),
        ] {
            for (index, marker) in markers.iter().enumerate() {
                assert!(!markers[..index].contains(marker));
            }
        }
    }
}
