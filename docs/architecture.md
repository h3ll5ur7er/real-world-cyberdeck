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

### Keyboard interception model

The Pi 500+'s built-in keyboard is connected internally over USB and appears
as a standard evdev input device.  The daemon opens this device and
immediately issues an `EVIOCGRAB` ioctl to **exclusively** claim it.  After
the grab, keystrokes are delivered *only* to the daemon — the Pi's local
TTY/desktop never sees them.  The daemon then forwards (and optionally
remaps, expands, or replaces) each keystroke to the USB HID gadget device
(`/dev/hidg0`), so the host computer receives the input.

This is critical for correct operation: without `EVIOCGRAB` every keystroke
would reach both the host and the Pi's own console, causing unintended
side-effects on the Pi.

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
│       ▲           │  EVIOCGRAB ──► │  ┌─────────────┐  │
│       │           │  keymapper ──► │  │ Future:     │  │
│  ┌────┴───────┐   │  HID writer ─► │  │ Vaultwarden │  │
│  │ ECM net    │   └────────────────┘  │ FIDO2       │  │
│  │ Mass store │                       │ Web vault   │  │
│  └────────────┘                       └─────────────┘  │
│                                                         │
│  Physical keyboard (built-in, internally USB-connected) │
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

### 3 · OTP Engine (`keyboard-daemon/src/otp/`) — *Phase 1 placeholder*

| Algorithm | RFC  | Key type          | Counter          |
|-----------|------|-------------------|------------------|
| HOTP      | 4226 | HMAC-SHA1         | monotone counter |
| TOTP      | 6238 | HMAC-SHA1/256/512 | 30 s time steps  |

OTP codes are typed out as if they were normal keyboard input – the daemon
breaks the numeric string into individual key-press/release HID reports.

> **Phase 2 transition:** The built-in OTP engine is a minimal Phase 1
> implementation.  In Phase 2 the daemon will retrieve OTP codes (and other
> secrets) from Vaultwarden via its REST API, making the built-in OTP module
> an offline fallback only.

---

## Data flow

```
Physical key press
        │
        ▼
evdev event  (/dev/input/eventX)
        │
        │  ← EVIOCGRAB: events go ONLY to daemon,
        │    not to Pi's local console
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
| OTP engine      | Rust     | Phase 1 standalone; Phase 2 delegates to Vaultwarden |
| Future: secrets | Vaultwarden | Self-hosted Bitwarden server — OTP, FIDO, credentials, web UI |

---

## Security principles

- **Secrets never leave the Pi.** All secrets (OTP seeds, credentials, FIDO
  keys) are stored on-device.  Phase 2 will integrate
  **[Vaultwarden](https://github.com/dani-garcia/vaultwarden)** — a
  lightweight, self-hosted Bitwarden-compatible server written in Rust — as
  the single secrets backend.  Vaultwarden provides encrypted at-rest storage,
  TOTP generation, FIDO2/WebAuthn credential storage, and a full management
  web vault, all on the Pi itself.
- **No custom crypto for secrets.** Instead of rolling our own vault, we
  delegate to Vaultwarden — a widely-deployed, battle-proven, open-source
  implementation of the Bitwarden protocol.
- **Exclusive keyboard grab.** The daemon issues `EVIOCGRAB` on the evdev
  device so keystrokes reach *only* the USB HID gadget — they never leak to
  the Pi's local console.
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

### Phase 2 — Vaultwarden integration (secrets backend)

Integrate **[Vaultwarden](https://github.com/dani-garcia/vaultwarden)** as
the central secrets backend.  Vaultwarden was selected over other candidates
after evaluating fit for the Pi 500+ use case:

| Candidate     | Pros                                         | Cons                                  |
|---------------|----------------------------------------------|---------------------------------------|
| **Vaultwarden** ✅ | Rust binary, tiny footprint, ARM64 native. Full Bitwarden API: TOTP, FIDO2, credentials, web vault. Active community, widely deployed. | Requires ~50 MB RAM at idle. |
| KeePassXC     | Mature, offline-first.                       | Desktop app only — no server, no web UI, no API for daemon integration. |
| Passwork      | Web UI, self-hosted.                         | PHP stack, heavier dependencies, commercial licensing. |
| pass (Unix)   | Ultra-lightweight, GPG-based.                | CLI only, no web UI, no native FIDO2 or HOTP. |

Vaultwarden gives us TOTP/HOTP, FIDO2/WebAuthn, credential storage, and a
management web vault — all battle-proven and encrypted — from a single Rust
binary that runs natively on the Pi's ARM64 CPU.

| Feature                                           | Status  |
|---------------------------------------------------|---------|
| Install & configure Vaultwarden on the Pi         | Planned |
| Daemon → Vaultwarden REST API integration for OTP | Planned |
| Credential / password storage & type-out via API  | Planned |
| FIDO2 / WebAuthn credential storage               | Planned |
| Migrate seed storage from flat TOML to vault      | Planned |
| Physical or shortcut-based trigger authorization   | Planned |
| Automatic HOTP counter increment + persistence    | Planned |

### Phase 3 — Management UI (via Vaultwarden web vault)

| Feature                                                | Status  |
|--------------------------------------------------------|---------|
| Vaultwarden web vault for token/credential management  | Planned |
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
