#!/usr/bin/env bash
# install.sh – Build and install the real-world-cyberdeck components.
#
# Must be run as root from the repository root directory.

set -euo pipefail

if [ "${EUID:-$(id -u)}" -ne 0 ]; then
    echo "ERROR: This installer must be run as root." >&2
    echo "Re-run with sudo or as the root user." >&2
    exit 1
fi

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL_DIR="/opt/cyberdeck"
CONFIG_DIR="/etc/cyberdeck"
DATA_DIR="/var/lib/cyberdeck"

echo "==> Installing real-world-cyberdeck from ${REPO_DIR}"

# ── 1. Install Rust if not present ───────────────────────────────────
if ! command -v cargo &>/dev/null; then
    echo "==> Installing Rust toolchain via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

# ── 2. Build the keyboard daemon ────────────────────────────────────
echo "==> Building cyberdeck-kbd..."
cd "${REPO_DIR}/keyboard-daemon"
cargo build --release

# ── 3. Install binary ───────────────────────────────────────────────
echo "==> Installing binaries to ${INSTALL_DIR}/bin/"
mkdir -p "${INSTALL_DIR}/bin"
cp target/release/cyberdeck-kbd "${INSTALL_DIR}/bin/"

# ── 4. Install USB gadget scripts ───────────────────────────────────
echo "==> Installing USB gadget scripts..."
mkdir -p "${INSTALL_DIR}/usb-gadget"
cp "${REPO_DIR}/usb-gadget/setup_gadget.sh"   "${INSTALL_DIR}/usb-gadget/"
cp "${REPO_DIR}/usb-gadget/teardown_gadget.sh" "${INSTALL_DIR}/usb-gadget/"
chmod +x "${INSTALL_DIR}/usb-gadget/"*.sh

# ── 5. Install configuration ────────────────────────────────────────
echo "==> Installing configuration to ${CONFIG_DIR}/"
mkdir -p "${CONFIG_DIR}"
# Only install defaults – do not overwrite existing config.
for f in keymap.toml otp.toml; do
    if [ ! -f "${CONFIG_DIR}/${f}" ]; then
        cp "${REPO_DIR}/config/${f}" "${CONFIG_DIR}/${f}"
    else
        echo "    ${CONFIG_DIR}/${f} already exists – skipping."
    fi
done
# Secure OTP config
chmod 600 "${CONFIG_DIR}/otp.toml"

# ── 6. Create data directory & storage image ─────────────────────────
mkdir -p "${DATA_DIR}"
if [ ! -f "${DATA_DIR}/storage.img" ]; then
    echo "==> Creating 64 MiB mass-storage image..."
    dd if=/dev/zero of="${DATA_DIR}/storage.img" bs=1M count=64 status=none
    if ! command -v mkfs.vfat &>/dev/null; then
        echo "ERROR: mkfs.vfat is required to format ${DATA_DIR}/storage.img but was not found." >&2
        echo "Install dosfstools and rerun this installer." >&2
        exit 1
    fi
    if ! mkfs.vfat "${DATA_DIR}/storage.img" >/dev/null 2>&1; then
        echo "ERROR: Failed to format ${DATA_DIR}/storage.img with mkfs.vfat." >&2
        echo "Ensure dosfstools is installed and try again." >&2
        exit 1
    fi
fi

# ── 7. Install systemd units ────────────────────────────────────────
echo "==> Installing systemd services..."
cp "${REPO_DIR}/scripts/systemd/usb-gadget.service"      /etc/systemd/system/
cp "${REPO_DIR}/scripts/systemd/keyboard-daemon.service"  /etc/systemd/system/
systemctl daemon-reload
systemctl enable usb-gadget.service
systemctl enable keyboard-daemon.service

echo ""
echo "Installation complete."
echo "  Reboot or run:"
echo "    sudo systemctl start usb-gadget"
echo "    sudo systemctl start keyboard-daemon"
