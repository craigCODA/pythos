# Physical Wake Diagnostic Plan

## Goal

Build an opt-in hardware diagnostic image that preserves the current
QEMU-accepted verify boot through the Phase 6 wake screen, then tests whether a
physical keyboard can provide enough raw input to accept exact `wake` plus
Enter.

## Scope

- Add a `verify`-only `physical-wake-diagnostic` feature.
- Pause immediately after `PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY`.
- Use keyboard-only PS/2 polling with IRQs disabled.
- Render a framebuffer panel with typed input, raw byte history, and status.
- Add QEMU acceptance using QMP key injection.
- Do not add USB HID, trackpad support, shell input, login/auth, storage
  expansion, or generic hardware claims.

## Tasks

- [x] Confirm current USB image reaches the physical wake screen.
- [x] Add failing host tests for exact `wake` recognition.
- [x] Implement the feature-gated polling and framebuffer diagnostic.
- [x] Run focused unit tests.
- [x] Run default milestone QEMU acceptance without the feature.
- [x] Run diagnostic QEMU acceptance with QMP `wake` input.
- [x] Copy the accepted diagnostic ESP to `P:`.

## Verification

```text
cargo test -p pythos-core physical_wake
10 passed

cargo test -p pythos-core
578 passed

python scripts\test-boot.py --slice milestone-1
BOOT_TEST_OK
QEMU_OUTCOME success

python scripts\test-physical-wake-diagnostic.py
PHYSICAL_WAKE_DIAGNOSTIC_TEST_OK
QEMU_OUTCOME success
```

The final `P:` copy was hash-verified against `image\esp` for
`BOOTX64.EFI`, `NvVars`, `BOOT.CFG`, `FONT.PSF`, `INIT.PAK`, and
`PYTHCORE.ELF`.
