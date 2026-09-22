//! Stable identity and acceptance markers for the Phase 14 DNS query proof.

pub const DNS_PROBE_PRINCIPAL_ID: u64 = 0x5059_444E_5300_0001;
pub const DNS_CONSUMER_SERVICE_ID: u64 = 0x5059_444E_5343_0001;
pub const DNS_OWNER_SERVICE_ID: u64 = 0x5059_444E_534F_0001;

pub const DNS_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:DNS:BOOTSTRAPPED";
pub const DNS_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:DNS:DESCRIBE_OK";
pub const DNS_ARP_SETUP_OK_MARKER: &str = "PYTHOS:CORE:DNS:ARP_SETUP_OK";
pub const DNS_QUERY_OK_MARKER: &str = "PYTHOS:CORE:DNS:QUERY_OK";
pub const DNS_RESPONSE_OK_MARKER: &str = "PYTHOS:CORE:DNS:RESPONSE_OK";
pub const DNS_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:DNS:TEARDOWN_REVOKED";
pub const DNS_READY_MARKER: &str = "PYTHOS:CORE:DNS_READY";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_program_manifest::DNS_PROBE_PROGRAM_NAME;

    #[test]
    fn dns_identity_constants_are_frozen_and_unique() {
        assert_eq!(DNS_PROBE_PROGRAM_NAME, b"dns-probe.elf");
        assert_eq!(DNS_PROBE_PRINCIPAL_ID, 0x5059_444E_5300_0001);
        assert_eq!(DNS_CONSUMER_SERVICE_ID, 0x5059_444E_5343_0001);
        assert_eq!(DNS_OWNER_SERVICE_ID, 0x5059_444E_534F_0001);
        assert_ne!(DNS_PROBE_PRINCIPAL_ID, DNS_CONSUMER_SERVICE_ID);
        assert_ne!(DNS_PROBE_PRINCIPAL_ID, DNS_OWNER_SERVICE_ID);
        assert_ne!(DNS_CONSUMER_SERVICE_ID, DNS_OWNER_SERVICE_ID);
    }

    #[test]
    fn dns_marker_strings_are_frozen_in_acceptance_order_and_unique() {
        let markers = [
            DNS_BOOTSTRAPPED_MARKER,
            DNS_DESCRIBE_OK_MARKER,
            DNS_ARP_SETUP_OK_MARKER,
            DNS_QUERY_OK_MARKER,
            DNS_RESPONSE_OK_MARKER,
            DNS_TEARDOWN_REVOKED_MARKER,
            DNS_READY_MARKER,
        ];
        assert_eq!(
            markers,
            [
                "PYTHOS:CORE:DNS:BOOTSTRAPPED",
                "PYTHOS:CORE:DNS:DESCRIBE_OK",
                "PYTHOS:CORE:DNS:ARP_SETUP_OK",
                "PYTHOS:CORE:DNS:QUERY_OK",
                "PYTHOS:CORE:DNS:RESPONSE_OK",
                "PYTHOS:CORE:DNS:TEARDOWN_REVOKED",
                "PYTHOS:CORE:DNS_READY",
            ]
        );
        for (index, marker) in markers.iter().enumerate() {
            assert!(!markers[..index].contains(marker));
        }
    }
}
