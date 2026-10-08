//! Real box art from RAWG, for people who switch it on.
//!
//! Off by default. The generated tile always works; this is extra. RAWG's
//! terms, read on 11 September 2026:
//!
//! - Every request carries an API key, and a key needs an account. Omoio uses
//!   the key the user gives it, so nothing is shipped in the app or the repo.
//! - Free for personal use up to 20,000 requests a month, non-commercial only.
//! - RAWG must be credited with an active link on every screen its images
//!   appear on. The interface does that wherever a RAWG cover is shown.
//!
//! RAWG knows games by name, not by title id, so a game is looked up by the
//! name its own metadata gives, and a result only counts when RAWG lists it
//! for the game's own console. Each game is asked about once and the answer kept, found
//! or not, so the monthly allowance lasts.
//!
//! The game's own picture is kept in the same folder, by `keep_own`. It needs
//! no key and nothing from the internet.

use crate::core::console::Console;
use std::path::{Path, PathBuf};

const SEARCH: &str = "https://api.rawg.io/api/games";
/// RAWG's name for each console's platform, as it appears in its results.
/// The PS3's was read off real results. The Wii U's has not been seen in one
/// yet. The Wii's and the GameCube's are as RAWG's own platform pages name
/// them (rawg.io/games/wii and rawg.io/games/gamecube, 8 October 2026).
fn platform(console: Console) -> &'static str {
    match console {
        Console::Ps3 => "PlayStation 3",
        Console::WiiU => "Wii U",
        Console::Wii => "Wii",
        Console::GameCube => "GameCube",
    }
}

/// Where a RAWG cover for a title is kept, beside but apart from the dump's
/// own ICON0, so switching RAWG off goes straight back to that.
pub fn cached_path(covers: &Path, title_id: &str) -> PathBuf {
    covers.join(format!("{title_id}.rawg.jpg"))
}

/// Keeps the game's own picture in `covers`, made by `picture` the first time
/// there is one, so the library keeps it after the drive goes away. Until
/// then `picture` is asked on every call, because one can turn up later:
/// Cemu writes a Wii U game's icon beside its first save. Once kept, it is
/// never made again.
pub fn keep_own(covers: &Path, title_id: &str, picture: impl FnOnce() -> Option<Vec<u8>>) -> Option<PathBuf> {
    if !is_plain_id(title_id) {
        return None;
    }
    let kept = covers.join(format!("{title_id}.png"));
    if !kept.is_file() {
        let picture = picture()?;
        std::fs::create_dir_all(covers).ok()?;
        // Written beside it and then renamed, so a write cut short never
        // leaves a broken picture that would be kept from then on.
        let part = kept.with_extension("png.part");
        std::fs::write(&part, picture).ok()?;
        std::fs::rename(&part, &kept).ok()?;
    }
    Some(kept)
}

/// Marks a title RAWG had nothing for, so it is not asked again.
fn miss_path(covers: &Path, title_id: &str) -> PathBuf {
    covers.join(format!("{title_id}.rawg.none"))
}

/// A title id is letters and digits and nothing else. It becomes part of a
/// file name here, so anything with a separator or a dot is refused before a
/// path is built from it.
fn is_plain_id(id: &str) -> bool {
    // Nine characters on the PS3, sixteen hex digits on the Wii U.
    !id.is_empty() && id.len() <= 16 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Downloads an image, or nothing if what comes back is not one.
async fn download_image(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let response = client.get(url).header("User-Agent", "Omoio").send().await.ok()?;
    let is_image = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("image/"));
    if !response.status().is_success() || !is_image {
        return None;
    }
    response.bytes().await.ok().map(|bytes| bytes.to_vec())
}

/// Folds a name down to what two spellings of the same game share: case goes,
/// a trailing note like "(EU)" goes, and trademark signs and punctuation become
/// spaces, so "LittleBigPlanet™2" and "LittleBigPlanet 2" meet.
fn simplify(name: &str) -> String {
    without_note(name)
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The name without a trailing bracketed note such as "(EU)", which says which
/// release it is rather than which game.
fn without_note(name: &str) -> &str {
    let trimmed = name.trim_end();
    match (trimmed.ends_with(')'), trimmed.rfind('(')) {
        (true, Some(open)) if open > 0 => trimmed[..open].trim_end(),
        _ => trimmed,
    }
}

/// The image of the best match on the right platform, preferring the one
/// whose name is the same as ours once both are simplified.
fn pick(results: &[serde_json::Value], name: &str, platform: &str) -> Option<String> {
    let wanted = simplify(name);
    let on_ps3 = |game: &&serde_json::Value| {
        game.get("platforms")
            .and_then(|p| p.as_array())
            .is_some_and(|platforms| {
                platforms.iter().any(|p| {
                    p.pointer("/platform/name").and_then(|n| n.as_str()) == Some(platform)
                })
            })
    };
    let image = |game: &serde_json::Value| {
        game.get("background_image")
            .and_then(|i| i.as_str())
            .filter(|url| url.starts_with("https://"))
            .map(str::to_string)
    };

    let candidates: Vec<&serde_json::Value> = results.iter().filter(on_ps3).collect();
    candidates
        .iter()
        .find(|game| {
            game.get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|n| simplify(n) == wanted)
        })
        .or(candidates.first())
        .and_then(|game| image(game))
}

/// Fetches and keeps the cover for one title. Returns whether there is one
/// now. A title already asked about is not asked again.
pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    covers: &Path,
    title_id: &str,
    name: &str,
    console: Console,
) -> Result<bool, String> {
    if !is_plain_id(title_id) {
        return Err("Couldn't look up that cover.".to_string());
    }
    let dest = cached_path(covers, title_id);
    if dest.is_file() {
        return Ok(true);
    }
    if miss_path(covers, title_id).is_file() {
        return Ok(false);
    }

    // Searched by the folded name, so a note like "(EU)" or a trademark sign
    // does not get in the way. The URL is built by hand because reqwest's
    // query helper sits behind a feature this build does not switch on.
    let search = simplify(name);
    let url = reqwest::Url::parse_with_params(
        SEARCH,
        &[("key", key), ("search", search.as_str()), ("page_size", "8")],
    )
    .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .header("User-Agent", "Omoio")
        .send()
        .await
        .map_err(|_| "Couldn't reach RAWG.".to_string())?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err("RAWG didn't accept that key.".to_string()),
        _ => return Err("RAWG isn't answering right now.".to_string()),
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "RAWG answered in a form Omoio doesn't understand.".to_string())?;
    let results = body
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    std::fs::create_dir_all(covers).map_err(|e| e.to_string())?;
    let Some(url) = pick(&results, name, platform(console)) else {
        let _ = std::fs::write(miss_path(covers, title_id), b"");
        return Ok(false);
    };

    // A full image is around 200 KB and a catalogue page shows sixty. RAWG's
    // image host also answers a resize path, /media/resize/640/-/, but that is
    // not in their API documentation, so it is tried first and the original is
    // used whenever it does not come back as an image.
    let small = url.replacen("/media/", "/media/resize/640/-/", 1);
    let bytes = match download_image(client, &small).await {
        Some(bytes) => bytes,
        None => download_image(client, &url)
            .await
            .ok_or("Couldn't download the cover.")?,
    };
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn game(name: &str, platform: &str, image: &str) -> serde_json::Value {
        json!({
            "name": name,
            "background_image": image,
            "platforms": [{ "platform": { "name": platform } }]
        })
    }

    #[test]
    fn the_own_picture_is_asked_for_until_there_is_one_and_then_kept() {
        let covers = std::env::temp_dir().join(format!("omoio-covers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&covers);

        assert_eq!(keep_own(&covers, "WUD87E51FD0F7F95", || None), None, "not played yet");
        assert!(!covers.join("WUD87E51FD0F7F95.png").exists());

        let kept = keep_own(&covers, "WUD87E51FD0F7F95", || Some(b"first".to_vec())).unwrap();
        assert_eq!(kept, covers.join("WUD87E51FD0F7F95.png"));
        assert_eq!(std::fs::read(&kept).unwrap(), b"first");

        let again = keep_own(&covers, "WUD87E51FD0F7F95", || panic!("made again")).unwrap();
        assert_eq!(std::fs::read(again).unwrap(), b"first");
        assert!(!covers.join("WUD87E51FD0F7F95.png.part").exists());

        assert_eq!(keep_own(&covers, "../outside", || Some(b"x".to_vec())), None);
        let _ = std::fs::remove_dir_all(&covers);
    }

    #[test]
    fn two_spellings_of_one_name_simplify_to_the_same() {
        assert_eq!(simplify("LittleBigPlanet™2 (EU)"), "littlebigplanet 2");
        assert_eq!(simplify("LittleBigPlanet™2 (EU)"), simplify("LittleBigPlanet 2"));
        assert_eq!(simplify("Demon's Souls™"), simplify("Demon s Souls"));
    }

    #[test]
    fn only_a_plain_id_is_allowed_near_a_file_name() {
        for id in ["BLES01689", "NPEA00243", "MRTC00002", "0005000010101E00"] {
            assert!(is_plain_id(id), "{id} should pass");
        }
        for bad in ["", "..", "../x", "a/b", "a\\b", "C:", "BLES01689.jpg", "WAYTOOLONGTITLEID"] {
            assert!(!is_plain_id(bad), "{bad:?} should be refused");
        }
    }

    #[test]
    fn only_a_ps3_release_is_taken() {
        let results = [
            game("Demon's Souls", "PlayStation 5", "https://a/ps5.jpg"),
            game("Demon's Souls", "PlayStation 3", "https://a/ps3.jpg"),
        ];
        assert_eq!(pick(&results, "Demon's Souls™", "PlayStation 3").as_deref(), Some("https://a/ps3.jpg"));
    }

    #[test]
    fn the_same_name_wins_over_a_closer_rank() {
        let results = [
            game("LittleBigPlanet Karting", "PlayStation 3", "https://a/karting.jpg"),
            game("LittleBigPlanet 2", "PlayStation 3", "https://a/lbp2.jpg"),
        ];
        assert_eq!(pick(&results, "LittleBigPlanet 2", "PlayStation 3").as_deref(), Some("https://a/lbp2.jpg"));
    }

    #[test]
    fn nothing_on_ps3_means_no_cover() {
        let results = [game("Halo 3", "Xbox 360", "https://a/halo.jpg")];
        assert_eq!(pick(&results, "Halo 3", "PlayStation 3"), None);
    }

    #[test]
    fn an_image_that_is_not_https_is_refused() {
        let results = [game("Flower", "PlayStation 3", "http://a/flower.jpg")];
        assert_eq!(pick(&results, "Flower", "PlayStation 3"), None);
    }
}
