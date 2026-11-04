//! The user's own keys for Wii U disc images, handed to Cemu.
//!
//! A disc image, `.wud` or `.wux`, is encrypted. Cemu reads it with the disc's
//! key from `keys.txt` in its user data folder (`KeyCache.cpp`), which for
//! Omoio's portable Cemu is `portable/keys.txt`. Omoio only adds the keys from
//! a file the user brings; Cemu does the decrypting. Omoio never supplies a
//! key and never says where to find one. This is the one exception to the rule
//! against getting round copy protection, granted by Bertram on 13 September
//! 2026.

use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// The key Cemu writes into a new keys.txt as an example. It opens no disc,
/// so it does not count.
const EXAMPLE: &str = "541b9889519b27d363cd21604b97c67a";

/// What Cemu writes at the top of a keys.txt it makes itself.
const HEADER: &str = "# this file contains keys needed for decryption of disc file system data (WUD/WUX)\r\n\
# 1 key per line, any text after a '#' character is considered a comment\r\n\
# the emulator will automatically pick the right key\r\n";

const NOT_TEXT: &str = "That file isn't plain text. Save your keys as a .txt file and add it again.";

/// U+FEFF, the byte order mark, as each encoding scheme writes it at the
/// start of a file (the Unicode Standard 18.0, section 3.10, D95 and D98,
/// https://www.unicode.org/versions/Unicode18.0.0/core-spec/chapter-3/#G28070,
/// and section 23.8.1, table 23-9,
/// https://www.unicode.org/versions/Unicode18.0.0/core-spec/chapter-23/#G25817;
/// the same table in https://www.unicode.org/faq/utf_bom.html#bom4; read 4
/// November 2026). D98: the mark is not part of the text.
const UTF8_MARK: &[u8] = b"\xEF\xBB\xBF";
const UTF16_BE_MARK: &[u8] = b"\xFE\xFF";
const UTF16_LE_MARK: &[u8] = b"\xFF\xFE";

/// The text of a keys file saved as UTF-8, with or without the mark, or as
/// UTF-16 either way round when its mark says which. A player's file saved
/// as Unicode was turned down as unreadable (Discord, Damian04122, seen 4
/// November 2026), and one in UTF-8 with the mark was turned down at line 1.
/// Anything else is not text to Omoio: a .zip renamed to .txt, or UTF-16
/// with no mark. D98 would read that as big endian, but a keys file is all
/// ASCII, so every other byte of it is NUL whichever way round it is. Most
/// binary files hold NUL too, so text holding NUL is refused. So is UTF-32,
/// whose little endian mark starts with UTF-16's, by the same check.
pub(super) fn plain_text(bytes: &[u8]) -> Option<String> {
    let text = if let Some(rest) = bytes.strip_prefix(UTF8_MARK) {
        std::str::from_utf8(rest).ok()?.to_owned()
    } else if let Some(rest) = bytes.strip_prefix(UTF16_LE_MARK) {
        utf16(rest, u16::from_le_bytes)?
    } else if let Some(rest) = bytes.strip_prefix(UTF16_BE_MARK) {
        utf16(rest, u16::from_be_bytes)?
    } else {
        std::str::from_utf8(bytes).ok()?.to_owned()
    };
    (!text.contains('\0')).then_some(text)
}

/// `unit` puts each pair of bytes together in the file's byte order. An odd
/// byte at the end, or half a surrogate pair, is not UTF-16.
fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Option<String> {
    let pairs = bytes.chunks_exact(2);
    if !pairs.remainder().is_empty() {
        return None;
    }
    let units: Vec<u16> = pairs.map(|pair| unit([pair[0], pair[1]])).collect();
    String::from_utf16(&units).ok()
}

/// The keys in Cemu's own keys.txt. Omoio writes it as UTF-8, but a player
/// may have saved it again some other way. One that isn't text by
/// `plain_text`, a comment with an accent saved in a Windows code page say,
/// Cemu still reads a byte at a time, and only ASCII hex makes a key there
/// (`FileStream::readLine` and `strishex`, see `merged`), so its keys are
/// read from the parts of it that are UTF-8, which hold all its ASCII.
fn keys_in_cemus(bytes: &[u8]) -> Vec<String> {
    match plain_text(bytes) {
        Some(text) => read(&text).0,
        None => read(&String::from_utf8_lossy(bytes)).0,
    }
}

/// The keys in a keys file, read the way Cemu reads them: anything after `#`
/// or `;` is a comment, spaces, tabs, `-` and `_` are dropped, and a line of
/// 32 hex characters is a key. Other lengths Cemu ignores, and so does this.
/// A line that is not hex Cemu shows in an error box, possibly over a game, so
/// the first one is returned to be refused rather than passed on.
pub(super) fn read(text: &str) -> (Vec<String>, Option<usize>) {
    let mut keys = Vec::new();
    let mut bad = None;
    for (number, line) in text.lines().enumerate() {
        let before_comment = line.split(['#', ';']).next().unwrap_or("");
        let packed: String = before_comment
            .chars()
            .filter(|c| !matches!(c, ' ' | '\t' | '-' | '_' | '\r'))
            .collect();
        if packed.is_empty() {
            continue;
        }
        if !packed.chars().all(|c| c.is_ascii_hexdigit()) {
            bad.get_or_insert(number + 1);
            continue;
        }
        if packed.len() == 32 {
            let key = packed.to_ascii_lowercase();
            if key != EXAMPLE && !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    (keys, bad)
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("portable").join("keys.txt"))
}

/// How many keys Cemu has, not counting its example.
pub fn count(app: &AppHandle) -> usize {
    path(app)
        .ok()
        .and_then(|file| std::fs::read(file).ok())
        .map(|bytes| keys_in_cemus(&bytes).len())
        .unwrap_or(0)
}

/// Adds the keys in the user's file to Cemu's, leaving those already there.
/// Returns how many were new.
pub fn add(app: &AppHandle, from: &Path) -> Result<usize, String> {
    if !super::install_dir(app)?.join("Cemu.exe").is_file() {
        return Err("Install Cemu first, then add your keys.".to_string());
    }
    let bytes = std::fs::read(from).map_err(|_| "Couldn't read that file.".to_string())?;
    added(path(app)?.as_path(), &bytes)
}

/// `merged`, from the bytes of the user's file.
fn added(file: &Path, bytes: &[u8]) -> Result<usize, String> {
    merged(file, &plain_text(bytes).ok_or_else(|| NOT_TEXT.to_string())?)
}

/// Cemu reads keys.txt a byte at a time, dropping only CR
/// (`FileStream::readLine`, src/Common/windows/FileStream_win32.cpp line
/// 118), and shows an error box for a line that isn't all hex once its
/// comment is cut off (`KeyCache_Prepare`, src/Cafe/Filesystem/FST/KeyCache.cpp
/// lines 85 to 114; it runs as a disc image is opened, FST.cpp line 224;
/// Bertram's Cemu at tag v2.6-2, read 4 November 2026). Nothing there skips
/// a byte order mark, so one would be glued to line 1: Cemu would say
/// "Error in keys.txt at line 1" and lose a key on that line, and in UTF-16
/// every line would be an error. So Omoio writes keys.txt as UTF-8 with no
/// mark and CR LF, under Cemu's own header, and one the player saved with a
/// mark or as UTF-16 is written back as UTF-8 with no mark when keys are
/// added. One that isn't text is kept byte for byte, with the new keys
/// after it, as Cemu reads them there too.
pub(super) fn merged(file: &Path, text: &str) -> Result<usize, String> {
    let (new, bad) = read(text);
    if let Some(line) = bad {
        return Err(format!(
            "Line {line} of that file isn't a key. A keys file has one key of 32 letters and digits per line."
        ));
    }
    if new.is_empty() {
        // Cemu makes this file itself, with one example key, so it is the
        // one people most often pick by mistake.
        return Err(if text.to_ascii_lowercase().contains(EXAMPLE) {
            "That's Cemu's own starting file. Its one key is an example that opens no disc, so there is nothing to add."
                .to_string()
        } else {
            "That file has no keys in it.".to_string()
        });
    }
    let existing = std::fs::read(file).unwrap_or_default();
    let have = keys_in_cemus(&existing);
    let fresh: Vec<String> = new.into_iter().filter(|key| !have.contains(key)).collect();
    if fresh.is_empty() {
        return Ok(0);
    }
    let mut out = match plain_text(&existing) {
        Some(text) if text.trim().is_empty() => HEADER.as_bytes().to_vec(),
        Some(text) => text.into_bytes(),
        None => existing,
    };
    if !out.ends_with(b"\n") {
        out.extend_from_slice(b"\r\n");
    }
    for key in &fresh {
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "Couldn't save the keys.".to_string())?;
    }
    std::fs::write(file, out).map_err(|_| "Couldn't save the keys.".to_string())?;
    Ok(fresh.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff";

    #[test]
    fn keys_are_read_the_way_cemu_reads_them() {
        let text = "# a comment\r\n\
                    0011-2233 4455_6677 8899AABBCCDDEEFF # my disc\r\n\
                    ; another comment\r\n\
                    abcd\r\n\
                    541b9889519b27d363cd21604b97c67a # example key (can be deleted)\r\n";
        let (keys, bad) = read(text);
        assert_eq!(keys, [KEY], "separators and case dropped, short line and example ignored");
        assert_eq!(bad, None);
    }

    #[test]
    fn a_line_that_is_not_hex_is_reported() {
        let (_, bad) = read(&format!("{KEY}\nnot a key\n"));
        assert_eq!(bad, Some(2));
    }

    #[test]
    fn new_keys_are_added_to_cemus_and_old_ones_kept() {
        let dir = std::env::temp_dir().join(format!("omoio-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("keys.txt");

        assert_eq!(merged(&file, KEY).unwrap(), 1);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# this file contains keys"), "Cemu's own header");
        assert!(text.contains(KEY));

        assert_eq!(merged(&file, KEY).unwrap(), 0, "already there");
        let other = "ffeeddccbbaa99887766554433221100";
        assert_eq!(merged(&file, &format!("{KEY}\n{other}\n")).unwrap(), 1);
        assert_eq!(read(&std::fs::read_to_string(&file).unwrap()).0.len(), 2);

        assert!(merged(&file, "no keys here\n").is_err());
        assert!(merged(&file, "# only a comment\n").is_err());
        let starting_file = format!("{HEADER}{EXAMPLE} # example key (can be deleted)\r\n");
        assert!(merged(&file, &starting_file).unwrap_err().contains("Cemu's own starting file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    const OTHER: &str = "ffeeddccbbaa99887766554433221100";

    /// A keys file the way a player saves one: a comment, then a key.
    fn players_file() -> String {
        format!("# my disc\r\n{KEY}\r\n")
    }

    fn encoded_utf16(text: &str, mark: &[u8], unit: fn(u16) -> [u8; 2]) -> Vec<u8> {
        let mut bytes = mark.to_vec();
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&unit(u));
        }
        bytes
    }

    /// A keys.txt of its own for each test, as the tests run side by side.
    fn cemus_file(test: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("omoio-keys-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("keys.txt");
        (dir, file)
    }

    fn keys_of(bytes: &[u8]) -> Vec<String> {
        let (keys, bad) = read(&plain_text(bytes).expect("taken for text"));
        assert_eq!(bad, None);
        keys
    }

    #[test]
    fn utf8_is_read() {
        assert_eq!(keys_of(players_file().as_bytes()), [KEY]);
    }

    #[test]
    fn the_utf8_mark_no_longer_fails_line_1() {
        let bytes = [UTF8_MARK, format!("{KEY}\r\n").as_bytes()].concat();
        let (_, bad) = read(&String::from_utf8(bytes.clone()).unwrap());
        assert_eq!(bad, Some(1), "the mark read as part of line 1");
        assert_eq!(keys_of(&bytes), [KEY]);
        assert!(!plain_text(&bytes).unwrap().starts_with('\u{FEFF}'), "the mark is not part of the text");
    }

    #[test]
    fn utf16_is_read_either_way_round_by_its_mark() {
        let little = encoded_utf16(&players_file(), UTF16_LE_MARK, u16::to_le_bytes);
        assert_eq!(&little[..4], b"\xFF\xFE#\0");
        assert_eq!(keys_of(&little), [KEY]);
        let big = encoded_utf16(&players_file(), UTF16_BE_MARK, u16::to_be_bytes);
        assert_eq!(&big[..4], b"\xFE\xFF\0#");
        assert_eq!(keys_of(&big), [KEY]);
    }

    #[test]
    fn what_is_not_text_is_refused() {
        let mut odd = encoded_utf16(&players_file(), UTF16_LE_MARK, u16::to_le_bytes);
        odd.push(b'0');
        assert_eq!(plain_text(&odd), None, "UTF-16 with an odd byte count");

        let half_a_pair = [UTF16_LE_MARK, b"0\x00\x00\xD8".as_slice()].concat();
        assert_eq!(plain_text(&half_a_pair), None, "UTF-16 with half a surrogate pair");

        let unmarked = encoded_utf16(&players_file(), b"", u16::to_le_bytes);
        assert!(std::str::from_utf8(&unmarked).is_ok(), "valid UTF-8, NUL and all");
        assert_eq!(plain_text(&unmarked), None, "UTF-16 with no mark");

        let code_page = [b"# caf\xE9\r\n".as_slice(), KEY.as_bytes()].concat();
        assert_eq!(plain_text(&code_page), None, "not UTF-8");

        let nul = format!("{KEY}\r\n\0\r\n");
        assert_eq!(plain_text(nul.as_bytes()), None, "NUL in UTF-8");
        let nul16 = encoded_utf16(&nul, UTF16_BE_MARK, u16::to_be_bytes);
        assert_eq!(plain_text(&nul16), None, "NUL after UTF-16 is read");

        let utf32 = [b"\xFF\xFE\x00\x00".as_slice(), b"0\x00\x00\x00".as_slice()].concat();
        assert_eq!(plain_text(&utf32), None, "UTF-32");
    }

    #[test]
    fn a_zip_named_txt_gets_the_plain_text_message() {
        // The start of a zip's local file header: signature, version 2.0,
        // no flags, deflate, then a time and date.
        let zip = b"PK\x03\x04\x14\x00\x00\x00\x08\x00\x8b\x5a\x64\x5b";
        let (dir, file) = cemus_file("zip");
        assert_eq!(
            added(&file, zip),
            Err("That file isn't plain text. Save your keys as a .txt file and add it again.".to_string())
        );
        assert!(!file.exists(), "Cemu's keys.txt untouched");
        assert_eq!(added(&file, &encoded_utf16(&players_file(), UTF16_LE_MARK, u16::to_le_bytes)), Ok(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cemu_keys_txt_with_the_mark_is_merged_and_written_without_it() {
        let (dir, file) = cemus_file("mark");
        std::fs::write(&file, [UTF8_MARK, format!("{KEY}\r\n").as_bytes()].concat()).unwrap();
        assert_eq!(keys_in_cemus(&std::fs::read(&file).unwrap()), [KEY]);
        assert_eq!(merged(&file, KEY), Ok(0), "line 1 is already there");
        assert_eq!(merged(&file, OTHER), Ok(1));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{KEY}\r\n{OTHER}\r\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cemu_keys_txt_saved_as_utf16_keeps_its_keys_and_is_written_as_utf8() {
        let (dir, file) = cemus_file("utf16");
        let text = format!("{HEADER}{KEY}\r\n");
        std::fs::write(&file, encoded_utf16(&text, UTF16_BE_MARK, u16::to_be_bytes)).unwrap();
        assert_eq!(merged(&file, OTHER), Ok(1));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{text}{OTHER}\r\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cemu_keys_txt_that_is_not_text_is_kept_byte_for_byte() {
        let (dir, file) = cemus_file("code-page");
        let before = [b"# caf\xE9\r\n".as_slice(), KEY.as_bytes()].concat();
        std::fs::write(&file, &before).unwrap();
        assert_eq!(keys_in_cemus(&before), [KEY], "the keys Cemu reads in it");
        assert_eq!(merged(&file, KEY), Ok(0));
        assert_eq!(merged(&file, OTHER), Ok(1));
        let after = std::fs::read(&file).unwrap();
        assert_eq!(after, [before.as_slice(), b"\r\n", OTHER.as_bytes(), b"\r\n"].concat(), "kept, keys after");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
