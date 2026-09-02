#!/usr/bin/env bash
set -u

SCRIPT_NAME="${0##*/}"
MODE="${1:-collect}"
ARG="${2:-}"

usage() {
  cat <<'USAGE'
PythOS Linux Mint field kit

Usage:
  bash run.sh stage-local
  bash run.sh collect
  bash run.sh mouse
  bash run.sh verify-usb
  bash run.sh copy-last
  bash run.sh copy-report /full/path/to/report.tar.gz
  bash run.sh all

Modes:
  stage-local  Copy this script to ~/pythos-field-kit/run.sh so the PythOS USB
               can be unplugged before mouse testing.
  collect      Collect PCI, xHCI, USB, HID, input, block, and dmesg evidence.
  mouse        Run collect, then prompt for USB mouse unplug/replug/move/click.
  verify-usb   Verify a mounted PYTHOS_ESP volume and hash its boot payload.
  copy-last    Copy the most recent generated report back to PYTHOS_ESP.
  copy-report  Copy a generated report tarball back to PYTHOS_ESP.
  all          Run verify-usb, collect, mouse, and copy the report if possible.

This script performs Linux-side discovery only. It does not format disks,
install PythOS to eMMC, write PCI/xHCI registers, or deploy boot files.
USAGE
}

owner_name() {
  if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER:-}" != "root" ]; then
    printf '%s\n' "${SUDO_USER}"
  else
    printf '%s\n' "${USER:-$(id -un 2>/dev/null || printf root)}"
  fi
}

owner_home() {
  if [ -n "${PYTHOS_FIELD_HOME:-}" ]; then
    printf '%s\n' "${PYTHOS_FIELD_HOME}"
    return
  fi

  if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER:-}" != "root" ] && command -v getent >/dev/null 2>&1; then
    local home
    home="$(getent passwd "${SUDO_USER}" | awk -F: '{print $6; exit}')"
    if [ -n "${home}" ]; then
      printf '%s\n' "${home}"
      return
    fi
  fi

  printf '%s\n' "${HOME:-/tmp}"
}

real_script_path() {
  if command -v realpath >/dev/null 2>&1; then
    realpath "$0"
  elif command -v readlink >/dev/null 2>&1; then
    readlink -f "$0" 2>/dev/null || printf '%s\n' "$0"
  else
    printf '%s\n' "$0"
  fi
}

stage_local() {
  local home install_dir src
  home="$(owner_home)"
  install_dir="${home}/pythos-field-kit"
  src="$(real_script_path)"
  mkdir -p "${install_dir}"
  cp "${src}" "${install_dir}/run.sh"
  chmod +x "${install_dir}/run.sh" 2>/dev/null || true

  cat >"${install_dir}/START-HERE.txt" <<EOF
PythOS Linux Mint field kit

Use Terminal. Do not double-click scripts from the PythOS USB; Mint may open
them in a text editor or mount the FAT volume with noexec.

Run the mouse capture:

  sudo bash ${install_dir}/run.sh mouse

After it creates a report tarball, reinsert and mount the PythOS USB, then run:

  sudo bash ${install_dir}/run.sh copy-last

Verify the PythOS USB:

  sudo bash ${install_dir}/run.sh verify-usb

Reports are stored under:

  ${home}/pythos-field-kit-runs
EOF

  cat >"${install_dir}/mouse.sh" <<EOF
#!/usr/bin/env bash
sudo bash "${install_dir}/run.sh" mouse
EOF

  cat >"${install_dir}/copy-last.sh" <<EOF
#!/usr/bin/env bash
sudo bash "${install_dir}/run.sh" copy-last
EOF

  cat >"${install_dir}/verify-usb.sh" <<EOF
#!/usr/bin/env bash
sudo bash "${install_dir}/run.sh" verify-usb
EOF

  chmod +x "${install_dir}/mouse.sh" "${install_dir}/copy-last.sh" "${install_dir}/verify-usb.sh" 2>/dev/null || true

  if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER:-}" != "root" ] && command -v chown >/dev/null 2>&1; then
    chown -R "${SUDO_USER}:$(id -gn "${SUDO_USER}" 2>/dev/null || printf '%s' "${SUDO_USER}")" "${install_dir}" 2>/dev/null || true
  fi

  printf 'Installed local field kit at %s/run.sh\n' "${install_dir}"
  printf 'Next:\n'
  printf '  bash %s/run.sh mouse\n' "${install_dir}"
  printf '  bash %s/run.sh verify-usb\n' "${install_dir}"
  printf '  bash %s/run.sh copy-last\n' "${install_dir}"
}

is_pythos_mount() {
  local mount="$1"
  [ -d "${mount}/EFI/BOOT" ] &&
    [ -f "${mount}/EFI/BOOT/BOOTX64.EFI" ] &&
    [ -d "${mount}/PYTHOS" ] &&
    [ -f "${mount}/PYTHOS/PYTHCORE.ELF" ]
}

find_pythos_mounts() {
  local name current_user target
  name="$(owner_name)"
  current_user="${USER:-${name}}"

  {
    for target in \
      "/media/${name}/PYTHOS_ESP" \
      "/run/media/${name}/PYTHOS_ESP" \
      "/media/${current_user}/PYTHOS_ESP" \
      "/run/media/${current_user}/PYTHOS_ESP" \
      "/mnt/PYTHOS_ESP" \
      "/mnt/pythos_esp"; do
      if is_pythos_mount "${target}"; then
        printf '%s\n' "${target}"
      fi
    done

    if command -v findmnt >/dev/null 2>&1; then
      while IFS= read -r target; do
        if [ -n "${target}" ] && is_pythos_mount "${target}"; then
          printf '%s\n' "${target}"
        fi
      done <<EOF
$(findmnt -rn -o TARGET 2>/dev/null)
EOF
    fi
  } | awk '!seen[$0]++'
}

report_setup() {
  local home root stamp
  home="$(owner_home)"
  root="${PYTHOS_FIELD_OUT:-${home}/pythos-field-kit-runs}"
  stamp="$(date +%Y%m%d-%H%M%S)"
  OUT_DIR="${root}/run-${stamp}"
  RAW_DIR="${OUT_DIR}/raw"
  SUMMARY="${OUT_DIR}/summary.txt"
  TARBALL="${root}/$(basename "${OUT_DIR}").tar.gz"
  mkdir -p "${RAW_DIR}"
  {
    echo "PythOS Linux Mint field kit"
    echo "timestamp=$(date -Is)"
    echo "mode=${MODE}"
    echo "user=$(owner_name)"
    echo "home=${home}"
    echo "kernel=$(uname -a 2>/dev/null || true)"
    echo
    echo "Discovery-only boundary:"
    echo "- no disk formatting"
    echo "- no PythOS eMMC install"
    echo "- no PCI/xHCI register writes"
    echo "- report writes are limited to this output directory unless copy-report is used"
  } >"${SUMMARY}"
}

capture() {
  local name="$1"
  shift
  local file="${RAW_DIR}/${name}.txt"
  {
    printf '$'
    printf ' %q' "$@"
    printf '\n\n'
    "$@"
    local rc=$?
    printf '\nexit_status=%s\n' "${rc}"
  } >"${file}" 2>&1 || true
  append_excerpt "${name}" "${file}"
}

capture_shell() {
  local name="$1"
  local command="$2"
  local file="${RAW_DIR}/${name}.txt"
  {
    printf '$ %s\n\n' "${command}"
    sh -c "${command}"
    local rc=$?
    printf '\nexit_status=%s\n' "${rc}"
  } >"${file}" 2>&1 || true
  append_excerpt "${name}" "${file}"
}

capture_func() {
  local name="$1"
  shift
  local file="${RAW_DIR}/${name}.txt"
  {
    printf '$ %s\n\n' "$*"
    "$@"
    local rc=$?
    printf '\nexit_status=%s\n' "${rc}"
  } >"${file}" 2>&1 || true
  append_excerpt "${name}" "${file}"
}

append_excerpt() {
  local title="$1"
  local file="$2"
  {
    echo
    echo "=== ${title} ==="
    sed -n '1,220p' "${file}" 2>/dev/null || true
    local lines
    lines="$(wc -l <"${file}" 2>/dev/null || printf '0')"
    if [ "${lines}" -gt 220 ] 2>/dev/null; then
      echo "... truncated in summary; full output: ${file}"
    fi
  } >>"${SUMMARY}"
}

cat_file() {
  local path="$1"
  if [ -r "${path}" ]; then
    cat "${path}"
  else
    printf 'unreadable:%s\n' "${path}"
  fi
}

collect_tool_status() {
  local tool
  for tool in lspci lsusb findmnt lsblk udevadm libinput xinput dmesg sha256sum tar; do
    if command -v "${tool}" >/dev/null 2>&1; then
      printf '%s=present:%s\n' "${tool}" "$(command -v "${tool}")"
    else
      printf '%s=missing\n' "${tool}"
    fi
  done
}

collect_pci_controllers() {
  local device class
  for device in /sys/bus/pci/devices/*; do
    [ -f "${device}/class" ] || continue
    class="$(cat_file "${device}/class")"
    case "${class}" in
      0x0c03*|0x0805*)
        echo "--- $(basename "${device}") ---"
        for field in vendor device subsystem_vendor subsystem_device class irq numa_node; do
          [ -f "${device}/${field}" ] && echo "${field}=$(cat_file "${device}/${field}")"
        done
        if [ -e "${device}/driver" ]; then
          echo "driver=$(basename "$(readlink -f "${device}/driver" 2>/dev/null)")"
        else
          echo "driver=none"
        fi
        if [ -f "${device}/resource" ]; then
          echo "resource:"
          cat_file "${device}/resource"
        fi
        ;;
    esac
  done
}

collect_usb_interfaces() {
  local interface parent field
  for interface in /sys/bus/usb/devices/*:*; do
    [ -f "${interface}/bInterfaceClass" ] || continue
    parent="${interface%:*}"
    echo "--- $(basename "${interface}") ---"
    echo "interface_path=${interface}"
    echo "device_path=${parent}"
    echo "class=$(cat_file "${interface}/bInterfaceClass")"
    echo "subclass=$(cat_file "${interface}/bInterfaceSubClass")"
    echo "protocol=$(cat_file "${interface}/bInterfaceProtocol")"
    if [ -e "${interface}/driver" ]; then
      echo "driver=$(basename "$(readlink -f "${interface}/driver" 2>/dev/null)")"
    else
      echo "driver=none"
    fi
    for field in idVendor idProduct manufacturer product serial busnum devnum speed devpath maxchild bcdUSB; do
      [ -f "${parent}/${field}" ] && echo "${field}=$(cat_file "${parent}/${field}")"
    done
  done
}

collect_usb_devices() {
  local device field
  for device in /sys/bus/usb/devices/*; do
    [ -f "${device}/idVendor" ] || continue
    echo "--- $(basename "${device}") ---"
    echo "path=${device}"
    for field in idVendor idProduct manufacturer product serial busnum devnum speed devpath maxchild bcdUSB configuration; do
      [ -f "${device}/${field}" ] && echo "${field}=$(cat_file "${device}/${field}")"
    done
    if [ -e "${device}/driver" ]; then
      echo "driver=$(basename "$(readlink -f "${device}/driver" 2>/dev/null)")"
    fi
  done
}

collect_hidraw() {
  local hidraw name
  for hidraw in /sys/class/hidraw/hidraw*; do
    [ -e "${hidraw}" ] || continue
    name="$(basename "${hidraw}")"
    echo "--- /dev/${name} ---"
    readlink -f "${hidraw}/device" 2>/dev/null || true
    if command -v udevadm >/dev/null 2>&1; then
      udevadm info -q property -n "/dev/${name}" 2>/dev/null |
        grep -E 'DEVPATH|HID_ID|HID_NAME|HID_PHYS|ID_BUS|ID_VENDOR|ID_MODEL|ID_PATH' || true
    fi
  done
}

collect_hid_devices() {
  local device field
  for device in /sys/bus/hid/devices/*; do
    [ -e "${device}" ] || continue
    echo "--- $(basename "${device}") ---"
    echo "path=${device}"
    for field in modalias name phys uniq country; do
      [ -f "${device}/${field}" ] && echo "${field}=$(cat_file "${device}/${field}")"
    done
    if [ -e "${device}/driver" ]; then
      echo "driver=$(basename "$(readlink -f "${device}/driver" 2>/dev/null)")"
    else
      echo "driver=none"
    fi
  done
}

collect_input_nodes() {
  local node base
  for node in /dev/input/event*; do
    [ -e "${node}" ] || continue
    base="$(basename "${node}")"
    echo "--- ${node} ---"
    if [ -r "/sys/class/input/${base}/device/name" ]; then
      echo "name=$(cat_file "/sys/class/input/${base}/device/name")"
    fi
    readlink -f "/sys/class/input/${base}/device" 2>/dev/null || true
    if command -v udevadm >/dev/null 2>&1; then
      udevadm info -q property -n "${node}" 2>/dev/null |
        grep -E 'DEVPATH|ID_BUS|ID_INPUT|ID_MODEL|ID_PATH|ID_SERIAL|ID_VENDOR|NAME|PHYS|PRODUCT' || true
    fi
  done
}

collect_i2c_devices() {
  local device field
  for device in /sys/bus/i2c/devices/*; do
    [ -e "${device}" ] || continue
    echo "--- $(basename "${device}") ---"
    echo "path=${device}"
    for field in name modalias firmware_node/path; do
      [ -f "${device}/${field}" ] && echo "${field}=$(cat_file "${device}/${field}")"
    done
    if [ -e "${device}/driver" ]; then
      echo "driver=$(basename "$(readlink -f "${device}/driver" 2>/dev/null)")"
    else
      echo "driver=none"
    fi
  done
}

collect_acpi_hid_devices() {
  local device field hid
  for device in /sys/bus/acpi/devices/*; do
    [ -e "${device}" ] || continue
    hid="$(cat_file "${device}/hid" 2>/dev/null || true)"
    case "${hid}" in
      *ELAN*|*PNP0C50*|*PNP0303*|*MSFT0001*|*DLL*|*SYN*|*I2C*)
        echo "--- $(basename "${device}") ---"
        echo "path=${device}"
        for field in hid modalias path status uid; do
          [ -f "${device}/${field}" ] && echo "${field}=$(cat_file "${device}/${field}")"
        done
        if [ -e "${device}/driver" ]; then
          echo "driver=$(basename "$(readlink -f "${device}/driver" 2>/dev/null)")"
        else
          echo "driver=none"
        fi
        ;;
    esac
  done
}

verify_pythos_usb_to_stdout() {
  local mount
  mount="${1:-}"
  if [ -z "${mount}" ]; then
    mount="$(find_pythos_mounts | head -n 1)"
  fi

  if [ -z "${mount}" ]; then
    echo "PYTHOS_USB_MOUNT=not_found"
    echo "Expected a mounted PYTHOS_ESP volume containing EFI/BOOT/BOOTX64.EFI and PYTHOS/PYTHCORE.ELF."
    return 1
  fi

  echo "PYTHOS_USB_MOUNT=${mount}"
  if command -v findmnt >/dev/null 2>&1; then
    findmnt -no SOURCE,TARGET,FSTYPE,LABEL,OPTIONS "${mount}" 2>/dev/null || true
  fi

  echo
  echo "--- root ---"
  ls -la "${mount}" 2>/dev/null || true

  echo
  echo "--- expected files ---"
  local file
  for file in \
    EFI/BOOT/BOOTX64.EFI \
    PYTHOS/PYTHCORE.ELF \
    PYTHOS/INIT.PAK \
    PYTHOS/BOOT.CFG \
    PYTHOS/FONT.PSF \
    NvVars \
    LINUX-USB-MOUSE-MAP.SH \
    PYTHOS-FIELD-KIT/run.sh; do
    if [ -e "${mount}/${file}" ]; then
      printf 'present %s ' "${file}"
      wc -c <"${mount}/${file}" 2>/dev/null || true
    else
      printf 'missing %s\n' "${file}"
    fi
  done

  if command -v sha256sum >/dev/null 2>&1; then
    echo
    echo "--- sha256 ---"
    for file in \
      EFI/BOOT/BOOTX64.EFI \
      PYTHOS/PYTHCORE.ELF \
      PYTHOS/INIT.PAK \
      PYTHOS/BOOT.CFG \
      PYTHOS/FONT.PSF \
      LINUX-USB-MOUSE-MAP.SH \
      PYTHOS-FIELD-KIT/run.sh; do
      [ -e "${mount}/${file}" ] && sha256sum "${mount}/${file}"
    done
  fi
}

copy_report_to_usb() {
  local report="$1"
  local mount report_dir dest
  if [ -z "${report}" ]; then
    echo "copy-report requires a report tarball path."
    return 2
  fi
  if [ ! -f "${report}" ]; then
    echo "report not found: ${report}"
    return 2
  fi

  mount="$(find_pythos_mounts | head -n 1)"
  if [ -z "${mount}" ]; then
    echo "PYTHOS_USB_MOUNT=not_found"
    echo "Reinsert and mount the PythOS USB, then rerun copy-report."
    return 1
  fi

  report_dir="${mount}/PYTHOS-FIELD-KIT/reports"
  mkdir -p "${report_dir}"
  dest="${report_dir}/$(basename "${report}")"
  cp "${report}" "${dest}"
  sync "${dest}" 2>/dev/null || sync
  echo "copied_report=${dest}"
}

latest_report() {
  local home kit_last root report
  home="$(owner_home)"
  kit_last="${home}/pythos-field-kit/LAST-REPORT.txt"
  root="${PYTHOS_FIELD_OUT:-${home}/pythos-field-kit-runs}"

  if [ -s "${kit_last}" ]; then
    report="$(sed -n '1p' "${kit_last}")"
    if [ -f "${report}" ]; then
      printf '%s\n' "${report}"
      return 0
    fi
  fi

  if [ -d "${root}" ]; then
    report="$(find "${root}" -maxdepth 1 -type f -name 'run-*.tar.gz' -printf '%T@ %p\n' 2>/dev/null | sort -nr | sed -n '1s/^[^ ]* //p')"
    if [ -n "${report}" ] && [ -f "${report}" ]; then
      printf '%s\n' "${report}"
      return 0
    fi

    report="$(ls -t "${root}"/run-*.tar.gz 2>/dev/null | head -n 1)"
    if [ -n "${report}" ] && [ -f "${report}" ]; then
      printf '%s\n' "${report}"
      return 0
    fi
  fi

  return 1
}

copy_last_report_to_usb() {
  local report
  if ! report="$(latest_report)"; then
    echo "No generated report tarball found under ~/pythos-field-kit-runs."
    echo "Run: sudo bash ~/pythos-field-kit/run.sh mouse"
    return 1
  fi

  copy_report_to_usb "${report}"
}

collect_common() {
  capture_func "tool-status" collect_tool_status
  capture_shell "os-release" 'cat /etc/os-release 2>/dev/null || true; uname -a 2>/dev/null || true; cat /proc/cmdline 2>/dev/null || true'
  capture_shell "block-devices" 'if command -v lsblk >/dev/null 2>&1; then lsblk -o NAME,KNAME,PATH,MODEL,SERIAL,TRAN,TYPE,SIZE,FSTYPE,LABEL,UUID,MOUNTPOINTS; else echo "lsblk missing"; fi'
  capture_shell "mounts" 'if command -v findmnt >/dev/null 2>&1; then findmnt -o SOURCE,TARGET,FSTYPE,LABEL,OPTIONS; else mount; fi'
  capture_shell "pci-usb-and-storage" 'if command -v lspci >/dev/null 2>&1; then lspci -Dnn | grep -iE "usb|xhci|ehci|ohci|uhci|sdhci|mmc|o2 micro|1217|8620" || true; else echo "lspci missing; install pciutils if needed"; fi'
  capture_shell "pci-verbose-usb-storage" 'if command -v lspci >/dev/null 2>&1; then if sudo -n true 2>/dev/null; then sudo -n lspci -Dnnvv; else lspci -Dnnvv; fi | sed -n "/USB controller/,+45p;/SD Host controller/,+45p"; else echo "lspci missing"; fi'
  capture_func "sysfs-pci-usb-storage" collect_pci_controllers
  capture_shell "usb-topology" 'if command -v lsusb >/dev/null 2>&1; then lsusb; echo; lsusb -t; else echo "lsusb missing; install usbutils if needed"; fi'
  capture_shell "usb-descriptors-413c-301a" 'if command -v lsusb >/dev/null 2>&1; then lsusb -v -d 413c:301a 2>/dev/null || true; else echo "lsusb missing"; fi'
  capture_func "sysfs-usb-devices" collect_usb_devices
  capture_func "sysfs-usb-interfaces" collect_usb_interfaces
  capture_func "hidraw-devices" collect_hidraw
  capture_func "hid-devices" collect_hid_devices
  capture_shell "proc-input-devices" 'cat /proc/bus/input/devices 2>/dev/null || true'
  capture_func "input-event-nodes" collect_input_nodes
  capture_func "i2c-devices" collect_i2c_devices
  capture_func "acpi-hid-input-devices" collect_acpi_hid_devices
  capture_shell "libinput-devices" 'if command -v libinput >/dev/null 2>&1; then libinput list-devices 2>/dev/null || sudo -n libinput list-devices 2>/dev/null || true; else echo "libinput missing; install libinput-tools if deeper pointer classification is needed"; fi'
  capture_shell "xinput-list" 'if command -v xinput >/dev/null 2>&1; then xinput list --long 2>/dev/null || true; else echo "xinput missing or not using X11"; fi'
  capture_shell "driver-modinfo" 'for module in xhci_pci xhci_hcd usbhid hid_generic hid_multitouch i2c_hid i2c_hid_acpi psmouse; do if command -v modinfo >/dev/null 2>&1; then modinfo "$module" 2>/dev/null | sed -n "1,80p"; fi; done'
  capture_shell "dmesg-hardware-tail" 'if command -v dmesg >/dev/null 2>&1; then sudo -n dmesg -T 2>/dev/null || dmesg -T 2>/dev/null || dmesg 2>/dev/null || true; fi | grep -Ei "usb|xhci|hid|i2c|elan|input|mouse|touchpad|mmc|sdhci|o2|1217|8620" | tail -n 500'
  capture_func "verify-pythos-usb" verify_pythos_usb_to_stdout
}

collect_mouse_interactive() {
  {
    echo
    echo "=== operator prompts ==="
    echo "Prompting for USB mouse unplug/replug/move/click/scroll."
  } >>"${SUMMARY}"

  echo
  echo "USB mouse mapping step."
  echo "If the PythOS USB is occupying the only port, run stage-local first, boot Mint from eMMC, and run the local copy."
  echo
  echo "Unplug the USB mouse now, then press Enter."
  read -r _unused

  if command -v dmesg >/dev/null 2>&1; then
    sudo -n dmesg -C 2>/dev/null || true
  fi

  echo "Plug the USB mouse in, move it for 10 seconds, click both buttons, scroll, then press Enter."
  read -r _unused

  capture_shell "mouse-dmesg-after-action" 'if command -v dmesg >/dev/null 2>&1; then sudo -n dmesg -T 2>/dev/null || dmesg -T 2>/dev/null || dmesg 2>/dev/null || true; fi'
  capture_shell "mouse-usb-topology-after-action" 'if command -v lsusb >/dev/null 2>&1; then lsusb; echo; lsusb -t; else echo "lsusb missing"; fi'
  capture_func "mouse-usb-devices-after-action" collect_usb_devices
  capture_func "mouse-usb-interfaces-after-action" collect_usb_interfaces
  capture_func "mouse-hidraw-after-action" collect_hidraw
  capture_shell "mouse-proc-input-after-action" 'cat /proc/bus/input/devices 2>/dev/null || true'
  capture_func "mouse-input-event-nodes-after-action" collect_input_nodes
}

package_report() {
  local home kit_dir
  home="$(owner_home)"
  kit_dir="${home}/pythos-field-kit"
  mkdir -p "$(dirname "${TARBALL}")"
  if command -v tar >/dev/null 2>&1; then
    tar -czf "${TARBALL}" -C "$(dirname "${OUT_DIR}")" "$(basename "${OUT_DIR}")"
    mkdir -p "${kit_dir}"
    printf '%s\n' "${TARBALL}" >"${kit_dir}/LAST-REPORT.txt"
    echo "report_dir=${OUT_DIR}"
    echo "report_tarball=${TARBALL}"
    echo "copy_back_command=sudo bash ${kit_dir}/run.sh copy-last"
    {
      echo
      echo "=== package ==="
      echo "report_dir=${OUT_DIR}"
      echo "report_tarball=${TARBALL}"
      echo "last_report_file=${kit_dir}/LAST-REPORT.txt"
      echo "copy_back_command=sudo bash ${kit_dir}/run.sh copy-last"
    } >>"${SUMMARY}"
  else
    echo "tar missing; report directory is ${OUT_DIR}"
  fi
}

case "${MODE}" in
  -h|--help|help)
    usage
    ;;
  stage-local)
    stage_local
    ;;
  collect)
    report_setup
    collect_common
    package_report
    ;;
  mouse)
    report_setup
    collect_common
    collect_mouse_interactive
    package_report
    ;;
  verify-usb)
    report_setup
    capture_func "verify-pythos-usb" verify_pythos_usb_to_stdout
    package_report
    ;;
  copy-report)
    copy_report_to_usb "${ARG}"
    ;;
  copy-last)
    copy_last_report_to_usb
    ;;
  all)
    report_setup
    collect_common
    collect_mouse_interactive
    package_report
    copy_last_report_to_usb || true
    ;;
  *)
    echo "unknown mode: ${MODE}" >&2
    usage >&2
    exit 2
    ;;
esac
