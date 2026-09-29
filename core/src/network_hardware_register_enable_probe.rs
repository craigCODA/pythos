pub const PCI_COMMAND_MEMORY_SPACE: u16 = 1 << 1;
pub const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandTransitionError {
    BusMasterAlreadyEnabled,
    EnableReadbackMismatch,
    RestoreReadbackMismatch,
}

pub const fn derive_enabled_command(original_status: u32) -> Result<u16, CommandTransitionError> {
    let original = original_status as u16;
    if original & PCI_COMMAND_BUS_MASTER != 0 {
        return Err(CommandTransitionError::BusMasterAlreadyEnabled);
    }
    Ok(original | PCI_COMMAND_MEMORY_SPACE)
}

pub const fn restore_command(original_status: u32) -> u16 {
    original_status as u16
}

pub const fn validate_enable_readback(
    original_status: u32,
    observed_status: u32,
) -> Result<(), CommandTransitionError> {
    match derive_enabled_command(original_status) {
        Ok(expected) if observed_status as u16 == expected => Ok(()),
        _ => Err(CommandTransitionError::EnableReadbackMismatch),
    }
}

pub const fn validate_restore_readback(
    original_status: u32,
    observed_status: u32,
) -> Result<(), CommandTransitionError> {
    if observed_status as u16 == restore_command(original_status) {
        Ok(())
    } else {
        Err(CommandTransitionError::RestoreReadbackMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_command_sets_only_memory_space_without_bus_master() {
        let original = 0x0000_0001;
        let enabled = derive_enabled_command(original).unwrap();
        assert_eq!(enabled, 0x0003);
        assert_eq!(enabled & PCI_COMMAND_BUS_MASTER, 0);
    }

    #[test]
    fn enable_command_preserves_existing_command_bits() {
        let original = 0x0000_0101;
        assert_eq!(derive_enabled_command(original), Ok(0x0103));
    }

    #[test]
    fn enable_command_rejects_preexisting_bus_master() {
        assert_eq!(
            derive_enabled_command(0x0000_0105),
            Err(CommandTransitionError::BusMasterAlreadyEnabled)
        );
    }

    #[test]
    fn restore_command_returns_original_low_word_exactly() {
        let original = 0xA5A5_0105;
        assert_eq!(restore_command(original), 0x0105);
    }

    #[test]
    fn readback_requires_memory_space_and_preserves_command_contract() {
        let original = 0x0000_0101;
        let enabled = 0x0000_0103;
        assert!(validate_enable_readback(original, enabled).is_ok());
        assert!(validate_enable_readback(original, 0x0000_0101).is_err());
        assert!(validate_restore_readback(original, original).is_ok());
        assert!(validate_restore_readback(original, 0x0000_0107).is_err());
    }
}
