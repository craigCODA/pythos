# ADR 0028: Phase 8 Syscall ABI

Status: Accepted

## Context

ADR 0026 proved CPL3 execution, and ADR 0027 proved that the fixed user proof
can run under a distinct CR3 root. The next Phase 8 boundary needs a real
kernel entry path for user-mode code. This is the first syscall ABI contract,
so syscall numbering, register use, and return behavior must be explicit rather
than implied by the implementation.

This slice still does not implement general user processes, user pointer
copy-in/copy-out, guarded user stacks, service-local Python runtimes, crash
containment, or hostile-code capability enforcement.

## Decision

PythCore uses the architectural x86-64 `syscall`/`sysret` path for the first
syscall gate. During the proof it configures:

```text
IA32_EFER.SCE = 1
IA32_STAR     = kernel CS 0x08, SYSRET user base 0x23
IA32_LSTAR    = syscall_entry_abi
IA32_FMASK    = IF | DF
```

The initial register ABI is intentionally narrow:

```text
RAX on entry  = syscall number
RAX on return = result code
```

The only syscall number defined by this slice is:

```text
0x5059_0001 = SYSCALL_SYSTEM_LOG_PROOF
```

No user pointers or user buffers are accepted. The syscall dispatcher runs a
fixed proof that exercises a capability-gated Phase 3 IPC send and the Phase 4
`system.log` host-call surface using the existing wake message:

```text
PythOS [HISS] We Are Woken
```

The syscall handler switches from the user stack to a fixed kernel syscall
stack before calling Rust code, preserves the `syscall` return state, returns
through `sysretq`, and then the user proof traps back through the already
verified CPL3 breakpoint path. The required serial markers are:

```text
PYTHOS:CORE:SYSCALL:MSRS_READY
PYTHOS:CORE:SYSCALL:ENTER
PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK
PYTHOS:CORE:SYSCALL:SYSTEM_LOG
PYTHOS:CORE:SYSCALL:RETURN
PYTHOS:CORE:SYSCALL_ENTRY_READY
```

## Consequences

This establishes the first stable user-to-kernel syscall gate and proves it can
run under the distinct user CR3 root from ADR 0027.

Future syscall numbers or register arguments require an ADR update or a new ADR
before implementation. User pointers remain rejected by omission in this ABI;
copy-in/copy-out rules belong to later Phase 8 slices.
