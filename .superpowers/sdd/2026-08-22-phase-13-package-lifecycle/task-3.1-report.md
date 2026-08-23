# Phase 13 Task 3.1 Report: Export Resolution Through Package Registry

Status: DONE

## Scope

- Implemented only Task 3.1: package export resolution through the package registry.
- Changed code in `core/src/package_registry.rs` and `core/src/package_service.rs`.
- Did not begin Task 3.2, did not add package launch behavior, and did not run a QEMU launch scenario.
- Preserved the Phase 13 identity split: package names and locators locate, package `ObjectId` identifies, digests verify immutable package content/revisions, manifest-derived export metadata describes relationships, and capability-bearing namespace roots authorize lookup.

## TDD Red

- Added failing `package_export_resolution` tests before production implementation.
- Command: `cargo test -p pythos-core package_export_resolution`
- Result: failed as expected because `PackageRegistryExportRecord`, `PackageRegistry::add_export_record`, and `PackageService::resolve_export` did not exist.

## Implementation

- Added `PackageRegistryExportRecord` with bounded package-locator and export-name storage plus package identity, revision, release digest, export metadata, and schema descriptor metadata.
- Added `PackageRegistry::add_export_record`, validating export records against an active package record before registration and rejecting duplicate namespace/package/export bindings.
- Added `PackageRegistry::export_for_locator(namespace_root, locator)`, using the existing object-locator validator before registry lookup and resolving only explicit `package/export` locators under the caller-provided namespace root.
- Added `PackageService::resolve_export` as the service facade over `PackageRegistry::export_for_locator`.
- Added regression coverage for explicit namespace-root resolution, missing exports, invalid locator syntax before lookup, and separation between locator text and package `ObjectId` identity.

## Verification

- RED: `cargo test -p pythos-core package_export_resolution` failed before implementation on the missing APIs named above.
- GREEN: `cargo test -p pythos-core package_export_resolution` -> 4 passed, 0 failed.
- Focused registry: `cargo test -p pythos-core package_registry` -> 15 passed, 0 failed.
- Focused locator mirror: `cargo test -p pythos-core package_locator_mirrors` -> 1 passed, 0 failed.
- Focused package service: `cargo test -p pythos-core package_service::tests` -> 41 passed, 0 failed.
- Formatting: `cargo fmt -p pythos-core -- --check` -> passed.
- Diff hygiene: `git diff --check` -> passed.

## Notes

- The focused Cargo test runs still print pre-existing unused-code warnings in unrelated modules such as `ps2.rs`, `storage_backend_screen.rs`, `fb_debug.rs`, `package_content_store.rs`, `pyth_service_supervisor.rs`, `sdhci.rs`, and `serial.rs`.
- Registry export lookup is intentionally explicit-root and bounded. It does not add POSIX path semantics, a global root fallback, a current working directory, or ObjectId parsing from locator text.
- Durable export snapshot encoding was not added in this task. The current branch's registry snapshot encoding already persists package/schema/content records; Task 3.1 only required the export-resolution behavior and tests, and Task 3.2/QEMU publication paths remain untouched.

## Commit

- Message: `feat(core): resolve installed package exports`
