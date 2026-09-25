# ADR 0108: Phase 15 Bounded PCI Memory-Space Enable Experiment

**Status:** Proposed for QEMU and explicitly gated physical implementation
**Date:** 2026-09-24
**Owners:** PythOS platform and transport-adapter maintainers

## Context

ADR 0107 completed the read-only register-reachability question. QEMU
`e1000`/`e1000e` registers were readable, while the Lenovo `10EC:C82F`
controller reported PCI command/status `0x00100000`: PCI Memory Space Enable
was clear, so the physical probe correctly skipped MMIO.

The next question is narrower than enabling or operating a network controller:
can PythOS temporarily set only the PCI Command register's Memory Space Enable
bit, verify the change, perform the already-approved one-register observation,
and restore the original command value without enabling bus mastering or
touching device registers before the read?

The PCI configuration layout places the Command register in the low 16 bits of
the dword at offset `0x04`; the Memory Space Enable bit is bit 1. The low-word
access is intentional: it avoids writing back the adjacent Status register's
upper 16 bits, whose device-specific write semantics must not be disturbed.
The PCI-SIG configuration-space layout and Linux PCI documentation are the
reference basis for this bounded operation:

- PCI-SIG configuration-space layout overview:
  <https://pcisig.com/sites/default/files/files/01_01_PCI_Express_Basics_%26_Background_FROZEN.pdf>
- Linux PCI driver guidance for enabling memory resources and configuration
  access: <https://docs.kernel.org/PCI/pci.html>

## Decision

Add a separate opt-in `network-hardware-register-enable-probe` profile. It
reuses ADR 0107 identity, BAR validation, mapping, and one-register read policy,
but adds one narrowly scoped PCI configuration transition only when Memory
Space Enable is initially clear:

```text
read original command/status dword
derive original low command word
write original low command word | MEMORY_SPACE_ENABLE using one 16-bit config write
read command/status and require MEMORY_SPACE_ENABLE
map the validated BAR window
perform the one fixed volatile 32-bit register read
write the original low command word using one 16-bit config write
read command/status and require the original command bits restored
```

If Memory Space Enable is already set, the profile performs no configuration
write and follows the existing read-only register path. The profile never sets
or preserves the PCI Bus Master Enable bit on behalf of this experiment. It
does not write BARs, device registers, queues, interrupt state, firmware state,
or network data.

The write is permitted only for the selected supported controller and only
after the existing target/BAR policy validates the intended 4 KiB window. A
failed write readback, invalid target, mapping failure, or restore readback is
a terminal diagnostic result; it is not register-reachability success.

The physical Lenovo run is a separate gated acceptance action. It must use the
new uniquely named ISO, capture the command-before/after/restored values and
the final screen, and stop at this experiment. It must not proceed to device
initialization, DMA, interrupts, queue setup, packet movement, or Wi-Fi
association.

## Evidence contract

The new profile emits an ordered transcript that distinguishes the temporary
PCI configuration transition from the MMIO observation:

```text
...:PCI_COMMAND_STATUS_ORIGINAL=...
...:PCI_MEMORY_SPACE_DISABLED
...:PCI_COMMAND_MSE_WRITE
...:PCI_COMMAND_STATUS_AFTER_ENABLE=...
...:PCI_MEMORY_SPACE_ENABLED
...:MMIO_MAPPED
...:REGISTER_READ_VALUE=...
...:PCI_COMMAND_MSE_RESTORE
...:PCI_COMMAND_STATUS_RESTORED=...
...:PCI_CONFIG_WRITE_SCOPED
...:REGISTER_REACHABILITY_READY
..._READY
```

The already-enabled path emits `PCI_MEMORY_SPACE_ALREADY_ENABLED` and
`PCI_CONFIG_WRITE_NOT_NEEDED` instead of the write/restore markers. Both paths
must end with a verified configuration state. Any failure emits a specific
failure marker and never emits the final ready marker.

## Acceptance

1. Pure tests prove that the derived command word sets only bit 1, never adds
   Bus Master Enable, and restores exactly the original low command word.
2. Static contract tests reject dword PCI writes, BAR writes, MMIO writes, bus
   mastering, DMA, interrupts, reset, queues, packets, sockets, NetworkPort,
   Wi-Fi association, and later Phase 15 markers.
3. QEMU `e1000` and `e1000e` pass the already-enabled branch with the fixed
   status read and no configuration write required; synthetic transcripts cover
   the disabled/write/restore branch and all failure ordering.
4. The physical Lenovo run is attempted only with this opt-in ISO and records
   the original command, post-enable command, register result or failure, and
   restored command. Any inability to verify restoration is a failed result.
5. Existing ADR 0107, identity, BAR, Phase 14, default, and normal-session
   profiles remain unchanged.

## Explicit non-goals and next boundary

This ADR does not create a generalized PCI configuration API, MMIO abstraction,
network driver, `VirtioTransport` implementation, NetworkPort consumer,
physical NIC support, Wi-Fi association, bus mastering, DMA, interrupts,
MSI/MSI-X, reset, firmware handling, multiqueue, offloads, queues, packet
movement, sockets, protocols, or persistent network state. Any device register
write, controller initialization, or network operation requires a later ADR.
