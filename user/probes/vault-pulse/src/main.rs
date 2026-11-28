#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::{arch::asm, panic::PanicInfo};
#[cfg(not(test))]
use pythos_shared::vault_pulse::{
    INVOCATION_LEN, InvalidPulse, Invocation, OUTCOME_LEN, arithmetic_failure,
};

/// Execute one admitted pulse and return to its PythCore caller.
///
/// # Safety
/// The input and output must identify the admitted, disjoint user data pages.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(
    input: *const [u8; INVOCATION_LEN],
    output: *mut [u8; OUTCOME_LEN],
) -> ! {
    // SAFETY: PythCore mapped these disjoint, bounded pages for this call.
    // It initialized the read-only input and suspended access to the result
    // until the sole ring-3 worker returns.
    let invocation = Invocation::decode(unsafe { &*input });
    let result = invocation
        .ok()
        .and_then(|invocation| match invocation.execute() {
            Ok(outcome) => Some(outcome.encode()),
            Err(InvalidPulse::Arithmetic) => Some(arithmetic_failure(invocation.generation)),
            Err(_) => None,
        });
    if let Some(result) = result {
        // SAFETY: output has the size and permissions of the admitted result
        // page. No other reference or worker accesses the page during entry.
        unsafe { output.write(result) };
    }
    // SAFETY: the returnable entry armed the ring-3 breakpoint path. A
    // failure to return to PythCore faults before execution can continue.
    unsafe { asm!("int3", "ud2", options(noreturn, nostack)) };
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    // SAFETY: an unexpected worker fault is contained by the returnable
    // process boundary and gives PythCore no valid outcome to commit.
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) };
}
