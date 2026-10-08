//! The one Dolphin release Omoio installs: the newest it has been checked
//! against.
//!
//! Omoio drives Dolphin's own windows from outside (its Skylanders portal),
//! writes its controller profiles with the names its input code gives each
//! pad, and sets values in its settings files by name. Every one of those
//! was read in Dolphin's source at this release, and a later release can
//! change any of them, so Omoio goes no further than this release until the
//! next one has been read and tried, as it does with Cemu.
//!
//! The download is a fixed file, so it is checked against the SHA-256 it had
//! when it was checked (8 October 2026: downloaded twice, the same both
//! times). Dolphin publishes no checksum for the archive itself.

/// The release, as Dolphin names it: year and month, and a letter for a fix.
pub const VERSION: &str = "2609a";

/// The Windows build of `VERSION`, from Dolphin's own download server, the
/// one dolphin-emu.org/download links to.
pub const DOWNLOAD: &str = "https://dl.dolphin-emu.org/releases/2609a/dolphin-2609a-x64.7z";

/// The file's name, which is what it is saved as while it downloads.
pub const FILE: &str = "dolphin-2609a-x64.7z";

/// SHA-256 of `DOWNLOAD`, as hex. 20,037,671 bytes.
pub const SHA256: &str = "bbfd13bf9e6d2a4164a15c392b1f32a9b18150e5136746b388704608fbb60f76";

/// The one folder the archive holds everything in.
pub const TOP_FOLDER: &str = "Dolphin-x64";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::versions::is_newer_release;

    #[test]
    fn the_download_is_the_release() {
        assert!(DOWNLOAD.contains(&format!("/releases/{VERSION}/")));
        assert!(DOWNLOAD.ends_with(&format!("/{FILE}")));
        assert!(FILE.ends_with("-x64.7z"), "the Windows build for x64");
        assert!(FILE.contains(VERSION));
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    #[test]
    fn an_older_dolphin_is_updated_to_the_checked_release_and_no_further() {
        assert!(is_newer_release(VERSION, "2609"));
        assert!(is_newer_release(VERSION, "2606"));
        assert!(!is_newer_release(VERSION, "2609a"), "the checked release is kept");
        assert!(!is_newer_release(VERSION, "2612"), "a newer Dolphin is never taken back");
    }
}
