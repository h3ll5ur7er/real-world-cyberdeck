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
    /// evdev code → remapped evdev code.
    remap: HashMap<u16, u16>,

    /// evdev code → list of evdev codes (macro expansion).
    macros: HashMap<u16, Vec<u16>>,

    /// evdev code → special action descriptor.
    specials: HashMap<u16, SpecialAction>,

    /// Loaded OTP profiles (`None` if no OTP config file was found).
    otp_config: Option<OtpConfig>,
}

impl KeyMapper {
    /// Build a new mapper from the daemon configuration.
    ///
    /// Eagerly loads the OTP config from `cfg.otp_config` if the file exists.
    pub fn new(cfg: &Config) -> Self {
        let name_to_code = build_name_to_code();

        let mut remap = HashMap::new();
        for (from, to) in &cfg.remap {
            let from_lc = from.to_lowercase();
            let to_lc = to.to_lowercase();
            if let (Some(&fc), Some(&tc)) = (name_to_code.get(&from_lc), name_to_code.get(&to_lc))
            {
                remap.insert(fc, tc);
            } else {
                warn!("Remap: unknown key name {} or {}", from, to);
            }
        }

        let mut macros = HashMap::new();
        for (trigger, keys) in &cfg.macros {
            if let Some(&tc) = name_to_code.get(&trigger.to_lowercase()) {
                let codes: Vec<u16> = keys
                    .iter()
                    .filter_map(|k| {
                        let c = name_to_code.get(&k.to_lowercase());
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
            if let Some(&tc) = name_to_code.get(&trigger.to_lowercase()) {
                specials.insert(tc, action.clone());
            } else {
                warn!("Special: unknown trigger key {}", trigger);
            }
        }

        // Eagerly load OTP config so special functions work at runtime.
        let otp_config = OtpConfig::load(Path::new(&cfg.otp_config))
            .map_err(|e| {
                if !specials.is_empty() {
                    warn!("Could not load OTP config {}: {}", cfg.otp_config, e);
                }
                e
            })
            .ok();

        KeyMapper {
            remap,
            macros,
            specials,
            otp_config,
        }
    }

    /// Override the OTP configuration (builder pattern).
    #[must_use]
    #[allow(dead_code)] // Used in tests; available for external consumers.
    pub fn with_otp_config(mut self, otp: OtpConfig) -> Self {
        self.otp_config = Some(otp);
        self
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
        match action {
            SpecialAction::Totp { profile } => {
                if let Some(cfg) = self.otp_config.as_ref().and_then(|c| c.find_profile(profile)) {
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
                if let Some(cfg) = self.otp_config.as_ref().and_then(|c| c.find_profile(profile)) {
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
}

// ── evdev key-name → code table ──────────────────────────────────────────────

/// Build the mapping from human-readable key names to Linux evdev codes.
///
/// Uses the standard values from `linux/input-event-codes.h`.
/// All names are stored in lowercase for case-insensitive matching.
fn build_name_to_code() -> HashMap<String, u16> {
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

    keys.iter().map(|&(name, code)| (name.to_lowercase(), code)).collect()
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

        // F1 (59) press → H press, H release, I press, I release
        // H = evdev 35, I = evdev 23
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
        assert_eq!(
            actions[2],
            Action::Key {
                code: 23,
                pressed: true
            }
        ); // I=23
        assert_eq!(
            actions[3],
            Action::Key {
                code: 23,
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

    #[test]
    fn test_special_totp() {
        let mut special = HashMap::new();
        special.insert(
            "F2".into(),
            SpecialAction::Totp {
                profile: "test".into(),
            },
        );
        let cfg = test_config(HashMap::new(), HashMap::new(), special);
        let mapper = KeyMapper::new(&cfg).with_otp_config(OtpConfig {
            profiles: vec![crate::config::OtpProfile {
                name: "test".into(),
                otp_type: "totp".into(),
                secret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into(),
                digits: 6,
                period: 30,
                counter: 0,
                algo: "SHA1".into(),
            }],
        });

        // F2 (60) press → should produce a TypeString action
        let actions = mapper.translate(60, true);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::TypeString(s) => assert_eq!(s.len(), 6),
            other => panic!("Expected TypeString, got {:?}", other),
        }

        // F2 release → suppressed
        let actions = mapper.translate(60, false);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_special_hotp() {
        let mut special = HashMap::new();
        special.insert(
            "F3".into(),
            SpecialAction::Hotp {
                profile: "test".into(),
            },
        );
        let cfg = test_config(HashMap::new(), HashMap::new(), special);
        let mapper = KeyMapper::new(&cfg).with_otp_config(OtpConfig {
            profiles: vec![crate::config::OtpProfile {
                name: "test".into(),
                otp_type: "hotp".into(),
                secret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into(),
                digits: 6,
                period: 30,
                counter: 0,
                algo: "SHA1".into(),
            }],
        });

        let actions = mapper.translate(61, true); // F3 = 61
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::TypeString(s) => {
                assert_eq!(s.len(), 6);
                // Counter=0 with the RFC4226 test secret should give "755224"
                assert_eq!(s, "755224");
            }
            other => panic!("Expected TypeString, got {:?}", other),
        }
    }

    #[test]
    fn test_name_to_code_no_duplicates() {
        let m = build_name_to_code();
        // Keys are stored lowercase for case-insensitive matching.
        assert_eq!(m.get("a"), Some(&30u16));
        assert_eq!(m.get("z"), Some(&44u16));
        assert_eq!(m.get("j"), Some(&36u16));
        assert_eq!(m.get("k"), Some(&37u16));
        assert_eq!(m.get("i"), Some(&23u16));
        assert_eq!(m.get("capslock"), Some(&58u16));
        assert_eq!(m.get("leftctrl"), Some(&29u16));
        // Ensure no bogus entries
        assert!(m.get("i_dup").is_none());
    }
}
