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
}
