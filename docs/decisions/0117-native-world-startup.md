# ADR 0117 — Native world startup on the WD and USB keyboard target

## First boot

The hardware world ISO composes two opt-in profiles with the identity-bound
NVMe profile from [ADR 0116](0116-identity-bound-nvme-world.md).

`world-session-nvme-initialize` checks the five reserved 4 KiB pages after
controller serial, namespace, and both GPT tables match the target. It opens
an existing valid world directly. For a fresh store it verifies that the five
pages are blank, writes and verifies the four checkpoint pages, then publishes
the identity header last. Each of these writes is flushed through the NVMe
controller. A mismatch or unexpected contents stop startup with a visible code.

`world-session-nvme-usb` uses the 2.4G Keyboard Mouse receiver
`062A:261B` found on the AMD xHCI controller `1022:15E0`, function 3, Windows
subsystem ID `SUBSYS_8619103C`, direct root port 3. It claims firmware
ownership, configures the matched device's HID boot-keyboard interface, and
requests boot reports. The kernel polls one in-flight transfer using its
existing timer wakeup. Completed reports become normalized key-down events
in the current world input queue. Releases rearm a key for its next press.
The input controller is quiesced before the session exits. [USB HID 1.11](https://www.usb.org/sites/default/files/hid1_11.pdf)
[xHCI specification](https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf)

The world opens from its saved arrival pose or starts as the blank half-acre
space. Its controls and saved records follow [ADR 0114](0114-native-editable-world.md).
The hardware acceptance sequence is to walk, place a prop and anchor, bind
the prop, use it, save the arrival pose, then restart from the same ISO and
verify the same world and controls.

## Acceptance

```powershell
python -B scripts/test-nvme-initialize.py
python -B scripts/test-nvme-world-usb.py
```

The first test proves initialization, successful retry after an injected
header-write failure, and restoration without reinitializing. The combined
test boots a disposable blank GPT image with an emulated xHCI keyboard
and the emulated PS/2 controller disabled. It checks repeated hardware-ring
wraps, movement, edits and binding, a saved arrival pose, and restoration
after process interruption. It compares all disk bytes outside the five
reserved pages and checks the disk again after a read-only restart.

The local hardware ISO build uses the captured WD and partition identity
with `scripts/build-nvme-target.py --boot-world`. Physical acceptance
requires running that image on the PC and performing the controls and
restart checks there.
