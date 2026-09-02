# PythOS

PythOS is a from-scratch, verification-driven x86-64 operating system. It
boots through UEFI, runs a capability-controlled ring-3 object shell, persists
typed and versioned objects across QEMU reboots, includes polling AHCI and
SDHCI/eMMC block backends verified in QEMU, and carries the accepted PythTIG
version 1 graph-package direction through Phase 7 cutover/cross-target
evidence.

Current `main` is stopped at the Phase 13 -> Phase 13.5 boundary. Phase 12
`path-vs-graph-decision` is recorded by ADR 0069: PythOS uses a
capability-scoped object locator namespace, not POSIX paths. ADR 0070 records
the internal `object-locator 0.1` resolver ABI, ADR 0071 records the finite
loader read-bound extension needed for the debug acceptance image, and ADR 0072
records the path adversarial suite through `PYTHOS:CORE:PHASE_12_COMPLETE`.
ADR 0073 records the local package lifecycle and schema-extensibility proof
through `PYTHOS:CORE:PHASE_13_COMPLETE`. The accompanying semantic-checkpoint
contract records how later parallel evidence lanes must compare artifact
digests, normalized markers, object graph state, capability transcripts,
denials, and storage/locator state before merge.

The SDHCI/eMMC backend and evidence terminal have target-specific physical
evidence on the disposable O2 Micro `1217:8620` laptop. The 2026-08-08 physical
capture shows five terminal pages with `count 00000139` (hexadecimal, 313
decimal markers), `drop 00000000`, and CRC `176F4C6E`; two separate physical
boots reproduced the same count, zero-drop state, and CRC. This is scoped
physical evidence, not a generic hardware-support claim.

ADR 0074 adds an opt-in physical wake diagnostic. It is QEMU-accepted through
`scripts/test-physical-wake-diagnostic.py` and has one operator-reported
physical acceptance on the current USB boot machine: after the wake screen, the
diagnostic accepted `wake` plus Enter from the physical keyboard. This proves
only that diagnostic polling path on that machine, not USB HID, trackpad input,
IRQ-driven input, shell keyboard control, or generic PC input support.

ADR 0075 adds the next opt-in QEMU-accepted physical input event diagnostic.
`scripts/test-physical-input-event-diagnostic.py` injects `space space
backspace backspace wake enter`, requires raw-byte logs plus normalized key
markers, and accepts only that fixed sequence. The same sequence is now
operator-accepted on the current USB boot target with framebuffer photo
evidence. This remains a diagnostic claim, not USB HID, trackpad input,
IRQ-driven input, shell keyboard control, or generic PC input support.

ADR 0076 adds opt-in QEMU-accepted physical keyboard ingress into the existing
ring-3 object-shell console syscall. With `physical-keyboard-console`, normal
boot keeps COM2 as the primary shell transport, then falls back to bounded
i8042 keyboard polling for letters, digits, Space, Enter, and Backspace. The
QEMU harness `scripts/test-physical-keyboard-console.py` types `help` through
QMP keyboard events and verifies the shell response over COM2. Physical shell
input on hardware is still pending.

ADR 0077 adds an opt-in normal boot hardware diagnostic after the first
hardware boot of the merged ADR 0076 image reached a plain white screen. With
`normal-boot-diagnostic`, PythCore renders visible `stage NN` breadcrumbs at
normal boot boundaries while preserving the default boot path. Its QEMU harness
`scripts/test-normal-boot-diagnostic.py` still clicks the launcher and verifies
`help` over the shell console. The first target boot of this diagnostic reached
`stage 19` / `init error`; the refined target boot reached `stage 19` /
`init block dev`. That proves normal boot is failing inside block-device
initialization on the current hardware target. The no-write `hardware-probe`
follow-up then identified the current target storage path as O2 Micro
`1217:8620` SDHCI/eMMC at BDF `01:00.0`, class/subclass/prog-if `08 05 01`,
BAR0 `0x00000000E8B01000`, and rendered read-only LBA0 evidence
(`csum 000006F9`, `bytes 0000000C`, `no disk writes`). The follow-up QEMU
harness `scripts/test-normal-boot-diagnostic-sdhci-emmc.py` now verifies normal
boot diagnostic plus `sdhci-emmc-backend`: it requires
`DEVICE_SELECTED_SDHCI_EMMC`, reaches ring 3, and verifies `help` through the
shell console. That feature set was written to the verified `P:` USB ESP target
on 2026-08-28 with source-to-target hash readback. A 2026-08-29 physical photo
shows the deployed image reached `stage 37` / `ring3 enter` while the launcher
tile remained visible as retained framebuffer content. That is photo-backed
physical ring-3 handoff evidence for the deployed normal SDHCI/eMMC candidate,
not trackpad input, visible shell I/O, or durable eMMC-persistence proof.

ADR 0078 adds an opt-in no-write USB/xHCI register probe for the next pointer
layer. Linux reconnaissance on the current target identified the external
Dell/PixArt USB mouse (`413c:301a`) behind AMD xHCI `1022:7914` at `00:10.0`,
BAR0 `0x00000000E8C68000`, while the built-in trackpad is I2C HID
`ELAN0666:00 04F3:304B`, not the existing PS/2 path. The QEMU harness
`scripts/test-usb-xhci-probe.py` attaches `qemu-xhci`, disables ordinary block
devices, requires explicit xHCI BAR0 mapping into PythCore page tables, reads
the xHCI register header, renders a framebuffer identity panel, and requires
`NO_DISK_WRITES`. The QEMU-accepted candidate was copied to the verified `P:`
USB ESP target on 2026-08-30 without formatting; source-to-target readback
reported `USB_XHCI_PROBE_VERIFY_OK files:8 bytes:3814456`, with deployed core
SHA-256 `479588E4268C65E6F03EECAEF0534D7D5F4ADEEF9EBA0B1DD50D3549BF67D0AA`.
On 2026-08-31, a physical photo from the target shows `xhci regs`,
`no disk writes`, BDF `00 10 00`, vendor/device `1022 7914`,
class/subclass/prog-if `0C 03 30`, BAR0 `00000000E8C68000`, CAPLENGTH `20`,
HCIVERSION `0100`, HCSPARAMS1 `08000820`, HCCPARAMS1 `014040C3`, and USBSTS
`00000009`. This proves physical xHCI register reachability for the target
controller, not USB enumeration, HID parsing, mouse movement, trackpad support,
IRQ input, or DMA rings.

ADR 0079 adds the next opt-in no-write USB/xHCI port-status probe. The
`usb-xhci-port-probe` feature depends on `usb-xhci-probe`, keeps ADR 0078's
header-register behavior intact, and additionally reads max-port, xECP,
USB-legacy ownership, and up to eight `PORTSC`/`PORTPMSC` pairs. Its QEMU
harness `scripts/test-usb-xhci-port-probe.py` attaches a USB mouse to
`qemu-xhci`, requires `XHCI_PORT_STATUS_READY`, and still requires
`NO_DISK_WRITES`. QEMU showed eight ports, port register base `0x440`, xECP
byte offset `0x20`, no legacy-support capability, and a changed port 5
`PORTSC` value `0x00000E03` for the attached mouse. The candidate was copied to
the verified `P:` USB ESP target on 2026-08-31 without formatting or a delete
pass; source-to-target readback reported
`USB_XHCI_PORT_VERIFY_OK files:8 bytes:3840440`, with deployed core SHA-256
`447D1F9CA8D97F8000F0905566628AE3B959C212E4E2B33C558220E436D94320`. This is
QEMU and deployment evidence for the port-observation layer, not physical
port-status proof, enumeration, HID parsing, or mouse movement.

ADR 0080 adds `usb-xhci-swap-probe` for the single-port physical test case. It
boots with no mouse required, renders `swap mouse now`, then polls the same
read-only xHCI port-status registers after the boot USB can be removed. The
first physical attempt showed that a plain "first changed port" rule stopped on
the boot-USB detach before the mouse was inserted. The corrected probe now
ignores and rebases on non-connect changes, then finishes only on a
disconnected-to-connected port transition. The QEMU harness
`scripts/test-usb-xhci-swap-probe.py` now simulates that sequence by removing a
USB storage device after `SWAP_READY`, waiting for
`XHCI_SWAP_POLL_IGNORED_CHANGE`, then QMP-hotplugging a USB mouse. QEMU
observed the ignored detach on port 1 and the later mouse connect on port 5
from `PORTSC 0x000002A0` to `0x00020EE1`, then emitted
`USB_XHCI_SWAP_PROBE_TEST_OK`. The corrected candidate was copied to the
verified `P:` USB ESP target without formatting or a delete pass;
source-to-target readback reported
`USB_XHCI_SWAP_CONNECT_VERIFY_OK files:8 bytes:3857656`, with deployed core
SHA-256
`A11B480D37A0C0299B4D6D96080C506C533BC0D1E3492CE1876C3F4F1A269BFE`. A
physical video still from the corrected image shows the diagnostic kept polling
after boot-USB detach and later rendered `xhci swap`, `chg p5`, `was sc 000002A0`,
and `now sc 000202E1` after the external mouse was inserted; the still's SHA-256 is
`20B2CCA74EB8FD23080943FB368147F3D56A884A09F753479A9E4D5FF9A038E8`. This is
QEMU hotplug, deployment, and physical port-connect evidence for a
swap-friendly port-observation diagnostic, not physical mouse input support.

ADR 0081 adds `usb-xhci-command-probe`, the first opt-in write/DMA xHCI driver
diagnostic. It depends on the swap-port observation layer, maps enough xHCI
MMIO for operational/runtime/doorbell registers, resets and starts the
controller, configures static page-aligned DCBAA/scratchpad/command-ring/
event-ring/ERST buffers, resets the selected connected root port, submits a
No-op Command, then submits Enable Slot and records the returned slot id. The
QEMU harness `scripts/test-usb-xhci-command-probe.py` simulates boot-USB
detach plus later mouse hotplug and requires `XHCI_COMMAND_RING_READY`,
`XHCI_EVENT_RING_READY`, `XHCI_NOOP_COMMAND_COMPLETE`,
`XHCI_ENABLE_SLOT_READY`, `NO_DISK_WRITES`, and `QEMU_OUTCOME success`; the
accepted QEMU run returned completion code `1` for both commands and slot id
`1` with `XHCI_SCRATCHPAD_COUNT=0`. The first physical command-ring attempt
reached `xhci cmd err` with `err 00000006`, mapped to unsupported scratchpad
buffers. The scratchpad-enabled image supports a static 32-buffer scratchpad
pool and was copied to the verified `P:` USB ESP target without formatting or
deleting preserved root files; source-to-target readback reported
`USB_XHCI_SCRATCHPAD_VERIFY_OK files:8 bytes:3949296`, with deployed core
SHA-256
`5E65C5A697A443369CB9AAC11E4AADAB7A26888B920EC89F43BEC5F33CF8CC44`. On
2026-09-01, the target then rendered the expected `xhci cmd` success panel:
BDF `00 10 00`, vendor/device `1022 7914`, port `06`, slot `01`, No-op
completion code `01`, Enable Slot completion code `01`, `USBSTS 00000000`,
`PORTSC 00220603`, scratchpad count `08`, and `no disk writes`. The photo
SHA-256 is
`534B40C205D3BC4FE43F8BF0CBF6D0EFA0687E7F19E247BE08C94F818664AC52`. This is
QEMU plus photo-backed physical command-ring acceptance, not USB addressing,
descriptor reads, HID parsing, interrupts, endpoint polling, cursor movement,
or trackpad support.

ADR 0082 adds `usb-xhci-address-probe`, the next opt-in xHCI driver diagnostic.
It depends on the command-ring path, prepares xHCI input/output contexts and an
endpoint-0 transfer ring, supports both 32-byte and 64-byte context layouts,
selects the default-control max-packet size from the reset port speed, issues
one Address Device command, and reports completion code, assigned address, slot
state, EP0 state, port speed, context size, and `NO_DISK_WRITES`. The QEMU
harness `scripts/test-usb-xhci-address-probe.py` returned Address Device
completion code `1`, device address `1`, slot state `2`, EP0 state `1`, context
size `32`, and max packet size `64`, then emitted
`USB_XHCI_ADDRESS_PROBE_TEST_OK` / `QEMU_OUTCOME success`. The image was copied
to the verified `P:` Lexar D70E USB ESP target without formatting or deleting
preserved root files; readback reported
`USB_XHCI_ADDRESS_VERIFY_OK files:8 bytes:3977256`, with deployed core SHA-256
`E666859BFEE4FE6162690F3D8860E24992492441F859AEE6C8F4FC14DDBC3D53`. On
2026-09-01, the physical target rendered `xhci addr`, `no disk writes`, BDF
`00 10 00`, vendor/device `1022 7914`, port `05`, slot `01`, No-op completion
code `01`, Enable Slot completion code `01`, Address Device completion code
`01`, device address `01`, slot state `02`, EP0 state `01`, speed `02`,
context size `32`, max packet size `0008`, `PORTSC 00220A03`, and scratchpad
count `08`. The photo SHA-256 is
`8A4D2D6D8F74AEE88D2B535F4447CBDC338E590944F0D8130E7B6FD6476A6D5A`. This is
QEMU plus photo-backed physical USB Address Device acceptance. HID parsing,
endpoint polling, cursor movement, and trackpad support remain pending.

ADR 0083 adds `usb-xhci-descriptor-probe`, the next opt-in xHCI diagnostic. It
depends on the Address Device path, queues one EP0 `GET_DESCRIPTOR(Device)`
control transfer, reads back the 18-byte device descriptor, and renders the
descriptor completion code plus parsed fields while preserving `NO_DISK_WRITES`.
The QEMU harness `scripts/test-usb-xhci-descriptor-probe.py` passed with
descriptor completion code `1`, descriptor length `18`, type `1`, USB BCD
`0200`, class/subclass/protocol `00 00 00`, MPS0 `64`, VID/PID `0627:0001`,
configuration count `1`, `USB_XHCI_DESCRIPTOR_PROBE_TEST_OK`, and
`QEMU_OUTCOME success`. The descriptor image was copied to the verified `P:`
Lexar D70E USB ESP target without formatting or deleting preserved report
files; readback reported `USB_XHCI_DESCRIPTOR_VERIFY_OK files:8 bytes:3996680`,
with deployed core SHA-256
`CFE2381F38DA91E11A31F021B315B4B12C030DB3F296C8B591BA6DF0A5289924`. The latest
Linux Mint field-kit run, archived as
`docs/evidence/2026-09-01-linux-mint-usb-mouse-map.tar.gz`, confirmed the
target Dell/PixArt mouse descriptor expected on physical boot: VID/PID
`413c:301a`, USB BCD `0200`, device BCD `0100`, MPS0 `8`, configuration count
`1`, HID boot mouse interface `03/01/02`, interrupt IN endpoint `0x81`, max
packet size `4`, interval `10`. The physical descriptor boot is now
photo-backed: the preserved frame
`docs/evidence/2026-09-01-physical-usb-xhci-device-descriptor-success.png`
shows `xhci desc`, `no disk writes`, BDF `00 10 00`, vendor/device
`1022 7914`, port `05`, slot `01`, Address Device CC `01`, descriptor CC
`01`, length `12`, type `01`, USB BCD `0200`, device BCD `0100`,
class/subclass/protocol `00 00 00`, MPS0 `008`, configuration count `01`,
VID/PID `413C 301A`, and scratchpad count `08`; SHA-256
`4204994560727C63A8F631A05CCECFA68C3FC20189E12A2834E621327FDA61B6`. This is
QEMU descriptor evidence, USB deployment evidence, Linux target mapping, and
photo-backed physical descriptor acceptance. Configuration descriptor reads,
HID parsing, endpoint polling, cursor movement, and trackpad support remain
pending.

ADR 0084 adds `usb-xhci-configuration-probe`, an opt-in bounded extension.
It reads the fixed nine-byte configuration header, validates `wTotalLength`
against a 256-byte cap, then reads exactly that bounded length and walks the
standard configuration, interface, and endpoint descriptors. The accepted
QEMU run reported total length `34`, configuration value `1`, one interface,
HID boot-mouse class/subclass/protocol `03/01/02`, interrupt-IN endpoint
`0x81`, attributes `0x03`, max packet size `4`, interval `7`,
`USB_XHCI_CONFIGURATION_PROBE_TEST_OK`, `QEMU_OUTCOME success`, and
`NO_DISK_WRITES`. The feature does not send `SET_CONFIGURATION`, configure a
non-control endpoint, parse or poll HID reports, move a cursor, or touch
storage. The QEMU-accepted image was deployed to the verified Lexar USB and
booted on the Lenovo `81VS`. The first photo shows the frozen initial snapshot
and `swap mouse now`; after boot-USB removal and mouse insertion, the second
shows a new connect on port `06` followed by `xhci cfg err` / `0x0F`. That
proves the intended handoff and physical controller path reached a finite wait,
but the shared timeout code did not identify which command or transfer stalled.
The follow-up keeps the same flow and adds exact timeout identities `0x2C`
through `0x31` plus a readable stage line. That refreshed image is now deployed
to the same re-identified Lexar D70E: 8 source files / 4,042,520 bytes matched
on readback, 111 unrelated files stayed byte-identical, and the deployed core
SHA-256 is `325A4F142282BDA353178110A74D97FEF20ADF78A0117EA1D3BAA44366990A11`.
A refreshed Lenovo boot is still required before claiming a physical
configuration-descriptor read.

The physical target also has Linux Mint on its eMMC. Use
`scripts/linux-usb-mouse-map.sh` as the Mint-side field kit for the next input
work: stage it locally with `stage-local`, collect USB mouse and trackpad paths
from Linux, and copy the generated tarball back to the PythOS USB without
formatting or deploying boot files. The staged USB includes a root
`START-HERE-LINUX-MINT.txt` with literal copy-paste commands. See
[Linux Mint field kit](docs/linux-mint-field-kit.md).

Start with [docs/TECHNICAL-OVERVIEW.md](docs/TECHNICAL-OVERVIEW.md) for the
current external-facing account of what the repository proves, how those claims
are verified, and what is not yet claimed.

Current-state references:

- [Technical overview](docs/TECHNICAL-OVERVIEW.md)
- [Handover](docs/HANDOVER.md)
- [Phase 12 roadmap](docs/ROADMAP-LATER-PHASES.md)
- [ADR 0069: object locator namespace and semantic checkpoints](docs/decisions/0069-phase-12-object-locator-and-semantic-checkpoints.md)
- [ADR 0070: object locator resolution ABI](docs/decisions/0070-phase-12-object-locator-resolution-abi.md)
- [ADR 0072: path adversarial suite](docs/decisions/0072-phase-12-path-adversarial-suite.md)
- [ADR 0073: package lifecycle and schema extensibility](docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md)
- [ADR 0074: physical wake diagnostic gate](docs/decisions/0074-physical-wake-diagnostic.md)
- [ADR 0075: physical input event diagnostic](docs/decisions/0075-physical-input-event-diagnostic.md)
- [ADR 0076: physical keyboard console ingress](docs/decisions/0076-physical-keyboard-console-ingress.md)
- [ADR 0077: normal boot hardware diagnostic](docs/decisions/0077-normal-boot-hardware-diagnostic.md)
- [ADR 0078: USB xHCI register probe](docs/decisions/0078-usb-xhci-register-probe.md)
- [ADR 0079: USB xHCI port status probe](docs/decisions/0079-usb-xhci-port-status-probe.md)
- [ADR 0080: USB xHCI swap port probe](docs/decisions/0080-usb-xhci-swap-port-probe.md)
- [ADR 0081: USB xHCI command ring driver diagnostic](docs/decisions/0081-usb-xhci-command-ring-driver.md)
- [ADR 0082: USB xHCI Address Device probe](docs/decisions/0082-usb-xhci-address-device-probe.md)
- [ADR 0083: USB xHCI Device Descriptor probe](docs/decisions/0083-usb-xhci-device-descriptor-probe.md)
- [ADR 0084: USB xHCI Configuration Descriptor probe](docs/decisions/0084-usb-xhci-configuration-descriptor-probe.md)
- [Linux Mint field kit](docs/linux-mint-field-kit.md)
- [Semantic checkpoint contract](docs/semantic-checkpoint-contract.md)
- [PythTIG acceptance](docs/pyth-tig/ACCEPTANCE.md)

Physical evidence records:

- [Phase 10 physical SDHCI/eMMC backend](docs/milestones/2026-08-01-physical-emmc-phase10.md)
- [Milestone 1 physical evidence terminal validation](docs/evidence/2026-08-08-physical-evidence-terminal.md)
- [ADR 0074 physical wake diagnostic gate](docs/decisions/0074-physical-wake-diagnostic.md)
- [ADR 0075 physical input event diagnostic](docs/decisions/0075-physical-input-event-diagnostic.md)
- [2026-08-28 no-write hardware-probe SDHCI/eMMC read frame](docs/evidence/2026-08-28-hardware-probe-o2micro-emmc-read.jpg)
- [2026-08-29 normal SDHCI/eMMC ring-3 handoff frame](docs/evidence/2026-08-29-normal-sdhci-ring3-enter.jpg)
- [2026-08-31 physical USB/xHCI register probe frame](docs/evidence/2026-08-31-physical-usb-xhci-register-probe.png)
- [2026-08-31 physical USB/xHCI swap-port connect frame](docs/evidence/2026-08-31-physical-usb-xhci-swap-port.jpg)
- [2026-09-01 physical USB/xHCI command scratchpad error frame](docs/evidence/2026-09-01-physical-usb-xhci-command-scratchpad-error.png)
- [2026-09-01 physical USB/xHCI command-ring success frame](docs/evidence/2026-09-01-physical-usb-xhci-command-ring-success.png)
- [2026-09-01 physical USB/xHCI Address Device success frame](docs/evidence/2026-09-01-physical-usb-xhci-address-device-success.png)
- [2026-09-01 physical USB/xHCI Device Descriptor success frame](docs/evidence/2026-09-01-physical-usb-xhci-device-descriptor-success.png)
- [2026-09-02 physical ADR 0084 swap-ready frame](docs/evidence/2026-09-02-physical-usb-xhci-configuration-swap-ready.jpg)
- [2026-09-02 physical ADR 0084 generic-timeout frame](docs/evidence/2026-09-02-physical-usb-xhci-configuration-timeout.jpg)
- [2026-09-01 Linux Mint USB mouse map archive](docs/evidence/2026-09-01-linux-mint-usb-mouse-map.tar.gz)
- Current USB/xHCI deployment state is tracked in
  `D:\PythOS-Workspace\CURRENT-STATE.md`.

Public milestone site:
[https://craigcoda.github.io/pythos/](https://craigcoda.github.io/pythos/)

Milestone release:
[PythOS Milestone 1: Physical Persistent Object Storage](https://github.com/craigCODA/pythos/releases/tag/milestone-1-physical-storage).
