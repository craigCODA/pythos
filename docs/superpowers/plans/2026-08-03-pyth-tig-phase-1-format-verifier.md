# PythTIG Phase 1 Format and Verifier Implementation Plan

**Status:** Complete on `main`. ADR 0065 is accepted and the tested PythTIG
version 1 package ABI is frozen as of 2026-08-08. The shared verifier, host
tool, canonical fixture, and mutation suite are covered by
`python scripts\test-pyth-tig-format.py`. The task checklist below is the
historical implementation plan, not current pending work.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Enforce red-green-refactor for every code change.

**Goal:** Implement the canonical PythTIG package ABI, no-alloc decoder, deterministic encoder tool, and shared verifier pipeline that rejects malformed, ill-typed, effect-invalid, capability-forged, and over-limit graph packages.

**Architecture:** All semantic constants and validation live in `pythos-shared` so PythCore and host tools use the same implementation. A small host tool builds canonical fixtures and mutation cases. No runtime launch exists in this phase.

**Tech Stack:** Rust `no_std` shared crate, Rust `std` host tool, Python acceptance harness.

## Global Constraints

- ADR 0065 sizes and numeric codes are Phase 1 candidates. Any format change
  made while implementing this verifier must update ADR 0065 in the same branch
  before merge.
- Unknown opcodes, types, flags, major versions, and nonzero reserved fields are rejected.
- Capability values cannot be constants.
- Effectful opcodes require one non-forking Effect chain.
- The verifier performs no allocation and accepts caller-supplied scratch storage only when required.
- Decoder success never implies verifier success.

---

### Task 1: Define candidate types, opcodes, and record layouts

**Files:**
- Create: `shared/src/pyth_tig/mod.rs`
- Create: `shared/src/pyth_tig/types.rs`
- Create: `shared/src/pyth_tig/opcode.rs`
- Create: `shared/src/pyth_tig/format.rs`
- Modify: `shared/src/lib.rs`

**Interfaces:**
- Produces: `PythType`, `Opcode`, `PythGraphHeader`, `TypeRecord`, `BlockRecord`, `NodeRecord`, `CapabilityImportRecord`, `NO_VALUE`, package bounds.

- [ ] **Step 1: Write failing layout and numeric-code tests**

In `shared/src/pyth_tig/format.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pyth_tig::{opcode::Opcode, types::PythType};

    #[test]
    fn v1_layouts_and_codes_are_recorded() {
        assert_eq!(PYTH_TIG_MAGIC, *b"PYTHTIG1");
        assert_eq!(PYTH_TIG_MAJOR, 1);
        assert_eq!(PYTH_TIG_MINOR, 0);
        assert_eq!(core::mem::size_of::<PythGraphHeader>(), 96);
        assert_eq!(core::mem::size_of::<TypeRecord>(), 8);
        assert_eq!(core::mem::size_of::<BlockRecord>(), 24);
        assert_eq!(core::mem::size_of::<NodeRecord>(), 40);
        assert_eq!(core::mem::size_of::<CapabilityImportRecord>(), 24);
        assert_eq!(PythType::Capability.code(), 0x000A);
        assert_eq!(PythType::Effect.code(), 0x000B);
        assert_eq!(Opcode::SystemLog.code(), 0x1000);
        assert_eq!(Opcode::TaskProposalEmit.code(), 0x1201);
        assert_eq!(NO_VALUE, u32::MAX);
    }
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-shared pyth_tig::format::tests::v1_layouts_and_codes_are_recorded
```

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Implement recorded enums and records**

Implement the types and opcodes recorded in Sections 6 through 8 of the design
spec, or update ADR 0065 and the spec in the same branch if the first corpus
shows the layout needs revision before the owner ABI freeze. Use `#[repr(u16)]`
for `PythType` and `Opcode`, `TryFrom<u16>` with explicit `UnknownType` and
`UnknownOpcode` errors, and `#[repr(C)]` for every record.

Export in `shared/src/pyth_tig/mod.rs`:

```rust
pub mod format;
pub mod opcode;
pub mod types;
pub mod verify;

pub use format::*;
pub use opcode::*;
pub use types::*;
```

Export from `shared/src/lib.rs`:

```rust
pub mod pyth_tig;
```

- [ ] **Step 4: Run GREEN and full shared tests**

```powershell
cargo test -p pythos-shared pyth_tig
cargo test -p pythos-shared
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add shared\src\lib.rs shared\src\pyth_tig
git commit -m "feat(pyth-tig): define graph package ABI"
```

---

### Task 2: Implement bounded package decoder

**Files:**
- Modify: `shared/src/pyth_tig/format.rs`

**Interfaces:**
- Produces: `PythGraphPackage<'a>`, typed section accessors, `PackageDecodeError`.

- [ ] **Step 1: Write failing decoder tests**

Add tests:

```rust
#[test]
fn decoder_exposes_non_overlapping_sections() {
    let bytes = crate::pyth_tig::test_support::minimal_log_package();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    assert_eq!(package.header().node_count, 3);
    assert_eq!(package.blocks().len(), 1);
    assert_eq!(package.nodes().len(), 3);
    assert_eq!(package.imports().len(), 1);
    assert_eq!(package.string_at(0, 5).unwrap(), b"hello");
}

#[test]
fn decoder_rejects_overlapping_sections_and_nonzero_reserved_fields() {
    let mut overlapping = crate::pyth_tig::test_support::minimal_log_package();
    crate::pyth_tig::test_support::set_nodes_offset_equal_blocks_offset(&mut overlapping);
    assert_eq!(PythGraphPackage::decode(&overlapping), Err(PackageDecodeError::SectionOverlap));

    let mut reserved = crate::pyth_tig::test_support::minimal_log_package();
    crate::pyth_tig::test_support::set_header_reserved(&mut reserved, 1);
    assert_eq!(PythGraphPackage::decode(&reserved), Err(PackageDecodeError::NonZeroReserved));
}
```

Create test-only helpers under `shared/src/pyth_tig/test_support.rs` behind `#[cfg(test)]`.

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-shared pyth_tig::format::tests::decoder
```

Expected: FAIL because decoder APIs are missing.

- [ ] **Step 3: Implement decoder**

Implement:

```rust
pub struct PythGraphPackage<'a> {
    bytes: &'a [u8],
    header: PythGraphHeader,
}

impl<'a> PythGraphPackage<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self, PackageDecodeError>;
    pub const fn header(&self) -> PythGraphHeader;
    pub fn types(&self) -> &'a [TypeRecord];
    pub fn blocks(&self) -> &'a [BlockRecord];
    pub fn nodes(&self) -> &'a [NodeRecord];
    pub fn imports(&self) -> &'a [CapabilityImportRecord];
    pub fn constant_pool(&self) -> &'a [u8];
    pub fn string_table(&self) -> &'a [u8];
    pub fn string_at(&self, offset: u32, len: u16) -> Result<&'a [u8], PackageDecodeError>;
}
```

Use checked arithmetic for every offset and length. Validate bounds, alignment, section ordering, section non-overlap, package maximum, counts, header version, reserved fields, and checksum before creating typed slices. Every unsafe slice conversion carries the required invariant comment.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythos-shared pyth_tig::format
cargo miri test -p pythos-shared pyth_tig::format
```

If Miri is not installed, record that as an environment limitation and run the full Rust test suite; do not claim Miri success.

- [ ] **Step 5: Commit**

```powershell
git add shared\src\pyth_tig\format.rs shared\src\pyth_tig\test_support.rs
git commit -m "feat(pyth-tig): decode bounded graph packages"
```

---

### Task 3: Implement structural and control-flow verifier passes

**Files:**
- Create: `shared/src/pyth_tig/verify.rs`
- Modify: `shared/src/pyth_tig/test_support.rs`

**Interfaces:**
- Produces: `verify_package(package: &PythGraphPackage<'_>) -> Result<VerifiedGraph<'_>, VerifyError>`, `VerifiedGraph` opaque proof wrapper.

- [ ] **Step 1: Write failing structural tests**

Add:

```rust
#[test]
fn verifier_rejects_missing_terminator_bad_target_and_use_before_definition() {
    let missing = test_support::package_without_terminator();
    assert_eq!(verify_bytes(&missing), Err(VerifyError::MissingTerminator { block: 0 }));

    let target = test_support::package_with_bad_branch_target();
    assert_eq!(verify_bytes(&target), Err(VerifyError::InvalidControlTarget { block: 0, target: 9 }));

    let use_before = test_support::package_with_use_before_definition();
    assert_eq!(verify_bytes(&use_before), Err(VerifyError::ValueNotAvailable { node: 1, input: 0 }));
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-shared pyth_tig::verify::tests::verifier_rejects_missing_terminator_bad_target_and_use_before_definition
```

- [ ] **Step 3: Implement passes 1 through 6**

Define stable errors:

```rust
pub enum VerifyError {
    Decode(PackageDecodeError),
    UnknownType { code: u16 },
    UnknownOpcode { code: u16 },
    InvalidBlockRange { block: u32 },
    MissingTerminator { block: u32 },
    MultipleTerminators { block: u32 },
    InvalidControlTarget { block: u32, target: u32 },
    BlockArgumentCountMismatch { source: u32, target: u32 },
    ValueNotAvailable { node: u32, input: u8 },
    ResultTypeForbidden { node: u32 },
    ResourceBudgetExceeded,
    NonCanonicalEncoding,
    ChecksumMismatch,
}
```

`VerifiedGraph` stores only a `PythGraphPackage` and has no public constructor.

Implement block ranges, exactly-one-last-terminator, control targets, block-argument arity, and a bounded dominance analysis using fixed arrays sized by ABI maxima.
The `ResourceBudgetExceeded` verifier error is a defense-in-depth result for
direct `verify_package` contexts. The public CLI and `verify_bytes` path must
reject over-limit package record counts during decode as `Decode(CountLimit)`
before a `VerifiedGraph` can exist.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test -p pythos-shared pyth_tig::verify
```

- [ ] **Step 5: Commit**

```powershell
git add shared\src\pyth_tig\verify.rs shared\src\pyth_tig\test_support.rs
git commit -m "feat(pyth-tig): verify graph structure and control flow"
```

---

### Task 4: Implement type, effect, and capability verification

**Files:**
- Modify: `shared/src/pyth_tig/verify.rs`
- Modify: `shared/src/pyth_tig/opcode.rs`
- Modify: `shared/src/pyth_tig/test_support.rs`

**Interfaces:**
- Produces: complete opcode signatures, effect-chain validation, capability-origin validation, import-rights validation.

- [ ] **Step 1: Write failing semantic tests**

Add:

```rust
#[test]
fn verifier_rejects_type_mismatch_effect_fork_and_capability_constant() {
    assert_eq!(
        verify_bytes(&test_support::package_with_add_bool()),
        Err(VerifyError::TypeMismatch { node: 2, input: 0, expected: PythType::U64, actual: PythType::Bool })
    );
    assert_eq!(
        verify_bytes(&test_support::package_with_effect_fork()),
        Err(VerifyError::EffectFork { producer: 0 })
    );
    assert_eq!(
        verify_bytes(&test_support::package_with_capability_constant()),
        Err(VerifyError::CapabilityOriginInvalid { node: 1 })
    );
}

#[test]
fn verifier_rejects_insufficient_import_rights() {
    assert_eq!(
        verify_bytes(&test_support::object_revise_with_read_only_import()),
        Err(VerifyError::ImportRightsInsufficient { node: 3, import_slot: 0 })
    );
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test -p pythos-shared pyth_tig::verify::tests::verifier_rejects_type_mismatch_effect_fork_and_capability_constant
cargo test -p pythos-shared pyth_tig::verify::tests::verifier_rejects_insufficient_import_rights
```

- [ ] **Step 3: Define opcode signatures**

In `opcode.rs`, define:

```rust
pub struct OpcodeSignature {
    pub inputs: [PythType; 4],
    pub input_count: u8,
    pub result: PythType,
    pub effectful: bool,
    pub terminator: bool,
    pub required_resource_kind: Option<u16>,
    pub required_rights: u64,
}

impl Opcode {
    pub const fn signature(self) -> OpcodeSignature;
}
```

Use the current provisional signatures from ADR 0065. `SystemLog` consumes
`[Effect, Capability, Utf8]` and returns `Effect`. Object mutations consume
`[Effect, Capability, ...]` and return typed result plus effect through the
node's prescribed result convention recorded in `auxiliary0`; do not encode
hidden extra results. If the first verifier corpus proves a signature should
change before freeze, update ADR 0065 in the same branch.

For v1, a declared capability import is materialized as an SSA value only by an
entry-block `BlockParam` whose `result_type` is `Capability` and whose
`auxiliary0` is the import slot. Host operations consume that value through
normal graph inputs and use its import provenance for resource-kind and rights
checks. They must not acquire authority from a hidden per-op import slot.

For v1, every effectful host operation returns `Effect`; typed host data is
written into a following `HostResult` structural node identified by
`auxiliary0`. Add `HostResult` as opcode `0x0008`, with verifier rules that it
immediately follows and references one effectful producer. This avoids
multi-result node ambiguity. A `HostResult` field is valid only when a per-op
host-result schema documents that field for the referenced producer. Phase 1
defines no capability-returning `HostResult`.

- [ ] **Step 4: Implement semantic passes**

Add stable errors:

```rust
TypeMismatch { node: u32, input: u8, expected: PythType, actual: PythType },
EffectInputMissing { node: u32 },
EffectFork { producer: u32 },
EffectChainDisconnected { node: u32 },
CapabilityOriginInvalid { node: u32 },
CapabilityImportMissing { node: u32, import_slot: u16 },
ImportTypeMismatch { import_slot: u16 },
ImportRightsInsufficient { node: u32, import_slot: u16 },
HostResultInvalid { node: u32 },
```

Track capability provenance as `Import(slot)`, and add `HostResult(node)` only
for a later documented capability-returning host operation. Integer constants
and arithmetic results can never be reinterpreted as `Capability`.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test -p pythos-shared pyth_tig
```

- [ ] **Step 6: Commit**

```powershell
git add shared\src\pyth_tig\verify.rs shared\src\pyth_tig\opcode.rs shared\src\pyth_tig\test_support.rs
git commit -m "feat(pyth-tig): verify types effects and capability origins"
```

---

### Task 5: Build canonical host fixture tool and mutation acceptance

**Files:**
- Modify: `Cargo.toml`
- Create: `tools/pyth-tig-tool/Cargo.toml`
- Create: `tools/pyth-tig-tool/src/main.rs`
- Create: `tools/pyth-tig-tool/src/encode.rs`
- Create: `tools/pyth-tig-tool/src/mutate.rs`
- Create: `scripts/test-pyth-tig-format.py`
- Create: `tests/fixtures/pyth-tig/.gitkeep`

**Interfaces:**
- Consumes: shared ABI and verifier.
- Produces: `pyth-tig-tool emit-minimal-log`, `verify`, and `mutate-suite` commands; binary fixtures.

- [ ] **Step 1: Write failing CLI acceptance**

Create `scripts/test-pyth-tig-format.py`:

```python
#!/usr/bin/env python
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "target" / "pyth-tig" / "minimal-log.tig"


def run(command: list[str], expected: int = 0) -> str:
    result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout)
    if result.returncode != expected:
        raise AssertionError(f"{command} returned {result.returncode}, expected {expected}")
    return result.stdout


def main() -> int:
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    run(["cargo", "run", "-p", "pyth-tig-tool", "--", "emit-minimal-log", str(FIXTURE)])
    output = run(["cargo", "run", "-p", "pyth-tig-tool", "--", "verify", str(FIXTURE)])
    if "PYTH_TIG_VERIFY_OK" not in output:
        raise AssertionError("valid package was not verified")
    mutation = run(["cargo", "run", "-p", "pyth-tig-tool", "--", "mutate-suite", str(FIXTURE)])
    if "PYTH_TIG_MUTATION_SUITE_OK" not in mutation:
        raise AssertionError("mutation suite did not complete")
    print("PYTH_TIG_FORMAT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Run RED**

```powershell
python scripts\test-pyth-tig-format.py
```

Expected: FAIL because `pyth-tig-tool` does not exist.

- [ ] **Step 3: Implement canonical encoder tool**

Add `tools/pyth-tig-tool` to the workspace. Implement commands:

```text
emit-minimal-log <path>
verify <path>
mutate-suite <path>
```

`emit-minimal-log` builds exactly one block with:

```text
EffectStart
ConstUtf8 "hello"
SystemLog using capability import slot 0
Return
```

The encoder calculates aligned offsets, zeroes reserved fields, computes checksum, and writes the canonical byte sequence.

`mutate-suite` creates in-memory mutations and requires these exact verifier errors:

```text
bad magic                   Decode(BadMagic)
unknown major               Decode(UnsupportedMajor)
section overlap             Decode(SectionOverlap)
checksum mismatch           Decode(ChecksumMismatch)
missing terminator          MissingTerminator { block: 0 }
bad control target          InvalidControlTarget { block: 0, target: 9 }
type mismatch               TypeMismatch { node: 3, input: 0, expected: U64, actual: Bool }
effect fork                 EffectFork { producer: 0 }
capability constant         CapabilityOriginInvalid { node: 1 }
insufficient rights         ImportRightsInsufficient { node: 3, import_slot: 0 }
node count limit            Decode(CountLimit)
```

Print `PYTH_TIG_MUTATION_SUITE_OK` only after every mutation produces its expected error.

- [ ] **Step 4: Run GREEN**

```powershell
python scripts\test-pyth-tig-format.py
cargo test -p pythos-shared pyth_tig
```

Expected:

```text
PYTH_TIG_FORMAT_TEST_OK
```

- [ ] **Step 5: Commit**

```powershell
git add Cargo.toml tools\pyth-tig-tool scripts\test-pyth-tig-format.py tests\fixtures\pyth-tig\.gitkeep
git commit -m "test(pyth-tig): add canonical fixture and mutation suite"
```

---

## Phase 1 verification

Run:

```powershell
cargo fmt --all -- --check
cargo test -p pythos-shared
cargo test -p pyth-tig-tool
cargo clippy -p pythos-shared -p pyth-tig-tool --all-targets -- -D warnings
python scripts\test-pyth-tig-format.py
python scripts\test-boot.py
python scripts\test-object-shell.py
```

Expected: all pass. Dispatch the ABI reviewer and security reviewer. Phase 2 and Phase 4 may start only after both reviewers approve the shared format and verifier.
