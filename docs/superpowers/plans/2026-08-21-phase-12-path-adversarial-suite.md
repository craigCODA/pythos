# Phase 12 Slice 3: path-adversarial-suite

## Scope

Implement only Phase 12 Slice 3, `path-adversarial-suite`, on top of the
accepted Slice 2 `object-locator 0.1` resolver ABI. This slice proves denied
namespace-confusion and authority-bypass cases for ADR 0069 without reopening
the POSIX-vs-graph decision.

Stop after Slice 3 passes at the Phase 12 -> Phase 13 boundary.

## Constraints

- No ambient root, current working directory, parent traversal, file
  descriptors, inode model, mount table, symlink model, hard-link model,
  POSIX permission bits, or byte-stream-first identity.
- Invalid locator syntax must die before graph resolution or capability
  checks.
- Names locate, relationships describe, capabilities authorize.
- Reuse ADR 0070 denial identities. Do not add a new resolver ABI unless a
  live conflict requires it.
- Boot serial markers remain the acceptance oracle.

## Tests First

1. Extend resolver unit coverage with adversarial cases:
   - invalid navigation/empty/host-root syntax stays grammar-only;
   - stale binding is distinct from missing segment;
   - missing traversal authority is distinct from denied traversal authority;
   - missing or insufficient final object authority is final-auth denial;
   - duplicate same-name bindings produce name-collision denial;
   - malformed binding and non-binding relationships do not redirect;
   - explicit root selection prevents global-root fallback.
2. Extend marker-contract tests for a new `path-adversarial-suite` slice after
   `OBJECT_LOCATOR_RESOLUTION_READY` and before framebuffer readiness.
3. Add a boot handoff test method that runs the new slice.

## Implementation

1. Add `run_adversarial_self_test()` beside `run_self_test()` in
   `core/src/object_locator.rs`.
2. Build bounded in-memory resolver fixtures only from existing object,
   relationship, revision, and capability tables.
3. Emit stable Slice 3 markers:
   - `PYTHOS:CORE:LOCATOR:EMPTY_SEGMENT_DENIED`
   - `PYTHOS:CORE:LOCATOR:STALE_BINDING_DENIED`
   - `PYTHOS:CORE:LOCATOR:MISSING_SEGMENT_DENIED`
   - `PYTHOS:CORE:LOCATOR:MISSING_TRAVERSAL_DENIED`
   - `PYTHOS:CORE:LOCATOR:MISSING_FINAL_AUTH_DENIED`
   - `PYTHOS:CORE:LOCATOR:NAME_COLLISION_DENIED`
   - `PYTHOS:CORE:LOCATOR:LINK_CONFUSION_DENIED`
   - `PYTHOS:CORE:LOCATOR:GLOBAL_ROOT_DENIED`
   - `PYTHOS:CORE:PATH_ADVERSARIAL_SUITE_READY`
   - `PYTHOS:CORE:PHASE_12_COMPLETE`
4. Call the adversarial self-test immediately after the Slice 2 resolver
   self-test and before framebuffer readiness.
5. Record marker/contract behavior in ADR 0072 and refresh handover, roadmap,
   technical overview, semantic checkpoint contract, and AGENTS boundary text.

## Verification

Run:

```text
cargo test -p pythos-core object_locator -- --nocapture
cargo test --workspace
py -m unittest tests.test_boot_marker_contract tests.test_interface_compatibility_freeze tests.test_ci_workflow tests.test_build_orchestration
py scripts\test-boot.py --slice path-adversarial-suite --timeout 60
git diff --check
```

If these pass, stop at Phase 12 -> Phase 13 and report evidence.
