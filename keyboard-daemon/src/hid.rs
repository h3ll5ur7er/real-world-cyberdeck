//! USB HID report builder and writer for the keyboard gadget device.
//!
//! Writes 8-byte boot-protocol keyboard reports to `/dev/hidg0`.

use log::debug;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};

/// Boot-protocol keyboard HID report (8 bytes).
///
/// ```text
/// Byte 0: Modifier keys (bit flags)
/// Byte 1: Reserved (0x00)
/// Bytes 2-7: Up to 6 simultaneous key-codes
/// ```
#[derive(Debug, Clone, Default)]
pub struct HidReport {
    pub modifiers: u8,
    pub keys: [u8; 6],
}

impl HidReport {
    /// Serialize the report to an 8-byte array.
    pub fn as_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0] = self.modifiers;
        // buf[1] = 0 (reserved)
        buf[2..8].copy_from_slice(&self.keys);
        buf
    }
}

/// Writes HID reports to the gadget character device.
pub struct HidWriter {
    file: File,
    state: HidReport,
}

impl HidWriter {
    /// Open the HID gadget device for writing.
    pub fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new().write(true).open(path)?;
        Ok(HidWriter {
            file,
            state: HidReport::default(),
        })
    }

    /// Construct a HidWriter from an already-open [`File`] (useful for testing).
    #[cfg(test)]
    pub fn from_file(file: File) -> Self {
        HidWriter {
            file,
            state: HidReport::default(),
        }
    }

    /// Send a key press or release event.
    ///
    /// `code` is the evdev key code; it is translated to the USB HID usage ID
    /// before inclusion in the report.
    pub fn send_key(&mut self, code: u16, pressed: bool) -> io::Result<()> {
        let hid_usage = evdev_to_hid(code);
        if hid_usage == 0 {
            // Modifier key?
            if let Some(bit) = modifier_bit(code) {
                if pressed {
                    self.state.modifiers |= bit;
                } else {
                    self.state.modifiers &= !bit;
                }
            } else {
                debug!("Unmapped evdev code {}", code);
                return Ok(());
            }
        } else if pressed {
            // Add to first empty slot.
            for slot in self.state.keys.iter_mut() {
                if *slot == 0 {
                    *slot = hid_usage;
                    break;
                }
            }
        } else {
            // Remove from slots.
            for slot in self.state.keys.iter_mut() {
                if *slot == hid_usage {
                    *slot = 0;
                    break;
                }
            }
        }

        self.flush_report()
    }

    /// Type a string of ASCII digits / letters as sequential HID key presses.
    ///
    /// Each character is a press+release cycle with an empty report in between
    /// to ensure the host registers each keystroke individually.
    pub fn type_string(&mut self, s: &str) -> io::Result<()> {
        for ch in s.chars() {
            let (hid_usage, need_shift) = char_to_hid(ch);
            if hid_usage == 0 {
                continue;
            }

            // Press
            self.state = HidReport::default();
            if need_shift {
                self.state.modifiers = 0x02; // Left Shift
            }
            self.state.keys[0] = hid_usage;
            self.flush_report()?;

            // Release
            self.state = HidReport::default();
            self.flush_report()?;
        }
        Ok(())
    }

    fn flush_report(&mut self) -> io::Result<()> {
        let bytes = self.state.as_bytes();
        self.file.write_all(&bytes)?;
        self.file.flush()
    }
}

// ── evdev → USB HID usage translation ────────────────────────────────────────

/// Convert an evdev key code to its USB HID keyboard usage ID.
///
/// Returns 0 for modifier keys (handled separately) or unknown codes.
pub fn evdev_to_hid(evdev: u16) -> u8 {
    match evdev {
        // Letters (KEY_A=30 → HID 0x04, KEY_B=48 → 0x05, etc.)
        // We must map by the actual evdev scancode, not alphabetically.
        30 => 0x04, // A
        48 => 0x05, // B
        46 => 0x06, // C
        32 => 0x07, // D
        18 => 0x08, // E
        33 => 0x09, // F
        34 => 0x0A, // G
        35 => 0x0B, // H
        23 => 0x0C, // I
        36 => 0x0D, // J
        37 => 0x0E, // K
        38 => 0x0F, // L
        50 => 0x10, // M
        49 => 0x11, // N
        24 => 0x12, // O
        25 => 0x13, // P
        16 => 0x14, // Q
        19 => 0x15, // R
        31 => 0x16, // S
        20 => 0x17, // T
        22 => 0x18, // U
        47 => 0x19, // V
        17 => 0x1A, // W
        45 => 0x1B, // X
        21 => 0x1C, // Y
        44 => 0x1D, // Z

        // Digits (1-9, 0)
        2 => 0x1E,  // 1
        3 => 0x1F,  // 2
        4 => 0x20,  // 3
        5 => 0x21,  // 4
        6 => 0x22,  // 5
        7 => 0x23,  // 6
        8 => 0x24,  // 7
        9 => 0x25,  // 8
        10 => 0x26, // 9
        11 => 0x27, // 0

        // Common keys
        28 => 0x28, // Return
        1 => 0x29,  // Escape
        14 => 0x2A, // BackSpace
        15 => 0x2B, // Tab
        57 => 0x2C, // Space
        12 => 0x2D, // Minus
        13 => 0x2E, // Equal
        26 => 0x2F, // LeftBrace [
        27 => 0x30, // RightBrace ]
        43 => 0x31, // BackSlash
        39 => 0x33, // Semicolon
        40 => 0x34, // Apostrophe
        41 => 0x35, // Grave `
        51 => 0x36, // Comma
        52 => 0x37, // Dot
        53 => 0x38, // Slash

        58 => 0x39, // CapsLock

        // Function keys
        59 => 0x3A,  // F1
        60 => 0x3B,  // F2
        61 => 0x3C,  // F3
        62 => 0x3D,  // F4
        63 => 0x3E,  // F5
        64 => 0x3F,  // F6
        65 => 0x40,  // F7
        66 => 0x41,  // F8
        67 => 0x42,  // F9
        68 => 0x43,  // F10
        87 => 0x44,  // F11
        88 => 0x45,  // F12

        // Navigation
        110 => 0x49, // Insert
        102 => 0x4A, // Home
        104 => 0x4B, // PageUp
        111 => 0x4C, // Delete
        107 => 0x4D, // End
        109 => 0x4E, // PageDown
        106 => 0x4F, // Right
        105 => 0x50, // Left
        108 => 0x51, // Down
        103 => 0x52, // Up

        // Modifiers return 0 – handled by modifier_bit()
        29 | 97 | 42 | 54 | 56 | 100 | 125 | 126 => 0,

        _ => 0,
    }
}

/// Return the modifier bit for a modifier evdev key, or None.
pub fn modifier_bit(evdev: u16) -> Option<u8> {
    match evdev {
        29 => Some(0x01),  // LeftCtrl
        42 => Some(0x02),  // LeftShift
        56 => Some(0x04),  // LeftAlt
        125 => Some(0x08), // LeftMeta
        97 => Some(0x10),  // RightCtrl
        54 => Some(0x20),  // RightShift
        100 => Some(0x40), // RightAlt
        126 => Some(0x80), // RightMeta
        _ => None,
    }
}

/// Map an ASCII character to (HID usage, needs_shift).
pub fn char_to_hid(ch: char) -> (u8, bool) {
    match ch {
        'a'..='z' => (0x04 + (ch as u8 - b'a'), false),
        'A'..='Z' => (0x04 + (ch as u8 - b'A'), true),
        '1'..='9' => (0x1E + (ch as u8 - b'1'), false),
        '0' => (0x27, false),
        ' ' => (0x2C, false),
        '\n' => (0x28, false),
        '-' => (0x2D, false),
        '=' => (0x2E, false),
        '[' => (0x2F, false),
        ']' => (0x30, false),
        '\\' => (0x31, false),
        ';' => (0x33, false),
        '\'' => (0x34, false),
        '`' => (0x35, false),
        ',' => (0x36, false),
        '.' => (0x37, false),
        '/' => (0x38, false),
        '!' => (0x1E, true),
        '@' => (0x1F, true),
        '#' => (0x20, true),
        '$' => (0x21, true),
        '%' => (0x22, true),
        '^' => (0x23, true),
        '&' => (0x24, true),
        '*' => (0x25, true),
        '(' => (0x26, true),
        ')' => (0x27, true),
        _ => (0, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_bytes() {
        let mut r = HidReport::default();
        r.modifiers = 0x02; // LeftShift
        r.keys[0] = 0x04; // A
        let bytes = r.as_bytes();
        assert_eq!(bytes[0], 0x02);
        assert_eq!(bytes[1], 0x00);
        assert_eq!(bytes[2], 0x04);
        assert_eq!(bytes[3..8], [0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_evdev_to_hid_letters() {
        assert_eq!(evdev_to_hid(30), 0x04); // A
        assert_eq!(evdev_to_hid(44), 0x1D); // Z
    }

    #[test]
    fn test_evdev_to_hid_digits() {
        assert_eq!(evdev_to_hid(2), 0x1E); // 1
        assert_eq!(evdev_to_hid(11), 0x27); // 0
    }

    #[test]
    fn test_modifier_bit() {
        assert_eq!(modifier_bit(29), Some(0x01)); // LeftCtrl
        assert_eq!(modifier_bit(42), Some(0x02)); // LeftShift
        assert_eq!(modifier_bit(200), None);
    }

    #[test]
    fn test_char_to_hid() {
        assert_eq!(char_to_hid('a'), (0x04, false));
        assert_eq!(char_to_hid('A'), (0x04, true));
        assert_eq!(char_to_hid('1'), (0x1E, false));
        assert_eq!(char_to_hid('0'), (0x27, false));
        assert_eq!(char_to_hid(' '), (0x2C, false));
    }

    #[test]
    fn test_hid_writer_send_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hidg0");
        let file = File::create(&path).unwrap();
        let mut writer = HidWriter::from_file(file);

        // Press 'A'
        writer.send_key(30, true).unwrap();
        // Release 'A'
        writer.send_key(30, false).unwrap();

        // Read back
        let data = std::fs::read(&path).unwrap();
        assert_eq!(data.len(), 16); // 2 reports × 8 bytes
        // First report: A pressed
        assert_eq!(data[2], 0x04);
        // Second report: A released (all zeros)
        assert_eq!(data[8..16], [0; 8]);
    }

    #[test]
    fn test_hid_writer_type_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hidg0");
        let file = File::create(&path).unwrap();
        let mut writer = HidWriter::from_file(file);

        writer.type_string("12").unwrap();

        let data = std::fs::read(&path).unwrap();
        // 2 chars × 2 reports (press + release) × 8 bytes = 32
        assert_eq!(data.len(), 32);
        // First press: digit '1' → HID 0x1E
        assert_eq!(data[2], 0x1E);
    }
}
