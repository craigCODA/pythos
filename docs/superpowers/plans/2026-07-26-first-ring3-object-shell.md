# First Ring-3 Object Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build ADR 0051's first ring-3 object/capability shell, driven over COM2, proving object create/revise/inspect/history and reboot restore through authority reconstruction.

**Architecture:** Keep COM1 as the boot/test oracle and add COM2 as the interactive shell transport. Launch a real ring-3 `shell.elf` on normal boot; the shell uses byte-oriented syscalls for serial I/O plus a narrow versioned object-shell transaction bridge. The temporary bridge remains kernel-backed, capability-gated, and explicitly scoped until a future user-space object service exists.

**Tech Stack:** Rust `no_std` PythCore, Rust `no_std` user shell ELF, QEMU q35/OVMF, COM1 file serial log, COM2 TCP serial socket, Python acceptance harness, Cargo feature `verify`.

## Global Constraints

- Read `docs/PythOS-SAS-001.md`, `docs/PythOS-TDD-001.md`, and `docs/decisions/0051-first-ring3-object-shell.md` before editing.
- Implement only the ring-3 object/capability shell slice described here.
- Do not begin universal-device support, networking, package management, user-space drivers, AI agents, `grant`, or `launch`.
- Preserve `verify`: existing serial-marker proofs and `scripts/test-boot.py` must keep running under `--features verify`.
- Normal boot must not call `qemu_exit::success()`.
- COM1 remains the verification oracle; COM2 carries interactive shell traffic.
- The shell is a ring-3 user process with declared bootstrap capabilities, not a privileged kernel console.
- Object IDs identify; capabilities authorize.
- Runtime capability handles are not persisted across reboot; fresh handles are minted from stable principal/workspace authority.
- Every unsafe block requires a documented invariant.
- Serial output, not screenshots, is the acceptance oracle.

---

## File Structure

- Modify `scripts/run-qemu.py`: add optional COM2 TCP socket wiring without changing default COM1 behavior.
- Create `scripts/test-object-shell.py`: build a normal image, drive COM2 commands, restart QEMU over the same storage image, and assert COM1/COM2 evidence.
- Modify `core/src/serial.rs`: generalize UART access enough for COM1 writes and COM2 read/write.
- Modify `core/src/syscall.rs`: preserve `0x5059_0000` and `0x5059_0001`, add register argument capture, shell serial syscalls, and object-shell bridge syscalls.
- Modify `core/src/main.rs`: register new modules, initialize the object-shell bridge, and launch `shell.elf` in normal boot.
- Modify `core/src/shell_objects.rs` and `core/src/typed_object_format.rs`: add `ObjectKind::Note` with a stable format code.
- Create `core/src/object_shell_protocol.rs`: parse ADR 0051 command text into typed requests and format typed responses.
- Create `core/src/object_shell_store.rs`: hold the temporary kernel-backed note object store, shell principal, workspace-root authority, revision history, persistence encoding, and capability rebinding.
- Create `core/src/object_shell_bridge.rs`: own bounded command/response queues and expose syscall-facing byte operations against the protocol/store.
- Create `user/shell/Cargo.toml`, `user/shell/src/main.rs`, and `user/shell/linker.ld`: build the first real ring-3 shell ELF.
- Modify root `Cargo.toml`: add `user/shell` as a workspace member.
- Create `scripts/build-user-shell.py`: build `pythos-user-shell` with shell-only linker flags so kernel flags remain untouched.
- Modify `scripts/build-image.py` and `scripts/build-iso.py`: embed `shell.elf` in the inner `INIT.PAK` bundle as a named user ELF payload.
- Modify `docs/ROADMAP.md` and `docs/HANDOVER.md`: record the implemented ADR 0051 slice after it passes.

---

### Task 1: COM2 QEMU Harness And Failing Acceptance Test

**Files:**
- Modify: `scripts/run-qemu.py`
- Create: `scripts/test-object-shell.py`
- Test: `scripts/test-object-shell.py`

**Interfaces:**
- Consumes: existing `scripts/run-qemu.py --serial-log`, `--storage-image`, `--timeout`, and `--expect-outcome`.
- Produces: `run_shell_session(storage_image: Path, commands: list[str]) -> ShellRunResult` inside `scripts/test-object-shell.py`, plus `--shell-port` in `scripts/run-qemu.py`.

- [ ] **Step 1: Write the failing COM2 acceptance test**

Create `scripts/test-object-shell.py` with this shape:

```python
#!/usr/bin/env python
"""Acceptance test for ADR 0051 ring-3 object shell."""

from __future__ import annotations

import socket
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "object-shell-com1.log"
SHELL_TRANSCRIPT = TARGET / "object-shell-com2.log"
STORAGE_IMAGE = TARGET / "object-shell-store.img"
STORAGE_SIZE_BYTES = 16 * 1024 * 1024
SHELL_PORT = 4582

READY = "PYTHOS:SHELL:READY"
CREATED = "CREATED object:1042 revision:1"
COMMITTED = "COMMITTED revision:2"
DENIED = "DENIED missing-capability"
RESTORED = "PYTHOS:SHELL:IDENTITY_RESTORED"


@dataclass
class ShellRunResult:
    com1: str
    com2: str


def run(command: list[str], expected_returncode: int = 0) -> str:
    print("+ " + " ".join(command))
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout)
    if result.returncode != expected_returncode:
        raise AssertionError(
            f"expected return code {expected_returncode}, got {result.returncode}"
        )
    return result.stdout


def build_normal_image() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/build-image.py"])


def prepare_storage(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def wait_for(sock: socket.socket, needle: str, timeout: float = 20.0) -> str:
    deadline = time.monotonic() + timeout
    data = ""
    while time.monotonic() < deadline:
        try:
            chunk = sock.recv(4096).decode("utf-8", errors="replace")
        except socket.timeout:
            continue
        if chunk:
            data += chunk
            if needle in data:
                return data
    raise AssertionError(f"timed out waiting for {needle!r}; transcript was {data!r}")


def run_shell_session(storage_image: Path, commands: list[str]) -> ShellRunResult:
    if SHELL_TRANSCRIPT.exists():
        SHELL_TRANSCRIPT.unlink()
    process = subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--shell-port",
            str(SHELL_PORT),
            "--storage-image",
            str(storage_image),
            "--timeout",
            "90",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    try:
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            try:
                with socket.create_connection(("127.0.0.1", SHELL_PORT), timeout=1) as sock:
                    transcript = wait_for(sock, READY)
                    for command in commands:
                        sock.sendall((command + "\n").encode("ascii"))
                        if command == "reboot":
                            transcript += wait_for(sock, "REBOOTING")
                            break
                        transcript += wait_for(sock, "pyth> ")
                    SHELL_TRANSCRIPT.write_text(
                        transcript,
                        encoding="utf-8",
                        errors="replace",
                    )
                    return ShellRunResult(
                        com1=SERIAL_LOG.read_text(encoding="utf-8", errors="replace"),
                        com2=transcript,
                    )
            except OSError:
                time.sleep(0.2)
        raise AssertionError("COM2 shell socket never became reachable")
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=3)
        if process.stdout is not None:
            print(process.stdout.read())


def assert_contains(value: str, needle: str) -> None:
    if needle not in value:
        raise AssertionError(f"missing {needle!r} in {value!r}")


def main() -> int:
    build_normal_image()
    prepare_storage(STORAGE_IMAGE)
    first = run_shell_session(
        STORAGE_IMAGE,
        [
            "create kind:note",
            'revise object:1042 text="hello"',
            "inspect object:9999",
            "reboot",
        ],
    )
    assert_contains(first.com1, "PYTHOS:CORE:NORMAL_BOOT_ALIVE")
    assert_contains(first.com1, "PYTHOS:SHELL:RING3_ENTER")
    assert_contains(first.com2, CREATED)
    assert_contains(first.com2, COMMITTED)
    assert_contains(first.com2, DENIED)

    second = run_shell_session(STORAGE_IMAGE, ["inspect object:1042", "history object:1042"])
    assert_contains(second.com1, RESTORED)
    assert_contains(second.com2, 'text="hello" revision:2')
    assert_contains(second.com2, "history object:1042 revisions:2")
    print("OBJECT_SHELL_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Run the failing test**

Run:

```powershell
python scripts\test-object-shell.py
```

Expected: FAIL before build because `scripts/build-user-shell.py`, package `pythos-user-shell`, and `--shell-port` do not exist.

- [ ] **Step 3: Add COM2 QEMU options without making the test pass**

In `scripts/run-qemu.py`, add:

```python
DEFAULT_SHELL_PORT = 4582
```

Add arguments:

```python
parser.add_argument("--shell-port", type=int)
```

When `args.shell_port` is present, append a second serial device:

```python
command += [
    "-serial",
    f"tcp:127.0.0.1:{args.shell_port},server=on,wait=off",
]
```

Do not alter the first `-serial file:{args.serial_log}` entry.

- [ ] **Step 4: Run harness parser checks**

Run:

```powershell
python -m py_compile scripts\run-qemu.py scripts\test-object-shell.py
```

Expected: PASS.

- [ ] **Step 5: Run the failing acceptance test again**

Run:

```powershell
python scripts\test-object-shell.py
```

Expected: FAIL because the user shell package and kernel shell markers are still absent.

- [ ] **Step 6: Commit**

```powershell
git add scripts\run-qemu.py scripts\test-object-shell.py
git commit -m "test(shell): add COM2 object shell acceptance harness"
```

---

### Task 2: COM2 UART Driver And Syscall Argument Register ABI

**Files:**
- Modify: `core/src/serial.rs`
- Modify: `core/src/syscall.rs`
- Test: `core/src/serial.rs`, `core/src/syscall.rs`

**Interfaces:**
- Consumes: COM1 UART constants and existing syscall numbers `0x5059_0000`, `0x5059_0001`.
- Produces: `serial::write_line_com2(value: &str)`, `serial::try_read_byte_com2() -> Option<u8>`, `serial::write_byte_com2(byte: u8)`, `SyscallArgs`, `SYSCALL_SHELL_READ_BYTE`, `SYSCALL_SHELL_WRITE_BYTE`.

- [ ] **Step 1: Write UART unit tests**

Add test-only pure helpers in `core/src/serial.rs`:

```rust
const COM1_BASE: u16 = 0x3F8;
const COM2_BASE: u16 = 0x2F8;

const fn line_status_port(base: u16) -> u16 {
    base + 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn com_ports_have_stable_legacy_bases() {
        assert_eq!(COM1_BASE, 0x3F8);
        assert_eq!(COM2_BASE, 0x2F8);
        assert_eq!(line_status_port(COM1_BASE), 0x3FD);
        assert_eq!(line_status_port(COM2_BASE), 0x2FD);
    }
}
```

- [ ] **Step 2: Run UART tests to verify failure**

Run:

```powershell
cargo test -p pythos-core serial::tests::com_ports_have_stable_legacy_bases
```

Expected: FAIL until constants/helpers are added.

- [ ] **Step 3: Implement COM2 UART access**

Replace the single COM1-only helpers with a small base-port helper:

```rust
const COM1_BASE: u16 = 0x3F8;
const COM2_BASE: u16 = 0x2F8;
const LINE_STATUS_OFFSET: u16 = 5;
const RECEIVE_READY: u8 = 0x01;
const TRANSMIT_EMPTY: u8 = 0x20;

const fn line_status_port(base: u16) -> u16 {
    base + LINE_STATUS_OFFSET
}

pub fn write_line(line: &str) {
    write_str_to(COM1_BASE, line);
    write_str_to(COM1_BASE, "\r\n");
}

pub fn write_line_com2(line: &str) {
    write_str_to(COM2_BASE, line);
    write_str_to(COM2_BASE, "\r\n");
}

pub fn write_byte_com2(byte: u8) {
    write_byte_to(COM2_BASE, byte);
}

pub fn try_read_byte_com2() -> Option<u8> {
    if (inb(line_status_port(COM2_BASE)) & RECEIVE_READY) == 0 {
        return None;
    }
    Some(inb(COM2_BASE))
}
```

Keep the existing `write_line` and `write_hex_u64` public behavior unchanged.

- [ ] **Step 4: Write syscall ABI argument tests**

Add these constants and tests in `core/src/syscall.rs`:

```rust
pub const SYSCALL_SHELL_READ_BYTE: u64 = 0x5059_0100;
pub const SYSCALL_SHELL_WRITE_BYTE: u64 = 0x5059_0101;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallArgs {
    pub number: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
    pub arg4: u64,
}

#[test]
fn shell_syscalls_are_sorted_after_existing_phase9_numbers() {
    assert!(SYSCALL_SYSTEM_LOG_PROOF < SYSCALL_SHELL_READ_BYTE);
    assert!(SYSCALL_SHELL_READ_BYTE < SYSCALL_SHELL_WRITE_BYTE);
    assert_eq!(validate_syscall_table(SYSCALL_TABLE), Ok(()));
}
```

- [ ] **Step 5: Run syscall tests to verify failure**

Run:

```powershell
cargo test -p pythos-core syscall::tests::shell_syscalls_are_sorted_after_existing_phase9_numbers
```

Expected: FAIL until syscall table entries exist.

- [ ] **Step 6: Implement register argument dispatch**

Change the non-test assembly so `syscall_entry_abi` passes number and five args:

```asm
mov r9, r8
mov r8, r10
mov rcx, rdx
mov rdx, rsi
mov rsi, rdi
mov rdi, rax
call syscall_dispatch_abi
```

Change the exported dispatcher:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch_abi(
    number: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> u64 {
    let result = dispatch(SyscallArgs { number, arg0, arg1, arg2, arg3, arg4 });
    let code = syscall_result_code(result);
    SYSCALL_LAST_RESULT.store(code, Ordering::SeqCst);
    SYSCALL_RETURNED.store(true, Ordering::SeqCst);
    code
}
```

Keep compatibility by making existing test callers pass zero args.

- [ ] **Step 7: Implement shell serial syscalls**

Add dispatch kinds:

```rust
ShellReadByte,
ShellWriteByte,
```

Define return values:

```rust
const SYSCALL_SHELL_NO_BYTE: u64 = 0;
const SYSCALL_SHELL_BYTE_READY: u64 = 1 << 8;
```

For read:

```rust
fn dispatch_shell_read_byte() -> u64 {
    match serial::try_read_byte_com2() {
        Some(byte) => SYSCALL_SHELL_BYTE_READY | u64::from(byte),
        None => SYSCALL_SHELL_NO_BYTE,
    }
}
```

For write:

```rust
fn dispatch_shell_write_byte(args: SyscallArgs) -> Result<u64, SyscallError> {
    let byte = (args.arg0 & 0xFF) as u8;
    serial::write_byte_com2(byte);
    Ok(SYSCALL_OK)
}
```

- [ ] **Step 8: Run focused tests**

Run:

```powershell
cargo test -p pythos-core serial syscall
```

Expected: PASS.

- [ ] **Step 9: Run verify boot smoke**

Run:

```powershell
python scripts\test-boot.py
```

Expected: `BOOT_TEST_OK`.

- [ ] **Step 10: Commit**

```powershell
git add core\src\serial.rs core\src\syscall.rs
git commit -m "feat(shell): add COM2 UART and shell serial syscalls"
```

---

### Task 3: Object Shell Protocol And Note Object Kind

**Files:**
- Create: `core/src/object_shell_protocol.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/shell_objects.rs`
- Modify: `core/src/typed_object_format.rs`
- Test: `core/src/object_shell_protocol.rs`, `core/src/typed_object_format.rs`

**Interfaces:**
- Consumes: `ObjectId`, `ObjectKind`, and `TypedObjectRecord`.
- Produces: `ShellCommand`, `ShellResponse`, `parse_command(line: &[u8]) -> Result<ShellCommand, ShellProtocolError>`, `format_response(response: ShellResponse, out: &mut ResponseBuffer)`.

- [ ] **Step 1: Add failing note-kind tests**

In `core/src/typed_object_format.rs`:

```rust
#[test]
fn note_kind_round_trips_with_stable_code() {
    let record = TypedObjectRecord::new(ObjectId::new(1042), ObjectKind::Note, 1);
    let decoded = TypedObjectRecord::decode(&record.encode()).unwrap();

    assert_eq!(decoded.object_id().raw(), 1042);
    assert_eq!(decoded.object_kind(), ObjectKind::Note);
}
```

- [ ] **Step 2: Run note-kind test to verify failure**

Run:

```powershell
cargo test -p pythos-core typed_object_format::tests::note_kind_round_trips_with_stable_code
```

Expected: FAIL because `ObjectKind::Note` is absent.

- [ ] **Step 3: Add `ObjectKind::Note`**

Update `core/src/shell_objects.rs`:

```rust
pub enum ObjectKind {
    ApplicationLauncherWindow,
    BootIdentitySurface,
    ServiceMonitorWindow,
    PythonConsoleWindow,
    SettingsPanelWindow,
    WorkspaceSession,
    ObjectBrowserWindow,
    ButtonWidget,
    TextFieldWidget,
    Note,
}
```

Update `core/src/typed_object_format.rs`:

```rust
ObjectKind::Note => 10,
```

and:

```rust
10 => Ok(ObjectKind::Note),
```

- [ ] **Step 4: Add failing protocol parser tests**

Create `core/src/object_shell_protocol.rs` with test module first:

```rust
#![cfg_attr(test, allow(dead_code))]

use crate::shell_objects::ObjectId;

pub const MAX_COMMAND_BYTES: usize = 96;
pub const MAX_RESPONSE_BYTES: usize = 160;
pub const NOTE_TEXT_FIELD_ID: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellCommand {
    Help,
    QueryNote,
    CreateNote,
    Inspect { object_id: ObjectId },
    ReviseText { object_id: ObjectId, text: ShellText },
    History { object_id: ObjectId },
    Reboot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellText {
    bytes: [u8; 16],
    len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellProtocolError {
    Empty,
    TooLong,
    InvalidUtf8,
    UnknownCommand,
    InvalidObjectId,
    TextTooLong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adr0051_command_surface() {
        assert_eq!(parse_command(b"help"), Ok(ShellCommand::Help));
        assert_eq!(parse_command(b"query kind:note"), Ok(ShellCommand::QueryNote));
        assert_eq!(parse_command(b"create kind:note"), Ok(ShellCommand::CreateNote));
        assert_eq!(
            parse_command(b"inspect object:1042"),
            Ok(ShellCommand::Inspect { object_id: ObjectId::new(1042) })
        );
        assert_eq!(
            parse_command(br#"revise object:1042 text="hello""#),
            Ok(ShellCommand::ReviseText {
                object_id: ObjectId::new(1042),
                text: ShellText::new(b"hello").unwrap(),
            })
        );
        assert_eq!(
            parse_command(b"history object:1042"),
            Ok(ShellCommand::History { object_id: ObjectId::new(1042) })
        );
        assert_eq!(parse_command(b"reboot"), Ok(ShellCommand::Reboot));
    }

    #[test]
    fn rejects_unknown_commands_and_bad_ids() {
        assert_eq!(parse_command(b""), Err(ShellProtocolError::Empty));
        assert_eq!(parse_command(b"ls /"), Err(ShellProtocolError::UnknownCommand));
        assert_eq!(
            parse_command(b"inspect object:notanumber"),
            Err(ShellProtocolError::InvalidObjectId)
        );
    }
}
```

- [ ] **Step 5: Run protocol tests to verify failure**

Run:

```powershell
cargo test -p pythos-core object_shell_protocol
```

Expected: FAIL until parser implementation exists and the module is registered.

- [ ] **Step 6: Implement parser and response formatter**

Add:

```rust
pub fn parse_command(line: &[u8]) -> Result<ShellCommand, ShellProtocolError> {
    if line.is_empty() {
        return Err(ShellProtocolError::Empty);
    }
    if line.len() > MAX_COMMAND_BYTES {
        return Err(ShellProtocolError::TooLong);
    }
    let text = core::str::from_utf8(line).map_err(|_| ShellProtocolError::InvalidUtf8)?;
    match text {
        "help" => Ok(ShellCommand::Help),
        "query kind:note" => Ok(ShellCommand::QueryNote),
        "create kind:note" => Ok(ShellCommand::CreateNote),
        "reboot" => Ok(ShellCommand::Reboot),
        _ if text.starts_with("inspect object:") => parse_object_tail(text, "inspect object:")
            .map(|object_id| ShellCommand::Inspect { object_id }),
        _ if text.starts_with("history object:") => parse_object_tail(text, "history object:")
            .map(|object_id| ShellCommand::History { object_id }),
        _ if text.starts_with("revise object:") => parse_revise(text),
        _ => Err(ShellProtocolError::UnknownCommand),
    }
}
```

Define `ResponseBuffer`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseBuffer {
    bytes: [u8; MAX_RESPONSE_BYTES],
    len: usize,
}
```

Implement `push_str`, `push_u64_decimal`, `as_bytes`, and response formatting for:

```rust
ShellResponse::Ready
ShellResponse::Created { object_id, revision }
ShellResponse::Committed { revision }
ShellResponse::DeniedMissingCapability
ShellResponse::InspectNote { text, revision }
ShellResponse::History { object_id, revision_count }
ShellResponse::Help
ShellResponse::Rebooting
ShellResponse::Error
```

- [ ] **Step 7: Register module and run tests**

In `core/src/main.rs` add:

```rust
mod object_shell_protocol;
```

Run:

```powershell
cargo test -p pythos-core typed_object_format object_shell_protocol
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add core\src\main.rs core\src\shell_objects.rs core\src\typed_object_format.rs core\src\object_shell_protocol.rs
git commit -m "feat(shell): define object shell protocol and note objects"
```

---

### Task 4: Capability-Gated Object Shell Store And Reboot Authority

**Files:**
- Create: `core/src/object_shell_store.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/object_shell_store.rs`

**Interfaces:**
- Consumes: `ShellCommand`, `ShellResponse`, `CapabilityTable`, `ResourceId`, `RightsMask`, `ServiceIdentityTable`, `RevisionHistory`, `TypedObjectRecord`, `BlockDeviceInfo`.
- Produces: `ObjectShellStore::restore_or_initialize(device: BlockDeviceInfo) -> Result<Self, ObjectShellError>`, `ObjectShellStore::execute(&mut self, command: ShellCommand) -> ShellResponse`, `ObjectShellStore::persist(device: BlockDeviceInfo) -> Result<(), ObjectShellError>`.

- [ ] **Step 1: Write authority and store tests**

Create `core/src/object_shell_store.rs` with tests first:

```rust
#![cfg_attr(test, allow(dead_code))]

use crate::{
    object_shell_protocol::{ShellCommand, ShellResponse, ShellText},
    shell_objects::ObjectId,
};

pub const SHELL_PRINCIPAL_ID: u64 = 0x5059_5348_454C_4C01;
pub const SHELL_NOTE_OBJECT_ID: ObjectId = ObjectId::new(1042);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_bootstrap_has_workspace_root_but_not_global_object_access() {
        let mut store = ObjectShellStore::new_for_test();

        assert!(store.shell_has_workspace_root());
        assert_eq!(
            store.execute(ShellCommand::Inspect { object_id: ObjectId::new(9999) }),
            ShellResponse::DeniedMissingCapability
        );
    }

    #[test]
    fn create_revise_inspect_history_are_capability_gated_transactions() {
        let mut store = ObjectShellStore::new_for_test();

        assert_eq!(
            store.execute(ShellCommand::CreateNote),
            ShellResponse::Created {
                object_id: SHELL_NOTE_OBJECT_ID,
                revision: 1,
            }
        );
        assert_eq!(
            store.execute(ShellCommand::ReviseText {
                object_id: SHELL_NOTE_OBJECT_ID,
                text: ShellText::new(b"hello").unwrap(),
            }),
            ShellResponse::Committed { revision: 2 }
        );
        assert_eq!(
            store.execute(ShellCommand::Inspect {
                object_id: SHELL_NOTE_OBJECT_ID,
            }),
            ShellResponse::InspectNote {
                text: ShellText::new(b"hello").unwrap(),
                revision: 2,
            }
        );
        assert_eq!(
            store.execute(ShellCommand::History {
                object_id: SHELL_NOTE_OBJECT_ID,
            }),
            ShellResponse::History {
                object_id: SHELL_NOTE_OBJECT_ID,
                revision_count: 2,
            }
        );
    }

    #[test]
    fn reboot_reconstructs_fresh_handles_from_stable_principal() {
        let mut store = ObjectShellStore::new_for_test();
        let first_handle = store.workspace_handle_generation_for_test();
        store.execute(ShellCommand::CreateNote);
        let snapshot = store.encode_snapshot_for_test().unwrap();

        let mut restored = ObjectShellStore::decode_snapshot_for_test(snapshot).unwrap();

        assert_ne!(first_handle, restored.workspace_handle_generation_for_test());
        assert_eq!(
            restored.execute(ShellCommand::Inspect {
                object_id: SHELL_NOTE_OBJECT_ID,
            }),
            ShellResponse::InspectNote {
                text: ShellText::new(b"").unwrap(),
                revision: 1,
            }
        );
    }
}
```

- [ ] **Step 2: Run store tests to verify failure**

Run:

```powershell
cargo test -p pythos-core object_shell_store
```

Expected: FAIL until the store module exists and is registered.

- [ ] **Step 3: Implement principal, workspace authority, and runtime rebinding**

Define:

```rust
pub const WORKSPACE_ROOT_RESOURCE: ResourceId = ResourceId::new(0x5059_5753_524F_4F54);
pub const NOTE_TEXT_FIELD_ID: u16 = object_shell_protocol::NOTE_TEXT_FIELD_ID;
const SHELL_TASK_ID: TaskId = TaskId::new(180);
const NOTE_READ_REVISE_RIGHTS: RightsMask =
    RightsMask::new(RightsMask::READ | RightsMask::WRITE);

pub struct ObjectShellStore {
    shell_service: ServiceId,
    capabilities: CapabilityTable,
    workspace_handle: CapabilityHandle,
    history: RevisionHistory,
    note_reachable: bool,
    note_text: ShellText,
}
```

`new_for_test()` and restore paths must:

```rust
let shell_service = identities.register_task(SHELL_TASK_ID)?;
let workspace_handle = capabilities.grant(
    shell_service,
    WORKSPACE_ROOT_RESOURCE,
    NOTE_READ_REVISE_RIGHTS,
)?;
```

Do not persist `CapabilityHandle`. Persist only principal id, object id, text,
revision count, and checksum.

- [ ] **Step 4: Implement object operations**

`execute()` must map commands:

```rust
ShellCommand::CreateNote => create note 1042 revision 1
ShellCommand::ReviseText { object_id: SHELL_NOTE_OBJECT_ID, text } => commit revision 2
ShellCommand::Inspect { object_id: SHELL_NOTE_OBJECT_ID } => return current text and revision
ShellCommand::History { object_id: SHELL_NOTE_OBJECT_ID } => return revision count
ShellCommand::Inspect { object_id: _ } => DeniedMissingCapability
ShellCommand::History { object_id: _ } => DeniedMissingCapability
ShellCommand::QueryNote => list only reachable note objects
ShellCommand::Help => help response
ShellCommand::Reboot => rebooting response
```

Each operation must validate `workspace_handle` before reading or mutating note state:

```rust
self.capabilities.validate(
    self.shell_service,
    self.workspace_handle,
    WORKSPACE_ROOT_RESOURCE,
    RightsMask::new(RightsMask::READ),
)?;
```

Use `RightsMask::WRITE` for `create` and `revise`.

- [ ] **Step 5: Implement snapshot encoding**

Use a dedicated ADR 0051 sector range:

```rust
const OBJECT_SHELL_SECTOR: u64 = 60;
const SNAPSHOT_MAGIC: [u8; 8] = *b"PY51SH01";
const SNAPSHOT_VERSION: u16 = 1;
const COMMIT_MARKER: u32 = 0x5059_5131;
```

Encode:

```text
0..8    magic
8..10   version
12..16  commit marker
16..20  checksum
24..32  shell principal id
32..40  object id
40..48  current revision
48..56  revision count
56..58  text length
64..80  text bytes
```

Decode rejects bad magic, unsupported version, missing commit marker, bad checksum, wrong principal, and text length greater than 16.

- [ ] **Step 6: Register module and run tests**

In `core/src/main.rs` add:

```rust
mod object_shell_store;
```

Run:

```powershell
cargo test -p pythos-core object_shell_store
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add core\src\main.rs core\src\object_shell_store.rs
git commit -m "feat(shell): add capability-gated object shell store"
```

---

### Task 5: Object Shell Bridge Syscalls

**Files:**
- Create: `core/src/object_shell_bridge.rs`
- Modify: `core/src/main.rs`
- Modify: `core/src/syscall.rs`
- Test: `core/src/object_shell_bridge.rs`, `core/src/syscall.rs`

**Interfaces:**
- Consumes: `ObjectShellStore`, `parse_command`, `format_response`, COM2 byte syscalls.
- Produces: `object_shell_bridge::initialize(device: BlockDeviceInfo)`, `push_command_byte(byte: u8) -> ShellBridgeStatus`, `execute_command() -> ShellBridgeStatus`, `pop_response_byte() -> Option<u8>`, syscall numbers `0x5059_0110..0x5059_0113`.

- [ ] **Step 1: Write bridge queue tests**

Create `core/src/object_shell_bridge.rs` with:

```rust
#![cfg_attr(test, allow(dead_code))]

pub const SYSCALL_OBJECT_SHELL_BEGIN: u64 = 0x5059_0110;
pub const SYSCALL_OBJECT_SHELL_PUSH_BYTE: u64 = 0x5059_0111;
pub const SYSCALL_OBJECT_SHELL_EXECUTE: u64 = 0x5059_0112;
pub const SYSCALL_OBJECT_SHELL_POP_BYTE: u64 = 0x5059_0113;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellBridgeStatus {
    Ok,
    Empty,
    Full,
    BadCommand,
    NoResponseByte,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_executes_command_bytes_and_returns_response_bytes() {
        let mut bridge = ObjectShellBridge::new_for_test();
        assert_eq!(bridge.begin_command(), ShellBridgeStatus::Ok);
        for byte in b"create kind:note" {
            assert_eq!(bridge.push_command_byte(*byte), ShellBridgeStatus::Ok);
        }
        assert_eq!(bridge.execute_command(), ShellBridgeStatus::Ok);

        let response = bridge.drain_response_for_test();
        assert_eq!(response, b"CREATED object:1042 revision:1\r\npyth> ");
    }

    #[test]
    fn bad_command_returns_typed_error_response() {
        let mut bridge = ObjectShellBridge::new_for_test();
        bridge.begin_command();
        for byte in b"ls /" {
            bridge.push_command_byte(*byte);
        }
        assert_eq!(bridge.execute_command(), ShellBridgeStatus::BadCommand);
        assert_eq!(bridge.drain_response_for_test(), b"ERROR unknown-command\r\npyth> ");
    }
}
```

- [ ] **Step 2: Run bridge tests to verify failure**

Run:

```powershell
cargo test -p pythos-core object_shell_bridge
```

Expected: FAIL until the bridge implementation exists.

- [ ] **Step 3: Implement bounded queues**

Define:

```rust
const COMMAND_CAPACITY: usize = 96;
const RESPONSE_CAPACITY: usize = 192;

pub struct ObjectShellBridge {
    store: ObjectShellStore,
    command: [u8; COMMAND_CAPACITY],
    command_len: usize,
    response: [u8; RESPONSE_CAPACITY],
    response_len: usize,
    response_cursor: usize,
}
```

`execute_command()` must parse command bytes, call `store.execute()`, format the response, append `\r\npyth> `, persist store state after successful `CreateNote` and `ReviseText`, and emit COM1 markers:

```text
PYTHOS:SHELL:COMMAND
PYTHOS:SHELL:OBJECT_CREATED
PYTHOS:SHELL:OBJECT_REVISED
PYTHOS:SHELL:ACCESS_DENIED
PYTHOS:SHELL:REBOOT_REQUESTED
```

- [ ] **Step 4: Add syscall dispatch entries**

In `core/src/syscall.rs`, add shell bridge dispatch kinds:

```rust
ObjectShellBegin,
ObjectShellPushByte,
ObjectShellExecute,
ObjectShellPopByte,
```

Add sorted table entries for `0x5059_0110..0x5059_0113`. Return low byte for `PopByte` with the same `SYSCALL_SHELL_BYTE_READY` convention used by COM2 read.

- [ ] **Step 5: Add a single-core global bridge**

In `core/src/object_shell_bridge.rs`, expose:

```rust
#[cfg(not(test))]
pub fn initialize(device: BlockDeviceInfo) -> Result<(), ObjectShellError>;

#[cfg(not(test))]
pub fn syscall_begin_command() -> ShellBridgeStatus;
#[cfg(not(test))]
pub fn syscall_push_command_byte(byte: u8) -> ShellBridgeStatus;
#[cfg(not(test))]
pub fn syscall_execute_command() -> ShellBridgeStatus;
#[cfg(not(test))]
pub fn syscall_pop_response_byte() -> Option<u8>;
```

If a `static mut` bridge is used, the unsafe block must state this invariant:

```rust
// SAFETY:
// 1. Invariant: ADR 0051 runs on the current single-core boot path and shell
//    bridge syscalls are not re-entered while a bridge operation is active.
// 2. Established by: QEMU target `-smp 1`, interrupts do not dispatch nested
//    shell syscalls, and COM2 shell execution is one user process.
// 3. Lifetime: the bridge is initialized before `shell.elf` is launched and
//    remains valid for the entire normal boot.
// 4. Pointer ownership: no borrowed references escape the critical section.
// 5. Alignment: static storage is naturally aligned for `ObjectShellBridge`.
// 6. Mapped length: exactly one `ObjectShellBridge` object is accessed.
// 7. Concurrency: SMP is out of scope for ADR 0051.
// 8. Violation: nested access would corrupt the command/response queues.
```

- [ ] **Step 6: Register module and run tests**

In `core/src/main.rs` add:

```rust
mod object_shell_bridge;
```

Run:

```powershell
cargo test -p pythos-core object_shell_bridge syscall
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add core\src\main.rs core\src\syscall.rs core\src\object_shell_bridge.rs
git commit -m "feat(shell): add object shell transaction syscalls"
```

---

### Task 6: Build And Package `shell.elf`

**Files:**
- Modify: `Cargo.toml`
- Create: `user/shell/Cargo.toml`
- Create: `user/shell/src/main.rs`
- Create: `user/shell/linker.ld`
- Create: `scripts/build-user-shell.py`
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`
- Test: `user/shell/src/main.rs`, `scripts/build-image.py`

**Interfaces:**
- Consumes: shell serial syscalls and object-shell bridge syscalls from Task 2 and Task 5.
- Produces: `target/x86_64-unknown-none/debug/pythos-user-shell`, embedded as a named user ELF payload in `INIT.PAK`.

- [ ] **Step 1: Add user shell workspace package**

Update root `Cargo.toml`:

```toml
members = [
    "boot",
    "core",
    "shared",
    "user/shell",
]
```

Create `user/shell/Cargo.toml`:

```toml
[package]
name = "pythos-user-shell"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish.workspace = true
rust-version.workspace = true

[[bin]]
name = "pythos-user-shell"
path = "src/main.rs"
```

- [ ] **Step 2: Add linker script**

Create `user/shell/linker.ld`:

```ld
ENTRY(_start)

SECTIONS
{
  . = 0x0000000000400000;
  .text : ALIGN(4096) { *(.text .text.*) }
  .rodata : ALIGN(4096) { *(.rodata .rodata.*) }
  .data : ALIGN(4096) { *(.data .data.*) }
  .bss : ALIGN(4096) { *(.bss .bss.* COMMON) }
}
```

- [ ] **Step 3: Write shell source**

Create `user/shell/src/main.rs`:

```rust
#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

const SYSCALL_SHELL_READ_BYTE: u64 = 0x5059_0100;
const SYSCALL_SHELL_WRITE_BYTE: u64 = 0x5059_0101;
const SYSCALL_OBJECT_SHELL_BEGIN: u64 = 0x5059_0110;
const SYSCALL_OBJECT_SHELL_PUSH_BYTE: u64 = 0x5059_0111;
const SYSCALL_OBJECT_SHELL_EXECUTE: u64 = 0x5059_0112;
const SYSCALL_OBJECT_SHELL_POP_BYTE: u64 = 0x5059_0113;
const BYTE_READY: u64 = 1 << 8;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    write_str("PYTHOS:SHELL:READY\r\npyth> ");
    begin_command();
    loop {
        let read = syscall1(SYSCALL_SHELL_READ_BYTE, 0);
        if (read & BYTE_READY) == 0 {
            spin();
            continue;
        }
        let byte = (read & 0xFF) as u8;
        if byte == b'\r' || byte == b'\n' {
            write_byte(b'\r');
            write_byte(b'\n');
            execute_command();
            drain_response();
            begin_command();
        } else {
            write_byte(byte);
            push_command_byte(byte);
        }
    }
}

fn begin_command() {
    syscall1(SYSCALL_OBJECT_SHELL_BEGIN, 0);
}

fn push_command_byte(byte: u8) {
    syscall1(SYSCALL_OBJECT_SHELL_PUSH_BYTE, u64::from(byte));
}

fn execute_command() {
    syscall1(SYSCALL_OBJECT_SHELL_EXECUTE, 0);
}

fn drain_response() {
    loop {
        let value = syscall1(SYSCALL_OBJECT_SHELL_POP_BYTE, 0);
        if (value & BYTE_READY) == 0 {
            return;
        }
        write_byte((value & 0xFF) as u8);
    }
}

fn write_str(value: &str) {
    for byte in value.bytes() {
        write_byte(byte);
    }
}

fn write_byte(byte: u8) {
    syscall1(SYSCALL_SHELL_WRITE_BYTE, u64::from(byte));
}

fn syscall1(number: u64, arg0: u64) -> u64 {
    let result: u64;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") arg0,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    result
}

fn spin() {
    core::hint::spin_loop();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        spin();
    }
}
```

If this crate introduces any unsafe block beyond `syscall1`, document the invariant before the block.

- [ ] **Step 4: Add shell build script with ET_EXEC flags**

Create `scripts/build-user-shell.py` so shell linker flags do not leak into the kernel package:

```python
#!/usr/bin/env python
"""Build the ADR 0051 ring-3 shell ELF."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHELL_LINKER = ROOT / "user" / "shell" / "linker.ld"


def main() -> int:
    env = os.environ.copy()
    env["RUSTFLAGS"] = " ".join(
        [
            "-C", "relocation-model=static",
            "-C", f"link-arg=-T{SHELL_LINKER}",
            "-C", "link-arg=--no-pie",
        ]
    )
    return subprocess.call(
        [
            "cargo",
            "build",
            "-p",
            "pythos-user-shell",
            "--target",
            "x86_64-unknown-none",
        ],
        cwd=ROOT,
        env=env,
    )


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 5: Build shell ELF**

Run:

```powershell
python scripts\build-user-shell.py
```

Expected: PASS and produce an ELF64 `ET_EXEC` binary.

- [ ] **Step 6: Embed shell ELF in INIT.PAK**

In `scripts/build-image.py` and `scripts/build-iso.py`, read the shell binary:

```python
SHELL_ELF = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
```

Add it to the ADR 0037 inner bundle with a stable name:

```python
(INIT_BUNDLE_USER_ELF_TYPE, build_named_user_elf_payload(b"shell.elf", SHELL_ELF.read_bytes()))
```

`build_named_user_elf_payload(name: bytes, elf: bytes)` must encode:

```text
u16 name_len
name bytes
u32 elf_len
elf bytes
```

The existing unnamed user ELF payloads must remain unchanged for verify tests.

- [ ] **Step 7: Run build and verify packaging**

Run:

```powershell
python scripts\build-user-shell.py
python scripts\build-image.py
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add Cargo.toml user\shell scripts\build-user-shell.py scripts\build-image.py scripts\build-iso.py
git commit -m "feat(shell): build and package ring-3 shell ELF"
```

---

### Task 7: Normal Boot Launches `shell.elf`

**Files:**
- Modify: `core/src/runtime_loader.rs`
- Modify: `core/src/user_elf.rs`
- Modify: `core/src/user_mode.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/runtime_loader.rs`, `core/src/user_mode.rs`, `scripts/test-object-shell.py`

**Interfaces:**
- Consumes: named `shell.elf` payload from Task 6, object shell bridge initialization from Task 5, existing dynamic ELF mapping helpers.
- Produces: `runtime_loader::load_named_user_elf_payload(boot_info: &PythBootInfo, name: &[u8]) -> Result<&[u8], RuntimeLoadError>`, `runtime_loader::validate_named_user_elf_payload_bytes(bytes: &[u8], name: &[u8]) -> Result<&[u8], RuntimeLoadError>`, `user_mode::enter_persistent_user(entry: u64, user_stack_top: u64) -> !`, normal boot marker `PYTHOS:SHELL:RING3_ENTER`.

- [ ] **Step 1: Write named user ELF loader tests**

In `core/src/runtime_loader.rs`, add tests for the named payload shape from Task 6:

```rust
#[test]
fn named_user_elf_payload_finds_shell_by_name() {
    let bundle = test_bundle_with_named_user_elf(b"shell.elf", b"\x7FELFpayload");
    let shell = validate_named_user_elf_payload_bytes(&bundle, b"shell.elf").unwrap();

    assert_eq!(shell, b"\x7FELFpayload");
}

#[test]
fn named_user_elf_lookup_rejects_missing_name() {
    let bundle = test_bundle_with_named_user_elf(b"other.elf", b"\x7FELFpayload");

    assert_eq!(
        validate_named_user_elf_payload_bytes(&bundle, b"shell.elf"),
        Err(RuntimeLoadError::MissingUserElfPayload)
    );
}
```

- [ ] **Step 2: Run named loader tests to verify failure**

Run:

```powershell
cargo test -p pythos-core runtime_loader::tests::named_user_elf
```

Expected: FAIL until named lookup exists.

- [ ] **Step 3: Implement named payload lookup**

Add:

```rust
pub fn load_named_user_elf_payload(
    boot_info: &PythBootInfo,
    name: &[u8],
) -> Result<&[u8], RuntimeLoadError>

pub fn validate_named_user_elf_payload_bytes(
    bytes: &[u8],
    name: &[u8],
) -> Result<&[u8], RuntimeLoadError>
```

`load_named_user_elf_payload()` should reuse the existing private `init_bundle_bytes(boot_info)` path, then call `validate_named_user_elf_payload_bytes()`. The validator must validate `name_len`, name bytes, `elf_len`, and range overflow before returning the ELF slice.

- [ ] **Step 4: Add persistent user entry**

In `core/src/user_mode.rs`, add:

```rust
#[cfg(not(test))]
pub fn enter_persistent_user(entry: u64, user_stack_top: u64) -> ! {
    tss::set_ring0_stack(kernel_trap_stack_top());
    serial::write_line("PYTHOS:SHELL:RING3_ENTER");
    unsafe {
        ring3_enter_forever_abi(entry, user_stack_top);
    }
}
```

Add a new assembly entry `ring3_enter_forever_abi` that performs the same selector setup as `ring3_enter_abi` but does not install a breakpoint recovery label. Syscalls must return to user mode; unexpected faults still flow through the existing diagnostics/crash containment path.

- [ ] **Step 5: Initialize bridge and launch shell in normal boot**

Change the normal-boot call site after `PYTHOS:CORE:MILESTONE_1_COMPLETE` so it passes the state required to launch the shell:

```rust
normal_event_loop(&mut physical_memory, boot_info, _block_device);
```

Change `normal_event_loop()` to accept those values:

```rust
fn normal_event_loop(
    physical_memory: &mut PhysicalMemory,
    boot_info: &PythBootInfo,
    block_device: BlockDeviceInfo,
) -> !
```

In `core/src/main.rs`, replace `normal_event_loop()` body with:

```rust
serial::write_line("PYTHOS:CORE:NORMAL_BOOT_ALIVE");
serial::write_line("PYTHOS:CORE:NORMAL_BOOT:FAST_PATH");
if object_shell_bridge::initialize(block_device).is_err() {
    serial::write_line("PYTHOS:PANIC");
    qemu_exit::panic();
}
let shell_payload = match runtime_loader::load_named_user_elf_payload(boot_info, b"shell.elf") {
    Ok(image) => image,
    Err(_) => {
        serial::write_line("PYTHOS:SHELL:ELF_MISSING");
        qemu_exit::panic();
    }
};
let (shell_address_space, loaded_shell, shell_image) =
    match build_user_elf_address_space_from_image(physical_memory, boot_info, shell_payload) {
        Ok(loaded) => loaded,
        Err(_) => {
            serial::write_line("PYTHOS:SHELL:ELF_INVALID");
            qemu_exit::panic();
        }
    };
if loaded_shell.entry() != shell_image.entry() {
    serial::write_line("PYTHOS:SHELL:ELF_INVALID");
    qemu_exit::panic();
}
// SAFETY:
// 1. Invariant: `shell_address_space` owns a valid user CR3 root built by
//    `build_user_elf_address_space_from_image()` from a validated user ELF.
// 2. Established by: `user_elf::validate()`, mapped segment checks, and
//    retained address-space frame ownership.
// 3. Lifetime: the retained address space is consumed by the persistent shell
//    launch path and is not reclaimed before entering user mode.
// 4. Concurrency: ADR 0051 normal boot is single-core.
unsafe {
    shell_address_space.activate();
}
user_mode::enter_persistent_user(shell_image.entry(), user_stacks::proof_stack_top());
```

Split `build_dynamic_elf_address_space` into a payload-specific helper and keep the ordinal-based helper as a wrapper for existing proofs:

```rust
fn build_user_elf_address_space_from_image(
    physical_memory: &mut PhysicalMemory,
    boot_info: &PythBootInfo,
    image: &[u8],
) -> Result<UserElfLaunch, UserElfLaunchError>
```

- [ ] **Step 6: Run focused unit tests**

Run:

```powershell
cargo test -p pythos-core runtime_loader user_mode
```

Expected: PASS.

- [ ] **Step 7: Run object-shell acceptance test**

Run:

```powershell
python scripts\test-object-shell.py
```

Expected: FAIL at this task only if object persistence or response formatting is not wired through the bridge yet; it must at least show `PYTHOS:SHELL:RING3_ENTER` on COM1 and `PYTHOS:SHELL:READY` on COM2 before the failure.

- [ ] **Step 8: Commit**

```powershell
git add core\src\runtime_loader.rs core\src\user_elf.rs core\src\user_mode.rs core\src\main.rs
git commit -m "feat(shell): launch shell ELF on normal boot"
```

---

### Task 8: End-To-End Object Shell Persistence

**Files:**
- Modify: `core/src/object_shell_bridge.rs`
- Modify: `core/src/object_shell_store.rs`
- Modify: `scripts/test-object-shell.py`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/HANDOVER.md`
- Test: `scripts/test-object-shell.py`, `scripts/test-boot.py`, `scripts/test-persistent-storage.py`

**Interfaces:**
- Consumes: all prior tasks.
- Produces: passing `OBJECT_SHELL_TEST_OK`, preserved `BOOT_TEST_OK`, preserved `PERSISTENT_STORAGE_TEST_OK`, and docs updated to record ADR 0051 completion.

- [ ] **Step 1: Tighten acceptance expectations**

Ensure `scripts/test-object-shell.py` asserts the exact command transcript:

```text
PYTHOS:SHELL:READY
pyth> create kind:note
CREATED object:1042 revision:1
pyth> revise object:1042 text="hello"
COMMITTED revision:2
pyth> inspect object:9999
DENIED missing-capability
pyth> reboot
REBOOTING
```

Second boot transcript:

```text
PYTHOS:SHELL:READY
pyth> inspect object:1042
text="hello" revision:2
pyth> history object:1042
history object:1042 revisions:2
```

- [ ] **Step 2: Run object shell test to verify failure**

Run:

```powershell
python scripts\test-object-shell.py
```

Expected: FAIL until response text, persistence, and reboot handling match exactly.

- [ ] **Step 3: Finish bridge response and persistence behavior**

In `ObjectShellBridge::execute_command()`:

```rust
let command = parse_command(&self.command[..self.command_len]);
let response = match command {
    Ok(command) => {
        let response = self.store.execute(command);
        if matches!(
            command,
            ShellCommand::CreateNote | ShellCommand::ReviseText { .. }
        ) {
            self.store.persist(self.device)?;
        }
        response
    }
    Err(ShellProtocolError::UnknownCommand) => ShellResponse::ErrorUnknownCommand,
    Err(_) => ShellResponse::Error,
};
self.queue_response(response);
self.queue_prompt();
```

For `ShellCommand::Reboot`, queue `REBOOTING\r\n`, emit `PYTHOS:SHELL:REBOOT_REQUESTED` on COM1, and leave QEMU restart to `scripts/test-object-shell.py`. Do not call `qemu_exit::success()` from normal boot.

- [ ] **Step 4: Finish restore markers**

When `ObjectShellStore::restore_or_initialize()` decodes an existing shell snapshot and rebuilds authority:

```rust
serial::write_line("PYTHOS:SHELL:IDENTITY_RESTORED");
serial::write_line("PYTHOS:SHELL:WORKSPACE_CAPABILITY_REBOUND");
serial::write_line("PYTHOS:SHELL:OBJECT_RESTORED");
```

When no shell snapshot exists:

```rust
serial::write_line("PYTHOS:SHELL:IDENTITY_BOOTSTRAPPED");
serial::write_line("PYTHOS:SHELL:WORKSPACE_CAPABILITY_GRANTED");
```

- [ ] **Step 5: Run the end-to-end shell test**

Run:

```powershell
python scripts\test-object-shell.py
```

Expected:

```text
OBJECT_SHELL_TEST_OK
```

- [ ] **Step 6: Run existing acceptance tests**

Run:

```powershell
python scripts\test-boot.py
python scripts\test-persistent-storage.py
```

Expected:

```text
BOOT_TEST_OK
PERSISTENT_STORAGE_TEST_OK
```

- [ ] **Step 7: Update docs**

In `docs/ROADMAP.md`, record ADR 0051 after the vertical-loop plan:

```text
The ADR 0051 `first-ring3-object-shell` slice launches a ring-3 shell over COM2,
creates and revises note object 1042 through capability-gated typed requests,
denies access to an ungranted object ID, restarts QEMU over the same storage
image, reconstructs authority from shell principal/workspace policy, and
restores the note and revision history.
```

In `docs/HANDOVER.md`, add:

```text
Next boundary: ADR 0051 object shell complete. Verification remains through
COM1; interactive shell evidence is captured on COM2 by
`scripts/test-object-shell.py`.
```

- [ ] **Step 8: Commit**

```powershell
git add core\src\object_shell_bridge.rs core\src\object_shell_store.rs scripts\test-object-shell.py docs\ROADMAP.md docs\HANDOVER.md
git commit -m "feat(shell): complete ADR 0051 object shell persistence loop"
```

---

## Self-Review

- ADR 0051 command surface is covered by Task 3 parser tests and Task 8 transcript assertions.
- Shell principal, bootstrap workspace authority, and runtime handle rebinding are covered by Task 4 and Task 8.
- Knowing `object:9999` without authority is denied in Task 4 and Task 8.
- COM1/COM2 split is covered by Task 1, Task 2, and Task 8.
- Ring-3 shell execution is covered by Task 6 and Task 7.
- Existing `verify` behavior is protected by Task 2 and Task 8.
- Universal-device support, networking, package management, `grant`, `launch`, user-space drivers, and agents are excluded by global constraints.
- The temporary object bridge is explicitly named in Task 4 and Task 5 and remains kernel-backed for this slice.
