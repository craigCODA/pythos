//! Stable acceptance markers for the Phase 14 ARP consumer proof.

pub const ARP_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:ARP:BOOTSTRAPPED";
pub const ARP_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:ARP:DESCRIBE_OK";
pub const ARP_REQUEST_OK_MARKER: &str = "PYTHOS:CORE:ARP:REQUEST_OK";
pub const ARP_REPLY_OK_MARKER: &str = "PYTHOS:CORE:ARP:REPLY_OK";
pub const ARP_TEARDOWN_REVOKED_MARKER: &str = "PYTHOS:CORE:ARP:TEARDOWN_REVOKED";
pub const ARP_READY_MARKER: &str = "PYTHOS:CORE:ARP_READY";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_strings_are_frozen() {
        assert_eq!(ARP_BOOTSTRAPPED_MARKER, "PYTHOS:CORE:ARP:BOOTSTRAPPED");
        assert_eq!(ARP_DESCRIBE_OK_MARKER, "PYTHOS:CORE:ARP:DESCRIBE_OK");
        assert_eq!(ARP_REQUEST_OK_MARKER, "PYTHOS:CORE:ARP:REQUEST_OK");
        assert_eq!(ARP_REPLY_OK_MARKER, "PYTHOS:CORE:ARP:REPLY_OK");
        assert_eq!(
            ARP_TEARDOWN_REVOKED_MARKER,
            "PYTHOS:CORE:ARP:TEARDOWN_REVOKED"
        );
        assert_eq!(ARP_READY_MARKER, "PYTHOS:CORE:ARP_READY");
    }
}
