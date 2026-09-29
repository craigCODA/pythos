# Task 1 implementation report

## Scope

Implemented only the pure, fixed-size conventional PCI capability-list parser/model and parser contract tests. No PCI I/O, feature wiring, boot path, framebuffer, oracle, CI, or physical-evidence work was added.

## Changed files

- `core/src/network_hardware_capability_probe.rs`
  - Added `CapabilityKind`, `PciCapabilityEntry`, `CapabilitySnapshot`, and `CapabilityParseError`.
  - Added allocation-free `parse_capability_list` with status-bit gating, pointer validation, repeated-offset rejection, the 48-entry bound, recognized fixed header spans, opaque unknown entries, traversal-order preservation, and first-recognized-offset summaries.
  - Added eight Rust unit tests covering every Task 1 required case.
- `core/src/main.rs`
  - Declared the parser module under `cfg(test)` so the standalone parser tests compile without adding Task 2 feature wiring.
- `tests/test_network_hardware_capability_probe.py`
  - Added parser-focused source contract tests for the public interface, fixed bounds, no-allocation/read-only constraints, and test-only module declaration.
- `.superpowers/sdd/2026-09-29-phase-15-pci-capability-metadata-snapshot/task-1-report.md`
  - This report.

## Test and formatting output

Initial red test, before the parser file existed:

```text
py -3 -m unittest tests/test_network_hardware_capability_probe.py
...
FileNotFoundError: ...\\core\\src\\network_hardware_capability_probe.rs
Ran 0 tests in 0.023s
FAILED (errors=1)
```

Focused Rust tests:

```text
cargo test -p pythos-core network_hardware_capability_probe
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 907 filtered out; finished in 0.00s
```

Focused Python contract tests:

```text
py -3 -m unittest tests/test_network_hardware_capability_probe.py
Ran 3 tests in 0.002s
OK
```

Repository Rust test suite:

```text
cargo test -p pythos-core
running 915 tests
test result: ok. 915 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Formatting and whitespace checks:

```text
cargo fmt
cargo fmt -- --check
git diff --check
```

All exited with status 0. The Rust test build emitted existing unused-code warnings from unrelated modules; no new parser warning or failure was reported.

## Self-review

- The parser reads exactly the ID and next-pointer bytes for each entry.
- Status bit 4 clear returns without invoking the byte reader and yields deterministic empty summaries.
- Nonzero pointers are checked for the `0x40..=0xFC` range and 4-byte alignment before entry reads.
- Repeated offsets and attempts beyond 48 entries are terminal errors.
- Recognized spans are PM `0x08`, PCIe `0x14`, MSI `0x0A`, and MSI-X `0x0C`; unknown entries have no inferred length.
- No `Vec`, allocator, unsafe code, I/O, or configuration write path was introduced.
- The pre-existing untracked implementation-plan file was not staged or committed.

## Concerns

The parser module is deliberately declared only for tests. Task 2 owns the production feature declaration and feature-gated module wiring, so this keeps Task 1 within the requested pure-parser boundary.

## Commit

To be filled after the focused Task 1 commit is created.
