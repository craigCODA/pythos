//! Active ring-3 process identity for syscall authorization.

#![cfg_attr(test, allow(dead_code))]

use core::cell::UnsafeCell;
use core::mem::size_of;

use crate::service_identity::ServiceId;
use crate::user_copy::{UserCopyError, UserCopyMap};
use crate::{user_elf, user_stacks};
use pythos_shared::object_shell_abi::BootstrapCapabilityBlock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveUserProcess {
    service_id: ServiceId,
    principal_id: u64,
    program_digest: u64,
    copy_map: UserCopyMap,
}

impl ActiveUserProcess {
    pub const fn new(service_id: ServiceId, principal_id: u64, program_digest: u64) -> Self {
        Self {
            service_id,
            principal_id,
            program_digest,
            copy_map: UserCopyMap::new(),
        }
    }

    const fn from_copy_map(
        service_id: ServiceId,
        principal_id: u64,
        program_digest: u64,
        copy_map: UserCopyMap,
    ) -> Self {
        Self {
            service_id,
            principal_id,
            program_digest,
            copy_map,
        }
    }

    pub fn from_validated_launch(
        service_id: ServiceId,
        principal_id: u64,
        program_digest: u64,
        image: &user_elf::UserElfImage,
        stack: user_stacks::UserStackRegion,
        bootstrap_user_ptr: u64,
    ) -> Result<Self, UserCopyError> {
        Ok(Self::from_copy_map(
            service_id,
            principal_id,
            program_digest,
            copy_map_from_validated_launch(image, stack, bootstrap_user_ptr)?,
        ))
    }

    #[cfg(test)]
    pub const fn with_copy_map(self, copy_map: UserCopyMap) -> Self {
        Self {
            service_id: self.service_id,
            principal_id: self.principal_id,
            program_digest: self.program_digest,
            copy_map,
        }
    }

    pub const fn service_id(self) -> ServiceId {
        self.service_id
    }

    pub const fn principal_id(self) -> u64 {
        self.principal_id
    }

    pub const fn program_digest(self) -> u64 {
        self.program_digest
    }

    pub const fn copy_map(self) -> UserCopyMap {
        self.copy_map
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessContextError {
    NoActiveProcess,
}

pub fn copy_map_from_validated_launch(
    image: &user_elf::UserElfImage,
    stack: user_stacks::UserStackRegion,
    bootstrap_user_ptr: u64,
) -> Result<UserCopyMap, UserCopyError> {
    let mut map = UserCopyMap::new();
    let mut index = 0usize;
    while index < image.segment_count() {
        let segment = image.segment(index).ok_or(UserCopyError::OutOfRange)?;
        map.add_mapping(
            segment.page_start(),
            segment.page_len(),
            true,
            segment.writable(),
        )?;
        index += 1;
    }
    map.add_mapping(stack.stack_start, stack.stack_len, true, true)?;
    map.add_mapping(
        bootstrap_user_ptr,
        size_of::<BootstrapCapabilityBlock>() as u64,
        true,
        false,
    )?;
    Ok(map)
}

struct ActiveProcessStorage(UnsafeCell<Option<ActiveUserProcess>>);

// SAFETY: ADR 0051 binds one active ring-3 process on one CPU in this slice.
// SMP and process migration are out of scope; future multi-core work must
// replace this with scheduler-owned synchronization before concurrent use.
unsafe impl Sync for ActiveProcessStorage {}

static CURRENT_PROCESS: ActiveProcessStorage = ActiveProcessStorage(UnsafeCell::new(None));

pub fn bind_current_process(process: ActiveUserProcess) {
    // SAFETY:
    // 1. Invariant: ADR 0051 runs one active ring-3 process at a time on one CPU.
    // 2. Established by: QEMU target is single-core and persistent shell launch is
    //    not preemptively migrated across address spaces in this slice.
    // 3. Lifetime: copied identity values are stored in static kernel-owned memory.
    // 4. Pointer ownership: no borrowed process table references escape.
    // 5. Alignment: `UnsafeCell<Option<ActiveUserProcess>>` preserves alignment.
    // 6. Mapped length: exactly one process context cell is accessed.
    // 7. Concurrency: SMP is out of scope for ADR 0051.
    // 8. Violation: concurrent mutation would allow wrong-caller authority checks.
    unsafe {
        *CURRENT_PROCESS.0.get() = Some(process);
    }
}

pub fn current_caller() -> Result<ActiveUserProcess, ProcessContextError> {
    // SAFETY:
    // 1. Invariant: ADR 0051 runs one active ring-3 process at a time on one CPU.
    // 2. Established by: QEMU target is single-core and persistent shell launch is
    //    not preemptively migrated across address spaces in this slice.
    // 3. Lifetime: copied identity values outlive the syscall that reads them.
    // 4. Pointer ownership: this returns a copied value, not a borrowed reference.
    // 5. Alignment: `UnsafeCell<Option<ActiveUserProcess>>` preserves alignment.
    // 6. Mapped length: exactly one process context cell is accessed.
    // 7. Concurrency: SMP is out of scope for ADR 0051.
    // 8. Violation: concurrent mutation would allow wrong-caller authority checks.
    unsafe { (*CURRENT_PROCESS.0.get()).ok_or(ProcessContextError::NoActiveProcess) }
}

pub fn clear_current_process() {
    // SAFETY:
    // 1. Invariant: ADR 0051 runs one active ring-3 process at a time on one CPU.
    // 2. Established by: persistent shell launch binds exactly one current
    //    process, and fault containment clears that binding before idling.
    // 3. Lifetime: clearing the copied process value ends only the static
    //    current-context record; it does not drop scheduler-owned state.
    // 4. Pointer ownership: no borrowed process-table references escape this
    //    storage, and the old value is copied data.
    // 5. Alignment: `UnsafeCell<Option<ActiveUserProcess>>` preserves alignment.
    // 6. Mapped length: exactly one process context cell is accessed.
    // 7. Concurrency: SMP is out of scope for ADR 0051.
    // 8. Violation: concurrent clear/bind could remove the wrong caller.
    unsafe {
        *CURRENT_PROCESS.0.get() = None;
    }
}

#[cfg(test)]
fn set_current_for_test(process: ActiveUserProcess) {
    bind_current_process(process);
}

#[cfg(test)]
fn current_caller_for_test() -> Result<ActiveUserProcess, ProcessContextError> {
    current_caller()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_identity::ServiceIdentityTable;
    use crate::tasks::TaskId;
    use crate::user_copy::{UserCopyAccess, UserCopyError};
    use pythos_shared::user_program_manifest::{INTRUDER_PRINCIPAL_ID, SHELL_PRINCIPAL_ID};

    #[test]
    fn caller_identity_comes_from_active_process_not_task_slot_constant() {
        let mut identities = ServiceIdentityTable::new();
        let shell_service = identities.register_task(TaskId::new(180)).unwrap();
        let intruder_service = identities.register_task(TaskId::new(181)).unwrap();
        let shell = ActiveUserProcess::new(shell_service, SHELL_PRINCIPAL_ID, 0xAA);
        let intruder = ActiveUserProcess::new(intruder_service, INTRUDER_PRINCIPAL_ID, 0xBB);

        set_current_for_test(shell);
        assert_eq!(
            current_caller_for_test().unwrap().principal_id(),
            SHELL_PRINCIPAL_ID
        );
        set_current_for_test(intruder);
        assert_eq!(
            current_caller_for_test().unwrap().principal_id(),
            INTRUDER_PRINCIPAL_ID
        );
    }

    #[test]
    fn launch_copy_map_retains_elf_stack_and_bootstrap_permissions() {
        let image = user_elf::validate(&minimal_user_elf()).unwrap();
        let stack = user_stacks::UserStackRegion {
            guard_start: 0x0070_0000,
            stack_start: 0x0070_1000,
            stack_len: 0x1000,
        };
        let bootstrap = 0x0080_0000;

        let map = copy_map_from_validated_launch(&image, stack, bootstrap).unwrap();

        assert!(
            map.validate_range(0x0040_0000, 2, UserCopyAccess::Read)
                .is_ok()
        );
        assert_eq!(
            map.validate_range(0x0040_0000, 2, UserCopyAccess::Write),
            Err(UserCopyError::PermissionDenied)
        );
        assert!(
            map.validate_range(0x0040_1000, 4, UserCopyAccess::Write)
                .is_ok()
        );
        assert!(
            map.validate_range(stack.stack_start, 32, UserCopyAccess::Write)
                .is_ok()
        );
        assert!(
            map.validate_range(
                bootstrap,
                size_of::<BootstrapCapabilityBlock>() as u64,
                UserCopyAccess::Read,
            )
            .is_ok()
        );
        assert_eq!(
            map.validate_range(bootstrap, 8, UserCopyAccess::Write),
            Err(UserCopyError::PermissionDenied)
        );
        assert_eq!(
            map.validate_range(0xFFFF_FFFF_8000_0000, 8, UserCopyAccess::Read),
            Err(UserCopyError::OutOfRange)
        );
    }

    fn minimal_user_elf() -> [u8; 8196] {
        let mut elf = [0u8; 8196];
        write_elf_header(&mut elf, 2, 0x0040_0000);
        write_phdr(&mut elf, 0, 0x5, 0x1000, 0x0040_0000, 2, 2);
        write_phdr(&mut elf, 1, 0x6, 0x2000, 0x0040_1000, 4, 16);
        elf[0x1000..0x1002].copy_from_slice(b"\x90\xc3");
        elf[0x2000..0x2004].copy_from_slice(b"DATA");
        elf
    }

    fn write_elf_header(elf: &mut [u8], phnum: u16, entry: u64) {
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[6] = 1;
        elf[16..18].copy_from_slice(&2u16.to_le_bytes());
        elf[18..20].copy_from_slice(&0x3Eu16.to_le_bytes());
        elf[20..24].copy_from_slice(&1u32.to_le_bytes());
        elf[24..32].copy_from_slice(&entry.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[52..54].copy_from_slice(&64u16.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&phnum.to_le_bytes());
    }

    fn write_phdr(
        elf: &mut [u8],
        index: usize,
        flags: u32,
        offset: u64,
        vaddr: u64,
        filesz: u64,
        memsz: u64,
    ) {
        let entry = 64 + index * 56;
        elf[entry..entry + 4].copy_from_slice(&1u32.to_le_bytes());
        elf[entry + 4..entry + 8].copy_from_slice(&flags.to_le_bytes());
        elf[entry + 8..entry + 16].copy_from_slice(&offset.to_le_bytes());
        elf[entry + 16..entry + 24].copy_from_slice(&vaddr.to_le_bytes());
        elf[entry + 24..entry + 32].copy_from_slice(&vaddr.to_le_bytes());
        elf[entry + 32..entry + 40].copy_from_slice(&filesz.to_le_bytes());
        elf[entry + 40..entry + 48].copy_from_slice(&memsz.to_le_bytes());
        elf[entry + 48..entry + 56].copy_from_slice(&4096u64.to_le_bytes());
    }
}
