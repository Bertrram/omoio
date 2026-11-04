//! What Windows itself wrote down about the game's emulator as it ended.
//!
//! Windows Error Reporting writes "Application Error", event 1000, for a
//! program an exception ended, with the exception's code and the module it
//! came from, and "Application Hang", event 1002, for one closed after it
//! stopped responding. Both are in the Application log and name the
//! process by its id and its start time. "Application Error" also writes
//! event 1005 for a program Windows closed because a file it was reading
//! went away, as a game drive unplugged does, naming the program by name
//! only. The System log has the display driver's resets (source Display:
//! 4097, 4101, 4109 and 4113), which name no process at all. Those two can
//! only be placed by their time. (Each event's fields as seen in this PC's
//! own logs, and its words in its publisher's metadata or message file,
//! Windows 11 22621, 4 November 2026; "Timeout detection and recovery
//! (TDR)", Microsoft Learn, read the same day.)
//!
//! Anyone signed in at the PC may read both logs, no administrator needed:
//! each one's access gives INTERACTIVE the right to read it (`wevtutil gl`
//! on this PC, and the isolation attribute in "ChannelType complex type",
//! Microsoft Learn, both 4 November 2026). An organisation can lock one by
//! policy, and it is then left out.
//!
//! The events are given as Windows renders them, XML; core::ending reads
//! them and says which are about the emulator.

use std::time::{Duration, Instant, SystemTime};
use windows::core::{Owned, HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_NO_MORE_ITEMS;
use windows::Win32::System::EventLog::{
    EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtQueryTolerateQueryErrors, EvtRender,
    EvtRenderEventXml, EVT_HANDLE,
};

/// Each log, the source in it, and the source's events worth reading.
const WANTED: [(&str, &str, &[u32]); 3] = [
    ("Application", "Application Error", &[1000, 1005]),
    ("Application", "Application Hang", &[1002]),
    ("System", "Display", &[4097, 4101, 4109, 4113]),
];

/// How often, and until how long after the end, Windows' report is looked
/// for. WerFault wrote event 1000 before the crashed process ended in the
/// one trace found (Helge Klein, "Anatomy of WerFault.exe's application
/// crash error reporting", read 4 November 2026), so it is normally there
/// at the first look; the rest is for one that comes late.
const LOOK_EVERY: Duration = Duration::from_millis(500);
const LOOK_FOR: Duration = Duration::from_secs(5);

/// Looked at from a little before the process started, so an event written
/// in the same moment isn't lost to a rounding.
const BEFORE_START: Duration = Duration::from_secs(2);

/// The most events one look reads, the newest. Another program crashing
/// over and over can fill the Application log, and the events of a game
/// that just ended are among the newest.
const MOST: usize = 100;
const BATCH: usize = 16;
/// How long one read of the results may wait, in milliseconds, as in
/// Microsoft's own example ("Querying for Events", Microsoft Learn, read 4
/// November 2026).
const NEXT_WAIT: u32 = 1000;

/// Newest first, from whichever of the two logs can be read. Without
/// tolerance EvtQuery fails at the first part of a structured query that
/// fails, so a log locked by policy would take the other with it
/// ("Querying for Events", Microsoft Learn, read 4 November 2026). The
/// same tolerance would quietly cut short a query Windows found malformed
/// (`EVT_QUERY_FLAGS`, the same day), which is why the test runs it
/// without.
const TOLERANT: u32 = EvtQueryChannelPath.0 | EvtQueryReverseDirection.0 | EvtQueryTolerateQueryErrors.0;

/// Every event of `WANTED` written from `since` until now, as XML, up to
/// the `MOST` newest, in the order the logs give them. Nothing when the
/// logs can't be read. Opens, reads and closes its query on the calling
/// thread, the only one its handles may be used on (EvtQuery, Remarks,
/// Microsoft Learn, read 4 November 2026).
pub fn events_since(since: SystemTime) -> Vec<String> {
    read(since, TOLERANT).unwrap_or_default()
}

/// The events `matches` takes, among those written since the emulator
/// `started`, looked for at once and again every `LOOK_EVERY` while none
/// has come, until `LOOK_FOR` after it `ended`. Takes up to `LOOK_FOR`, so
/// it runs on a thread of its own, never the window's; each look is one
/// `events_since` on that thread. What `matches` takes also ends the wait,
/// so it should take only what is worth waiting for: the events about the
/// emulator's own process. Those placed by time alone are better read with
/// one `events_since` afterwards.
pub fn wait_for_events(started: SystemTime, ended: SystemTime, matches: impl Fn(&str) -> bool) -> Vec<String> {
    // Counted from here on Omoio's own clock, so a change to the PC's time
    // can't stretch the wait.
    let left = (ended + LOOK_FOR)
        .duration_since(SystemTime::now())
        .unwrap_or_default()
        .min(LOOK_FOR);
    let give_up = Instant::now() + left;
    loop {
        let found: Vec<String> = events_since(started).into_iter().filter(|event| matches(event.as_str())).collect();
        if !found.is_empty() || Instant::now() >= give_up {
            return found;
        }
        std::thread::sleep(LOOK_EVERY.min(give_up.saturating_duration_since(Instant::now())));
    }
}

fn read(since: SystemTime, flags: u32) -> windows::core::Result<Vec<String>> {
    let age = SystemTime::now().duration_since(since).unwrap_or_default() + BEFORE_START;
    let query = HSTRING::from(query_text(age));
    // No path: a structured query names its own logs (EvtQuery, Path,
    // Microsoft Learn, read 4 November 2026). Owned, so the results are
    // closed however this returns.
    let results = unsafe { Owned::new(EvtQuery(None, PCWSTR::null(), &query, flags)?) };
    let mut events = Vec::new();
    while events.len() < MOST {
        let mut batch = [0isize; BATCH];
        let mut returned = 0u32;
        let next = unsafe { EvtNext(*results, &mut batch, NEXT_WAIT, 0, &mut returned) };
        // Each event's handle owned at once, so every one is closed,
        // rendered or not (EvtNext, Microsoft Learn, read 4 November 2026).
        let batch: Vec<Owned<EVT_HANDLE>> = batch
            .iter()
            .take(returned as usize)
            .map(|&event| unsafe { Owned::new(EVT_HANDLE(event)) })
            .collect();
        events.extend(batch.iter().filter_map(|event| render(**event)));
        match next {
            Ok(()) => {}
            // How the results end (the same page).
            Err(error) if error.code() == ERROR_NO_MORE_ITEMS.to_hresult() => break,
            Err(error) if events.is_empty() => return Err(error),
            // What was read before the log stopped answering is kept.
            Err(_) => break,
        }
    }
    events.truncate(MOST);
    Ok(events)
}

/// One structured query over both logs, a Select for each source
/// ("Consuming Events", Microsoft Learn, read 4 November 2026). `timediff`
/// gives an event's age in milliseconds; the `<=` is escaped, as the query
/// is XML. The same query read this PC's events 1000 and 1002 through
/// `wevtutil qe /sq:true` (4 November 2026). The process is matched by
/// core::ending as a number, not here: the query compares text, and
/// Windows writes the id in hex.
fn query_text(age: Duration) -> String {
    let recent = format!("TimeCreated[timediff(@SystemTime) &lt;= {}]", age.as_millis());
    let selects: String = WANTED
        .iter()
        .map(|(log, source, ids)| {
            let ids: Vec<String> = ids.iter().map(|id| format!("EventID={id}")).collect();
            format!(
                "<Select Path=\"{log}\">*[System[Provider[@Name='{source}'] and ({}) and {recent}]]</Select>",
                ids.join(" or ")
            )
        })
        .collect();
    format!("<QueryList><Query Id=\"0\">{selects}</Query></QueryList>")
}

/// One event as Windows writes it out, XML: asked first with no buffer for
/// the size it needs, in bytes, then rendered into one that size, as UTF-16
/// ending in a NUL (EvtRender, Microsoft Learn, read 4 November 2026).
fn render(event: EVT_HANDLE) -> Option<String> {
    let (mut used, mut properties) = (0u32, 0u32);
    let _ = unsafe { EvtRender(None, event, EvtRenderEventXml.0, 0, None, &mut used, &mut properties) };
    let mut text = vec![0u16; (used as usize).div_ceil(2)];
    if text.is_empty() {
        return None;
    }
    let size = u32::try_from(text.len() * 2).ok()?;
    unsafe {
        EvtRender(None, event, EvtRenderEventXml.0, size, Some(text.as_mut_ptr().cast()), &mut used, &mut properties)
    }
    .ok()?;
    let end = text.iter().position(|&unit| unit == 0).unwrap_or(text.len());
    Some(String::from_utf16_lossy(&text[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRICT: u32 = EvtQueryChannelPath.0 | EvtQueryReverseDirection.0;

    #[test]
    fn reads_both_logs_on_this_pc() {
        // Strict, so a query Windows found malformed fails here rather than
        // being cut short. It may find nothing.
        let month = Duration::from_secs(30 * 24 * 60 * 60);
        let events = read(SystemTime::now() - month, STRICT).expect("Windows runs the query");
        assert!(events.len() <= MOST);
        assert!(events.iter().all(|event| event.starts_with("<Event")));
    }

    #[test]
    fn looks_once_when_the_wait_is_already_over() {
        let ended = SystemTime::now() - 2 * LOOK_FOR;
        let begun = Instant::now();
        assert!(wait_for_events(ended - Duration::from_secs(60), ended, |_| false).is_empty());
        assert!(begun.elapsed() < LOOK_FOR);
    }
}
