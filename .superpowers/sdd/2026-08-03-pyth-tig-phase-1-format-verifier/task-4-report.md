# Task 4 Report: Type, Effect, and Capability Verification

## Scope

Implemented Task 4 only.

Touched implementation files:

- `shared/src/pyth_tig/verify.rs`
- `shared/src/pyth_tig/opcode.rs`
- `shared/src/pyth_tig/test_support.rs`

No boot, runtime, scripts, tools, compiler, or Cargo manifest files were
modified.

## RED

Commands:

```powershell
cargo test -p pythos-shared pyth_tig::verify::tests::verifier_rejects_type_mismatch_effect_fork_and_capability_constant
cargo test -p pythos-shared pyth_tig::verify::tests::verifier_rejects_insufficient_import_rights
```

Result: both failed before implementation. The compiler reported the new Task 4
fixture helpers and `VerifyError` variants were missing.

## Implementation

- Added `OpcodeSignature` and provisional no-alloc signature metadata in
  `opcode.rs`.
- Added resource-kind and rights constants for verifier-side import checks.
- Added semantic verifier passes for:
  - input/result type signatures;
  - effect input presence;
  - single non-forking effect chain;
  - capability-typed value origin rejection for constants/forgeries;
  - host import existence, expected capability type, resource kind, and rights;
  - immediate `HostResult` producer reference and field/result structure.
- Added Task 4 negative fixtures in `test_support.rs`.

## GREEN

Commands and results:

```powershell
cargo test -p pythos-shared pyth_tig::verify
```

Passed: 6 passed.

```powershell
cargo test -p pythos-shared pyth_tig
```

Passed: 15 passed.

```powershell
cargo test -p pythos-shared
```

Passed: 69 passed.

```powershell
cargo fmt --all -- --check
```

Passed.

```powershell
cargo clippy -p pythos-shared --all-targets -- -D warnings
```

Passed.

```powershell
cargo miri test -p pythos-shared pyth_tig::verify
```

Not run successfully. Exact limitation:

```text
error: the 'miri' component which provides the command 'cargo-miri.exe' is not available for the '1.93.1-x86_64-pc-windows-msvc' toolchain
```

## Concern

The verifier follows the live package convention already used by the Task 1/2
fixtures and Phase 2 plan: host operation `auxiliary0` names the declared
capability import slot, while graph value inputs carry ordinary typed values.
This keeps capability imports out of graph constants and still validates import
existence, expected capability type, resource kind, and rights. The Task 4 brief
also says `SystemLog` consumes `[Effect, Capability, Utf8]`; making that literal
would require a capability-valued graph input in the minimal log package, which
does not match the current fixture/tool convention.

## Commit

`2eb9985`
