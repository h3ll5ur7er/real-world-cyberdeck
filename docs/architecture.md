# Architecture

## Overview

The **real-world-cyberdeck** turns a Raspberry Pi 500+ into a "keyboard on
steroids". The system is composed of three layers that work together:

```
┌─────────────────────────────────────────────────────────┐
│               Host computer (USB host)                  │
│  sees: HID keyboard + CDC-ECM ethernet + mass storage   │
└────────────────┬────────────────────────────────────────┘
                 │ USB cable
┌────────────────▼────────────────────────────────────────┐
│          Raspberry Pi 500+  (USB device / gadget)       │
│                                                         │
│  ┌────────────┐   ┌────────────────┐  ┌─────────────┐  │
│  │ USB Gadget │   │  Keyboard      │  │ OTP engine  │  │
│  │  (kernel   │◄──│  Daemon (Rust) │◄─│  (Rust)     │  │
│  │  ConfigFS) │   │                │  └─────────────┘  │
│  └────────────┘   │  evdev ──────► │                    │
│                   │  keymapper ──► │                    │
│                   │  HID writer ─► │                    │
│                   └────────────────┘                    │
│                                                         │
│  Physical keyboard (built-in to Pi 500+)                │
└─────────────────────────────────────────────────────────┘
```

---

## Components

### 1 · USB Gadget (`usb-gadget/`)

Configured via the Linux **USB Gadget / ConfigFS** framework
(`/sys/kernel/config/usb_gadget/`).

| Function           | Linux module         | Purpose                        |
|--------------------|----------------------|--------------------------------|
| HID keyboard       | `usb_f_hid`         | Forward key events to host     |
| CDC-ECM Ethernet   | `usb_f_ecm`         | SSH / network access from host |
| Mass storage       | `usb_f_mass_storage` | Expose a disk image to host   |

**Setup flow**

1. `setup_gadget.sh` – creates ConfigFS nodes, binds functions, enables gadget.
2. `teardown_gadget.sh` – disables and removes gadget cleanly.
3. The `usb-gadget.service` systemd unit runs `setup_gadget.sh` at boot.

---

### 2 · Keyboard Daemon (`keyboard-daemon/`)

A single Rust binary (`cyberdeck-kbd`) that:

1. **Opens** the physical keyboard via `/dev/input/eventX` (evdev).
2. **Applies** key mappings from `config/keymap.toml`:
   - **key → key** – remap one key to another.
   - **key → macro** – expand a key press into a sequence of key events.
   - **key → special** – trigger a special function (OTP type-out, etc.).
3. **Writes** resulting HID reports to `/dev/hidg0` (the kernel HID gadget
   device).

#### Crate layout

```
keyboard-daemon/
├── Cargo.toml
└── src/
    ├── main.rs          – argument parsing, event loop
    ├── config.rs        – TOML config (serde)
    ├── keymapper.rs     – mapping engine
    ├── hid.rs           – HID report builder + /dev/hidg0 writer
    └── otp/
        ├── mod.rs       – public re-exports
        ├── hotp.rs      – RFC 4226 HOTP
        └── totp.rs      – RFC 6238 TOTP
```

---

### 3 · OTP Engine (`keyboard-daemon/src/otp/`)

| Algorithm | RFC  | Key type          | Counter          |
|-----------|------|-------------------|------------------|
| HOTP      | 4226 | HMAC-SHA1         | monotone counter |
| TOTP      | 6238 | HMAC-SHA1/256/512 | 30 s time steps  |

OTP codes are typed out as if they were normal keyboard input – the daemon
breaks the numeric string into individual key-press/release HID reports.

---

## Data flow

```
Physical key press
        │
        ▼
evdev event  (/dev/input/eventX)
        │
        ▼
KeyMapper::translate()
    ├─ key → key   ──► single HID key report
    ├─ key → macro ──► sequence of HID key reports
    └─ key → otp   ──► OTP string ──► sequence of digit HID reports
        │
        ▼
HidWriter::send_report()  ──► /dev/hidg0
        │
        ▼
USB host receives HID keyboard input
```

---

## Configuration

### `config/keymap.toml`

```toml
[remap]
CapsLock = "LeftCtrl"

[macros]
F1 = ["h", "e", "l", "l", "o", "Return"]

[special]
F2 = { type = "totp", profile = "email" }
F3 = { type = "hotp", profile = "vpn" }
```

### `config/otp.toml`

```toml
[[profiles]]
name    = "email"
type    = "totp"
secret  = "BASE32ENCODEDSECRET"
digits  = 6
period  = 30
algo    = "SHA1"

[[profiles]]
name    = "vpn"
type    = "hotp"
secret  = "BASE32ENCODEDSECRET"
digits  = 6
counter = 0
```

---

## Technology choices

| Layer           | Language | Reason                                               |
|-----------------|----------|------------------------------------------------------|
| USB gadget      | Shell    | Direct sysfs/configfs manipulation, no runtime deps  |
| Keyboard daemon | Rust     | Low-latency, memory-safe, excellent evdev/HID crates |
| OTP engine      | Rust     | Co-located with daemon, no additional runtime        |
| Future: FIDO2   | Rust     | Extend with `ctap2` / WebAuthn crate                 |

---

## Deployment

See [`docs/setup.md`](setup.md) for step-by-step instructions.
