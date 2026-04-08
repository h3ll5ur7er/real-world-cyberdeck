# real-world-cyberdeck

A secure, programmable USB keyboard appliance built on the Raspberry Pi 500+.

When plugged into a host PC over USB-C, the Pi becomes a self-contained
composite USB device providing a programmable HID keyboard, USB networking
(SSH-over-USB), and file transfer — all controlled by a custom interception
and event-processing layer. Secrets never leave the device.

## Features

### Implemented

- **USB composite device** via Linux ConfigFS gadget (HID keyboard, CDC-ECM
  Ethernet, mass storage)
- **Exclusive keyboard grab** (`EVIOCGRAB`) — keystrokes reach only the USB
  HID gadget, never the Pi's local console
- **Key remapping** — remap any key to another key, a macro sequence, or a
  special function
- **OTP type-out** — press a key to type a TOTP or HOTP code directly as
  keystrokes
- **Low-latency** — Rust-based keyboard daemon reads evdev events and writes
  HID reports directly
- **TOML configuration** — all key mappings and OTP profiles defined in simple
  config files

### Planned

- **Vaultwarden integration** — self-hosted Bitwarden-compatible server as the
  central backend for all secrets: OTP seeds, FIDO2/WebAuthn keys, credentials,
  with a built-in management web vault
- **Credential type-out** — press a key to type stored passwords and usernames
- **SSH-over-USB** — auto-configured RNDIS/ECM networking with setup guide
- **Advanced macros** — mode switching, per-app profiles, dead-man timeouts,
  LED/on-screen status indicators

See [`docs/architecture.md`](docs/architecture.md) for the full roadmap.

## Repository layout

```
├── config/              Default configuration files (TOML)
│   ├── keymap.toml      Key-mapping configuration
│   └── otp.toml         OTP profile definitions
├── docs/                Documentation
│   ├── architecture.md  System architecture overview
│   ├── setup.md         Step-by-step setup guide
│   └── key-remapping.md Key-remapping reference
├── keyboard-daemon/     Rust crate: evdev → keymapper → HID writer
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs      Event loop and CLI
│       ├── config.rs    TOML configuration loading
│       ├── keymapper.rs Key-mapping engine
│       ├── hid.rs       USB HID report builder
│       └── otp/         HOTP (RFC 4226) and TOTP (RFC 6238)
├── usb-gadget/          Shell scripts for USB gadget setup/teardown
├── scripts/             Installer and systemd unit files
└── README.md
```

## Quick start

See [`docs/setup.md`](docs/setup.md) for the full guide. In short:

```bash
# On a Raspberry Pi 500+ running Raspberry Pi OS (64-bit)
git clone https://github.com/h3ll5ur7er/real-world-cyberdeck.git
cd real-world-cyberdeck
sudo bash scripts/install.sh
sudo reboot
```

## Development

### Build

```bash
cd keyboard-daemon
cargo build
```

### Test

```bash
cd keyboard-daemon
cargo test
```

## License

MIT
