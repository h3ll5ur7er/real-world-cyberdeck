#!/usr/bin/env bash
# setup_gadget.sh – Create a USB composite gadget via ConfigFS.
#
# Functions enabled:
#   1. HID keyboard  (usb_f_hid)
#   2. CDC-ECM ethernet (usb_f_ecm)
#   3. Mass storage   (usb_f_mass_storage)
#
# Must be run as root.

set -euo pipefail

GADGET_DIR="/sys/kernel/config/usb_gadget/cyberdeck"
STORAGE_IMG="/var/lib/cyberdeck/storage.img"
UDC=$(ls /sys/class/udc 2>/dev/null | head -n1 || true)

if [ -z "${UDC}" ]; then
    echo "ERROR: No UDC (USB Device Controller) found." >&2
    echo "       Make sure dtoverlay=dwc2 is set in /boot/firmware/config.txt" >&2
    exit 1
fi

# Ensure required kernel modules are loaded.
modprobe libcomposite
modprobe usb_f_hid
modprobe usb_f_ecm
modprobe usb_f_mass_storage

# --- Create gadget root -------------------------------------------------------
mkdir -p "${GADGET_DIR}"
cd "${GADGET_DIR}"

# USB device descriptor
echo 0x1d6b > idVendor   # Linux Foundation
echo 0x0104 > idProduct  # Multifunction Composite Gadget
echo 0x0100 > bcdDevice  # v1.0.0
echo 0x0200 > bcdUSB     # USB 2.0

# Device strings (English)
mkdir -p strings/0x409
echo "h3ll5ur7er"          > strings/0x409/manufacturer
echo "Real-World Cyberdeck" > strings/0x409/product
echo "000000000001"         > strings/0x409/serialnumber

# --- Configuration 1 ----------------------------------------------------------
mkdir -p configs/c.1/strings/0x409
echo "Cyberdeck Composite" > configs/c.1/strings/0x409/configuration
echo 250                   > configs/c.1/MaxPower   # 250 mA

# --- Function: HID keyboard ---------------------------------------------------
mkdir -p functions/hid.usb0
echo 1   > functions/hid.usb0/protocol    # 1 = Keyboard
echo 1   > functions/hid.usb0/subclass    # 1 = Boot interface subclass
echo 8   > functions/hid.usb0/report_length

# Standard boot-protocol keyboard HID report descriptor (8 bytes per report).
# Modifier byte | Reserved | 6 key-codes
echo -ne '\x05\x01\x09\x06\xa1\x01' \
         '\x05\x07\x19\xe0\x29\xe7\x15\x00\x25\x01\x75\x01\x95\x08\x81\x02' \
         '\x95\x01\x75\x08\x81\x03' \
         '\x95\x05\x75\x01\x05\x08\x19\x01\x29\x05\x91\x02\x95\x01\x75\x03\x91\x03' \
         '\x95\x06\x75\x08\x15\x00\x25\x65\x05\x07\x19\x00\x29\x65\x81\x00' \
         '\xc0' > functions/hid.usb0/report_desc

ln -sf functions/hid.usb0 configs/c.1/

# --- Function: CDC-ECM Ethernet -----------------------------------------------
mkdir -p functions/ecm.usb0

# Use locally administered unicast MACs (LAA bit set, multicast bit clear).
# Change these if running multiple Pi cyberdecks on the same host to avoid
# conflicts.
HOST_MAC="4a:6f:73:74:50:43"   # Locally administered "HostPC"
DEV_MAC="46:65:76:50:69:00"    # Locally administered "DevPi\0"
echo "${HOST_MAC}" > functions/ecm.usb0/host_addr
echo "${DEV_MAC}"  > functions/ecm.usb0/dev_addr

ln -sf functions/ecm.usb0 configs/c.1/

# --- Function: Mass Storage ---------------------------------------------------
mkdir -p functions/mass_storage.usb0

# Create backing image if it does not exist.
if [ ! -f "${STORAGE_IMG}" ]; then
    mkdir -p "$(dirname "${STORAGE_IMG}")"
    dd if=/dev/zero of="${STORAGE_IMG}" bs=1M count=64 status=none
    if ! mkfs.vfat "${STORAGE_IMG}" >/dev/null 2>&1; then
        echo "ERROR: Failed to format ${STORAGE_IMG} as FAT." >&2
        echo "       Ensure mkfs.vfat is installed (for example via dosfstools)." >&2
        exit 1
    fi
fi

echo 1                > functions/mass_storage.usb0/stall
echo 0                > functions/mass_storage.usb0/lun.0/cdrom
echo 0                > functions/mass_storage.usb0/lun.0/ro
echo 0                > functions/mass_storage.usb0/lun.0/nofua
echo "${STORAGE_IMG}" > functions/mass_storage.usb0/lun.0/file

ln -sf functions/mass_storage.usb0 configs/c.1/

# --- Activate ------------------------------------------------------------------
echo "${UDC}" > UDC

echo "USB composite gadget activated on ${UDC}"
