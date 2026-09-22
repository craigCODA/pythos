//! Stable identity and acceptance markers for the Phase 14 TCP stream proof.

pub const TCP_PROBE_PRINCIPAL_ID: u64 = 0x5059_5443_5000_0001;
pub const TCP_CONSUMER_SERVICE_ID: u64 = 0x5059_5443_4353_0001;
pub const TCP_OWNER_SERVICE_ID: u64 = 0x5059_5443_4F57_0001;

pub const TCP_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:TCP:BOOTSTRAPPED";
pub const TCP_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:TCP:DESCRIBE_OK";
pub const TCP_ARP_SETUP_OK_MARKER: &str = "PYTHOS:CORE:TCP:ARP_SETUP_OK";
pub const TCP_HANDSHAKE_OK_MARKER: &str = "PYTHOS:CORE:TCP:HANDSHAKE_OK";
pub const TCP_TX_OK_MARKER: &str = "PYTHOS:CORE:TCP:TX_OK";
pub const TCP_RX_OK_MARKER: &str = "PYTHOS:CORE:TCP:RX_OK";
pub const TCP_CLOSE_OK_MARKER: &str = "PYTHOS:CORE:TCP:CLOSE_OK";
pub const TCP_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:TCP:TEARDOWN_REVOKED";
pub const TCP_READY_MARKER: &str = "PYTHOS:CORE:TCP_READY";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_program_manifest::TCP_PROBE_PROGRAM_NAME;

    #[test]
    fn tcp_identity_constants_are_frozen_and_unique() {
        assert_eq!(TCP_PROBE_PROGRAM_NAME, b"tcp-probe.elf");
        assert_eq!(TCP_PROBE_PRINCIPAL_ID, 0x5059_5443_5000_0001);
        assert_eq!(TCP_CONSUMER_SERVICE_ID, 0x5059_5443_4353_0001);
        assert_eq!(TCP_OWNER_SERVICE_ID, 0x5059_5443_4F57_0001);
        assert_ne!(TCP_PROBE_PRINCIPAL_ID, TCP_CONSUMER_SERVICE_ID);
        assert_ne!(TCP_PROBE_PRINCIPAL_ID, TCP_OWNER_SERVICE_ID);
        assert_ne!(TCP_CONSUMER_SERVICE_ID, TCP_OWNER_SERVICE_ID);
    }

    #[test]
    fn tcp_marker_strings_are_frozen_in_acceptance_order_and_unique() {
        let markers = [
            TCP_BOOTSTRAPPED_MARKER,
            TCP_DESCRIBE_OK_MARKER,
            TCP_ARP_SETUP_OK_MARKER,
            TCP_HANDSHAKE_OK_MARKER,
            TCP_TX_OK_MARKER,
            TCP_RX_OK_MARKER,
            TCP_CLOSE_OK_MARKER,
            TCP_TEARDOWN_REVOKED_MARKER,
            TCP_READY_MARKER,
        ];
        assert_eq!(
            markers,
            [
                "PYTHOS:CORE:TCP:BOOTSTRAPPED",
                "PYTHOS:CORE:TCP:DESCRIBE_OK",
                "PYTHOS:CORE:TCP:ARP_SETUP_OK",
                "PYTHOS:CORE:TCP:HANDSHAKE_OK",
                "PYTHOS:CORE:TCP:TX_OK",
                "PYTHOS:CORE:TCP:RX_OK",
                "PYTHOS:CORE:TCP:CLOSE_OK",
                "PYTHOS:CORE:TCP:TEARDOWN_REVOKED",
                "PYTHOS:CORE:TCP_READY",
            ]
        );
        for (index, marker) in markers.iter().enumerate() {
            assert!(!markers[..index].contains(marker));
        }
    }
}
