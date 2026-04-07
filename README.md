# real-world-cyberdeck

Set of tools to turn a Raspberry Pi 500+ into a real-world cyberdeck —
a "keyboard on steroids" that presents itself as a USB composite device
(HID keyboard + CDC-ECM Ethernet + mass storage) to any host computer.

## Features

- **USB composite device** via Linux ConfigFS gadget (HID keyboard, CDC-ECM
  Ethernet, mass storage)
- **Key remapping** — remap any key to another key, a macro sequence, or a
  special function
- **OTP type-out** — press a key to type a TOTP or HOTP code
- **Low-latency** — Rust-based keyboard daemon reads evdev events and writes
  HID reports directly

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
