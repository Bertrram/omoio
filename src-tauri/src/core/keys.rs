//! The keys and mouse inputs a keyboard-and-mouse player can be given, and
//! the layout they start with.
//!
//! A key is named by where it sits, as a web page sees it in
//! `KeyboardEvent.code` (W3C UI Events KeyboardEvent code values): "KeyW" is
//! the key right of Q on a US keyboard, which is Z on a French one and still
//! W on a Danish or German one. So a layout means the same keys on every
//! keyboard, and each emulator's writer turns a key into that emulator's own
//! name for it, by its place where the emulator names keys by place, or
//! through the keyboard layout Windows has now where it names them by what
//! they print. Each key carries its scan code for that, set 1 as Windows
//! reports it in `WM_KEYDOWN`'s lParam, with 0xE0 in the high byte for the
//! extended keys (Microsoft's "Keyboard Input Overview", Scan codes).
//!
//! The mouse has names of its own here, which no key has: its buttons, the
//! wheel, and the mouse moving, which turns a stick where an emulator can.

/// Every key a player can be given, by its `KeyboardEvent.code` name, with
/// its scan code.
///
/// Left out on purpose: Insert and Tab, which open the portal menu
/// (portal_menu.rs, `MENU_KEY` and `SECOND_MENU_KEY`), and F11, which is Omoio's fullscreen
/// (session.rs). The other function keys, Escape, Ctrl, Alt and the Windows
/// keys are left out too, as the emulators and Windows itself give them jobs
/// of their own while a game runs, read in each emulator's source on 20
/// October 2026: Escape leaves fullscreen in RPCS3 and Cemu and stops the
/// game in Dolphin, Alt with Enter changes fullscreen in all three, Ctrl
/// with S saves a state in RPCS3 and restarts the game from it, Ctrl with R
/// restarts it, Ctrl with P opens Dolphin's Skylanders portal, and F10, F12
/// and Dolphin's F1 to F8 pause, take screenshots and load states
/// (RPCS3's rpcs3qt/shortcut_settings.cpp lines 73 to 94 at build
/// 0.0.43-20344, Cemu's MainWindow::OnKeyUp at v2.6, Dolphin's
/// HotkeyManager.cpp lines 409 to 476 at 2609a).
pub const KEYS: [(&str, u16); 79] = [
    ("KeyA", 0x1E),
    ("KeyB", 0x30),
    ("KeyC", 0x2E),
    ("KeyD", 0x20),
    ("KeyE", 0x12),
    ("KeyF", 0x21),
    ("KeyG", 0x22),
    ("KeyH", 0x23),
    ("KeyI", 0x17),
    ("KeyJ", 0x24),
    ("KeyK", 0x25),
    ("KeyL", 0x26),
    ("KeyM", 0x32),
    ("KeyN", 0x31),
    ("KeyO", 0x18),
    ("KeyP", 0x19),
    ("KeyQ", 0x10),
    ("KeyR", 0x13),
    ("KeyS", 0x1F),
    ("KeyT", 0x14),
    ("KeyU", 0x16),
    ("KeyV", 0x2F),
    ("KeyW", 0x11),
    ("KeyX", 0x2D),
    ("KeyY", 0x15),
    ("KeyZ", 0x2C),
    ("Digit1", 0x02),
    ("Digit2", 0x03),
    ("Digit3", 0x04),
    ("Digit4", 0x05),
    ("Digit5", 0x06),
    ("Digit6", 0x07),
    ("Digit7", 0x08),
    ("Digit8", 0x09),
    ("Digit9", 0x0A),
    ("Digit0", 0x0B),
    ("Minus", 0x0C),
    ("Equal", 0x0D),
    ("Backspace", 0x0E),
    ("BracketLeft", 0x1A),
    ("BracketRight", 0x1B),
    ("Enter", 0x1C),
    ("Semicolon", 0x27),
    ("Quote", 0x28),
    ("Backquote", 0x29),
    ("ShiftLeft", 0x2A),
    ("Backslash", 0x2B),
    ("Comma", 0x33),
    ("Period", 0x34),
    ("Slash", 0x35),
    ("ShiftRight", 0x36),
    ("Space", 0x39),
    ("CapsLock", 0x3A),
    ("IntlBackslash", 0x56),
    ("NumpadMultiply", 0x37),
    ("Numpad7", 0x47),
    ("Numpad8", 0x48),
    ("Numpad9", 0x49),
    ("NumpadSubtract", 0x4A),
    ("Numpad4", 0x4B),
    ("Numpad5", 0x4C),
    ("Numpad6", 0x4D),
    ("NumpadAdd", 0x4E),
    ("Numpad1", 0x4F),
    ("Numpad2", 0x50),
    ("Numpad3", 0x51),
    ("Numpad0", 0x52),
    ("NumpadDecimal", 0x53),
    ("NumpadEnter", 0xE01C),
    ("NumpadDivide", 0xE035),
    ("Home", 0xE047),
    ("ArrowUp", 0xE048),
    ("PageUp", 0xE049),
    ("ArrowLeft", 0xE04B),
    ("ArrowRight", 0xE04D),
    ("End", 0xE04F),
    ("ArrowDown", 0xE050),
    ("PageDown", 0xE051),
    ("Delete", 0xE053),
];

/// The mouse's buttons and wheel, which any place can be given.
pub const MOUSE_BUTTONS: [&str; 7] = [
    "MouseLeft",
    "MouseRight",
    "MouseMiddle",
    "MouseBack",
    "MouseForward",
    "WheelUp",
    "WheelDown",
];

/// The mouse moving, which turns a stick in the emulators that can.
pub const MOUSE_MOVES: [&str; 4] = ["MouseMoveLeft", "MouseMoveRight", "MouseMoveUp", "MouseMoveDown"];

/// Keys that can never be given to a player, with why, for the Controller
/// screen to say.
pub const REFUSED: [(&str, &str); 3] = [
    ("Insert", "Insert opens the portal menu, so it can't be a button in the game."),
    ("Tab", "Tab opens the portal menu, so it can't be a button in the game."),
    ("F11", "F11 switches Omoio between a window and the full screen, so it can't be a button in the game."),
];

/// A key's scan code, for a key in `KEYS`.
pub fn scan_code(key: &str) -> Option<u16> {
    KEYS.iter().find(|(name, _)| *name == key).map(|(_, scan)| *scan)
}

/// Whether `input` is something a keyboard-and-mouse player can press.
pub fn known(input: &str) -> bool {
    scan_code(input).is_some() || MOUSE_BUTTONS.contains(&input) || MOUSE_MOVES.contains(&input)
}

/// The layout a keyboard-and-mouse player starts with, by place, each with
/// one input or more separated by spaces. Empty for a place nothing presses.
///
/// The left stick is W, A, S and D, which PC games use for walking, and the
/// right stick is the mouse moving, for aiming and turning the camera. The
/// face buttons are under the right hand on the mouse, and again on J, K and
/// L for anyone who would rather keep the mouse still. The shoulders sit
/// around W, under the left hand. The d-pad is on 1 to 4, in the order up,
/// right, down, left, clockwise as on the pad. Back is on Backspace rather
/// than Tab: Dolphin's own hotkeys turn its speed limit off while Tab is
/// held (HotkeyManager.cpp line 433, `HK_TOGGLE_THROTTLE` on TAB, and
/// HotkeyScheduler.cpp line 492, at 2609a), which Omoio's empty Hotkeys.ini
/// prevents only where the player hasn't set hotkeys up in Dolphin
/// (backends/dolphin/settings.rs, `quiet_hotkeys`), and RPCS3 hears Tab as
/// another key, Backtab, while Shift, the left stick's press, is held
/// (Qt's qwindowskeymapper.cpp lines 1219 to 1220, Qt 6.12.0). The Guide
/// button has no key:
/// it opens the console's own menu, which nobody needs mid-game, and a key
/// for it would be one more to press by mistake.
pub fn default_input(place: &str) -> &'static str {
    match place {
        "South" => "Space",
        "West" => "MouseLeft KeyJ",
        "North" => "MouseRight KeyK",
        "East" => "MouseMiddle KeyL",
        "LB" => "KeyQ",
        "RB" => "KeyE",
        "LT" => "KeyR",
        "RT" => "KeyF",
        "LS" => "ShiftLeft",
        "RS" => "KeyC",
        "Start" => "Enter",
        "Back" => "Backspace",
        "Up" => "Digit1",
        "Right" => "Digit2",
        "Down" => "Digit3",
        "Left" => "Digit4",
        "LS Y+" => "KeyW",
        "LS Y-" => "KeyS",
        "LS X-" => "KeyA",
        "LS X+" => "KeyD",
        "RS Y+" => "MouseMoveUp",
        "RS Y-" => "MouseMoveDown",
        "RS X-" => "MouseMoveLeft",
        "RS X+" => "MouseMoveRight",
        _ => "",
    }
}

/// `inputs` as kept: each input known, once, separated by single spaces.
/// `None` when any is not known, so a mistyped file never binds half a
/// place.
pub fn normalise(inputs: &str) -> Option<String> {
    let mut kept: Vec<&str> = Vec::new();
    for input in inputs.split_whitespace() {
        if !known(input) {
            return None;
        }
        if !kept.contains(&input) {
            kept.push(input);
        }
    }
    Some(kept.join(" "))
}

/// The mouse speed a player starts with, in percent of the speed each
/// emulator has by default.
pub const DEFAULT_MOUSE: u32 = 100;

/// The slowest and fastest the mouse can be set to, in percent.
pub const MOUSE_RANGE: (u32, u32) = (20, 300);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pad_layout::INPUTS;

    #[test]
    fn every_default_input_is_known() {
        for place in INPUTS {
            assert_eq!(normalise(default_input(place)).as_deref(), Some(default_input(place)), "{place}");
        }
    }

    #[test]
    fn omoios_own_keys_are_never_given() {
        for (key, _) in REFUSED {
            assert!(!known(key), "{key}");
        }
    }

    #[test]
    fn every_key_is_listed_once_with_its_own_scan_code() {
        for (at, (name, scan)) in KEYS.iter().enumerate() {
            assert!(KEYS[at + 1..].iter().all(|(other, s)| other != name && s != scan), "{name}");
        }
    }

    #[test]
    fn inputs_are_kept_once_and_unknown_ones_refused() {
        assert_eq!(normalise("  MouseLeft   KeyJ MouseLeft ").as_deref(), Some("MouseLeft KeyJ"));
        assert_eq!(normalise("KeyJ Insert"), None);
        assert_eq!(normalise("").as_deref(), Some(""));
    }

    #[test]
    fn scan_codes_are_by_place() {
        assert_eq!(scan_code("KeyW"), Some(0x11));
        assert_eq!(scan_code("ArrowUp"), Some(0xE048));
        assert_eq!(scan_code("MouseLeft"), None);
    }
}
