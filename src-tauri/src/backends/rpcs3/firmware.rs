use std::path::{Path, PathBuf};
use tauri::AppHandle;

const PUP_MAGIC: &[u8] = b"SCEUF\0\0\0";

fn dev_flash_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("dev_flash"))
}

pub fn detect_version(app: &AppHandle) -> Option<String> {
    let path = dev_flash_dir(app)
        .ok()?
        .join("vsh")
        .join("etc")
        .join("version.txt");
    parse_version(&std::fs::read_to_string(path).ok()?)
}

// version.txt reads "release:04.8900:..." - we read the same file and apply the
// same trimming RPCS3 does, so the number we show matches the emulator's.
fn parse_version(contents: &str) -> Option<String> {
    let after_first = contents.find(':')? + 1;
    let rest = contents.get(after_first..)?;
    let raw = &rest[..rest.find(':')?];

    let first_significant = raw.find(|c: char| c != '0')?;
    let begin = if raw.as_bytes()[first_significant] == b'.' {
        // Keep one digit before the dot, so "00.3100" reads "0.31", not ".31".
        first_significant.checked_sub(1)?
    } else {
        first_significant
    };

    let trimmed = &raw[begin..];
    let dot = trimmed.find('.')?;

    // Two decimals always, so "04.9000" reads "4.90" rather than "4.9".
    let mut len = trimmed.len();
    while len > dot + 3 && trimmed.as_bytes()[len - 1] == b'0' {
        len -= 1;
    }
    Some(trimmed[..len].to_string())
}

fn looks_like_pup(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic).is_ok() && magic == PUP_MAGIC
}

// RPCS3 does the actual work: we hand it the file the user picked and let it
// unpack the firmware, exactly as its own menu would. `--headless` is the only
// mode that installs without a window and exits when it's finished; `--no-gui`
// refuses outright.
pub fn install(app: &AppHandle, pup: &Path) -> Result<String, String> {
    let exe = super::exe_path(app)?;
    if !exe.exists() {
        return Err("Install RPCS3 first, then add the firmware.".to_string());
    }
    if !looks_like_pup(pup) {
        return Err("That file isn't PS3 firmware. Look for one named PS3UPDAT.PUP.".to_string());
    }
    super::refuse_while_running(app)?;

    let before = detect_version(app);

    super::command(&exe)
        .arg("--headless")
        .arg("--installfw")
        .arg(pup)
        .output()
        .map_err(|e| e.to_string())?;

    // A headless install reports success even when it failed, so the installed
    // version on disk is the only thing worth believing.
    match detect_version(app) {
        Some(version) => Ok(version),
        None if before.is_some() => {
            Err("Couldn't install that firmware. Your existing firmware is untouched.".to_string())
        }
        None => Err("Couldn't install that firmware. The file may be damaged.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{looks_like_pup, parse_version};

    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn with(name: &str, bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!("omoio-{}-{name}", std::process::id()));
            std::fs::write(&path, bytes).unwrap();
            Self(path)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn recognises_a_pup_by_its_magic() {
        let good = TempFile::with("good.pup", b"SCEUF\0\0\0rest of the file");
        assert!(looks_like_pup(&good.0));
    }

    #[test]
    fn rejects_files_that_are_not_pups() {
        let wrong = TempFile::with("wrong.pup", b"PK\x03\x04not a pup at all");
        let truncated = TempFile::with("short.pup", b"SCEUF");
        let empty = TempFile::with("empty.pup", b"");
        assert!(!looks_like_pup(&wrong.0));
        assert!(!looks_like_pup(&truncated.0));
        assert!(!looks_like_pup(&empty.0));
        assert!(!looks_like_pup(std::path::Path::new("no-such-file.pup")));
    }

    #[test]
    fn trims_padding_the_way_rpcs3_does() {
        assert_eq!(parse_version("release:04.8900:").as_deref(), Some("4.89"));
        assert_eq!(parse_version("release:04.9000:").as_deref(), Some("4.90"));
        assert_eq!(parse_version("release:00.3100:").as_deref(), Some("0.31"));
        assert_eq!(parse_version("release:04.9300:").as_deref(), Some("4.93"));
    }

    #[test]
    fn reads_a_full_version_file() {
        let contents = "release:04.9300:\nbuild:1234\n";
        assert_eq!(parse_version(contents).as_deref(), Some("4.93"));
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("release"), None);
        assert_eq!(parse_version("release:04.8900"), None);
        assert_eq!(parse_version("release:.8900:"), None);
        assert_eq!(parse_version("release:0489:"), None);
    }
}
