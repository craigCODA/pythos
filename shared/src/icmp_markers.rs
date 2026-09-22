//! Stable acceptance markers for the Phase 14 ICMP Echo consumer proof.

pub const ICMP_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:ICMP:BOOTSTRAPPED";
pub const ICMP_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:ICMP:DESCRIBE_OK";
pub const ICMP_ARP_SETUP_OK_MARKER: &str = "PYTHOS:CORE:ICMP:ARP_SETUP_OK";
pub const ICMP_TX_OK_MARKER: &str = "PYTHOS:CORE:ICMP:TX_OK";
pub const ICMP_RX_OK_MARKER: &str = "PYTHOS:CORE:ICMP:RX_OK";
pub const ICMP_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:ICMP:TEARDOWN_REVOKED";
pub const ICMP_READY_MARKER: &str = "PYTHOS:CORE:ICMP_READY";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_strings_are_frozen_in_acceptance_order() {
        assert_eq!(
            [
                ICMP_BOOTSTRAPPED_MARKER,
                ICMP_DESCRIBE_OK_MARKER,
                ICMP_ARP_SETUP_OK_MARKER,
                ICMP_TX_OK_MARKER,
                ICMP_RX_OK_MARKER,
                ICMP_TEARDOWN_REVOKED_MARKER,
                ICMP_READY_MARKER,
            ],
            [
                "PYTHOS:CORE:ICMP:BOOTSTRAPPED",
                "PYTHOS:CORE:ICMP:DESCRIBE_OK",
                "PYTHOS:CORE:ICMP:ARP_SETUP_OK",
                "PYTHOS:CORE:ICMP:TX_OK",
                "PYTHOS:CORE:ICMP:RX_OK",
                "PYTHOS:CORE:ICMP:TEARDOWN_REVOKED",
                "PYTHOS:CORE:ICMP_READY",
            ]
        );
    }

    #[test]
    fn marker_strings_are_unique() {
        let markers = [
            ICMP_BOOTSTRAPPED_MARKER,
            ICMP_DESCRIBE_OK_MARKER,
            ICMP_ARP_SETUP_OK_MARKER,
            ICMP_TX_OK_MARKER,
            ICMP_RX_OK_MARKER,
            ICMP_TEARDOWN_REVOKED_MARKER,
            ICMP_READY_MARKER,
        ];

        for (index, marker) in markers.iter().enumerate() {
            assert!(!markers[..index].contains(marker));
        }
    }
}
