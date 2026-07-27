//! Static retained normal-boot service owner for ADR 0052.
#![cfg_attr(test, allow(dead_code))]

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(test))]
use crate::block_device::BlockDeviceInfo;
#[cfg(not(test))]
use crate::general_storage_persistence::GeneralStoragePersistenceError;
use crate::object_service::{ObjectService, ObjectServiceError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetainedServiceError {
    ObjectServiceAlreadyInitialized,
    ObjectServiceUnavailable,
    ObjectService(ObjectServiceError),
    #[cfg(not(test))]
    Persistence(GeneralStoragePersistenceError),
    /// Task 11: `persist_object_service` was called while running on a stack
    /// other than the guarded `syscall_kernel_stack` (see
    /// `syscall::kernel_stack_bounds`). Only that stack's headroom has been
    /// measured and hardened with guard pages; calling this from, e.g.,
    /// `normal_boot.rs`'s main boot stack is refused until that stack gets
    /// the same treatment.
    #[cfg(not(test))]
    UnmeasuredStackContext,
}

struct RetainedObjectServiceStorage(UnsafeCell<MaybeUninit<ObjectService>>);

// SAFETY:
// 1. Invariant: object service storage is initialized exactly once before
//    shell launch and syscall dispatch.
// 2. Established by: normal_boot calls initialize_object_service before
//    enter_persistent_user_process, and verify boot does not use this path.
// 3. Lifetime: storage is static and lives for the whole boot.
// 4. Pointer ownership: with_object_service grants one mutable borrow for the
//    duration of one syscall dispatch closure.
// 5. Alignment: MaybeUninit<ObjectService> provides ObjectService alignment.
// 6. Mapped length: exactly one ObjectService object is accessed.
// 7. Concurrency: ADR 0051 is single-core and does not re-enter syscalls while
//    one object-service borrow is active.
// 8. Violation: concurrent access could corrupt object state or grant authority
//    to the wrong caller.
unsafe impl Sync for RetainedObjectServiceStorage {}

static OBJECT_SERVICE: RetainedObjectServiceStorage =
    RetainedObjectServiceStorage(UnsafeCell::new(MaybeUninit::uninit()));
static OBJECT_SERVICE_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[cfg(not(test))]
struct RetainedDeviceStorage(UnsafeCell<MaybeUninit<BlockDeviceInfo>>);

#[cfg(not(test))]
// SAFETY:
// 1. Invariant: the retained device handle is written exactly once, before
//    the object service is marked initialized, and read only by
//    `persist_object_service` after that point.
// 2. Established by: `initialize_object_service_from_device` writes this slot
//    and only then flips `OBJECT_SERVICE_INITIALIZED`, which every reader
//    checks first.
// 3. Lifetime: storage is static and lives for the whole boot.
// 4. Pointer ownership: this module exclusively owns the storage.
// 5. Alignment: `MaybeUninit<BlockDeviceInfo>` provides its alignment.
// 6. Mapped length: exactly one `BlockDeviceInfo` value is accessed.
// 7. Concurrency: ADR 0051 normal boot is single-core and non-reentrant.
// 8. Violation: reading before the one write would expose uninitialized data;
//    the initialized-flag check on every read path prevents this.
unsafe impl Sync for RetainedDeviceStorage {}

#[cfg(not(test))]
static OBJECT_SERVICE_DEVICE: RetainedDeviceStorage =
    RetainedDeviceStorage(UnsafeCell::new(MaybeUninit::uninit()));

pub fn initialize_object_service(service: ObjectService) -> Result<(), RetainedServiceError> {
    if OBJECT_SERVICE_INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(RetainedServiceError::ObjectServiceAlreadyInitialized);
    }
    // SAFETY:
    // 1. Invariant: this is the only initialization write to the static slot.
    // 2. Established by: the compare_exchange above transitions false -> true
    //    for exactly one caller in this single-core slice.
    // 3. Lifetime: the written ObjectService lives in static storage for the
    //    rest of the boot.
    // 4. Pointer ownership: OBJECT_SERVICE owns the storage; the input service
    //    is moved into it and not used again by the caller.
    // 5. Alignment: MaybeUninit<ObjectService> provides ObjectService alignment.
    // 6. Mapped length: exactly one ObjectService object is written.
    // 7. Concurrency: ADR 0051 normal boot is single-core before SMP.
    // 8. Violation: a second writer would overwrite retained service state.
    unsafe {
        (*OBJECT_SERVICE.0.get()).write(service);
    }
    Ok(())
}

#[cfg(not(test))]
pub fn initialize_object_service_from_device(
    device: BlockDeviceInfo,
) -> Result<(), RetainedServiceError> {
    if OBJECT_SERVICE_INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(RetainedServiceError::ObjectServiceAlreadyInitialized);
    }
    // SAFETY:
    // 1. Invariant: this is the only initialization path for the retained
    //    object service in normal boot.
    // 2. Established by: the compare_exchange above transitions false -> true
    //    once, before shell launch and before syscall dispatch.
    // 3. Lifetime: the MaybeUninit slot is static and survives the full boot.
    // 4. Pointer ownership: ObjectService writes exactly one initialized value
    //    into the retained slot and no reference escapes during construction.
    // 5. Alignment: MaybeUninit<ObjectService> provides ObjectService alignment.
    // 6. Mapped length: exactly one ObjectService object is initialized.
    // 7. Concurrency: ADR 0051 normal boot is single-core and non-reentrant.
    // 8. Violation: a second initializer could race and expose partial state.
    let result = ObjectService::restore_or_initialize_in_place(
        unsafe { &mut *OBJECT_SERVICE.0.get() },
        device,
    );
    if let Err(error) = result {
        OBJECT_SERVICE_INITIALIZED.store(false, Ordering::SeqCst);
        return Err(RetainedServiceError::ObjectService(error));
    }
    // SAFETY: matches RetainedDeviceStorage's own invariant above - this is
    // the one write, performed before any caller can observe
    // OBJECT_SERVICE_INITIALIZED as true and call persist_object_service.
    unsafe {
        (*OBJECT_SERVICE_DEVICE.0.get()).write(device);
    }
    Ok(())
}

/// Encode the retained object service's current state and write it to the
/// ADR 0052 two-slot checkpoint, so it survives a reboot. Callers invoke this
/// after any syscall that mutates durable object state (create, revise).
#[cfg(not(test))]
pub fn persist_object_service() -> Result<(), RetainedServiceError> {
    if !running_on_syscall_kernel_stack() {
        return Err(RetainedServiceError::UnmeasuredStackContext);
    }
    let snapshot = with_object_service(|service| service.encode_snapshot())?
        .map_err(RetainedServiceError::ObjectService)?;
    // SAFETY: matches RetainedDeviceStorage's own invariant above - this read
    // only happens once OBJECT_SERVICE_INITIALIZED is true (checked inside
    // with_object_service above, which this call is gated behind), and the
    // device slot is always written before that flag is observably true.
    let device = unsafe { (*OBJECT_SERVICE_DEVICE.0.get()).assume_init() };
    crate::object_service_checkpoint::write_object_service_checkpoint(device, &snapshot)
        .map_err(RetainedServiceError::Persistence)
}

/// Task 11: `persist_object_service`'s call chain has only been stress-tested
/// and bisected for headroom on the syscall-entry kernel stack (see
/// `syscall::kernel_stack_bounds`). Refuse to run it from any other stack
/// (e.g. `normal_boot.rs`'s main boot stack) until that stack gets the same
/// measurement and guard-page treatment.
#[cfg(not(test))]
fn running_on_syscall_kernel_stack() -> bool {
    let (start, end) = crate::syscall::kernel_stack_bounds();
    let rsp = current_rsp();
    rsp >= start && rsp < end
}

#[cfg(not(test))]
fn current_rsp() -> u64 {
    let rsp: u64;
    // SAFETY:
    // 1. Invariant: reading RSP into a general-purpose register is valid at
    //    any privilege level and has no side effects.
    // 2. Established by: this is a plain register read.
    // 3. Lifetime: the read value is a snapshot; not a borrow.
    // 4. Pointer ownership: no memory is accessed.
    // 5. Alignment: not applicable to a register read.
    // 6. Mapped length: not applicable.
    // 7. Concurrency: single-core boot; RSP is per-CPU-core state.
    // 8. Violation: none; this instruction cannot fault.
    unsafe {
        core::arch::asm!("mov {out}, rsp", out = out(reg) rsp, options(nomem, nostack, preserves_flags));
    }
    rsp
}

pub fn with_object_service<R>(
    f: impl FnOnce(&mut ObjectService) -> R,
) -> Result<R, RetainedServiceError> {
    if !OBJECT_SERVICE_INITIALIZED.load(Ordering::SeqCst) {
        return Err(RetainedServiceError::ObjectServiceUnavailable);
    }
    // SAFETY:
    // 1. Invariant: object service storage is initialized exactly once before
    //    shell launch and syscall dispatch.
    // 2. Established by: initialize_object_service sets the initialized flag
    //    only after writing one ObjectService into the static slot.
    // 3. Lifetime: storage is static and lives for the whole boot.
    // 4. Pointer ownership: this grants one mutable borrow for the duration of
    //    one syscall dispatch closure and does not store the borrow.
    // 5. Alignment: MaybeUninit<ObjectService> provides ObjectService alignment.
    // 6. Mapped length: exactly one ObjectService object is accessed.
    // 7. Concurrency: ADR 0051 is single-core and does not re-enter syscalls
    //    while one object-service borrow is active.
    // 8. Violation: concurrent access could corrupt object state or grant
    //    authority to the wrong caller.
    Ok(f(unsafe { (*OBJECT_SERVICE.0.get()).assume_init_mut() }))
}

#[cfg(test)]
fn reset_for_test() {
    if OBJECT_SERVICE_INITIALIZED.swap(false, Ordering::SeqCst) {
        // SAFETY:
        // 1. Invariant: the initialized flag was true, so the static slot
        //    contains one initialized ObjectService.
        // 2. Established by: initialize_object_service is the only writer and
        //    set the flag after writing the value.
        // 3. Lifetime: this test-only reset ends the current stored value's
        //    lifetime before the next test initializes a replacement.
        // 4. Pointer ownership: tests hold no service borrow while calling this.
        // 5. Alignment: MaybeUninit<ObjectService> provides ObjectService alignment.
        // 6. Mapped length: exactly one ObjectService object is dropped.
        // 7. Concurrency: retained_services tests take TEST_LOCK before reset.
        // 8. Violation: resetting while borrowed would invalidate a live
        //    mutable reference.
        unsafe {
            (*OBJECT_SERVICE.0.get()).assume_init_drop();
        }
    }
}

#[cfg(test)]
pub(crate) struct RetainedServiceTestGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for RetainedServiceTestGuard {
    fn drop(&mut self) {
        reset_for_test();
    }
}

#[cfg(test)]
static TEST_SERVICE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn initialize_object_service_for_test(
    service: ObjectService,
) -> RetainedServiceTestGuard {
    let lock = TEST_SERVICE_LOCK.lock().unwrap();
    reset_for_test();
    initialize_object_service(service).unwrap();
    RetainedServiceTestGuard { _lock: lock }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_objects::ObjectKind;

    #[test]
    fn with_object_service_requires_initialization() {
        let _guard = TEST_SERVICE_LOCK.lock().unwrap();
        reset_for_test();

        assert_eq!(
            with_object_service(|_| ()),
            Err(RetainedServiceError::ObjectServiceUnavailable)
        );
    }

    #[test]
    fn retained_object_service_mutation_survives_between_borrows() {
        let _guard = initialize_object_service_for_test(ObjectService::new_for_test());

        let created_id = with_object_service(|service| {
            let shell = service.test_shell_caller();
            let workspace = service.test_shell_workspace_capability();
            service
                .create_object(shell, workspace, ObjectKind::Note)
                .unwrap()
                .object_id
        })
        .unwrap();

        let queried_id = with_object_service(|service| {
            let shell = service.test_shell_caller();
            let workspace = service.test_shell_workspace_capability();
            service
                .query_objects(shell, workspace, ObjectKind::Note)
                .unwrap()[0]
                .object_id
        })
        .unwrap();

        assert_eq!(queried_id, created_id.raw());
    }
}
