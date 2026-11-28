#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::{arch::asm, panic::PanicInfo};
#[cfg(not(test))]
use pythos_shared::world_session::{FRAME_LEN, INVOCATION_LEN, Invocation};

/// Interpret one admitted input with the saved world and current view.
///
/// # Safety
/// The pointers must identify the disjoint input and output pages admitted for this invocation.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(
    input: *const [u8; INVOCATION_LEN],
    output: *mut [u8; FRAME_LEN],
) -> ! {
    // SAFETY: the loader grants this instance an initialized, aligned read-only
    // input and a separate writable result. Both are mapped for the whole call,
    // and only this worker can access them until the return trap.
    if let Ok(invocation) = Invocation::decode(unsafe { &*input }) {
        let result = pythos_user_world_session::step(invocation);
        // SAFETY: output is the private writable result page, aligned for the
        // bounded frame. PythCore validates the bytes only after this call returns.
        unsafe { output.write(result.encode()) };
    }
    // SAFETY: the returnable entry arms the ring-3 breakpoint return. A
    // failed return cannot continue into other code.
    unsafe { asm!("int3", "ud2", options(noreturn, nostack)) };
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    // SAFETY: the returnable process boundary contains this worker fault.
    unsafe { asm!("ud2", options(noreturn, nomem, nostack)) };
}
