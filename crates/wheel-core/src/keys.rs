//! Keyboard shortcuts as text (`"Ctrl+Shift+V"`) and as Windows virtual-key codes.

/// Virtual-key code of Shift.
pub const VK_SHIFT: u16 = 0x10;
/// Virtual-key code of Ctrl.
pub const VK_CONTROL: u16 = 0x11;
/// Virtual-key code of Alt.
pub const VK_MENU: u16 = 0x12;
/// Virtual-key code of the left Windows key.
pub const VK_LWIN: u16 = 0x5B;

/// A parsed shortcut: the modifiers (in the order written) and the single main key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    /// Virtual-key codes of the modifiers.
    pub modifiers: Vec<u16>,
    /// Virtual-key code of the main key.
    pub key: u16,
}

fn modifier(name: &str) -> Option<u16> {
    Some(match name {
        "ctrl" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" => VK_MENU,
        "win" => VK_LWIN,
        _ => return None,
    })
}

/// Virtual-key code of a non-modifier key name (already lowercase).
fn key_code(name: &str) -> Option<u16> {
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        let c = bytes[0];
        if c.is_ascii_lowercase() {
            return Some(u16::from(c.to_ascii_uppercase()));
        }
        if c.is_ascii_digit() {
            return Some(u16::from(c));
        }
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u16>().ok())
        && (1..=24).contains(&n)
        && !name.starts_with("f0")
    {
        return Some(0x70 + n - 1);
    }
    Some(match name {
        "enter" => 0x0D,
        "esc" => 0x1B,
        "tab" => 0x09,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "printscreen" => 0x2C,
        "volumemute" => 0xAD,
        "volumedown" => 0xAE,
        "volumeup" => 0xAF,
        "medianext" => 0xB0,
        "mediaprev" => 0xB1,
        "mediaplaypause" => 0xB3,
        _ => return None,
    })
}

/// Parses `"Ctrl+Shift+V"`, `"Win+V"`, `"F2"` and so on. Case does not matter, spaces around `+`
/// are allowed. Exactly one key that is not a modifier is required.
pub fn parse_shortcut(text: &str) -> Result<Shortcut, String> {
    if text.trim().is_empty() {
        return Err("empty shortcut".to_string());
    }
    let mut modifiers = Vec::new();
    let mut key = None;
    for part in text.split('+') {
        let name = part.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(format!("empty element in shortcut \"{text}\""));
        }
        if let Some(vk) = modifier(&name) {
            if modifiers.contains(&vk) {
                return Err(format!(
                    "repeated modifier \"{}\" in \"{text}\"",
                    part.trim()
                ));
            }
            modifiers.push(vk);
        } else if let Some(vk) = key_code(&name) {
            if key.replace(vk).is_some() {
                return Err(format!("more than one key in \"{text}\""));
            }
        } else {
            return Err(format!("unknown key \"{}\" in \"{text}\"", part.trim()));
        }
    }
    match key {
        Some(key) => Ok(Shortcut { modifiers, key }),
        None => Err(format!("no key in \"{text}\" (only modifiers)")),
    }
}

/// Whether a key needs `KEYEVENTF_EXTENDEDKEY` when sent with `SendInput`.
pub fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x28 // PageUp, PageDown, End, Home, arrows
        | 0x2C..=0x2E // PrintScreen, Insert, Delete
        | 0x5B..=0x5D // Windows keys, Apps
        | 0xA6..=0xB7 // browser, volume and media keys, launchers
        | 0x6F // numpad divide
        | 0x90 // NumLock
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Shortcut {
        parse_shortcut(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    #[test]
    fn valid_shortcuts() {
        assert_eq!(
            ok("Ctrl+Shift+V"),
            Shortcut {
                modifiers: vec![VK_CONTROL, VK_SHIFT],
                key: u16::from(b'V')
            }
        );
        assert_eq!(ok("Win+V").modifiers, vec![VK_LWIN]);
        assert_eq!(ok("Alt+Enter").key, 0x0D);
        assert_eq!(ok("F2").key, 0x71);
        assert_eq!(ok("F24").key, 0x87);
        assert_eq!(ok("Delete").key, 0x2E);
        assert_eq!(ok("Del"), ok("Delete"));
        assert_eq!(ok("Ctrl+5").key, u16::from(b'5'));
        assert_eq!(ok("MediaPlayPause").key, 0xB3);
        assert!(ok("F2").modifiers.is_empty());
    }

    #[test]
    fn case_and_spaces() {
        assert_eq!(ok("ctrl+shift+v"), ok("Ctrl+Shift+V"));
        assert_eq!(ok("CTRL + SHIFT +  V"), ok("Ctrl+Shift+V"));
        assert_eq!(ok("  pageUP "), ok("PageUp"));
    }

    #[test]
    fn errors() {
        for bad in [
            "",
            "  ",
            "Ctrl+",
            "+V",
            "Ctrl++V",
            "Ctrl+A+B",
            "F1+F2",
            "Ctrl",
            "Ctrl+Shift",
            "Foo",
            "Ctrl+Foo",
            "Ctrl+Ctrl+V",
            "ctrl+CTRL+V",
            "F0",
            "F25",
            "F+1",
            "AB",
        ] {
            assert!(parse_shortcut(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn extended_keys() {
        for name in [
            "Up",
            "Down",
            "Left",
            "Right",
            "Insert",
            "Delete",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "VolumeMute",
            "MediaNext",
        ] {
            assert!(is_extended(ok(name).key), "{name}");
        }
        assert!(is_extended(VK_LWIN));
        for name in ["A", "Enter", "F2", "Space", "Tab"] {
            assert!(!is_extended(ok(name).key), "{name}");
        }
        assert!(!is_extended(VK_CONTROL));
    }
}
