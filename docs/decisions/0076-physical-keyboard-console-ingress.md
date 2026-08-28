# ADR 0076: Physical Keyboard Console Ingress

Date: 2026-08-27
Status: Accepted

## Context

ADR 0075 proved that the current physical USB boot target can deliver raw
keyboard bytes to an opt-in verify diagnostic, and that PythCore can normalize
the fixed `space space backspace backspace wake enter` sequence into key
events. That still did not let the ring-3 object shell receive physical
keyboard input. The shell's existing interactive path is COM2, exposed through
the capability-gated console syscall.

The next bounded question is whether PythCore can keep owning i8042 port reads
while feeding a small physical keyboard byte stream through the existing shell
console syscall. This must not grant ring-3 direct I/O port access, replace
COM2, or claim USB HID, trackpad, framebuffer terminal, modifier layout, or
generic keyboard support.

## Decision

Add an opt-in `physical-keyboard-console` Cargo feature for normal boot. When
enabled, normal boot keeps the existing launcher flow, then after the launcher
click reinitializes only PS/2 controller port 1 for keyboard polling. PythCore
reads the i8042 status/command port `0x64` and data port `0x60`; shell code
continues to call `SYSCALL_CONSOLE_READ_BYTE`.

`SYSCALL_CONSOLE_READ_BYTE` preserves COM2 priority. It returns a waiting COM2
byte first, then falls back to one translated physical keyboard byte. The
physical decoder accepts the existing bounded `KeyCode` surface: `A` through
`Z`, digits `0` through `9`, `Space`, `Enter`, and `Backspace`. It ignores key
release bytes and accepts translated scancode-set-1 bytes plus the equivalent
small scancode-set-2 set. The user shell line editor now handles ASCII
Backspace (`0x08`) and Delete (`0x7F`) by deleting the previous line byte.

New serial markers are feature-only:

```text
PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:READY
PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:BYTE
PYTHOS:CORE:PHYSICAL_KEYBOARD_CONSOLE:PS2_INIT_FAILED
```

`scripts/test-physical-keyboard-console.py` is the QEMU acceptance harness. It
builds a normal boot image with `physical-keyboard-console`, connects COM2
before waiting on COM1 markers, clicks through the existing launcher over QMP,
types `help` through QMP keyboard events, and requires the normal object-shell
help output over COM2.

## Consequences

Default builds are unchanged. COM2 remains the primary interactive shell
transport and the automated transcript surface. Ring-3 code still has no direct
I/O port privilege.

This proves QEMU physical-keyboard ingress into the existing object-shell
console syscall for a small shell command. It does not prove physical shell use
on the current USB boot target until the feature image is copied and accepted
on hardware. It also does not prove USB HID, trackpad input, IRQ-driven input,
a framebuffer terminal, punctuation/modifier layout, or general keyboard
support.
