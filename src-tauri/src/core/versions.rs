//! Comparing emulator releases.
//!
//! RPCS3 names a build like `0.0.42-19985-6ba56a52`: the build number is what
//! orders them, and the commit hash after it says nothing about order. Cemu
//! tags a release like `v2.6`. Dolphin names a release by its year and month,
//! `2609`, and a fix to it with a letter after, `2609a`
//! (dl.dolphin-emu.org/releases/2609a/). All are read as numbers piece by
//! piece, up to the first piece that is not a number, a release's letter
//! counting as one more number after it.

fn numbers(version: &str) -> Vec<u64> {
    let version = version.trim();
    let mut found = Vec::new();
    for piece in version.strip_prefix('v').unwrap_or(version).split(['.', '-']) {
        if let Ok(number) = piece.parse() {
            found.push(number);
            continue;
        }
        let digits = piece.trim_end_matches(|c: char| c.is_ascii_lowercase());
        let letters = &piece[digits.len()..];
        match (digits.parse(), letters.as_bytes()) {
            (Ok(number), [letter]) => found.extend([number, u64::from(letter - b'a') + 1]),
            _ => break,
        }
    }
    found
}

/// Whether `newest` is a later release than `installed`.
pub fn is_newer_release(newest: &str, installed: &str) -> bool {
    let (a, b) = (numbers(newest), numbers(installed));
    if a.is_empty() {
        return false;
    }
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpcs3_builds_go_by_their_build_number() {
        assert!(is_newer_release("0.0.42-19985-6ba56a52", "0.0.42-19981-954d7968"));
        assert!(!is_newer_release("0.0.42-19981-954d7968", "0.0.42-19985-6ba56a52"));
        assert!(!is_newer_release("0.0.42-19985-6ba56a52", "0.0.42-19985-6ba56a52"));
        assert!(is_newer_release("0.0.43-20001-0a1b2c3d", "0.0.42-19985-6ba56a52"));
    }

    #[test]
    fn cemu_tags_compare_as_numbers() {
        assert!(is_newer_release("2.7", "2.6"));
        assert!(is_newer_release("2.10", "2.9"), "not as decimals");
        assert!(!is_newer_release("2.6", "2.6"));
        assert!(!is_newer_release("v2.6", "2.6"));
    }

    #[test]
    fn dolphin_releases_and_their_fixes_come_in_order() {
        assert!(is_newer_release("2609a", "2609"), "a fix comes after its release");
        assert!(is_newer_release("2609b", "2609a"));
        assert!(is_newer_release("2609", "2606a"));
        assert!(!is_newer_release("2609a", "2609a"));
        assert!(!is_newer_release("2609", "2609a"));
        assert!(!is_newer_release("2606a", "2609"));
    }

    #[test]
    fn nothing_readable_is_never_newer() {
        assert!(!is_newer_release("", "2.6"));
        assert!(!is_newer_release("nightly", "2.6"));
    }
}
