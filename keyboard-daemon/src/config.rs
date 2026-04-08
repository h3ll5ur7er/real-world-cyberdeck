//! Configuration loading for the cyberdeck keyboard daemon.
//!
//! Reads and deserializes the TOML-based keymap and OTP config files.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Top-level daemon configuration (parsed from `keymap.toml`).
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Path to the evdev keyboard device.
    pub keyboard_device: String,

    /// Path to the USB HID gadget device.
    pub hid_device: String,

    /// Path to the OTP profile configuration file.
    #[serde(default = "default_otp_config")]
    pub otp_config: String,

    /// Key → key remappings.
    #[serde(default)]
    pub remap: HashMap<String, String>,

    /// Key → macro (list of key names to type in sequence).
    #[serde(default)]
    pub macros: HashMap<String, Vec<String>>,

    /// Key → special function.
    #[serde(default)]
    pub special: HashMap<String, SpecialAction>,
}

fn default_otp_config() -> String {
    "/etc/cyberdeck/otp.toml".to_string()
}

/// A special-function binding.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SpecialAction {
    /// Generate and type a TOTP code.
    Totp { profile: String },
    /// Generate and type an HOTP code.
    Hotp { profile: String },
}

/// OTP profile configuration (parsed from `otp.toml`).
#[derive(Debug, Deserialize)]
pub struct OtpConfig {
    #[serde(default)]
    pub profiles: Vec<OtpProfile>,
}

/// A single OTP profile entry.
#[derive(Debug, Clone, Deserialize)]
pub struct OtpProfile {
    /// Human-readable profile name (referenced by `SpecialAction`).
    pub name: String,

    /// `"totp"` or `"hotp"`.
    #[serde(rename = "type")]
    #[allow(dead_code)] // Deserialized metadata; used by external tooling.
    pub otp_type: String,

    /// Base-32 encoded secret (no padding).
    pub secret: String,

    /// Number of digits in the OTP code.
    #[serde(default = "default_digits")]
    pub digits: u32,

    /// TOTP period in seconds (ignored for HOTP).
    #[serde(default = "default_period")]
    pub period: u64,

    /// HOTP counter (ignored for TOTP).
    #[serde(default)]
    pub counter: u64,

    /// Hash algorithm: SHA1, SHA256, SHA512.
    #[serde(default = "default_algo")]
    pub algo: String,
}

fn default_digits() -> u32 {
    6
}
fn default_period() -> u64 {
    30
}
fn default_algo() -> String {
    "SHA1".to_string()
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| format!("parse {}: {}", path.display(), e))
    }
}

impl OtpConfig {
    /// Load OTP configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| format!("parse {}: {}", path.display(), e))
    }

    /// Look up a profile by name.
    pub fn find_profile(&self, name: &str) -> Option<&OtpProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_minimal_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keymap.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
keyboard_device = "/dev/input/event0"
hid_device = "/dev/hidg0"
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.keyboard_device, "/dev/input/event0");
        assert_eq!(cfg.hid_device, "/dev/hidg0");
        assert!(cfg.remap.is_empty());
        assert!(cfg.macros.is_empty());
        assert!(cfg.special.is_empty());
    }

    #[test]
    fn test_load_full_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keymap.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
keyboard_device = "/dev/input/event0"
hid_device = "/dev/hidg0"
otp_config = "/tmp/otp.toml"

[remap]
CapsLock = "LeftCtrl"

[macros]
F1 = ["H", "e", "l", "l", "o"]

[special]
F2 = {{ type = "totp", profile = "gh" }}
F3 = {{ type = "hotp", profile = "vpn" }}
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.remap.get("CapsLock").unwrap(), "LeftCtrl");
        assert_eq!(cfg.macros.get("F1").unwrap().len(), 5);
        assert!(matches!(
            cfg.special.get("F2").unwrap(),
            SpecialAction::Totp { profile } if profile == "gh"
        ));
        assert!(matches!(
            cfg.special.get("F3").unwrap(),
            SpecialAction::Hotp { profile } if profile == "vpn"
        ));
    }

    #[test]
    fn test_load_otp_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("otp.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[[profiles]]
name = "email"
type = "totp"
secret = "JBSWY3DPEHPK3PXP"
digits = 6
period = 30
algo = "SHA1"

[[profiles]]
name = "vpn"
type = "hotp"
secret = "JBSWY3DPEHPK3PXP"
digits = 6
counter = 42
"#
        )
        .unwrap();

        let otp = OtpConfig::load(&path).unwrap();
        assert_eq!(otp.profiles.len(), 2);
        assert_eq!(otp.find_profile("email").unwrap().otp_type, "totp");
        assert_eq!(otp.find_profile("vpn").unwrap().counter, 42);
        assert!(otp.find_profile("nonexistent").is_none());
    }
}
