# PythTIG Cross-Target Matrix

This matrix tracks where an unchanged PythTIG package has been accepted.
Backend discovery and transport may differ by target; package bytes, package
runtime digest, and normalized semantic markers must not.

The automated Phase 7 package is `target/pyth-tig/hello.tig`, produced by
`python scripts/build-pyth-graph.py` and embedded by
`python scripts/build-image.py --with-pythtig`. The QEMU harness runs that exact
package through the existing graph-control sector and records normalized JSON
with:

- `package_checksum`: SHA-256 of the package bytes.
- `package_runtime_digest`: the 64-bit digest emitted by PythCore in
  `PYTHOS:PYTHTIG:PACKAGE_VALID`.
- `semantic_markers`: PythTIG markers normalized to remove backend noise.
- `storage_restore`: true only when the log contains an object/general-storage
  restore marker or an object-restore graph marker. The current automated
  `hello.tig` proof reports false; reboot/restore coverage remains in the
  default-boot and object-shell harnesses.
- `raw_log_sha256`: SHA-256 of the source serial log.

## Current Targets

| Target | Backend | State | Evidence Path | Claim Boundary |
| --- | --- | --- | --- | --- |
| QEMU q35 | legacy virtio-blk | automated | `python scripts/test-pyth-cross-target.py --automated-only` | Package/runtime semantics match AHCI for the same package bytes. |
| QEMU q35 | polling AHCI | automated | `python scripts/test-pyth-cross-target.py --automated-only` | Package/runtime semantics match virtio with virtio disabled and AHCI selected. |
| O2 Micro `1217:8620` | SDHCI/eMMC | physical-log pending for PythTIG package | `python scripts/pyth_cross_target.py physical-log --backend sdhci-emmc --package <tig> --log <serial.txt> --output <json>` | Accept only exact target/controller logs containing matching package digest and required serial markers. |
| NVMe | none accepted | pending | none | No PythTIG backend support claim. |
| Other Intel/AMD PCs | target-specific | pending | none | Add only after exact boot/backend evidence. |
| Apple Intel | target-specific | pending | none | Add only after exact boot/backend evidence. |
| Apple silicon | outside x86-64 PythTIG v1 | out of scope | none | Not a PythTIG v1 target. |

## Rules

Physical evidence is target-specific. A passing physical log for one
machine/controller must not be generalized to another machine/controller.

The adapter rejects logs with panic/package-rejection markers, missing backend
selection, wrong package digest, missing runtime entry/exit, or out-of-order
PythTIG markers. It does not manufacture missing markers and does not accept
screenshot-only evidence.
