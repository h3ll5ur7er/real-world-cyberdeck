# Setup Guide

## Requirements

| Item | Notes |
|------|-------|
| Raspberry Pi 500+ | Built-in mechanical keyboard |
| Raspberry Pi OS (64-bit, Bookworm or later) | Fresh install recommended |
| USB-C cable | Connect Pi 500+ to host computer |
| MicroSD card ≥ 16 GB | For the OS image |

---

## 1 · Raspberry Pi OS initial configuration

1. Flash the latest **Raspberry Pi OS Lite (64-bit)** to the SD card with
   Raspberry Pi Imager.
2. **Enable SSH and set credentials** via the Imager's *OS Customisation*
   dialog (click the gear icon).  Setting hostname, SSH, and Wi-Fi here lets
   you boot the Pi headless — no monitor or extra keyboard needed.
3. Boot the Pi and connect via SSH.

> **Tip — Raspberry Pi Imager advantage:** The Imager pre-configures SSH,
> Wi-Fi, hostname, and locale in a single step during flashing.  This means
> you can plug the Pi 500+ into the host, SSH in over Wi-Fi, and run the
> installer without ever needing to edit `/boot/firmware/config.txt` manually
> for initial access.  The only manual `config.txt` change required is
> enabling the dwc2 overlay (step 2 below).

---

## 2 · Enable USB OTG (dwc2) overlay

The Pi 500+ uses the **dwc2** USB controller in device (gadget) mode.

```bash
# Append to /boot/firmware/config.txt
echo "dtoverlay=dwc2" | sudo tee -a /boot/firmware/config.txt

# Load modules at boot
echo "dwc2" | sudo tee -a /etc/modules
echo "libcomposite" | sudo tee -a /etc/modules
```

Reboot after making these changes.

---

## 3 · Clone this repository

```bash
git clone https://github.com/h3ll5ur7er/real-world-cyberdeck.git
cd real-world-cyberdeck
```

---

## 4 · Run the installer

```bash
sudo bash scripts/install.sh
```

The installer:
- Installs Rust (if not present) via `rustup`
- Builds the `cyberdeck-kbd` binary
- Copies configuration to `/etc/cyberdeck/`
- Creates a mass-storage backing image at `/var/lib/cyberdeck/storage.img`
- Installs and enables systemd services

---

## 5 · Verify the USB gadget

After the next reboot (or `sudo systemctl start usb-gadget`) plug the USB-C
cable into a host computer. You should see:

```
$ lsusb          # on the host
Bus 001 Device 042: ID 1d6b:0104 Linux Foundation Multifunction Composite Gadget
```

The host will register:
- A HID keyboard device.
- A CDC-ECM ethernet adapter (typically `usb0` on Linux/macOS hosts).
- A USB mass storage device (backed by `/var/lib/cyberdeck/storage.img`).

---

## 6 · Configure key remapping

Edit `/etc/cyberdeck/keymap.toml` – see
[`docs/key-remapping.md`](key-remapping.md) for the full reference.

### Finding the correct keyboard device path

The Pi 500+'s built-in keyboard is connected internally over USB.  The evdev
device name varies by firmware version.  To discover it:

```bash
# List all input devices
cat /proc/bus/input/devices

# Or list by-id symlinks (stable across reboots)
ls -l /dev/input/by-id/

# Or use evtest (install with: sudo apt install evtest)
sudo evtest
```

Set `keyboard_device` in `keymap.toml` to the correct path.  Prefer
`/dev/input/by-id/…` or `/dev/input/by-path/…` symlinks — they are stable
across reboots, unlike `/dev/input/eventN` which can change.

> **Important:** The daemon calls `EVIOCGRAB` to exclusively grab the
> keyboard.  Once running, keystrokes go *only* to the USB HID gadget — the
> Pi's local console will not receive them.  If you need local console access,
> stop the daemon first: `sudo systemctl stop keyboard-daemon`

Restart the daemon after editing:

```bash
sudo systemctl restart keyboard-daemon
```

---

## 7 · Configure OTP profiles

Edit `/etc/cyberdeck/otp.toml` and add your TOTP/HOTP secrets (base-32
encoded, as provided by your authenticator app / service).

```toml
[[profiles]]
name   = "github"
type   = "totp"
secret = "JBSWY3DPEHPK3PXP"
digits = 6
period = 30
algo   = "SHA1"
```

**Keep this file secure:**

```bash
sudo chmod 600 /etc/cyberdeck/otp.toml
sudo chown root:root /etc/cyberdeck/otp.toml
```

---

## 8 · (Phase 2) Install Docker and Vaultwarden

Vaultwarden runs in Docker for easy deployment and updates.  The keyboard
daemon stays on the host — see [`docs/architecture.md`](architecture.md) §
"Deployment strategy" for the rationale.

```bash
# Install Docker (official convenience script).
# For a more cautious approach, download and review the script first:
#   curl -fsSL https://get.docker.com -o get-docker.sh
#   less get-docker.sh    # review before running
#   sudo sh get-docker.sh
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER"
# Log out and back in for group membership to take effect.

# Start Vaultwarden
cd real-world-cyberdeck

# Enable sign-ups for initial account creation
# In docker-compose.yml, temporarily set SIGNUPS_ALLOWED: "true"
docker compose up -d

# Open the web vault (from the Pi or via SSH tunnel)
# http://localhost:8080
```

After creating your account, disable sign-ups:

```bash
# In docker-compose.yml, set SIGNUPS_ALLOWED back to "false"
docker compose up -d   # recreates with new setting
```

---

## 9 · Updating

### Keyboard daemon & gadget scripts

```bash
cd real-world-cyberdeck
git pull
sudo bash scripts/install.sh
```

### Vaultwarden

```bash
cd real-world-cyberdeck
docker compose pull
docker compose up -d
```

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| No USB devices on host | dwc2 not loaded | Check `/proc/modules` for `dwc2`; verify `config.txt` |
| Keyboard daemon fails to start | Wrong evdev path | Check `journalctl -u keyboard-daemon`; adjust `keyboard_device` in config |
| OTP code wrong | Clock drift | `sudo timedatectl set-ntp true` |
| Mass storage not appearing | Missing image file | Run `sudo dd if=/dev/zero of=/var/lib/cyberdeck/storage.img bs=1M count=64` |
