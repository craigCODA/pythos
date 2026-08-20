# PythTIG Physical Evidence Procedure

This procedure prepares and verifies PythTIG physical-target evidence without
generalizing beyond the tested machine/controller.

## Prepare

From the repository root:

```powershell
python scripts/prepare-pyth-physical-image.py
```

The script builds the `pythtig-phase2-test` boot profile, embeds
`target/pyth-tig/hello.tig`, prepares a companion graph-control storage image,
and writes `target/pyth-physical-image-manifest.json`.

The manifest records:

- Git HEAD and dirty-state summary.
- `target/pyth-tig/hello.tig` SHA-256 and PythCore runtime digest.
- `image/esp` deterministic tree SHA-256.
- Companion storage-control image SHA-256 and graph-control sector/mode.
- Required PythTIG serial markers and forbidden panic/rejection markers.
- Explicit non-claims.

For a real boot, copy the `image/esp` contents to the intended FAT EFI system
partition. Stage the graph-control sector only on a disposable test medium or
explicitly approved target backend. Do not write the control image to an
unapproved internal disk.

## Capture

Record the exact target:

- Machine model and serial/asset identifier.
- Storage controller identity, including PCI vendor/device when available.
- Backend path used: virtio, AHCI, SDHCI/eMMC, or another explicitly added
  backend.
- Boot medium and whether the companion control sector was staged.
- Cold-boot count and whether each boot was full power-cycle or warm reboot.
- Manifest path, manifest SHA-256, ESP tree hash, package SHA-256, and package
  runtime digest.

Capture serial output whenever available. Screenshot-only evidence is not
accepted for PythTIG package verification. If the evidence terminal is used as
a visual transcript path, record the visible page sequence and the terminal drop
count; a nonzero drop count is not acceptance.

## Verify

For a serial log:

```powershell
python scripts/verify-pyth-physical-log.py `
  --manifest target/pyth-physical-image-manifest.json `
  --log <serial.txt> `
  --backend sdhci-emmc `
  --target-id "<machine-controller-id>" `
  --output target/pyth-physical-log-verification.json
```

When the evidence terminal is the capture path, include:

```powershell
  --evidence-terminal --evidence-terminal-drop-count 0
```

The verifier prints `PYTH_PHYSICAL_LOG_VERIFY_OK` only after the log matches the
manifest package hash/digest, required marker order, backend selection marker,
runtime exit status, forbidden-marker absence, raw-log SHA-256 recording, and
the evidence-terminal zero-drop rule when requested.

## Claim Boundary

A passing record means only:

```text
The named package bytes reached the accepted PythTIG runtime markers on the
named machine/controller under the named backend.
```

It does not claim generic SDHCI/eMMC support, NVMe support, broad Intel/AMD PC
support, Apple support, filesystem support, package management, networking,
SMP, or AI inside the trusted core.
