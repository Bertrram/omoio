//! Where RPCS3 keeps a game's saved games, for Omoio's backups of them
//! (crate::saves).
//!
//! RPCS3 keeps saves under `dev_hdd0/home/<user>/savedata/`, one folder per
//! game plus whatever variants a game makes of its own. That was read off a
//! real install rather than assumed. A backup keeps each user's part under
//! the user's folder name.
//!
//! Only saves. Trophies sit under `trophy/NPWR00160_00`, keyed by a trophy
//! identifier with no obvious link back to a title, and guessing at that
//! mapping would either miss trophies or claim the wrong ones.

use crate::backends::SaveFolder;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

fn home(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("dev_hdd0").join("home"))
}

pub fn folders(app: &AppHandle, title_id: &str) -> Vec<SaveFolder> {
    home(app).map(|home| folders_in(&home, title_id)).unwrap_or_default()
}

/// The folder a backup's part for the user `kept_as` goes back into.
pub fn folder(app: &AppHandle, kept_as: &str) -> Option<PathBuf> {
    Some(home(app).ok()?.join(kept_as).join("savedata"))
}

/// Every user's save folder that holds some of this game's saves.
///
/// A game makes more than one: LittleBigPlanet keeps `BCES00141` beside
/// whatever else it decides to write, and RPCS3 supports several users, so
/// this looks through all of them rather than assuming the first.
fn folders_in(home: &Path, title_id: &str) -> Vec<SaveFolder> {
    let Ok(users) = std::fs::read_dir(home) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for user in users.flatten() {
        let savedata = user.path().join("savedata");
        let Ok(saves) = std::fs::read_dir(&savedata) else {
            continue;
        };
        let mut mine: Vec<PathBuf> = saves
            .flatten()
            // A game's own folders all begin with its title id, which is how
            // RPCS3 groups saves and their variants.
            .filter(|save| save.file_name().to_string_lossy().starts_with(title_id) && save.path().is_dir())
            .map(|save| save.path())
            .collect();
        if mine.is_empty() {
            continue;
        }
        mine.sort();
        found.push(SaveFolder {
            kept_as: user.file_name().to_string_lossy().into_owned(),
            path: savedata,
            saves: mine,
        });
    }
    found.sort_by(|a, b| a.kept_as.cmp(&b.kept_as));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_games_saves_are_found_under_every_user_with_their_variants() {
        let home = std::env::temp_dir().join(format!("omoio-rpcs3-saves-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for folder in [
            "00000002/savedata/BCES00141",
            "00000001/savedata/BCES00141",
            "00000001/savedata/BCES00141-AUTO",
            "00000001/savedata/BLUS30001",
            "00000003/savedata/BLUS30001",
        ] {
            std::fs::create_dir_all(home.join(folder)).unwrap();
        }
        std::fs::write(home.join("00000001/savedata/BCES00141.txt"), b"not a save").unwrap();

        let found = folders_in(&home, "BCES00141");

        let user = |name: &str| home.join(name).join("savedata");
        assert_eq!(
            found,
            [
                SaveFolder {
                    kept_as: "00000001".to_string(),
                    path: user("00000001"),
                    saves: vec![user("00000001").join("BCES00141"), user("00000001").join("BCES00141-AUTO")],
                },
                SaveFolder {
                    kept_as: "00000002".to_string(),
                    path: user("00000002"),
                    saves: vec![user("00000002").join("BCES00141")],
                },
            ]
        );
        assert!(folders_in(&home, "BLES99999").is_empty());
        assert!(folders_in(&home.join("missing"), "BCES00141").is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
