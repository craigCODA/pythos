# Post-Phase 8 Verification Checks

This note records follow-up checks performed after Phase 8 closed. It is not a
new phase and does not change the active implementation scope.

## Preemption Flake Stress

Command:

```powershell
for ($i = 1; $i -le 10; $i++) {
  python scripts\test-boot.py --slice preemption --timeout 60
}
```

Result:

```text
PREEMPTION_STRESS_PASSES 10
```

Each run reported both `QEMU_OUTCOME success` and `BOOT_TEST_OK`. This closes
the open question about the earlier preemption marker-ordering race moving to a
different marker under repeated execution.

## VMware Boot Check

The current ISO was cold-booted under a local VMware Workstation VM with EFI
firmware and COM1 serial capture enabled. The VM was powered off before the
check; an old suspended checkpoint was preserved separately and cleared so the
run used a fresh ISO boot rather than resuming old VM state.

Observed result:

```text
PYTHOS:LOADER:ENTER
...
PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY
PYTHOS:CORE:PHASE_6_COMPLETE
PYTHOS:PANIC
```

The serial capture contained 145 `PYTHOS:*` markers and reached the Phase 6
completion boundary. It did not reach Phase 7 or Phase 8 completion under
VMware.

Interpretation: this is a real VMware cold-boot result, but not a full current
milestone pass. The current Phase 7+ image depends on the explicit QEMU legacy
`virtio-blk` storage target selected in the `block-device-driver` slice. VMware
does not provide that device in this VM, so the current full image remains a
QEMU acceptance target. VMware support beyond the Phase 6 boundary belongs to a
future hardware-expansion or alternate-block-device slice.
