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
2. Enable SSH and set credentials via the Imager advanced options.
3. Boot the Pi and connect via SSH.

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
- A CDC-ECM ethernet adapter (typically `usb0` / `RNDIS` on Windows).
- A USB mass storage device (backed by `/var/lib/cyberdeck/storage.img`).

---

## 6 · Configure key remapping

Edit `/etc/cyberdeck/keymap.toml` – see
[`docs/key-remapping.md`](key-remapping.md) for the full reference.

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

## 8 · Updating

```bash
cd real-world-cyberdeck
git pull
sudo bash scripts/install.sh
```

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| No USB devices on host | dwc2 not loaded | Check `/proc/modules` for `dwc2`; verify `config.txt` |
| Keyboard daemon fails to start | Wrong evdev path | Check `journalctl -u keyboard-daemon`; adjust `keyboard_device` in config |
| OTP code wrong | Clock drift | `sudo timedatectl set-ntp true` |
| Mass storage not appearing | Missing image file | Run `sudo dd if=/dev/zero of=/var/lib/cyberdeck/storage.img bs=1M count=64` |
