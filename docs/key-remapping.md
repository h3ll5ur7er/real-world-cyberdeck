# Key Remapping Reference

The keyboard daemon reads `/etc/cyberdeck/keymap.toml` (or the path supplied
via `--config`) at startup. Changes take effect on the next daemon restart.

---

## File structure

```toml
# Path to the evdev keyboard device to intercept.
keyboard_device = "/dev/input/by-id/usb-Raspberry_Pi_keyboard-event-kbd"

# Path to the USB HID gadget device.
hid_device = "/dev/hidg0"

# Path to the OTP configuration file.
otp_config = "/etc/cyberdeck/otp.toml"

[remap]
# key → key remappings (both sides are Linux key names)

[macros]
# key → macro: pressing the key types a sequence of keys

[special]
# key → special: pressing the key triggers a built-in function
```

---

## Key names

Key names follow the Linux `evdev` naming convention (without the `KEY_`
prefix):

| Name        | Key                |
|-------------|--------------------|
| `A` – `Z`  | Letter keys        |
| `1` – `0`  | Top-row digit keys |
| `Return`    | Enter              |
| `Space`     | Space bar          |
| `Tab`       | Tab                |
| `BackSpace` | Backspace          |
| `Escape`    | Escape             |
| `LeftCtrl`  | Left Control       |
| `RightCtrl` | Right Control      |
| `LeftShift` | Left Shift         |
| `RightShift`| Right Shift        |
| `LeftAlt`   | Left Alt           |
| `RightAlt`  | Right Alt / AltGr  |
| `LeftMeta`  | Left Super/Win     |
| `CapsLock`  | Caps Lock          |
| `F1`–`F24`  | Function keys      |
| `Up`, `Down`, `Left`, `Right` | Arrow keys |

---

## `[remap]` – key → key

Replace one key with another, transparently.

```toml
[remap]
CapsLock = "LeftCtrl"
```

---

## `[macros]` – key → sequence of keys

Press one key; the daemon types a list of keys in order (each is a full
press+release cycle).

```toml
[macros]
F1 = ["H", "e", "l", "l", "o", "Return"]
```

---

## `[special]` – key → special function

Each entry maps a key to a table with a `type` field.

### TOTP – time-based one-time password

```toml
[special]
F4 = { type = "totp", profile = "github" }
```

When F4 is pressed the daemon looks up the `github` profile in
`otp.toml`, generates the current TOTP code and types it out as digit
key-presses.

### HOTP – HMAC-based one-time password

```toml
[special]
F5 = { type = "hotp", profile = "vpn" }
```

> **Note:** HOTP counter management is not yet automated. After each use you
> must manually increment the `counter` value in `/etc/cyberdeck/otp.toml` and
> restart the daemon. Automatic counter persistence is planned for a future
> release.

---

## Full example

```toml
keyboard_device = "/dev/input/by-id/usb-Raspberry_Pi_keyboard-event-kbd"
hid_device      = "/dev/hidg0"
otp_config      = "/etc/cyberdeck/otp.toml"

[remap]
CapsLock = "LeftCtrl"

[macros]
F1 = ["H", "e", "l", "l", "o", "Space", "W", "o", "r", "l", "d", "Return"]

[special]
F2 = { type = "totp", profile = "github" }
F3 = { type = "hotp", profile = "vpn" }
```
