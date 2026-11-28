#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::{arch::asm, cell::UnsafeCell, panic::PanicInfo};
#[cfg(not(test))]
use pythos_shared::{
    graph_window::{INVOCATION_LEN, Invocation, RESULT_LEN},
    pyth_tig::{format::PythGraphPackage, verify::VerifiedGraph},
};
#[cfg(not(test))]
use pythos_user_graph_window::{Scratch, evaluate};

#[cfg(not(test))]
struct WorkerScratch(UnsafeCell<Scratch>);

// SAFETY: ProofWorker serializes entry through one ring-3 context on one CPU.
// Each entry holds the only scratch borrow until it returns; the runtime
// provides no thread creation or callbacks. The scratch cell has static
// lifetime and preserves alignment and size for a complete Scratch. Concurrent
// entry would alias the interpreter tables and violates the launch contract.
#[cfg(not(test))]
unsafe impl Sync for WorkerScratch {}

#[cfg(not(test))]
static SCRATCH: WorkerScratch = WorkerScratch(UnsafeCell::new(Scratch::new()));

/// Execute the admitted package and return one bounded scene.
///
/// # Safety
/// PythCore supplies disjoint payload pages and verifies the exact graph bytes
/// before entry. Input is read-only and output is private to this invocation.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(
    input: *const [u8; INVOCATION_LEN],
    output: *mut [u8; RESULT_LEN],
) -> ! {
    // SAFETY: ProofWorker maps one initialized input page aligned to 4 KiB.
    // The page remains read-only for this single worker until the return trap.
    // The kernel owns the backing; violating its mapping contract faults the worker.
    let input = unsafe { &*input };
    if let Ok(invocation) = Invocation::decode(input)
        && let Ok(package) = PythGraphPackage::decode(invocation.package)
    {
        // SAFETY: the kernel verified these exact bytes before copying them to
        // the read-only invocation page. The checked decode bounds the byte
        // slice in that page and materializes unaligned fields by value. The
        // kernel owns the page through the single worker's return. A different
        // byte sequence would bypass the shared verifier.
        let verified = unsafe { VerifiedGraph::assume_kernel_verified_package(&package) };
        // SAFETY: the one active worker owns the only mutable reference to
        // the complete, correctly aligned Scratch until evaluate returns.
        // It resides in this address space for the retained worker lifetime.
        // Reentry would alias the scratch; ProofWorker serializes calls.
        let scratch = unsafe { &mut *SCRATCH.0.get() };
        if let Ok(result) = evaluate(verified, invocation.workspace, scratch) {
            // SAFETY: ProofWorker grants a disjoint, 4 KiB-aligned writable page.
            // This worker owns its first RESULT_LEN bytes through the return
            // trap. The kernel reads only afterward. A bad mapping faults here.
            unsafe { output.write(result.encode()) };
        }
    }
    // SAFETY: ProofWorker arms the returnable breakpoint and restores its
    // supervisor stack. These instructions access no pointers or memory
    // operands. If execution continued in this worker, UD2 contains the error.
    unsafe { asm!("int3", "ud2", options(noreturn, nostack)) };
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    // SAFETY: UD2 needs no mapped operands. The supervisor contains the fault
    // from this single active worker and discards the incomplete reply.
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) };
}
