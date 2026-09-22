# Phase 14 Secure-Transport Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove one bounded, capability-gated TLS 1.3 client exchange above the accepted socket boundary without creating a public TLS API or changing the Virtio/NetworkPort architecture.

**Architecture:** Add one opt-in native secure-transport consumer and a private acceptance harness. Reuse the existing copied-frame `NetworkPort`, ARP/IPv4/TCP codecs, socket authority lifecycle, and QEMU frame oracle. Use the reviewed no-`std`, no-allocator TLS 1.3 backend selected by Task 1; never implement cryptography locally. Keep authentication and all TLS state private to the finite proof.

**Tech Stack:** Rust 1.93.1, `x86_64-unknown-none`, existing PythOS `no_std` user probe model, selected TLS 1.3 backend, Python `ssl.MemoryBIO` or an equivalent host TLS oracle, existing QEMU socket framing and serial markers, `unittest`.

**Spec:** `docs/superpowers/specs/2026-09-22-phase-14-secure-transport-design.md`

## Global Constraints

- Keep `VirtioTransport`, transport adapter, and `NetworkPort` as the PythOS architectural names.
- Do not change the existing NetworkPort ABI, capability-right definitions, frozen PythTIG v1 ABI, or Virtio lifecycle.
- Use the reviewed no-`std`, no-allocator TLS 1.3 backend selected by Task 1; do not hand-write cryptography.
- Prove exactly one client connection, one full 1-RTT handshake, one bounded request/reply, one tamper rejection, and one revocation path.
- Exclude 0-RTT, resumption, tickets, KeyUpdate, client authentication, DNS, HTTP, update/recovery, persistent state, physical hardware, and Phase 15.
- Keep the host TCP socket limited to the QEMU Virtio frame oracle; it is not a PythOS socket or TLS API.

## Review Focus

- Backend target compatibility and authentication: reject any backend that cannot compile for the guest target or verify the pinned test identity.
- TCP/TLS stream boundaries: prove segmentation and coalescing do not release partial records or plaintext before authentication.
- Nonce/sequence and tamper handling: altered ciphertext/tag must fail closed with no application bytes.
- Capability denial: denied startup must produce no TCP/TLS frame and no secure marker.
- Scope and lifecycle: no new public ABI, persistent state, hardware behavior, storage evidence, or changed earlier proof.

---

### Task 1: Freeze backend and private secure-transport contract

**Files:**
- Create: `docs/decisions/0104-phase-14-secure-transport-proof.md`
- Modify: `Cargo.toml` and the relevant probe manifest only after backend verification
- Test: `tests/test_secure_transport_contract.py`

**Interfaces:**
- Consumes: the secure-transport design spec, RFC 8446 profile, and existing
  socket/NetworkPort identity conventions.
- Produces: exact backend/version, feature flags, pinned test identity,
  marker constants, fixed record limits, and an explicit no-go decision if
  the candidate backend cannot satisfy the guest target/authentication gate.

- [ ] **Step 1: Verify the candidate backend before adding it**

Run the candidate backend's minimal no-`std` compile for
`x86_64-unknown-none` with default features disabled. Confirm that its public
API supports a blocking/sans-I/O transport adapter, a real cryptographic RNG
interface, and pinned server identity verification. Record the exact failure
instead of replacing any missing primitive with local cryptography.

- [ ] **Step 2: Add only the verified dependency and exact feature gates**

Add the backend at a pinned version with default features disabled. Add
mutually exclusive `secure-transport-probe` and
`secure-transport-denied-probe` features that reuse the existing granted and
denied socket lifecycle gates while depending on `verify`. Do not enable
either feature by default.

- [ ] **Step 3: Add contract tests before the consumer**

Assert the exact feature exclusions, marker names/order, TLS version, disabled
0-RTT/resumption/tickets/KeyUpdate profile, fixed request/response limits,
identity pin, and unchanged architectural names. Run:

```text
py -3 -m unittest tests.test_secure_transport_contract -v
```

- [ ] **Step 4: Commit**

```text
git add Cargo.toml Cargo.lock core/Cargo.toml docs/decisions/0104-phase-14-secure-transport-proof.md tests/test_secure_transport_contract.py
git commit -m "docs(net): freeze secure transport proof contract"
```

### Task 2: Implement the finite native TLS consumer

**Files:**
- Create or modify: `user/probes/secure-transport/src/lib.rs`, `src/main.rs`, and `Cargo.toml`
- Create: `user/probes/secure-transport/linker.ld`
- Modify: `core/src/main.rs`, `core/src/secure_transport_probe.rs`, and `core/Cargo.toml`
- Modify: `shared/src/secure_transport_markers.rs` and `shared/src/lib.rs` only for private marker literals
- Test: Rust unit tests in the secure probe and core contract tests

**Interfaces:**
- Consumes: `NetworkPortBootstrapV1`, existing private capability authority,
  accepted Ethernet/ARP/IPv4/TCP codecs, and the selected TLS backend.
- Produces: a service-local TLS client that exposes no public syscall or
  reusable resource and emits only the frozen acceptance markers.

- [ ] **Step 1: Write failing pure contract tests**

Pin the TLS profile, record limits, exact request/reply bytes, identity
fingerprint, no-early-data rule, and tamper failure result. Include the
backend's RFC vectors where its API exposes them.

- [ ] **Step 2: Implement bounded transport adaptation**

Drive the backend over the existing private TCP stream with fixed buffers.
Accumulate complete TLS records across TCP segments, reject oversize/truncated
input, and release application bytes only after the backend authenticates the
record. Keep the connection and all keys on the stack/fixed storage and wipe
private buffers during teardown where the backend permits it.

- [ ] **Step 3: Implement the granted, tamper, and denied state paths**

The granted path performs ARP/TCP setup, the full TLS handshake, one encrypted
request/reply, close, revocation, and exit. The tamper path changes one byte in
the protected response and requires authentication failure with no plaintext.
The denied path stops before TCP setup and produces zero frames.

- [ ] **Step 4: Run focused guest tests**

```text
cargo test -p pythos-user-secure-transport-probe
cargo test -p pythos-core secure_transport_probe --features secure-transport-probe
cargo test -p pythos-core secure_transport_probe --features secure-transport-denied-probe
cargo clippy -p pythos-core --target x86_64-unknown-none --features secure-transport-probe -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features secure-transport-denied-probe -- -D warnings
cargo clippy -p pythos-user-secure-transport-probe --target x86_64-unknown-none -- -D warnings
```

- [ ] **Step 5: Commit**

```text
git add core shared user/probes/secure-transport Cargo.toml Cargo.lock
git commit -m "feat(net): add bounded secure transport proof"
```

### Task 3: Add the serialized QEMU TLS oracle

**Files:**
- Create: `scripts/build-secure-transport-probe.py`, `scripts/test-secure-transport.py`, `tests/test_secure_transport.py`
- Modify: only shared harness helpers when a focused test proves the change is generic and backward compatible

**Interfaces:**
- Consumes: the guest secure-transport ELF, existing QEMU frame envelope,
  current TCP oracle, and a fixed TLS 1.3 host peer/certificate fixture.
- Produces: granted, tamper, and denied serialized evidence with exact frame,
  marker, outcome, and cleanup assertions.

- [ ] **Step 1: Add host-side unit tests first**

Test the fixed identity, TLS transcript/profile, request/reply plaintext,
record tamper mutation, marker sequences, zero-frame denial, and rejection of
missing/duplicate/reordered markers. Test that every QEMU case is serialized.

- [ ] **Step 2: Implement the host TLS peer**

Use a fixed test certificate/key fixture and a standard host TLS implementation
through memory BIOs over the existing TCP frame oracle. The peer must exchange
only the bounded handshake and one application request/reply, alter exactly
one protected response byte in the tamper case, and reject extra frames.

- [ ] **Step 3: Implement cleanup and exact acceptance assertions**

Require `QEMU_OUTCOME success`, exact marker order, expected encrypted-vs-
plaintext relation, no released tampered plaintext, no storage evidence, no
panic/timeout/transport error, and removal of serial/ESP artifacts.

- [ ] **Step 4: Run the focused harness**

```text
py -3 -m py_compile scripts/build-secure-transport-probe.py scripts/test-secure-transport.py tests/test_secure_transport.py
py -3 -m unittest tests.test_secure_transport -v
py -3 scripts/test-secure-transport.py --self-test
py -3 scripts/test-secure-transport.py
```

- [ ] **Step 5: Commit**

```text
git add scripts tests
git commit -m "test(net): prove secure transport acceptance cases"
```

### Task 4: Integrate CI and close the Phase 14 boundary

**Files:**
- Modify: `.github/workflows/qemu-acceptance.yml`, `tests/test_ci_workflow.py`
- Modify: `docs/ROADMAP.md`, `docs/ROADMAP-LATER-PHASES.md`, `docs/HANDOVER.md`, `README.md`, `docs/TECHNICAL-OVERVIEW.md`
- Modify: `docs/decisions/0104-phase-14-secure-transport-proof.md`
- Test: full Python and Rust suites plus the final whole-branch review

**Interfaces:**
- Consumes: all prior accepted Phase 14 ADRs and the three secure-transport
  acceptance cases.
- Produces: CI-gated evidence and a final Phase 14 closeout that does not
  claim production TLS, update authenticity, physical networking, or Phase 15.

- [ ] **Step 1: Add secure-transport commands once, after socket, to the
  milestone workflow and never to handoff**

Add compile, host tests, self-test, live serialized acceptance, and the two
guest feature checks. Add ordering/uniqueness contract tests.

- [ ] **Step 2: Update current documentation only**

Record the exact local evidence and accepted stopping point. Reconcile the
stale later-phase sentence that still names the socket API as next. Do not
rewrite historical checkpoints or claim TLS as a production update channel.

- [ ] **Step 3: Run the complete verification set**

```text
py -3 -m unittest discover -s tests
cargo fmt --all -- --check
cargo test --workspace --quiet
git diff --check
```

- [ ] **Step 4: Obtain independent review for each task and the whole branch**

Review backend suitability, authentication, entropy, record framing, tamper
rejection, capability denial, CI placement, and Phase 15 scope. Fix all
Critical/Important findings and rerun affected gates.

- [ ] **Step 5: Commit the closeout**

```text
git add .github docs README.md tests
git commit -m "docs(net): close phase 14 secure transport boundary"
```
