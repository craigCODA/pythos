# ADR 0077: Normal Boot Hardware Diagnostic

Date: 2026-08-28
Status: Accepted

## Context

ADR 0076 added opt-in physical keyboard ingress into the existing object-shell
console syscall and passed QEMU acceptance. The first hardware boot of the
merged ADR 0076 image reached a plain white screen instead of the cinematic,
launcher, or shell.

Plain white matches PythCore's early post-IDT framebuffer progress color. That
evidence narrows the failure to after early PythCore handoff, but without
serial capture it does not identify whether normal substrate setup, block-device
selection, object/package restore, service admission, launcher rendering, PS/2
initialization, or shell entry is the blocking layer.

## Decision

Add an opt-in `normal-boot-diagnostic` Cargo feature for normal boot. The
feature is mutually exclusive with `verify` and `hardware-probe`. When enabled,
PythCore writes serial markers and renders a small framebuffer panel at normal
boot boundaries. The panel displays:

```text
normal boot diag
stage NN
stage label
if stuck send photo
```

The diagnostic records stages from post-IDT entry through normal substrate
construction, block-device selection, object/package restore, default service
admission, COM2 initialization, cinematic launch, launcher wait, physical
keyboard-console setup, bootstrap construction, and the final ring-3 entry
handoff.

The first target boot of the diagnostic image reached `stage 19` /
`init error`. Stage 19 is the normal-init error bucket, not a root-cause label,
so the diagnostic now preserves the returned `NormalInitError` variant and
renders a cause-specific stage-19 label such as `init block dev`,
`init memory`, or `init shell map`. It also emits a matching
`PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:*` serial marker for QEMU and any
future serial-capable target run.

The refined target boot reached `stage 19` / `init block dev`. That identifies
the failing normal-init layer as block-device initialization. The follow-up
artifact for this target is the existing no-write `hardware-probe` image, which
halts after PCI/storage discovery and renders the detected storage controller
identity on the framebuffer. It is used to determine whether the normal image
failed because no supported backend was present, the controller was behind a
bridge, a BAR was unusable, or an unsupported backend such as NVMe, VMD, RAID,
or SDHCI/eMMC was the only available storage path.

The 2026-08-28 no-write hardware-probe boot on the current target rendered
`emmc read`, `no disk writes`, count `0000000000000002`, BDF `01 00 00`,
vendor/device `1217 8620`, class/subclass/prog-if `08 05 01`, BAR0
`00000000E8B01000`, OCR `C0FF8080`, LBA0 `00000000`, first word `00000000`,
checksum `000006F9`, and `bytes 0000000C`. This proves read-only SDHCI/eMMC
controller/card access on that target and explains why the normal diagnostic
image built without `sdhci-emmc-backend` stopped at block-device initialization.

`scripts/test-normal-boot-diagnostic.py` is the QEMU acceptance harness. It
builds a normal boot image with `physical-keyboard-console,normal-boot-diagnostic`,
uses a fresh disposable store image, requires the diagnostic markers in order,
clicks the launcher through QMP, types `help`, and verifies the object-shell
help output over COM2.

`scripts/test-normal-boot-diagnostic-sdhci-emmc.py` is the follow-up QEMU
acceptance harness for the next physical candidate. It builds with
`physical-keyboard-console,normal-boot-diagnostic,sdhci-emmc-backend`, boots
from ISO so the QEMU ESP boot medium is not selected as an AHCI disk, disables
virtio block, attaches a disposable eMMC image behind SDHCI, requires
`DEVICE_SELECTED_SDHCI_EMMC`, rejects virtio/AHCI fallback markers, clicks the
launcher, types `help`, and verifies shell output over COM2.

After operator approval on 2026-08-28, that candidate was written to the
verified `P:` USB ESP target without formatting. Source-to-target readback
reported `USB_NORMAL_SDHCI_VERIFY_OK files:8 bytes:8230744`; the deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 was
`1432589585001622FF623D8F7751D227AE3011561457C1173C411A07840EDBF8`.

On 2026-08-29, a physical photo of that deployed candidate showed
`normal boot diag`, `stage 37`, `ring3 enter`, and `if stuck send photo`, with
the `Enter Shell` launcher tile still visible. In this diagnostic map,
stage 37 is `NormalBootDiagnosticStage::Ring3Enter`. The retained launcher tile
is expected framebuffer content from before the ring-3 handoff; it is not
evidence of physical trackpad input, a framebuffer terminal, or shell output on
the laptop display.

## Consequences

Default builds are unchanged. The diagnostic feature is not a fix for the
hardware white screen and does not widen the physical keyboard-console claim.
It creates a bounded evidence image for serial-less physical hardware: the last
visible `stage NN` and label identify the last reached normal boot boundary or
the specific normal-init error variant. A block-device failure must be followed
by controller identity evidence before selecting or changing any storage
backend.

This does not prove physical shell input, USB HID, trackpad input, IRQ-driven
input, punctuation/modifier layout, or broad hardware support. Physical
acceptance still requires the diagnostic or production image to reach the shell
on the current hardware target and accept a bounded command from the physical
keyboard. The no-write probe does not prove physical normal boot writes or
object/package persistence over the target eMMC. The stage-37 photo proves a
physical ring-3 handoff for this deployed candidate, but not durable persistence
across a second physical boot or physical shell input.
