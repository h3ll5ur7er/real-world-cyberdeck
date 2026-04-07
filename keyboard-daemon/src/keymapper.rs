//! Key mapping engine for the cyberdeck keyboard daemon.
//!
//! Translates evdev key codes into actions (single key, macro, or special
//! function) based on the loaded configuration.

use crate::config::{Config, OtpConfig, SpecialAction};
use crate::otp;
use log::warn;
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// An action to be executed by the HID writer.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Send a single key press or release.
    Key { code: u16, pressed: bool },
    /// Type a string as a sequence of HID key-presses (digits, etc.).
    TypeString(String),
}

/// Maps evdev key codes → actions using the loaded configuration.
pub struct KeyMapper {
    /// key-name → evdev code (reverse of EVDEV_NAMES).
    name_to_code: HashMap<String, u16>,

    /// evdev code → remapped evdev code.
    remap: HashMap<u16, u16>,

    /// evdev code → list of evdev codes (macro expansion).
    macros: HashMap<u16, Vec<u16>>,

    /// evdev code → special action descriptor.
    specials: HashMap<u16, SpecialAction>,

    /// Loaded OTP profiles (lazy; `None` until first OTP use).
    otp_config: Option<OtpConfig>,

    /// Path to the OTP config file.
    otp_config_path: String,
}

impl KeyMapper {
    /// Build a new mapper from the daemon configuration.
    pub fn new(cfg: &Config) -> Self {
        let name_to_code = build_name_to_code();

        let mut remap = HashMap::new();
        for (from, to) in &cfg.remap {
            if let (Some(&fc), Some(&tc)) = (name_to_code.get(from), name_to_code.get(to)) {
                remap.insert(fc, tc);
            } else {
                warn!("Remap: unknown key name {} or {}", from, to);
            }
        }

        let mut macros = HashMap::new();
        for (trigger, keys) in &cfg.macros {
            if let Some(&tc) = name_to_code.get(trigger) {
                let codes: Vec<u16> = keys
                    .iter()
                    .filter_map(|k| {
                        let c = name_to_code.get(k);
                        if c.is_none() {
                            warn!("Macro: unknown key name {}", k);
                        }
                        c.copied()
                    })
                    .collect();
                macros.insert(tc, codes);
            } else {
                warn!("Macro: unknown trigger key {}", trigger);
            }
        }

        let mut specials = HashMap::new();
        for (trigger, action) in &cfg.special {
            if let Some(&tc) = name_to_code.get(trigger) {
                specials.insert(tc, action.clone());
            } else {
                warn!("Special: unknown trigger key {}", trigger);
            }
        }

        KeyMapper {
            name_to_code,
            remap,
            macros,
            specials,
            otp_config: None,
            otp_config_path: cfg.otp_config.clone(),
        }
    }

    /// Translate an evdev key event into zero or more [`Action`]s.
    pub fn translate(&self, code: u16, is_press: bool) -> Vec<Action> {
        // 1. Special functions (only fire on press).
        if is_press {
            if let Some(special) = self.specials.get(&code) {
                return self.handle_special(special);
            }
        }
        // Suppress release for special keys too.
        if self.specials.contains_key(&code) {
            return vec![];
        }

        // 2. Macros (only fire on press).
        if is_press {
            if let Some(seq) = self.macros.get(&code) {
                return seq
                    .iter()
                    .flat_map(|&c| {
                        vec![
                            Action::Key {
                                code: c,
                                pressed: true,
                            },
                            Action::Key {
                                code: c,
                                pressed: false,
                            },
                        ]
                    })
                    .collect();
            }
        }
        // Suppress release for macro keys too.
        if self.macros.contains_key(&code) {
            return vec![];
        }

        // 3. Simple remap.
        let mapped = self.remap.get(&code).copied().unwrap_or(code);
        vec![Action::Key {
            code: mapped,
            pressed: is_press,
        }]
    }

    /// Execute a special action and return the resulting actions.
    fn handle_special(&self, action: &SpecialAction) -> Vec<Action> {
        let otp_cfg = self.get_otp_config();
        match action {
            SpecialAction::Totp { profile } => {
                if let Some(cfg) = otp_cfg.and_then(|c| c.find_profile(profile)) {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    match otp::totp::generate_totp(
                        &cfg.secret, now, cfg.period, cfg.digits, &cfg.algo,
                    ) {
                        Ok(code) => vec![Action::TypeString(code)],
                        Err(e) => {
                            warn!("TOTP generation failed for {}: {}", profile, e);
                            vec![]
                        }
                    }
                } else {
                    warn!("OTP profile '{}' not found", profile);
                    vec![]
                }
            }
            SpecialAction::Hotp { profile } => {
                if let Some(cfg) = otp_cfg.and_then(|c| c.find_profile(profile)) {
                    match otp::hotp::generate_hotp(&cfg.secret, cfg.counter, cfg.digits) {
                        Ok(code) => vec![Action::TypeString(code)],
                        Err(e) => {
                            warn!("HOTP generation failed for {}: {}", profile, e);
                            vec![]
                        }
                    }
                } else {
                    warn!("OTP profile '{}' not found", profile);
                    vec![]
                }
            }
        }
    }

    /// Lazily load the OTP config.
    fn get_otp_config(&self) -> Option<&OtpConfig> {
        // In production this would use interior mutability (OnceCell / Mutex).
        // For now we try to load on each call if not cached.
        // The caller holds &self so we return a reference to the Option.
        if self.otp_config.is_some() {
            return self.otp_config.as_ref();
        }
        // We cannot mutate self here; the runtime will load it once at
        // construction if the file exists.  See `with_otp_config` below.
        None
    }

    /// Construct a KeyMapper with pre-loaded OTP config (for testing / init).
    pub fn with_otp_config(mut self, path: &Path) -> Self {
        match OtpConfig::load(path) {
            Ok(c) => self.otp_config = Some(c),
            Err(e) => warn!("Could not load OTP config: {}", e),
        }
        self
    }
}

// ── evdev key-name → code table ──────────────────────────────────────────────

/// Build the mapping from human-readable key names to Linux evdev codes.
///
/// This is a representative subset; extend as needed.
pub fn build_name_to_code() -> HashMap<String, u16> {
    let mut m = HashMap::new();
    // Letters
    for (i, c) in ('A'..='Z').enumerate() {
        m.insert(c.to_string(), 30 + i as u16); // KEY_A = 30 .. KEY_Z = 55 (approx)
    }
    // Correct well-known evdev codes for the alphabet:
    //   KEY_A=30, KEY_B=48, KEY_C=46, ... (QWERTY scan order)
    // We'll use the standard Linux input-event-codes.h values.
    m.clear();
    let keys: &[(&str, u16)] = &[
        ("Escape", 1),
        ("1", 2),
        ("2", 3),
        ("3", 4),
        ("4", 5),
        ("5", 6),
        ("6", 7),
        ("7", 8),
        ("8", 9),
        ("9", 10),
        ("0", 11),
        ("Minus", 12),
        ("Equal", 13),
        ("BackSpace", 14),
        ("Tab", 15),
        ("Q", 16),
        ("W", 17),
        ("E", 18),
        ("R", 19),
        ("T", 20),
        ("Y", 21),
        ("U", 22),
        ("I", 23),
        ("O", 24),
        ("P", 25),
        ("LeftBrace", 26),
        ("RightBrace", 27),
        ("Return", 28),
        ("LeftCtrl", 29),
        ("A", 30),
        ("S", 31),
        ("D", 32),
        ("F", 33),
        ("G", 34),
        ("H", 35),
        ("I_dup", 36), // placeholder
        ("J", 36),
        ("K", 37),
        ("L", 38),
        ("Semicolon", 39),
        ("Apostrophe", 40),
        ("Grave", 41),
        ("LeftShift", 42),
        ("BackSlash", 43),
        ("Z", 44),
        ("X", 45),
        ("C", 46),
        ("V", 47),
        ("B", 48),
        ("N", 49),
        ("M", 50),
        ("Comma", 51),
        ("Dot", 52),
        ("Slash", 53),
        ("RightShift", 54),
        ("KPAsterisk", 55),
        ("LeftAlt", 56),
        ("Space", 57),
        ("CapsLock", 58),
        ("F1", 59),
        ("F2", 60),
        ("F3", 61),
        ("F4", 62),
        ("F5", 63),
        ("F6", 64),
        ("F7", 65),
        ("F8", 66),
        ("F9", 67),
        ("F10", 68),
        ("NumLock", 69),
        ("ScrollLock", 70),
        ("F11", 87),
        ("F12", 88),
        ("RightCtrl", 97),
        ("RightAlt", 100),
        ("Home", 102),
        ("Up", 103),
        ("PageUp", 104),
        ("Left", 105),
        ("Right", 106),
        ("End", 107),
        ("Down", 108),
        ("PageDown", 109),
        ("Insert", 110),
        ("Delete", 111),
        ("LeftMeta", 125),
        ("RightMeta", 126),
        ("F13", 183),
        ("F14", 184),
        ("F15", 185),
        ("F16", 186),
        ("F17", 187),
        ("F18", 188),
        ("F19", 189),
        ("F20", 190),
        ("F21", 191),
        ("F22", 192),
        ("F23", 193),
        ("F24", 194),
    ];

    for &(name, code) in keys {
        m.insert(name.to_string(), code);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal Config for testing.
    fn test_config(
        remap: HashMap<String, String>,
        macros: HashMap<String, Vec<String>>,
        special: HashMap<String, SpecialAction>,
    ) -> Config {
        Config {
            keyboard_device: "/dev/null".into(),
            hid_device: "/dev/null".into(),
            otp_config: "/dev/null".into(),
            remap,
            macros,
            special,
        }
    }

    #[test]
    fn test_passthrough() {
        let cfg = test_config(HashMap::new(), HashMap::new(), HashMap::new());
        let mapper = KeyMapper::new(&cfg);
        let actions = mapper.translate(30, true); // KEY_A press
        assert_eq!(
            actions,
            vec![Action::Key {
                code: 30,
                pressed: true
            }]
        );
    }

    #[test]
    fn test_remap() {
        let mut remap = HashMap::new();
        remap.insert("CapsLock".into(), "LeftCtrl".into());
        let cfg = test_config(remap, HashMap::new(), HashMap::new());
        let mapper = KeyMapper::new(&cfg);

        // CapsLock (58) → LeftCtrl (29)
        let actions = mapper.translate(58, true);
        assert_eq!(
            actions,
            vec![Action::Key {
                code: 29,
                pressed: true
            }]
        );

        let actions = mapper.translate(58, false);
        assert_eq!(
            actions,
            vec![Action::Key {
                code: 29,
                pressed: false
            }]
        );
    }

    #[test]
    fn test_macro_expansion() {
        let mut macros = HashMap::new();
        macros.insert("F1".into(), vec!["H".into(), "I".into()]);
        let cfg = test_config(HashMap::new(), macros, HashMap::new());
        let mapper = KeyMapper::new(&cfg);

        // F1 press → H press, H release, I press, I release
        let actions = mapper.translate(59, true);
        assert_eq!(actions.len(), 4);
        assert_eq!(
            actions[0],
            Action::Key {
                code: 35,
                pressed: true
            }
        ); // H=35
        assert_eq!(
            actions[1],
            Action::Key {
                code: 35,
                pressed: false
            }
        );

        // F1 release → suppressed
        let actions = mapper.translate(59, false);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_unmapped_key_passthrough() {
        let cfg = test_config(HashMap::new(), HashMap::new(), HashMap::new());
        let mapper = KeyMapper::new(&cfg);

        // Random code 200 should pass through.
        let actions = mapper.translate(200, true);
        assert_eq!(
            actions,
            vec![Action::Key {
                code: 200,
                pressed: true
            }]
        );
    }
}
