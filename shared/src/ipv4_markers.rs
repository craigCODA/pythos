//! Stable acceptance markers for the Phase 14 IPv4 consumer proof.

pub const IPV4_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:IPV4:BOOTSTRAPPED";
pub const IPV4_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:IPV4:DESCRIBE_OK";
pub const IPV4_ARP_SETUP_OK_MARKER: &str = "PYTHOS:CORE:IPV4:ARP_SETUP_OK";
pub const IPV4_TX_OK_MARKER: &str = "PYTHOS:CORE:IPV4:TX_OK";
pub const IPV4_RX_OK_MARKER: &str = "PYTHOS:CORE:IPV4:RX_OK";
pub const IPV4_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:IPV4:TEARDOWN_REVOKED";
pub const IPV4_READY_MARKER: &str = "PYTHOS:CORE:IPV4_READY";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_strings_are_frozen_in_acceptance_order() {
        assert_eq!(
            [
                IPV4_BOOTSTRAPPED_MARKER,
                IPV4_DESCRIBE_OK_MARKER,
                IPV4_ARP_SETUP_OK_MARKER,
                IPV4_TX_OK_MARKER,
                IPV4_RX_OK_MARKER,
                IPV4_TEARDOWN_REVOKED_MARKER,
                IPV4_READY_MARKER,
            ],
            [
                "PYTHOS:CORE:IPV4:BOOTSTRAPPED",
                "PYTHOS:CORE:IPV4:DESCRIBE_OK",
                "PYTHOS:CORE:IPV4:ARP_SETUP_OK",
                "PYTHOS:CORE:IPV4:TX_OK",
                "PYTHOS:CORE:IPV4:RX_OK",
                "PYTHOS:CORE:IPV4:TEARDOWN_REVOKED",
                "PYTHOS:CORE:IPV4_READY",
            ]
        );
    }
}
