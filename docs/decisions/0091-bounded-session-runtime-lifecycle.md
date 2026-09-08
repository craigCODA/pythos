# ADR 0091: Bounded Session Runtime Lifecycle

Date: 2026-09-07

Status: Accepted in QEMU; normal-boot, physical, and Viewing integration pending

## Context

ADR 0089 assigns semantic input routing and cursor-feature state to Viewing.
ADR 0090 establishes a capability-gated mechanism for delivering normalized
input to one authorized ring-3 consumer. Neither decision establishes a
retained session process or decides how a session graph is invoked more than
once.

Phase 13.5 Slice 2 needs a narrow lifecycle proof above ADR 0090 without
turning an acceptance probe into the production Session Manager. The proof
must keep one separately named ring-3 runtime and stable session identity
alive across two independent Session Manager graph invocations, while keeping
invocation-local interpreter state fresh and denying ambient host authority.

## Decision

The opt-in `session-runtime-probe` profile admits one separately named
`session-runtime.elf`. PythCore creates one retained address space and one
stable `ServiceId` for that runtime per boot. The runtime principal, graph
principal, and session service identity are non-zero and distinct. PythCore
authenticates the runtime ELF and the single `session-manager.tig` package,
maps the bounded bootstrap/package/fixture/result payload, grants the required
console, session-command, and ADR 0090 input capabilities, and binds the input
stream exactly once.

The ring-3 runtime owns neutral session-lifetime state: the stable session
identity, count of consumed input events, count of graph invocations, the
previous graph exit status, and whether recovery was requested. For each of
the two fixture commands it creates fresh graph interpreter state, host-result
state, command/result state, and graph-exit state. It executes the unchanged
`session-manager.tig` in process and validates a clean graph return before the
next invocation. The retained runtime process is entered once; graph
reinvocation does not recreate the session identity or ring-3 address space.

The session-command host is a separate, capability-scoped host surface. It
accepts only session-command resource ID `0x5059_5345_5343_4D44` with exact
`READ | APPEND` rights, exactly one `CommandRead`, and the matching result
emission for the active invocation. It rejects unrelated host operations,
wrong imports, forged or zero capabilities, invalid ordering, and mismatched
results. The two accepted commands are immutable `CREATE_NOTE` test fixtures
and are not derived from physical input.

The lifecycle decision is shared in
`shared/src/session_runtime_lifecycle.rs`: a successful graph exit requests
`Reinvoke`; a failed, budget-exhausted, or unknown exit requests recovery.
Both the retained ring-3 runtime and PythCore return validation use this
policy. A failure requests recovery once and does not enter a reinvocation
fault loop.

Polling for the two exact ADR 0090 events, the bounded result page, and one
expected ring-3 `int3` return are acceptance-only mechanisms. The finite
empty-poll limit is not a production wait/wakeup contract, scheduler
integration, or timing guarantee. PythCore restores its root, clears the
active caller, and validates the terminal result before accepting the proof.

Returnable user entry also has a dedicated, transient fault context rather
than reusing the normal expected-breakpoint state. A CPL3 fault while that
context is active records the principal, vector, RIP, RSP, and CR2, disarms
the expected breakpoint, clears the active caller, restores the saved kernel
root, and returns a typed contained-fault result to the session probe. The
probe accepts that result only after root restoration, caller clearing, and
an exact principal match, then requests recovery instead of continuing the
normal readiness path. All returnable transient state is cleared on exit.

## Evidence

The original bounded lifecycle implementation evidence source is clean commit
`6109425047de86cf60e4367313811fc284c43bad`. The reviewed returnable-fault
containment is commit `6dc1b4cfa47391472a7540375bd80fe7d7f8dd0c`; its
reviewed fault harness and milestone-only CI gate are commit
`2441c6d442378609fb43a52c4e537c4cac1df740`.

The complete fast gate passed on Windows:

```text
cargo fmt --check
cargo test --workspace
cargo clippy -p pythos-user-session-runtime --target x86_64-unknown-none -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features session-runtime-probe -- -D warnings
py -3 -m py_compile scripts/qemu_probe_support.py scripts/build-session-runtime.py scripts/test-session-runtime-probe.py
py -3 -m unittest tests.test_build_orchestration tests.test_ci_workflow tests.test_session_input_bridge_boundary tests.test_session_runtime_boundary
py -3 scripts/test-session-input-bridge-probe.py --self-test
py -3 scripts/test-session-runtime-probe.py --self-test
```

The initial focused Python gate passed 54 tests. The Slice 1 and Slice 2 oracle
self-tests passed. The strict Clippy invocations completed without warnings.
After review added the adversarial milestone-only CI mutation test, the exact
CI suite passed 6 tests. After the contained-fault review commits, the Slice 2
oracle self-test passed 33 tests, the boundary suite passed 14 tests, the CI
suite again passed 6 tests, and fresh full Python discovery passed 155 tests.

`py -3 scripts/test-session-runtime-probe.py --fault-test` passed one fresh
boot on local QEMU 11.0.50. The synthetic `session-runtime.elf` entered at
`0x0000000000400000` and executed `UD2`. COM1 recorded exactly:

```text
PYTHOS:CORE:SESSION_RUNTIME:FAULT_CONTAINED principal:50595352544D0001 vector:6 rip:0000000000400000 rsp:FFFFFFFF80084FF0 cr2:0000000000000000
PYTHOS:CORE:SESSION_RUNTIME:RECOVERY_REQUESTED
```

The RSP above is the value recorded by this live boot, not a fixed-address
contract. The fault path emitted no normal `USER_MODE:RETURN`,
`RING3_RETURN`, or `PYTHOS:CORE:SESSION_RUNTIME:READY` marker and no COM2
session-runtime output. It
produced exactly one `QEMU_OUTCOME success`, reaped the complete runner tree,
and ended with `SESSION_RUNTIME_FAULT_PROBE_OK`. Its disposable storage image
remained 16,777,216 bytes with SHA-256
`080ACF35A507AC9849CFCBA47DC2AD83E01B75663A516279C8B9D243B719643E`
before and after the boot.

`py -3 scripts/test-session-runtime-probe.py` then passed two fresh boots on
local QEMU 11.0.50. Each boot emitted exactly one `QEMU_OUTCOME success`, each
runner tree was reaped, and after both boots the harness emitted
`SESSION_RUNTIME_PROBE_OK` once. Both COM2 transcripts began with
`PYTHOS:SESSION_RUNTIME:BOOT_STATE_0`, proving that session state was new for
the second boot rather than durable across reboot. Within each boot, neutral
session state advanced exactly `0 -> 1 -> 2`, and the transcript proved two
contiguous normalized events, two independent
`CREATE_NOTE` commands, two successful fresh interpreter invocations, one
stable session identity, invocation-local reset, and retained neutral state.

The disposable storage fixture remained 16,777,216 bytes with SHA-256
`080ACF35A507AC9849CFCBA47DC2AD83E01B75663A516279C8B9D243B719643E`
before boot 1, after boot 1, and after boot 2. The two local COM1 logs were
byte-identical at 1,573 bytes with SHA-256
`4863AFE1A482286A54CACA7C6072DBFE5668E7D11758B5A0E11FD9DFE464F4F1`.
The two local COM2 logs were byte-identical at 1,102 bytes with SHA-256
`0D185C2E936796449852BE48CB5B0EC5C104F0C6FD88CF31201EC5ACE507C66A`.

An independent Windows runner on JacesPC verified the exact `2441c6d` Git
bundle at SHA-256
`6A8604E5EE66CF66165E11F26F9CB7CC899CFF97C9500612C6404AF72B022FFC`
and reproduced the proof with QEMU 11.1.0. The native-fault boot reached the
same exact principal, vector 6, RIP, CR2, and recovery outcome, produced one
success outcome, reaped its process tree, and retained the same storage size
and hash. The unchanged two-boot oracle then reached the exact per-channel
terminal markers with one `QEMU_OUTCOME success` per boot; both process trees
were reaped, both COM2 transcripts began at boot state zero, and the storage
hash was unchanged at all three checkpoints. The harness emitted
`SESSION_RUNTIME_PROBE_OK` once after the completed two-boot acceptance.

The predecessor/regression gate also passed:

```text
py -3 scripts/test-session-input-bridge-probe.py
  SESSION_INPUT_BRIDGE_PROBE_OK
  QEMU_OUTCOME success
py -3 scripts/test-normal-fast-boot.py
  NORMAL_FAST_BOOT_TEST_OK
py -3 scripts/test-persistent-storage.py
  PERSISTENT_STORAGE_TEST_OK
  QEMU_OUTCOME success
```

GitHub Actions remains pinned to QEMU 11.1.1 and OVMF
`2024.02-2ubuntu0.9`. That hosted version is a CI contract, not local or remote
runtime evidence until the hosted workflow passes the documentation commit.

## Consequences

Phase 13.5 Slice 2 accepts a bounded, opt-in retained session-runtime
lifecycle and its failure boundary. It does not cut this runtime into default
normal boot and does not establish a production persistent Session Manager.
Session state is retained only within the bounded process lifetime and is
fresh after reboot.

Slice 3 is a separately invoked boundary: bind ADR 0089's
`SessionControlInterpreter` and session-lifetime `ViewingState` to this
retained owner. This ADR does not implement or expand Viewing behavior,
activation/deactivation semantics, traversal, FocusMark presentation, USB or
xHCI input, click/scroll/zoom semantics, durable session persistence,
production wait/wakeup, or default-boot integration.

No USB media was written and no new physical Lenovo validation occurred.
QEMU evidence is not physical-hardware acceptance.
