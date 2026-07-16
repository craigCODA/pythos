# ADR 0020: Phase 6 AC97 Audio Target

## Status

Accepted

## Context

Phase 6 needs one QEMU-supported audio target before any native audio code is
added. The local QEMU binary advertises `AC97`, `intel-hda`, `ich9-intel-hda`,
`sb16`, `ES1370`, `usb-audio`, and `virtio-sound-pci` sound devices.

Phase 6 must remain a bounded boot-sequence milestone. It must not introduce
persistent storage, user-configurable themes, broad hardware discovery, or a
general driver model.

## Decision

Use QEMU's PCI `AC97` device, backed by the `none` audio backend for ordinary
serial-oracle acceptance tests and by the `wav` backend only when an explicit
audio capture is requested.

PythCore will perform a bounded primary-bus PCI scan for the AC97 multimedia
audio function, validate the mixer and bus-master I/O BARs, enable I/O space
and bus mastering for that function, and treat absence as a nonfatal condition
handled by the Phase 6 fallback path.

## Consequences

The native Phase 6 audio path can use fixed I/O registers and deterministic
PCM buffers without adding a generic PCI subsystem. The QEMU runner owns
whether the AC97 device exists for a boot. The serial marker chain remains the
acceptance oracle; optional WAV capture is supporting evidence, not the boot
oracle.

This ADR does not add support for HDA, Sound Blaster, USB audio, virtio-sound,
physical audio hardware, or runtime device hotplug.
