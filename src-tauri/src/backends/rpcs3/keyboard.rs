//! RPCS3's names for what a keyboard-and-mouse player presses.
//!
//! Omoio keeps a key by where it sits (core/keys.rs). RPCS3's keyboard pad
//! keeps it by name, and matches a key pressed in the game window by turning
//! both the name in the profile and the key pressed into one number, Qt's
//! key code (`GetKeyCode`, `keyboard_pad_handler.cpp` lines 934 to 961, and
//! `processKeyEvent`, lines 468 to 507, RPCS3 build 0.0.43-20344, read 20
//! October 2026). So a name written here has to read back as the code Qt
//! gives that key as it is pressed.
//!
//! - The shift keys and the numeric keypad RPCS3 names by scan code itself
//!   ("Shift Left", "Num+7"; `native_scan_code_from_string` and
//!   `native_scan_code_to_string`, lines 963 to 1036).
//! - Space, and the keys that type no printable character, Qt names with a
//!   word from its own table ("Return", "PgUp"; `keyname`, qkeysequence.cpp
//!   lines 391 to 411, Qt 6.12.0, which RPCS3's Windows builds use). Windows
//!   gives each the same virtual key on every layout (Qt's `KeyTbl`,
//!   qwindowskeymapper.cpp lines 193 to 231).
//! - Every other key Qt names by the character it types on the keyboard
//!   layout Windows has now, upper-cased: `toKeyOrUnicode`, lines 555 to 580,
//!   asks `ToUnicode` with no modifier held (`setKbdState`, line 684), and a
//!   key's code is that (`translateKeyEventInternal`, line 1042). On a German
//!   keyboard the key a US one has Y on is "Z", and on a Danish one the key
//!   right of L is "Æ", Qt's `Key_AE`, written and read back as "Æ"
//!   (`keyName`, qkeysequence.cpp line 1307; `decodeString`, lines 1044 and
//!   1169 to 1177).
//!
//! The mouse has fixed names (`mouse_list`, keyboard_pad_handler.h lines 26
//! to 66).

use crate::core::keys;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, MapVirtualKeyExW, ToUnicodeEx, MAPVK_VSC_TO_VK_EX};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// A keyboard layout: for a key's scan code, the virtual key Windows gives it
/// and the character it types with nothing else held, `None` for a key that
/// types nothing.
pub type Layout = dyn Fn(u16) -> Option<(u16, char)>;

/// Omoio's mouse inputs by RPCS3's names (`mouse_list`, keyboard_pad_handler.h
/// lines 29 to 33 for the buttons, 57 to 60 for moving, 62 and 63 for the
/// wheel).
const MOUSE: [(&str, &str); 11] = [
    ("MouseLeft", "Mouse Left"),
    ("MouseRight", "Mouse Right"),
    ("MouseMiddle", "Mouse Middle"),
    ("MouseBack", "Mouse Back"),
    ("MouseForward", "Mouse Fwd"),
    ("WheelUp", "Wheel Up"),
    ("WheelDown", "Wheel Down"),
    ("MouseMoveLeft", "Mouse MLeft"),
    ("MouseMoveRight", "Mouse MRight"),
    ("MouseMoveUp", "Mouse MUp"),
    ("MouseMoveDown", "Mouse MDown"),
];

/// Keys named the same whatever the layout.
///
/// Qt's words, from its `keyname` table (qkeysequence.cpp, Qt 6.12.0): Space
/// line 391, Tab 393, Backspace 395, Return 396 (Enter is the keypad's own),
/// Del 399, Home 403, End 404, Left 405, Up 406, Right 407, Down 408, PgUp
/// 409, PgDown 410, CapsLock 411.
///
/// RPCS3's own, by scan code (keyboard_pad_handler.cpp lines 966, 967 and
/// 975 to 990). The keypad's decimal key is written "Num+Del", which RPCS3
/// reads as the same key (line 985): its other name, "Num+,", ends in the
/// comma RPCS3 splits a button's keys at (`cfg_pad::get_combos`,
/// pad_config.cpp lines 14 to 53), and comes back as "Num+", which is no key.
const FIXED: [(&str, &str); 32] = [
    ("Space", "Space"),
    ("Enter", "Return"),
    ("Backspace", "Backspace"),
    ("Tab", "Tab"),
    ("CapsLock", "CapsLock"),
    ("Home", "Home"),
    ("End", "End"),
    ("PageUp", "PgUp"),
    ("PageDown", "PgDown"),
    ("Delete", "Del"),
    ("ArrowUp", "Up"),
    ("ArrowDown", "Down"),
    ("ArrowLeft", "Left"),
    ("ArrowRight", "Right"),
    ("ShiftLeft", "Shift Left"),
    ("ShiftRight", "Shift Right"),
    ("Numpad0", "Num+0"),
    ("Numpad1", "Num+1"),
    ("Numpad2", "Num+2"),
    ("Numpad3", "Num+3"),
    ("Numpad4", "Num+4"),
    ("Numpad5", "Num+5"),
    ("Numpad6", "Num+6"),
    ("Numpad7", "Num+7"),
    ("Numpad8", "Num+8"),
    ("Numpad9", "Num+9"),
    ("NumpadDecimal", "Num+Del"),
    ("NumpadAdd", "Num++"),
    ("NumpadSubtract", "Num+-"),
    ("NumpadMultiply", "Num+*"),
    ("NumpadDivide", "Num+/"),
    ("NumpadEnter", "Num+Enter"),
];

/// RPCS3's name for one of Omoio's inputs on `layout`, `None` for one it
/// can't name.
pub fn name(input: &str, layout: &Layout) -> Option<String> {
    if let Some((_, name)) = MOUSE.iter().chain(FIXED.iter()).find(|(omoio, _)| *omoio == input) {
        return Some(name.to_string());
    }
    let (virtual_key, typed) = layout(keys::scan_code(input)?)?;
    typed_name(virtual_key, typed)
}

/// Qt's name for a key behind `virtual_key` that types `typed`, when RPCS3
/// reads that name back as the key.
///
/// - A control character or none: Qt names such a key from its own table,
///   and none of the keys left to this has an entry there.
/// - Qt upper-cases with one character for one (`QChar::toUpper`), and reads
///   a name back lower-cased whole and upper-cased again (`decodeString`). A
///   character that doesn't come back the same, such as the Turkish dotted
///   İ, would be another key.
/// - Qt counts a key typing a digit or one of + - . , * as the keypad's
///   unless Windows calls it a digit key or one of the punctuation keys
///   VK_OEM_PLUS to VK_OEM_3 (qwindowskeymapper.cpp lines 1076 to 1093), and
///   RPCS3 then reads it as a keypad key.
fn typed_name(virtual_key: u16, typed: char) -> Option<String> {
    if typed <= ' ' || typed == '\u{7f}' {
        return None;
    }
    let upper = upper(typed);
    let mut lower = upper.to_lowercase();
    let back = lower.next().filter(|_| lower.next().is_none()).map(self::upper)?;
    if back != upper {
        return None;
    }
    let digit_key = (0x30..=0x39).contains(&virtual_key);
    let punctuation_key = (0xBB..=0xC0).contains(&virtual_key);
    if "+-.,*0123456789".contains(upper) && !digit_key && !punctuation_key {
        return None;
    }
    Some(upper.to_string())
}

/// Upper case as `QChar::toUpper` gives it: one character, or the character
/// itself where upper case takes more than one, as ß does.
fn upper(c: char) -> char {
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

/// What RPCS3 is given for a button: RPCS3's name for each input that has
/// one, each once, separated by commas, which RPCS3 reads as any of them
/// (`cfg_pad::get_combos`). Empty for a button with none, as RPCS3 writes an
/// unbound button.
pub fn button<'a>(inputs: impl Iterator<Item = &'a str>, layout: &Layout) -> String {
    let mut names: Vec<String> = Vec::new();
    for found in inputs.filter_map(|input| name(input, layout)) {
        if !names.contains(&found) {
            names.push(found);
        }
    }
    names.join(",")
}

/// The keyboard layout in use now, read as Qt reads it in RPCS3's window.
///
/// The game's window isn't there yet as the file is written, so the window in
/// front stands in for it, Omoio's as Play is pressed: Windows keeps a layout
/// for each thread, one for each app when set to, and a thread of Omoio's
/// that never took keyboard input can be on another. Without a window in front, this thread's layout is used
/// (`GetWindowThreadProcessId` gives 0, and `GetKeyboardLayout(0)` is the
/// calling thread's).
///
/// `ToUnicodeEx` is told to leave the keyboard's state alone (flag bit 2,
/// Windows 10 1607 and later, Microsoft's ToUnicodeEx documentation): a dead
/// key, such as ^ on a German keyboard, otherwise waits for the next key and
/// changes what that one types. A dead key gives its own character, as Qt
/// reads it (a result of -1).
pub fn layout_now() -> impl Fn(u16) -> Option<(u16, char)> {
    const KEEP_STATE: u32 = 0x4;
    let thread = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), None) };
    let layout = unsafe { GetKeyboardLayout(thread) };
    move |scan| {
        let virtual_key = unsafe { MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, Some(layout)) };
        let virtual_key = u16::try_from(virtual_key).ok().filter(|key| *key != 0)?;
        let mut typed = [0u16; 8];
        let count = unsafe {
            ToUnicodeEx(u32::from(virtual_key), u32::from(scan), &[0; 256], &mut typed, KEEP_STATE, Some(layout))
        };
        if count == 0 {
            return None;
        }
        char::from_u32(u32::from(typed[0])).map(|c| (virtual_key, c))
    }
}

/// A US keyboard: each place's virtual key and what it types.
#[cfg(test)]
pub fn us(scan: u16) -> Option<(u16, char)> {
    let letters = "qwertyuiop\0\0\0\0asdfghjkl\0\0\0\0\0zxcvbnm";
    if (0x10..=0x32).contains(&scan) {
        let c = letters.chars().nth(usize::from(scan - 0x10)).filter(|c| *c != '\0');
        if let Some(c) = c {
            return Some((c.to_ascii_uppercase() as u16, c));
        }
    }
    if (0x02..=0x0B).contains(&scan) {
        let digit = b"1234567890"[usize::from(scan - 0x02)];
        return Some((u16::from(digit), char::from(digit)));
    }
    let punctuation = [
        (0x0C, 0xBD, '-'),
        (0x0D, 0xBB, '='),
        (0x1A, 0xDB, '['),
        (0x1B, 0xDD, ']'),
        (0x27, 0xBA, ';'),
        (0x28, 0xDE, '\''),
        (0x29, 0xC0, '`'),
        (0x2B, 0xDC, '\\'),
        (0x33, 0xBC, ','),
        (0x34, 0xBE, '.'),
        (0x35, 0xBF, '/'),
        (0x56, 0xE2, '\\'),
    ];
    punctuation
        .iter()
        .find(|(place, ..)| *place == scan)
        .map(|(_, key, c)| (*key, *c))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A German keyboard, where Y and Z swap places and the punctuation keys
    /// type umlauts, ß, and a dead ^.
    fn german(scan: u16) -> Option<(u16, char)> {
        let differs = [
            (0x15, 0x5A, 'z'),
            (0x2C, 0x59, 'y'),
            (0x0C, 0xDB, 'ß'),
            (0x0D, 0xDD, '´'),
            (0x1A, 0xBA, 'ü'),
            (0x1B, 0xBB, '+'),
            (0x27, 0xC0, 'ö'),
            (0x28, 0xDE, 'ä'),
            (0x29, 0xDC, '^'),
            (0x2B, 0xBF, '#'),
            (0x35, 0xBD, '-'),
            (0x56, 0xE2, '<'),
        ];
        match differs.iter().find(|(place, ..)| *place == scan) {
            Some((_, key, c)) => Some((*key, *c)),
            None => us(scan),
        }
    }

    fn danish(scan: u16) -> Option<(u16, char)> {
        let differs = [(0x27, 0xC0, 'æ'), (0x28, 0xDE, 'ø'), (0x1A, 0xDD, 'å')];
        match differs.iter().find(|(place, ..)| *place == scan) {
            Some((_, key, c)) => Some((*key, *c)),
            None => us(scan),
        }
    }

    /// RPCS3's reading of a button's text into its keys, as
    /// `cfg_pad::get_combos` does it (pad_config.cpp lines 14 to 63): split at
    /// commas, where a comma right after another is a key, then each part at
    /// `&`, which would make a combination.
    fn rpcs3_reads(text: &str) -> Vec<Vec<String>> {
        let split = |s: &str, sep: char| -> Vec<String> {
            let chars: Vec<char> = s.chars().collect();
            let mut parts = Vec::new();
            let (mut was_sep, mut start) = (true, 0);
            for (i, &c) in chars.iter().enumerate() {
                if c == sep && !was_sep {
                    was_sep = true;
                    parts.push(chars[start..i].iter().collect());
                    continue;
                }
                if was_sep {
                    was_sep = false;
                    start = i;
                }
                if i == chars.len() - 1 {
                    parts.push(chars[start..].iter().collect());
                }
            }
            parts.sort();
            parts.dedup();
            parts
        };
        split(text, ',').iter().map(|part| split(part, '&')).collect()
    }

    #[test]
    fn every_key_has_a_name_on_a_us_keyboard() {
        for (key, _) in keys::KEYS {
            assert!(name(key, &us).is_some(), "{key}");
        }
        for input in keys::MOUSE_BUTTONS.iter().chain(keys::MOUSE_MOVES.iter()) {
            assert!(name(input, &us).is_some(), "{input}");
        }
    }

    #[test]
    fn keys_are_named_by_what_they_type() {
        assert_eq!(name("KeyW", &us).as_deref(), Some("W"));
        assert_eq!(name("Digit1", &us).as_deref(), Some("1"));
        assert_eq!(name("Semicolon", &us).as_deref(), Some(";"));
        assert_eq!(name("KeyY", &german).as_deref(), Some("Z"));
        assert_eq!(name("KeyZ", &german).as_deref(), Some("Y"));
        assert_eq!(name("BracketLeft", &german).as_deref(), Some("Ü"));
        assert_eq!(name("Minus", &german).as_deref(), Some("ß"), "ß has no upper case of one letter");
        assert_eq!(name("Backquote", &german).as_deref(), Some("^"), "a dead key");
        assert_eq!(name("BracketRight", &german).as_deref(), Some("+"));
        assert_eq!(name("Slash", &german).as_deref(), Some("-"));
        assert_eq!(name("Semicolon", &danish).as_deref(), Some("Æ"));
        assert_eq!(name("Quote", &danish).as_deref(), Some("Ø"));
        assert_eq!(name("BracketLeft", &danish).as_deref(), Some("Å"));
    }

    #[test]
    fn some_keys_are_named_the_same_on_every_layout() {
        for layout in [&us as &Layout, &german] {
            assert_eq!(name("Space", layout).as_deref(), Some("Space"));
            assert_eq!(name("Enter", layout).as_deref(), Some("Return"));
            assert_eq!(name("ShiftLeft", layout).as_deref(), Some("Shift Left"));
            assert_eq!(name("ArrowUp", layout).as_deref(), Some("Up"));
            assert_eq!(name("PageDown", layout).as_deref(), Some("PgDown"));
            assert_eq!(name("Delete", layout).as_deref(), Some("Del"));
            assert_eq!(name("Numpad7", layout).as_deref(), Some("Num+7"));
            assert_eq!(name("NumpadDecimal", layout).as_deref(), Some("Num+Del"));
        }
    }

    #[test]
    fn the_mouse_has_rpcs3s_names() {
        assert_eq!(name("MouseLeft", &us).as_deref(), Some("Mouse Left"));
        assert_eq!(name("MouseForward", &us).as_deref(), Some("Mouse Fwd"));
        assert_eq!(name("WheelDown", &us).as_deref(), Some("Wheel Down"));
        assert_eq!(name("MouseMoveUp", &us).as_deref(), Some("Mouse MUp"));
    }

    #[test]
    fn keys_rpcs3_would_misread_are_left_out() {
        let nothing = |_: u16| -> Option<(u16, char)> { None };
        assert_eq!(name("KeyA", &nothing), None, "types nothing");
        assert_eq!(name("Insert", &us), None, "not a key a player is given");
        // Behind VK_OEM_8, Qt would count a minus as the keypad's.
        let odd = |scan: u16| -> Option<(u16, char)> { (scan == 0x35).then_some((0xDF, '-')) };
        assert_eq!(name("Slash", &odd), None);
        let turkish = |scan: u16| -> Option<(u16, char)> { (scan == 0x28).then_some((0xDE, 'İ')) };
        assert_eq!(name("Quote", &turkish), None);
    }

    #[test]
    fn a_button_lists_each_name_once() {
        let inputs = ["MouseLeft", "KeyJ", "Insert", "KeyJ"];
        assert_eq!(button(inputs.into_iter(), &us), "Mouse Left,J");
        assert_eq!(button(std::iter::empty(), &us), "");
    }

    /// A comma or an ampersand typed by a key still reads back as that key,
    /// wherever it sits among a button's keys.
    #[test]
    fn separators_typed_by_a_key_survive_rpcs3s_reading() {
        let amp = |scan: u16| match scan {
            0x02 => Some((0x31, '&')),
            _ => us(scan),
        };
        for inputs in [
            vec!["Comma", "KeyJ"],
            vec!["KeyJ", "Comma"],
            vec!["KeyJ", "Comma", "KeyK"],
            vec!["Digit1", "Comma"],
            vec!["Comma", "Digit1"],
        ] {
            let text = button(inputs.iter().copied(), &amp);
            let mut wanted: Vec<Vec<String>> =
                inputs.iter().map(|input| vec![name(input, &amp).unwrap()]).collect();
            wanted.sort();
            assert_eq!(rpcs3_reads(&text), wanted, "{text}");
        }
    }
}
