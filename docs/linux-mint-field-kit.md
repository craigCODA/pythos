# Linux Mint Field Kit

The physical target has Linux Mint on the eMMC. Use that Mint install as the
hardware-discovery environment for USB mouse, xHCI, trackpad, input, and block
path evidence. This avoids needing a separate live USB or RAM boot just to run
Linux-side checks.

## Boundary

The field kit is discovery-only by default:

- no disk formatting;
- no PythOS install to eMMC;
- no PCI or xHCI register writes;
- no boot-file deployment;
- report writes go under `~/pythos-field-kit-runs`;
- `copy-report` writes only a generated report tarball back to a mounted
  `PYTHOS_ESP` volume.

The current PythOS driver evidence still comes from QEMU serial evidence plus
physical PythOS framebuffer photos. Linux evidence identifies the real hardware
paths that the next PythOS driver slice should target.

## Returned Evidence

Latest accepted report:

- Run: `run-20260901-033724`
- Source on returned USB: `P:\pythos-field-kit-runs\run-20260901-033724.tar.gz`
- Archived in repo: `docs/evidence/2026-09-01-linux-mint-usb-mouse-map.tar.gz`
- SHA-256:
  `528058762026647AFA2D5FD94E6086128C6B4EDD88CE60880C573294AFA3006B`

Key results from that run:

- Linux Mint `22.3 (Zena)`, kernel `7.0.0-30-generic`.
- xHCI controller: AMD `1022:7914`, PCI `0000:00:10.0`, BAR0
  `0x00000000E8C68000`, driver `xhci_hcd`.
- USB mouse: Dell/PixArt `413c:301a`, path `/sys/bus/usb/devices/2-1`, low
  speed, device descriptor length `18`, descriptor type `1`, USB BCD `0200`,
  max packet size `8`, device BCD `0100`, manufacturer index `1`, product
  index `2`, serial index `0`, configuration count `1`.
- Mouse interface: HID boot mouse `03/01/02`, interrupt IN endpoint `0x81`,
  max packet size `4`, interval `10`, `hidraw1`, input event
  `/dev/input/event12`.
- Built-in trackpad: I2C HID `ELAN0666:00 04F3:304B`, ACPI path
  `\_SB_.I2CD.TPDD`, driver `hid-multitouch`, input events
  `/dev/input/event6` and `/dev/input/event7`.

The report's `verify-pythos-usb.txt` can say `PYTHOS_USB_MOUNT=not_found` for
the one-port mouse workflow, because the USB drive has to be unplugged before
the mouse is inserted. That is not a PythOS boot-payload verification failure
when the report was copied back manually afterward.

## One-Port Workflow

If the PythOS USB occupies the only useful external USB port:

1. Boot Linux Mint from the eMMC.
2. Insert and mount the PythOS USB.
3. Open `START-HERE-LINUX-MINT.txt` at the root of the PythOS USB, or stage
   the script into the Mint home directory with:

   ```bash
   bash -c 'for p in "/media/$USER/PYTHOS_ESP/PYTHOS-FIELD-KIT/run.sh" "/run/media/$USER/PYTHOS_ESP/PYTHOS-FIELD-KIT/run.sh" "/mnt/PYTHOS_ESP/PYTHOS-FIELD-KIT/run.sh"; do [ -f "$p" ] && exec bash "$p" stage-local; done; p="$(find /media /run/media /mnt -maxdepth 5 -path "*/PYTHOS-FIELD-KIT/run.sh" -print -quit 2>/dev/null)"; [ -n "$p" ] && exec bash "$p" stage-local; echo "PythOS field kit not found. Mount PYTHOS_ESP, then try again."; exit 1'
   ```

4. Unmount and unplug the PythOS USB if the USB mouse needs that port.
5. Plug in the USB mouse and run:

   ```bash
   sudo bash ~/pythos-field-kit/run.sh mouse
   ```

6. The script prints a tarball path like:

   ```text
   ~/pythos-field-kit-runs/run-YYYYMMDD-HHMMSS.tar.gz
   ```

7. Reinsert and mount the PythOS USB, then copy the report back:

   ```bash
   sudo bash ~/pythos-field-kit/run.sh copy-last
   ```

On Windows, copied reports appear under:

```text
P:\PYTHOS-FIELD-KIT\reports\
```

## Other Commands

Run a non-interactive collection:

```bash
sudo bash ~/pythos-field-kit/run.sh collect
```

Verify the mounted PythOS USB and hash the boot payload:

```bash
sudo bash ~/pythos-field-kit/run.sh verify-usb
```

Copy the latest generated report back to the mounted PythOS USB:

```bash
sudo bash ~/pythos-field-kit/run.sh copy-last
```

Run the full flow and copy the report back if the PythOS USB is mounted:

```bash
sudo bash ~/pythos-field-kit/run.sh all
```

Use `bash path/to/run.sh` rather than executing the file directly; FAT mounts
may be `noexec`.

## Evidence Collected

The report captures:

- `/etc/os-release`, kernel command line, and `uname`;
- block devices, filesystem labels, and mount points;
- PCI USB/xHCI and SDHCI/eMMC controller identity;
- sysfs PCI vendor/device/class/resource data;
- USB topology, interface class/subclass/protocol, and descriptor data for the
  known Dell/PixArt mouse `413c:301a` when present;
- HID raw devices, HID bus devices, input event nodes, and udev properties;
- I2C and ACPI HID devices for trackpad classification;
- `libinput` and `xinput` device lists when available;
- filtered hardware-related `dmesg` output;
- mounted `PYTHOS_ESP` file presence and SHA-256 hashes.

If `lspci`, `lsusb`, `libinput`, or `xinput` are missing, the script records
that fact and continues. On Mint those come from `pciutils`, `usbutils`,
`libinput-tools`, and `xinput`.

## Stop Conditions

Stop and send the tarball or `summary.txt` if:

- `PYTHOS_USB_MOUNT=not_found` appears while the USB is supposed to be mounted;
- the USB mouse does not appear in `lsusb` after replug;
- the report shows a different xHCI controller than AMD `1022:7914`;
- the trackpad is not listed as an I2C or ACPI HID device;
- the PythOS boot payload hashes do not match the just-deployed image.
