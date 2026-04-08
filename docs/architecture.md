# Architecture

## Vision

The **real-world-cyberdeck** turns a Raspberry Pi 500+ into a secure,
programmable USB keyboard appliance. When plugged into a host PC over USB-C
the Pi presents itself as a composite USB device providing:

1. **Programmable HID keyboard** — forwards, remaps, and generates keystrokes
   (macros, OTP codes, automation shortcuts).
2. **USB mass storage** — exposes a small virtual disk for easy file transfer
   between the Pi and the host.
3. **USB network interface (RNDIS/ECM)** — provides SSH-over-USB so the user
   can manage the Pi without an external network.

Secrets never leave the Pi. OTP seeds are stored on-device and synthetic
keystrokes are generated locally through a custom interception and
event-processing layer.

---

## System diagram

```
┌─────────────────────────────────────────────────────────┐
│               Host computer (USB host)                  │
│  sees: HID keyboard + CDC-ECM ethernet + mass storage   │
└────────────────┬────────────────────────────────────────┘
                 │ USB-C cable
┌────────────────▼────────────────────────────────────────┐
│          Raspberry Pi 500+  (USB device / gadget)       │
│                                                         │
│  ┌────────────┐   ┌────────────────┐  ┌─────────────┐  │
│  │ USB Gadget │   │  Keyboard      │  │ OTP engine  │  │
│  │  (kernel   │◄──│  Daemon (Rust) │◄─│  (Rust)     │  │
│  │  ConfigFS) │   │                │  └─────────────┘  │
│  └────────────┘   │  evdev ──────► │                    │
│       ▲           │  keymapper ──► │  ┌─────────────┐  │
│       │           │  HID writer ─► │  │ Future:     │  │
│  ┌────┴───────┐   └────────────────┘  │  FIDO2      │  │
│  │ ECM net    │                       │  Secure UI  │  │
│  │ Mass store │                       │  Seed vault │  │
│  └────────────┘                       └─────────────┘  │
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
| OTP engine      | Rust     | Co-located with daemon; will migrate to password manager backend |
| Future: secrets | PM       | Open-source password manager for OTP, FIDO, credentials, web UI |

---

## Security principles

- **Secrets never leave the Pi.** All secrets (OTP seeds, credentials, FIDO
  keys) are stored on-device. The project will integrate an open-source
  password manager (e.g. Vaultwarden/Bitwarden, KeePassXC, or Passwork) as the
  single, battle-proven backend for secrets management — providing encrypted
  at-rest storage, HOTP/TOTP generation, FIDO/WebAuthn credential storage,
  and a management web UI out of the box.
- **No custom crypto for secrets.** Instead of rolling our own vault, we
  delegate to a proven open-source password manager for all secrets-related
  functionality.
- **OTP codes are generated locally** and typed as synthetic keystrokes — they
  are never transmitted over a network.
- **No mass-storage exposure of secrets.** The shared USB disk image is
  separate from the secrets directory.
- **Programmable per-key behavior** via TOML configuration.

---

## Roadmap

### Phase 1 — Programmable HID keyboard ✅

Completed.

| Feature                        | Status |
|--------------------------------|--------|
| USB composite gadget (HID + ECM + mass storage) | ✅ Done |
| evdev capture → HID forwarding | ✅ Done |
| Key → key remapping            | ✅ Done |
| Key → macro expansion          | ✅ Done |
| TOTP type-out (RFC 6238)       | ✅ Done |
| HOTP type-out (RFC 4226)       | ✅ Done |
| TOML-based config              | ✅ Done |
| systemd services               | ✅ Done |
| Install script                 | ✅ Done |

### Phase 2 — Open-source password manager integration

Integrate an open-source password manager as the central secrets backend.
The right choice (e.g. Vaultwarden/Bitwarden, KeePassXC, or Passwork) gives
us battle-proven, encrypted storage for OTP seeds, credentials, and FIDO keys
— plus a management web UI — without rolling our own crypto.

| Feature                                           | Status  |
|---------------------------------------------------|---------|
| Evaluate & select open-source password manager    | Planned |
| Integrate OTP generation (HOTP/TOTP) from password manager | Planned |
| Migrate seed storage from flat TOML to password manager vault | Planned |
| Credential / password storage & type-out          | Planned |
| FIDO2 / WebAuthn credential storage via password manager | Planned |
| Physical or shortcut-based trigger authorization   | Planned |
| Automatic HOTP counter increment + persistence    | Planned |

### Phase 3 — Management UI (via password manager)

| Feature                                                | Status  |
|--------------------------------------------------------|---------|
| Password manager web UI for token/credential management | Planned |
| Key-mapping editor (lightweight custom UI or extension) | Planned |
| Event log viewer                                       | Planned |
| USB gadget settings panel                              | Planned |

### Phase 4 — USB networking & mass storage workflows

| Feature                                          | Status  |
|--------------------------------------------------|---------|
| RNDIS/ECM network auto-configuration scripts     | Planned |
| SSH-over-USB setup guide                         | Planned |
| Mass storage file-sharing workflow & docs        | Planned |
| Secure admin channel over the RNDIS link         | Planned |

### Phase 5 — Advanced keyboard features

| Feature                                   | Status  |
|-------------------------------------------|---------|
| Keystroke-based mode switching            | Planned |
| Per-application key profiles              | Planned |
| Dead-man timeout for sensitive actions    | Planned |
| LED / on-screen status indicators         | Planned |
| Typing-template macro engine              | Planned |

---

## Deployment

See [`docs/setup.md`](setup.md) for step-by-step instructions.
