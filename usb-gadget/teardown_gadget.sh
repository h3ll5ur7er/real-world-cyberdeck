#!/usr/bin/env bash
# teardown_gadget.sh – Cleanly disable and remove the USB composite gadget.
#
# Must be run as root.

set -euo pipefail

GADGET_DIR="/sys/kernel/config/usb_gadget/cyberdeck"

if [ ! -d "${GADGET_DIR}" ]; then
    echo "Gadget directory does not exist – nothing to tear down."
    exit 0
fi

cd "${GADGET_DIR}"

# Deactivate gadget
echo "" > UDC 2>/dev/null || true

# Remove function symlinks from configuration
for link in configs/c.1/*.usb0; do
    [ -L "${link}" ] && rm -f "${link}"
done

# Remove configuration strings and directory
rm -rf configs/c.1/strings/0x409
rmdir configs/c.1 2>/dev/null || true

# Remove functions
for func_dir in functions/*/; do
    [ -d "${func_dir}" ] && rmdir "${func_dir}" 2>/dev/null || true
done

# Remove gadget strings and directory
rm -rf strings/0x409
cd /
rmdir "${GADGET_DIR}" 2>/dev/null || true

echo "USB composite gadget removed."
