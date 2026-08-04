#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;
#[cfg(not(test))]
use pythos_shared::{
    pyth_runtime_abi::PythGraphBootstrapBlock,
    pyth_tig::{format::PythGraphPackage, verify::verify_package},
};
#[cfg(not(test))]
use pythos_user_pyth_runtime::{interpreter::Interpreter, syscalls};

/// Ring-3 PythTIG runtime entry. PythCore passes a read-only
/// `PythGraphBootstrapBlock` pointer in `rdi`.
///
/// # Safety
///
/// The caller must provide a valid graph runtime bootstrap mapping, package
/// mapping, result mapping, and capability import table according to the
/// Phase 2 runtime ABI. This function never returns.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(bootstrap_ptr: *const PythGraphBootstrapBlock) -> ! {
    let Ok(bootstrap) = (unsafe { syscalls::bootstrap_graph(bootstrap_ptr) }) else {
        spin_forever();
    };
    let Ok(imports) = syscalls::import_table(&bootstrap) else {
        spin_forever();
    };
    let Ok(package_bytes) = (unsafe { syscalls::package_bytes(&bootstrap) }) else {
        spin_forever();
    };
    let Ok(package) = PythGraphPackage::decode(package_bytes) else {
        spin_forever();
    };
    let Ok(verified) = verify_package(&package) else {
        spin_forever();
    };

    let mut host = syscalls::GraphSyscallHost;
    let exit =
        Interpreter::new(verified, &imports, bootstrap.instruction_budget).execute(&mut host);
    unsafe {
        syscalls::write_exit_record(&bootstrap, &exit);
    }
    spin_forever();
}

#[cfg(not(test))]
fn spin_forever() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    spin_forever()
}
