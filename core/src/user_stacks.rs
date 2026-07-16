//! Fixed guarded user-mode stack pool for Phase 8.
#![cfg_attr(test, allow(dead_code))]

use core::cell::UnsafeCell;

pub const USER_STACK_PAGE_SIZE: usize = 4096;
pub const USER_STACK_COUNT: usize = 2;
const USER_STACK_SLOT_PAGES: usize = 2;
const USER_STACK_SLOT_SIZE: usize = USER_STACK_SLOT_PAGES * USER_STACK_PAGE_SIZE;
const USER_STACK_POOL_SIZE: usize = USER_STACK_COUNT * USER_STACK_SLOT_SIZE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserStackError {
    Misaligned,
    BadGuardLayout,
    Overlap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserStackRegion {
    pub guard_start: u64,
    pub stack_start: u64,
    pub stack_len: u64,
}

#[repr(align(4096))]
struct UserStackPool(UnsafeCell<[u8; USER_STACK_POOL_SIZE]>);

// SAFETY:
// 1. Invariant: each stack slot is used only by the single active Phase 8
//    proof; no two user contexts run concurrently yet.
// 2. Established by: Phase 8 still has no scheduler-visible user processes.
// 3. Lifetime: the pool is static for all of PythCore.
// 4. Pointer ownership: this module owns the backing pages; the CPU owns stack
//    pushes while executing the proof.
// 5. Alignment: the wrapper gives page alignment.
// 6. Mapped length: exactly `USER_STACK_POOL_SIZE` bytes.
// 7. Concurrency: single-core proof, one user stack active at a time.
// 8. Violation: concurrent reuse would corrupt user stack frames.
unsafe impl Sync for UserStackPool {}

static USER_STACK_POOL: UserStackPool = UserStackPool(UnsafeCell::new([0; USER_STACK_POOL_SIZE]));

pub fn run_self_test() -> Result<(), UserStackError> {
    let regions = regions();
    for region in regions {
        if !is_page_aligned(region.guard_start) || !is_page_aligned(region.stack_start) {
            return Err(UserStackError::Misaligned);
        }
        if region.stack_len != USER_STACK_PAGE_SIZE as u64 {
            return Err(UserStackError::BadGuardLayout);
        }
        if region.guard_start + USER_STACK_PAGE_SIZE as u64 != region.stack_start {
            return Err(UserStackError::BadGuardLayout);
        }
    }
    if regions[0].stack_start >= regions[1].guard_start {
        return Err(UserStackError::Overlap);
    }
    Ok(())
}

pub fn regions() -> [UserStackRegion; USER_STACK_COUNT] {
    [region(0), region(1)]
}

pub fn proof_stack_region() -> (u64, u64) {
    let region = region(0);
    (region.stack_start, region.stack_len)
}

pub fn proof_stack_top() -> u64 {
    let (start, len) = proof_stack_region();
    start + len - 16
}

fn region(index: usize) -> UserStackRegion {
    debug_assert!(index < USER_STACK_COUNT);
    let guard_start = pool_start() + (index * USER_STACK_SLOT_SIZE) as u64;
    UserStackRegion {
        guard_start,
        stack_start: guard_start + USER_STACK_PAGE_SIZE as u64,
        stack_len: USER_STACK_PAGE_SIZE as u64,
    }
}

fn pool_start() -> u64 {
    USER_STACK_POOL.0.get().cast::<u8>() as u64
}

fn is_page_aligned(value: u64) -> bool {
    value.is_multiple_of(USER_STACK_PAGE_SIZE as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_stack_regions_have_guard_pages_below_usable_pages() {
        let regions = regions();

        assert_eq!(run_self_test(), Ok(()));
        assert_eq!(
            regions[0].guard_start + USER_STACK_PAGE_SIZE as u64,
            regions[0].stack_start
        );
        assert_eq!(
            regions[1].guard_start + USER_STACK_PAGE_SIZE as u64,
            regions[1].stack_start
        );
    }

    #[test]
    fn proof_stack_uses_first_guarded_region() {
        let first = regions()[0];

        assert_eq!(proof_stack_region(), (first.stack_start, first.stack_len));
        assert!(proof_stack_top() < first.stack_start + first.stack_len);
        assert!(proof_stack_top() >= first.stack_start);
    }
}
