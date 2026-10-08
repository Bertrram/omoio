//! Dolphin's settings files, changed one value at a time.
//!
//! Dolphin keeps its settings in INI files: `[Section]` lines, then
//! `Key = Value` lines. It matches section names and keys without regard to
//! case, trims the spaces around both, takes quotes off a value and skips a
//! line starting with `#` (`IniFile::Load` and `IniFile::ParseLine` in
//! Source/Core/Common/IniFile.cpp, Dolphin 2609a, read 8 October 2026).
//! Omoio sets only the values it means to and leaves every other line as it
//! was, so whatever the user changed in Dolphin itself stays.

/// The line ending a file already uses, so an edited file keeps its own.
fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// The section a `[Name]` line opens, if it is one.
fn section_name(line: &str) -> Option<&str> {
    let line = line.trim();
    let inside = line.strip_prefix('[')?;
    Some(&inside[..inside.find(']')?])
}

/// The key and value of a `Key = Value` line.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value);
    Some((key.trim(), value))
}

/// The value of `key` in `section`, as Dolphin would read it.
pub fn get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        if let Some(name) = section_name(line) {
            inside = name.eq_ignore_ascii_case(section);
        } else if inside {
            if let Some((found, value)) = key_value(line) {
                if found.eq_ignore_ascii_case(key) {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// The text with `key` in `section` set to `value`: the line replaced where
/// it is, added at the end of its section, or added with a new section at
/// the end of the file.
pub fn set(text: &str, section: &str, key: &str, value: &str) -> String {
    let ending = line_ending(text);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let wanted = format!("{key} = {value}");

    let start = lines
        .iter()
        .position(|line| section_name(line).is_some_and(|name| name.eq_ignore_ascii_case(section)));
    match start {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|line| section_name(line).is_some())
                .map_or(lines.len(), |at| start + 1 + at);
            let found = lines[start + 1..end]
                .iter()
                .position(|line| key_value(line).is_some_and(|(found, _)| found.eq_ignore_ascii_case(key)));
            match found {
                Some(at) => lines[start + 1 + at] = wanted,
                None => {
                    // After the section's last value, ahead of any blank
                    // lines that part it from the next section.
                    let mut at = end;
                    while at > start + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, wanted);
                }
            }
        }
        None => {
            lines.push(format!("[{section}]"));
            lines.push(wanted);
        }
    }
    let mut out = lines.join(ending);
    out.push_str(ending);
    out
}

/// The text without `key` in `section`, so Dolphin falls back to its own
/// default for it.
pub fn remove(text: &str, section: &str, key: &str) -> String {
    let ending = line_ending(text);
    let mut inside = false;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            if let Some(name) = section_name(line) {
                inside = name.eq_ignore_ascii_case(section);
                return true;
            }
            !(inside && key_value(line).is_some_and(|(found, _)| found.eq_ignore_ascii_case(key)))
        })
        .collect();
    if kept.is_empty() {
        return String::new();
    }
    let mut out = kept.join(ending);
    out.push_str(ending);
    out
}

/// Whether Dolphin keeps a line of a section as it is rather than as a
/// value: one starting with `$`, `+` or `*`, a comment, or one with nothing
/// on either side of an `=` (`IniFile::Load`). Patches, codes and the lists
/// of which are on are kept this way.
fn is_kept_whole(line: &str) -> bool {
    if line.starts_with(['$', '+', '*', '#']) {
        return true;
    }
    key_value(line).is_none_or(|(key, value)| key.is_empty() && value.is_empty())
}

/// The lines Dolphin keeps whole in `section`, untrimmed and in the order
/// they come, from every `[section]` of that name in the file. Dolphin
/// skips a byte order mark at the start and empty lines, and opens a
/// section only with a `[` at the very start of a line.
pub fn lines(text: &str, section: &str) -> Vec<String> {
    let mut inside = false;
    let mut found = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if let Some(name) = section_name(line) {
                inside = name.eq_ignore_ascii_case(section);
            }
        } else if inside && is_kept_whole(line) {
            found.push(line.to_string());
        }
    }
    found
}

/// The text with `line` added at the end of `section`, unless the section
/// has it already. A section that isn't there is added at the end of the
/// file.
pub fn add_line(text: &str, section: &str, line: &str) -> String {
    if lines(text, section).iter().any(|kept| kept.trim() == line.trim()) {
        return text.to_string();
    }
    let ending = line_ending(text);
    let mut all: Vec<String> = text.lines().map(str::to_string).collect();
    let start = all
        .iter()
        .position(|kept| section_name(kept).is_some_and(|name| name.eq_ignore_ascii_case(section)));
    match start {
        Some(start) => {
            let mut at = all[start + 1..]
                .iter()
                .position(|kept| section_name(kept).is_some())
                .map_or(all.len(), |next| start + 1 + next);
            while at > start + 1 && all[at - 1].trim().is_empty() {
                at -= 1;
            }
            all.insert(at, line.to_string());
        }
        None => {
            all.push(format!("[{section}]"));
            all.push(line.to_string());
        }
    }
    let mut out = all.join(ending);
    out.push_str(ending);
    out
}

/// The text without any line in `section` that reads `line`, spaces aside,
/// and without the section once nothing is left in it.
pub fn remove_line(text: &str, section: &str, line: &str) -> String {
    let ending = line_ending(text);
    let mut kept: Vec<&str> = Vec::new();
    // Where the section's heading is in `kept`, while inside it.
    let mut heading: Option<usize> = None;
    let drop_if_empty = |kept: &mut Vec<&str>, heading: Option<usize>| {
        if let Some(at) = heading {
            if kept[at + 1..].iter().all(|rest| rest.trim().is_empty()) {
                kept.truncate(at);
            }
        }
    };
    for each in text.lines() {
        if let Some(name) = section_name(each) {
            drop_if_empty(&mut kept, heading);
            heading = name.eq_ignore_ascii_case(section).then_some(kept.len());
        } else if heading.is_some() && each.trim() == line.trim() {
            continue;
        }
        kept.push(each);
    }
    drop_if_empty(&mut kept, heading);
    if kept.iter().all(|rest| rest.trim().is_empty()) {
        return String::new();
    }
    let mut out = kept.join(ending);
    out.push_str(ending);
    out
}

/// Reads a settings file, sets every value in `values` as (section, key,
/// value), and writes it back only if anything changed. A file that isn't
/// there yet is made.
pub fn update(path: &std::path::Path, values: &[(&str, &str, &str)]) -> std::io::Result<()> {
    let before = std::fs::read_to_string(path).unwrap_or_default();
    let after = values
        .iter()
        .fold(before.clone(), |text, (section, key, value)| set(&text, section, key, value));
    if after == before {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOLPHIN_INI: &str = "[General]\r\nISOPaths = 0\r\n[Interface]\r\nConfirmStop = True\r\n\r\n[Core]\r\nGFXBackend = D3D\r\n";

    #[test]
    fn values_are_read_the_way_dolphin_reads_them() {
        assert_eq!(get(DOLPHIN_INI, "interface", "confirmstop").as_deref(), Some("True"));
        assert_eq!(get("[A]\nName = \"spaced value\"\n", "A", "Name").as_deref(), Some("spaced value"));
        assert_eq!(get("[A]\n# Name = 1\n", "A", "Name"), None, "a comment");
        assert_eq!(get(DOLPHIN_INI, "Core", "ConfirmStop"), None, "another section's key");
    }

    #[test]
    fn a_value_is_changed_where_it_is() {
        let text = set(DOLPHIN_INI, "Interface", "ConfirmStop", "False");
        assert_eq!(
            text,
            "[General]\r\nISOPaths = 0\r\n[Interface]\r\nConfirmStop = False\r\n\r\n[Core]\r\nGFXBackend = D3D\r\n"
        );
    }

    #[test]
    fn a_new_value_goes_at_the_end_of_its_section() {
        let text = set(DOLPHIN_INI, "interface", "PauseOnFocusLost", "False");
        assert_eq!(
            text,
            "[General]\r\nISOPaths = 0\r\n[Interface]\r\nConfirmStop = True\r\nPauseOnFocusLost = False\r\n\r\n[Core]\r\nGFXBackend = D3D\r\n"
        );
    }

    #[test]
    fn a_new_section_goes_at_the_end() {
        assert_eq!(set("", "Input", "BackgroundInput", "False"), "[Input]\nBackgroundInput = False\n");
        let text = set("[Core]\nGFXBackend = D3D\n", "Input", "BackgroundInput", "False");
        assert_eq!(text, "[Core]\nGFXBackend = D3D\n[Input]\nBackgroundInput = False\n");
    }

    #[test]
    fn setting_the_same_value_changes_nothing() {
        assert_eq!(set(DOLPHIN_INI, "Core", "GFXBackend", "D3D"), DOLPHIN_INI);
    }

    #[test]
    fn a_value_can_be_taken_out() {
        let text = remove(DOLPHIN_INI, "core", "gfxbackend");
        assert_eq!(text, "[General]\r\nISOPaths = 0\r\n[Interface]\r\nConfirmStop = True\r\n\r\n[Core]\r\n");
        assert_eq!(remove(DOLPHIN_INI, "General", "Missing"), DOLPHIN_INI);
    }

    /// A made-up game file in the shape Dolphin's own come in.
    const GAME_INI: &str = "\u{feff}# GAME01 - A game\n\n[Core]\nCPUThread = False\n\n[OnFrame]\n# Add patches here.\n$Fix hang\n0x80001234:dword:0x60000000\n\n[Gecko]\n$Speed = 2x [Someone]\n*Runs faster.\n04001234 00000001\n";

    #[test]
    fn whole_lines_are_read_the_way_dolphin_keeps_them() {
        assert_eq!(
            lines(GAME_INI, "onframe"),
            ["# Add patches here.", "$Fix hang", "0x80001234:dword:0x60000000"]
        );
        // A name with an `=` in it is still a whole line, not a value.
        assert_eq!(lines(GAME_INI, "Gecko"), ["$Speed = 2x [Someone]", "*Runs faster.", "04001234 00000001"]);
        assert!(lines(GAME_INI, "Core").is_empty(), "values are not whole lines");
        assert!(lines(GAME_INI, "Missing").is_empty());
        // Every section of the same name counts, in order.
        assert_eq!(lines("[A]\n$One\n[B]\n$Two\n[a]\n$Three\n", "A"), ["$One", "$Three"]);
    }

    #[test]
    fn a_line_goes_at_the_end_of_its_section_once() {
        let text = add_line(GAME_INI, "OnFrame", "$Other");
        assert!(text.contains("0x80001234:dword:0x60000000\n$Other\n\n[Gecko]"), "{text}");
        assert_eq!(add_line(&text, "OnFrame", "$Other"), text, "already there");
        assert_eq!(add_line("", "Gecko_Enabled", "$Speed"), "[Gecko_Enabled]\n$Speed\n");
        assert_eq!(
            add_line("[Controls]\r\nWiimoteSource0 = 1\r\n", "OnFrame_Enabled", "$Fix hang"),
            "[Controls]\r\nWiimoteSource0 = 1\r\n[OnFrame_Enabled]\r\n$Fix hang\r\n"
        );
    }

    #[test]
    fn a_line_is_taken_out_and_an_empty_section_with_it() {
        let text = "[Controls]\nWiimoteSource0 = 1\n[OnFrame_Enabled]\n$One\n$Two\n[Gecko_Enabled]\n$One\n";
        assert_eq!(
            remove_line(text, "OnFrame_Enabled", "$One"),
            "[Controls]\nWiimoteSource0 = 1\n[OnFrame_Enabled]\n$Two\n[Gecko_Enabled]\n$One\n",
            "only that section's line goes"
        );
        let text = remove_line(text, "OnFrame_Enabled", "$One");
        let text = remove_line(&text, "OnFrame_Enabled", "$Two");
        assert_eq!(text, "[Controls]\nWiimoteSource0 = 1\n[Gecko_Enabled]\n$One\n");
        assert_eq!(remove_line("[Gecko_Enabled]\n$One\n\n", "Gecko_Enabled", "$One"), "");
    }
}
