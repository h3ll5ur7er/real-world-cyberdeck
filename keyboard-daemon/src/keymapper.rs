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
        ("escape", 1),
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
        ("minus", 12),
        ("equal", 13),
        ("backspace", 14),
        ("tab", 15),
        ("q", 16),
        ("w", 17),
        ("e", 18),
        ("r", 19),
        ("t", 20),
        ("y", 21),
        ("u", 22),
        ("i", 23),
        ("o", 24),
        ("p", 25),
        ("leftbrace", 26),
        ("rightbrace", 27),
        ("return", 28),
        ("leftctrl", 29),
        ("a", 30),
        ("s", 31),
        ("d", 32),
        ("f", 33),
        ("g", 34),
        ("h", 35),
        ("j", 36),
        ("k", 37),
        ("l", 38),
        ("semicolon", 39),
        ("apostrophe", 40),
        ("grave", 41),
        ("leftshift", 42),
        ("backslash", 43),
        ("z", 44),
        ("x", 45),
        ("c", 46),
        ("v", 47),
        ("b", 48),
        ("n", 49),
        ("m", 50),
        ("comma", 51),
        ("dot", 52),
        ("slash", 53),
        ("rightshift", 54),
        ("kpasterisk", 55),
        ("leftalt", 56),
        ("space", 57),
        ("capslock", 58),
        ("f1", 59),
        ("f2", 60),
        ("f3", 61),
        ("f4", 62),
        ("f5", 63),
        ("f6", 64),
        ("f7", 65),
        ("f8", 66),
        ("f9", 67),
        ("f10", 68),
        ("numlock", 69),
        ("scrolllock", 70),
        ("f11", 87),
        ("f12", 88),
        ("rightctrl", 97),
        ("rightalt", 100),
        ("home", 102),
        ("up", 103),
        ("pageup", 104),
        ("left", 105),
        ("right", 106),
        ("end", 107),
        ("down", 108),
        ("pagedown", 109),
        ("insert", 110),
        ("delete", 111),
        ("leftmeta", 125),
        ("rightmeta", 126),
        ("f13", 183),
        ("f14", 184),
        ("f15", 185),
        ("f16", 186),
        ("f17", 187),
        ("f18", 188),
        ("f19", 189),
        ("f20", 190),
        ("f21", 191),
        ("f22", 192),
        ("f23", 193),
        ("f24", 194),
    ];

    keys.iter().map(|&(name, code)| (name.to_string(), code)).collect()
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
