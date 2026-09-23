//! Stable identity and acceptance markers for the Phase 14 UDP datagram proof.

pub const UDP_CONSUMER_SERVICE_ID: u64 = 0x5059_5544_4353_0001;
pub const UDP_OWNER_SERVICE_ID: u64 = 0x5059_5544_4F57_0001;

pub const UDP_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:UDP:BOOTSTRAPPED";
pub const UDP_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:UDP:DESCRIBE_OK";
pub const UDP_ARP_SETUP_OK_MARKER: &str = "PYTHOS:CORE:UDP:ARP_SETUP_OK";
pub const UDP_TX_OK_MARKER: &str = "PYTHOS:CORE:UDP:TX_OK";
pub const UDP_RX_OK_MARKER: &str = "PYTHOS:CORE:UDP:RX_OK";
pub const UDP_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:UDP:TEARDOWN_REVOKED";
pub const UDP_READY_MARKER: &str = "PYTHOS:CORE:UDP_READY";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_frozen() {
        assert_eq!(UDP_CONSUMER_SERVICE_ID, 0x5059_5544_4353_0001);
        assert_eq!(UDP_OWNER_SERVICE_ID, 0x5059_5544_4F57_0001);
        assert_ne!(UDP_CONSUMER_SERVICE_ID, UDP_OWNER_SERVICE_ID);
    }

    #[test]
    fn marker_strings_are_frozen_in_acceptance_order_and_unique() {
        let markers = [
            UDP_BOOTSTRAPPED_MARKER,
            UDP_DESCRIBE_OK_MARKER,
            UDP_ARP_SETUP_OK_MARKER,
            UDP_TX_OK_MARKER,
            UDP_RX_OK_MARKER,
            UDP_TEARDOWN_REVOKED_MARKER,
            UDP_READY_MARKER,
        ];
        assert_eq!(
            markers,
            [
                "PYTHOS:CORE:UDP:BOOTSTRAPPED",
                "PYTHOS:CORE:UDP:DESCRIBE_OK",
                "PYTHOS:CORE:UDP:ARP_SETUP_OK",
                "PYTHOS:CORE:UDP:TX_OK",
                "PYTHOS:CORE:UDP:RX_OK",
                "PYTHOS:CORE:UDP:TEARDOWN_REVOKED",
                "PYTHOS:CORE:UDP_READY",
            ]
        );
        for (index, marker) in markers.iter().enumerate() {
            assert!(!markers[..index].contains(marker));
        }
    }
}
