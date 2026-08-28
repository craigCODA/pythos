//! `shell.elf`: the first ring-3 object/capability shell (ADR 0051/0052).
//!
//! Human command parsing and presentation live entirely here; PythCore
//! exposes only typed, capability-gated console/object/system-control
//! syscalls and never parses command grammar.
//!
//! Like `core/src/main.rs`, `no_main`/`no_std` are only applied for real
//! (non-test) builds: `cargo test` builds this crate for the host target,
//! where an unconditional `#![no_main]` + a `_start` entry point conflicts
//! with the host linker (MSVC's `link.exe` has no concept of an ELF `_start`
//! entry point).
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;
#[cfg(not(test))]
use pythos_shared::object_shell_abi::BootstrapCapabilityBlock;
#[cfg(not(test))]
use pythos_user_shell::{
    capability_map::CapabilityMap,
    commands::{Command, parse_command},
    line_editor::{LineAction, LineEditor},
    syscalls,
};

/// Process entry. PythCore's launch ABI passes the read-only
/// `BootstrapCapabilityBlock` pointer in `rdi` (the standard `extern "C"`
/// first-argument register), matching `_start`'s single parameter.
///
/// # Safety
///
/// The caller (PythCore's launch path) must place a valid, initialized,
/// read-only-mapped `BootstrapCapabilityBlock` pointer in `rdi` before
/// transferring control here, per ADR 0052. This function never returns.
///
/// Gated to non-test builds only: under `cfg(test)` this crate builds for the
/// host target, which has no PythCore launch path to satisfy this contract,
/// and a `_start`/`no_mangle` symbol risks colliding with the host CRT's own
/// entry point.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(bootstrap_ptr: *const BootstrapCapabilityBlock) -> ! {
    // SAFETY: matches this function's own `# Safety` contract above; the
    // pointer comes directly from the PythCore launch ABI argument.
    let bootstrap = match unsafe { syscalls::bootstrap_capabilities(bootstrap_ptr) } {
        Ok(bootstrap) => bootstrap,
        Err(_) => loop {
            core::hint::spin_loop();
        },
    };
    let console = bootstrap.console;
    let system_control = bootstrap.system_control;
    let mut object_caps = CapabilityMap::from_bootstrap(&bootstrap);

    syscalls::write_str(console, "PYTHOS:SHELL:READY\r\n");
    syscalls::write_str(console, "pyth> ");

    let mut line_editor = LineEditor::new();
    loop {
        match syscalls::read_byte(console) {
            Some(byte) => match line_editor.input(byte) {
                LineAction::None => {}
                LineAction::Echo(byte) => syscalls::write_byte(console, byte),
                LineAction::Erase => syscalls::write_str(console, "\x08 \x08"),
                LineAction::Execute { line, len } => {
                    syscalls::write_str(console, "\r\n");
                    run_line(console, system_control, &mut object_caps, &line[..len]);
                    syscalls::write_str(console, "pyth> ");
                }
                LineAction::RejectOverflow => {
                    syscalls::write_str(console, "\r\nERROR line-too-long\r\npyth> ");
                }
            },
            None => core::hint::spin_loop(),
        }
    }
}

#[cfg(not(test))]
fn run_line(
    console: pythos_shared::object_shell_abi::PackedCapability,
    system_control: pythos_shared::object_shell_abi::PackedCapability,
    object_caps: &mut CapabilityMap,
    line: &[u8],
) {
    match parse_command(line) {
        Ok(Command::Help) => syscalls::write_help(console),
        Ok(Command::Reboot) => syscalls::request_reboot(system_control),
        Ok(Command::Object {
            mut request,
            text,
            text_len,
        }) => {
            syscalls::dispatch_object_request(console, object_caps, &mut request, &text[..text_len])
        }
        Ok(Command::Task {
            mut request,
            input,
            input_len,
        }) => {
            syscalls::dispatch_task_request(console, object_caps, &mut request, &input[..input_len])
        }
        Err(_) => syscalls::write_str(console, "ERROR unknown-command\r\n"),
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
