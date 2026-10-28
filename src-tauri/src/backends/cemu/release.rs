//! The one Cemu release Omoio installs: its own build of the newest Cemu it
//! has been checked against.
//!
//! Omoio's Cemu code was read out of Cemu's source at one tag: the folders it
//! uses, the settings it writes, and above all how its SDL input names a pad
//! (sdl.rs), which a profile has to match byte for byte. A later Cemu can
//! change any of that. Cemu's own main branch has moved on to SDL 3 since
//! v2.6 (its `vcpkg.json`), and SDL 3 names a Bluetooth pad differently and
//! reports a Switch pad's face buttons by place (`HIDAPI_AddDevice`, its
//! migration notes), so a profile written for v2.6 would leave such a pad
//! without a player. Omoio updates Cemu without
//! asking, so it goes no further than this release until the next one has
//! been read and tried.
//!
//! The release is Cemu v2.6 with three additions (github.com/Bertrram/Cemu,
//! branch omoio-portal-ipc, tag v2.6-2): a server on 127.0.0.1 that puts
//! figures on the emulated Skylanders portal and takes them off, which the
//! portal menu uses instead of Cemu's windows (portal.rs; v2.6-1, Bertram,
//! 22 October 2026); Imaginators' last two level pieces and the Mystery
//! Chests in the figure maker; and Trap Team's portal speaker, mixed into
//! the TV's sound (v2.6-2, 28 October 2026). It is built by that
//! repository's own GitHub workflow with the libraries v2.6 was built with,
//! SDL2 2.30.3 and wxWidgets 3.2.5 among them, so it names pads as v2.6
//! does, and packed as Cemu packs a release. An installed v2.6 or v2.6-1 is
//! replaced by it, as by any newer release.
//!
//! The download is a fixed file, so it is checked against the SHA-256 it had
//! when it was published (28 October 2026).

/// The release, by its tag.
pub const TAG: &str = "v2.6-2";

/// The Windows build of `TAG`, from the fork's GitHub releases.
pub const DOWNLOAD: &str = "https://github.com/Bertrram/Cemu/releases/download/v2.6-2/cemu-2.6-2-windows-x64.zip";

/// The file's name, which is what it is saved as while it downloads.
pub const FILE: &str = "cemu-2.6-2-windows-x64.zip";

/// SHA-256 of `DOWNLOAD`, as hex.
pub const SHA256: &str = "f057ff6499aed794d4b5575f66fcae2231414d890008aa9aba523ac6afb3e986";

/// Cemu's own release this one is built from, and Omoio's earlier build of
/// it, which name SDL pads the same way and are what an Omoio before this
/// build installed. A pad stays named while such a Cemu waits to be updated.
const BUILT_FROM: [&str; 2] = ["2.6", "2.6-1"];

/// `TAG` as `detect_version` reports a version, without the `v`.
pub fn version() -> &'static str {
    TAG.strip_prefix('v').unwrap_or(TAG)
}

/// Whether the Cemu installed, by the version `detect_version` reports, is
/// the release Omoio was checked against, or Cemu's own release it is built
/// from, so its SDL pads are named the way sdl.rs names them. Any other
/// version, or one not known, is not: its PlayStation and Switch pads are
/// then left to stand in for or warn about, rather than named in a way Cemu
/// may not find.
pub fn names_sdl_pads_as_checked(installed: Option<&str>) -> bool {
    installed.is_some_and(|got| {
        let got = got.trim().strip_prefix('v').unwrap_or(got.trim());
        got == version() || BUILT_FROM.contains(&got)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::versions::is_newer_release;

    #[test]
    fn the_download_is_the_tagged_release() {
        assert_eq!(version(), "2.6-2");
        assert!(DOWNLOAD.contains(&format!("/releases/download/{TAG}/")));
        assert!(DOWNLOAD.ends_with(&format!("/{FILE}")));
        assert!(FILE.ends_with("-windows-x64.zip"), "the Windows build");
        assert!(FILE.contains(version()));
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    #[test]
    fn an_older_cemu_is_updated_to_the_checked_release_and_no_further() {
        // What the update check does with `newest_version`, which is this
        // release.
        assert!(is_newer_release(version(), "2.5"));
        assert!(is_newer_release(version(), "2.0-94"));
        assert!(is_newer_release(version(), "2.6"), "Cemu's own 2.6 is replaced by Omoio's build");
        assert!(is_newer_release(version(), "2.6-1"), "and so is Omoio's earlier build");
        assert!(!is_newer_release(version(), "2.6-2"), "the checked release is kept");
        assert!(!is_newer_release(version(), "2.7"), "a newer Cemu is never taken back");
    }

    #[test]
    fn only_the_checked_release_has_its_sdl_pads_named() {
        assert!(names_sdl_pads_as_checked(Some("2.6-2")));
        assert!(names_sdl_pads_as_checked(Some("v2.6-2")));
        assert!(names_sdl_pads_as_checked(Some("2.6-1")), "Omoio's earlier build, until it is updated");
        assert!(names_sdl_pads_as_checked(Some("v2.6-1")));
        assert!(names_sdl_pads_as_checked(Some("2.6")), "the release it is built from");
        assert!(names_sdl_pads_as_checked(Some("v2.6")));
        assert!(!names_sdl_pads_as_checked(Some("2.7")), "not read yet");
        assert!(!names_sdl_pads_as_checked(Some("2.5")), "not read either");
        assert!(!names_sdl_pads_as_checked(Some("2.6.1")));
        assert!(!names_sdl_pads_as_checked(None), "no version file");
    }
}
