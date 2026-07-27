//! Active ring-3 process identity for syscall authorization.

#![cfg_attr(test, allow(dead_code))]

use core::cell::UnsafeCell;

use crate::service_identity::ServiceId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveUserProcess {
    service_id: ServiceId,
    principal_id: u64,
    program_digest: u64,
}

impl ActiveUserProcess {
    pub const fn new(service_id: ServiceId, principal_id: u64, program_digest: u64) -> Self {
        Self {
            service_id,
            principal_id,
            program_digest,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessContextError {
    NoActiveProcess,
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
}
