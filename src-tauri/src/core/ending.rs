//! How a game's session ended, from what is left behind: the emulator's
//! exit code, Windows' own events about its process, and the marks the
//! emulator writes in its log.
//!
//! Nothing here reads a file or asks Windows anything. session.rs gathers
//! the evidence and this decides what it shows, so every rule can be tested
//! with the codes, events and log lines the sources give.
//!
//! The marks are read from the emulator's own log alone, before any of
//! Omoio's notes are added to the copy that is kept, so nothing Omoio writes
//! can ever pass for a crash.

use crate::core::console::Console;
use crate::core::playlog::Ending;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------

/// "The application terminated as a result of a CTRL+C."
/// (STATUS_CONTROL_C_EXIT, ntstatus.h line 4976, Windows SDK 10.0.26100.0,
/// read 4 November 2026). An ending, not a crash.
const CONTROL_C_EXIT: u32 = 0xC000_013A;

/// The warning-severity exceptions a program can die of: a guard page, a
/// misaligned access, a breakpoint (what Cemu's `cemu_assert` raises,
/// `src/Common/precompiled.h` lines 329 to 335, fork v2.6-2) and a single
/// step (ntstatus.h, Windows SDK 10.0.26100.0; both read 4 November 2026).
/// Named one by one rather than the whole 0x8 range, because programs also
/// end with HRESULTs such as 0x80004005.
const WARNING_EXCEPTIONS: [u32; 4] = [0x8000_0001, 0x8000_0002, 0x8000_0003, 0x8000_0004];

/// What Microsoft's C++ compiler raises for a C++ exception, and so the exit
/// code of one that reaches Windows uncaught. It is not in ntstatus.h, and no
/// Microsoft Learn page giving it was found (4 November 2026); it is in this
/// list because Cemu's crash handler takes it like any other exception
/// (`ExceptionHandler_win32.cpp` lines 276 to 280, fork v2.6-2, read
/// 4 November 2026).
const CPP_EXCEPTION: u32 = 0xE06D_7363;

/// Whether an exit code is the code of an exception that ended the program.
///
/// A program an exception ended gets "the exception value for an unhandled
/// exception that caused the process to terminate" as its exit code
/// (GetExitCodeProcess, Remarks, and "Terminating a Process", Microsoft
/// Learn, read 4 November 2026). An NTSTATUS has its severity in bits 31 and
/// 30, 11 for an error, and bit 29 is set only for codes a program defines
/// itself ([MS-ERREF] 2.3, learn.microsoft.com/en-us/openspecs/
/// windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781;
/// ntstatus.h lines 69 to 92; read 4 November 2026). So an error Microsoft
/// defined counts, and that leaves out the 0xFFFFFFFF of exit(-1) and of
/// PowerShell's Stop-Process, the 1 that taskkill leaves (both measured on
/// this PC, 4 November 2026), and any small number a program returns.
pub fn is_exception(code: u32) -> bool {
    ((code & 0xE000_0000) == 0xC000_0000 && code != CONTROL_C_EXIT)
        || WARNING_EXCEPTIONS.contains(&code)
        || code == CPP_EXCEPTION
}

/// Plain words for the exception codes an emulator is likely to end with,
/// after Windows' own names and texts (ntstatus.h in the Windows SDK
/// 10.0.26100.0, at the line given, and [MS-ERREF] 2.3.1; read 4 November
/// 2026).
const NAMED: &[(u32, &str)] = &[
    // STATUS_ACCESS_VIOLATION, line 2112.
    (0xC000_0005, "an access violation: it used memory it had no right to"),
    // STATUS_IN_PAGE_ERROR, line 2121: the data "was not placed into memory
    // because of an I/O error".
    (0xC000_0006, "an in-page error: Windows couldn't read part of a file back from its disk, as when a drive goes away"),
    // STATUS_ILLEGAL_INSTRUCTION, line 2341.
    (0xC000_001D, "an illegal instruction: the processor met an instruction it doesn't have"),
    // STATUS_PRIVILEGED_INSTRUCTION, line 3464.
    (0xC000_0096, "a privileged instruction: the processor refused an instruction"),
    // STATUS_INTEGER_DIVIDE_BY_ZERO, line 3444.
    (0xC000_0094, "a division by zero"),
    // STATUS_STACK_OVERFLOW, line 4410.
    (0xC000_00FD, "a stack overflow"),
    // STATUS_NO_MEMORY, line 2284.
    (0xC000_0017, "no memory: it ran out of memory"),
    // STATUS_COMMITMENT_LIMIT, line 4855.
    (0xC000_012D, "the commit limit: Windows ran out of memory and page file"),
    // STATUS_HEAP_CORRUPTION, line 8770: "A heap has been corrupted."
    (0xC000_0374, "heap corruption: Windows found its memory damaged and stopped it"),
    // STATUS_STACK_BUFFER_OVERRUN, line 8998, a name from before fail-fast.
    // __fastfail ends a program with it and runs none of its exception
    // handlers (Microsoft Learn, __fastfail), and abort() in a release build
    // goes the same way (Microsoft Learn, abort; the UCRT's abort.cpp lines
    // 71 to 79, SDK 10.0.26100.0).
    (0xC000_0409, "a fail-fast: it stopped itself at once on a fatal error, as abort() also does"),
    // STATUS_FAIL_FAST_EXCEPTION, line 10924.
    (0xC000_0602, "a fail-fast: it stopped itself at once on a fatal error"),
    // STATUS_ASSERTION_FAILURE, line 9188.
    (0xC000_0420, "an assertion failure: a check inside it failed"),
    // STATUS_FATAL_USER_CALLBACK_EXCEPTION, line 9179: "An unhandled
    // exception was encountered during a user callback."
    (0xC000_041D, "an exception in a user callback: it crashed while handling a window message"),
    // STATUS_DLL_NOT_FOUND, line 4927.
    (0xC000_0135, "a missing DLL: a file it needs to start is missing"),
    // STATUS_ENTRYPOINT_NOT_FOUND, line 4966.
    (0xC000_0139, "a missing entry point: a file it needs is the wrong version"),
    // STATUS_DLL_INIT_FAILED, line 5054.
    (0xC000_0142, "a DLL that failed to start: part of it failed to start"),
    // STATUS_INVALID_IMAGE_FORMAT, line 3207.
    (0xC000_007B, "an invalid image: a file it needs is damaged or not for this Windows"),
    // STATUS_INVALID_IMAGE_HASH, line 9251.
    (0xC000_0428, "an invalid image hash: Windows blocked one of its files"),
    // STATUS_BREAKPOINT, line 1480: "A breakpoint has been reached."
    (0x8000_0003, "a breakpoint: it stopped on a check built into it"),
    (CPP_EXCEPTION, "a C++ exception: an error inside it that nothing caught"),
];

/// Small codes the way a program returns them, the rest the way Windows
/// writes an NTSTATUS.
fn code_text(code: u32) -> String {
    if code >= 0x8000_0000 {
        format!("0x{code:08X}")
    } else {
        code.to_string()
    }
}

/// Omoio's note of how the emulator's process ended, for the kept log.
pub fn exit_note(emulator: &str, code: u32) -> String {
    let what = if is_exception(code) {
        NAMED
            .iter()
            .find(|(named, _)| *named == code)
            .map(|(_, words)| *words)
            .unwrap_or("an exception code Omoio has no name for")
    } else {
        match code {
            0 => "the code a program gives when it ends on its own",
            // taskkill /F and Rust's Child::kill both end a program with 1
            // (measured on this PC, 4 November 2026; std's process/windows.rs
            // lines 694 to 706, Rust 1.98.1). Task Manager most likely does
            // too: Chromium's base/process/kill.h lines 35 to 41 says so,
            // and no Microsoft page was found (4 November 2026).
            1 => "the code taskkill leaves when it ends a program from outside, and most likely Task Manager too",
            // "If the Windows error reporting handler isn't invoked, then
            // abort calls _exit to terminate the process with exit code 3"
            // (Microsoft Learn, abort, read 4 November 2026).
            3 => "the code abort() leaves when Windows Error Reporting isn't called",
            // STILL_ACTIVE. A program may end with it itself, so it is read
            // only once the process has ended (GetExitCodeProcess, the
            // Important box, Microsoft Learn, read 4 November 2026).
            259 => "the number Windows also gives for a program still running, read here once it had ended",
            // exit(-1), and what PowerShell's Stop-Process leaves (measured on
            // this PC, 4 November 2026).
            0xFFFF_FFFF => "or -1, the code exit(-1) leaves, and PowerShell's Stop-Process too",
            CONTROL_C_EXIT => "Windows' code for a program ended with Ctrl+C",
            _ => "a code of its own rather than an exception code",
        }
    };
    format!("Omoio's note: {emulator} ended with exit code {}, {what}.", code_text(code))
}

// ---------------------------------------------------------------------------
// Windows' events
// ---------------------------------------------------------------------------

/// The providers and events read for a session. The provider names are not
/// translated on other Windows languages, and neither are the field names
/// (seen on this PC with wevtutil, 4 November 2026).
pub const APPLICATION_ERROR: &str = "Application Error";
pub const APPLICATION_HANG: &str = "Application Hang";
pub const DISPLAY: &str = "Display";
/// A program ended by an exception Windows Error Reporting saw.
pub const CRASH_EVENT: u32 = 1000;
/// A program that had stopped responding "stopped interacting with Windows
/// and was closed" (Application Hang's message, read on this PC, 4 November
/// 2026).
pub const HANG_EVENT: u32 = 1002;

/// What each display event says, from the message table of dxgwdi.dll, the
/// Display source's message file (read on this PC, 4 November 2026). The
/// source has no manifest, so its one field, the driver's name, has no name
/// of its own, and the sentence is the only way to give the event in
/// Windows' words. A reset comes when the graphics card stops answering for
/// two seconds ("Timeout detection and recovery", Microsoft Learn, read
/// 4 November 2026).
const DISPLAY_SAYS: [(u32, &str); 4] = [
    (4097, "Display driver %1 detected and recovered from a failure. Some graphics operations may have failed."),
    (4101, "Display driver %1 stopped responding and has successfully recovered."),
    (4109, "Application %1 has been blocked from accessing Graphics hardware."),
    (4113, "Display driver %1 stopped responding and has been successfully disabled."),
];

/// The display events read for a session.
pub const DISPLAY_EVENTS: [u32; 4] = [DISPLAY_SAYS[0].0, DISPLAY_SAYS[1].0, DISPLAY_SAYS[2].0, DISPLAY_SAYS[3].0];

/// One of Windows' events, as EvtRender writes it in XML.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WinEvent {
    /// The log it is in, "Application" or "System".
    pub channel: String,
    /// Who wrote it, such as "Application Error".
    pub provider: String,
    pub id: u32,
    /// TimeCreated's SystemTime as Windows wrote it, in UTC.
    pub time: String,
    /// EventData's fields in order, under their own names. A source with no
    /// manifest, such as Display, writes its fields without names, and then
    /// the name is empty.
    pub data: Vec<(String, String)>,
}

impl WinEvent {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.data.iter().find(|(named, _)| named == name).map(|(_, value)| value.as_str())
    }
}

/// Reads one event from the XML EvtRender gives with EvtRenderEventXml.
/// None when it isn't an event: no provider, or no event id.
pub fn parse_event(xml: &str) -> Option<WinEvent> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    enum Reading {
        Id,
        Channel,
        Data(String),
    }

    let mut reader = Reader::from_str(xml);
    let mut event = WinEvent::default();
    let mut id = None;
    let mut in_data = false;
    let mut reading = None;
    // Text and entity references come as separate pieces; they are put back
    // together as written and unescaped once the element ends.
    let mut text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match element.local_name().as_ref() {
                b"Provider" => event.provider = attribute(&element, b"Name").unwrap_or_default(),
                b"TimeCreated" => event.time = attribute(&element, b"SystemTime").unwrap_or_default(),
                b"EventID" => {
                    reading = Some(Reading::Id);
                    text.clear();
                }
                b"Channel" => {
                    reading = Some(Reading::Channel);
                    text.clear();
                }
                b"EventData" => in_data = true,
                b"Data" if in_data => {
                    reading = Some(Reading::Data(attribute(&element, b"Name").unwrap_or_default()));
                    text.clear();
                }
                _ => {}
            },
            Ok(Event::Empty(element)) => match element.local_name().as_ref() {
                b"Provider" => event.provider = attribute(&element, b"Name").unwrap_or_default(),
                b"TimeCreated" => event.time = attribute(&element, b"SystemTime").unwrap_or_default(),
                b"Data" if in_data => {
                    event.data.push((attribute(&element, b"Name").unwrap_or_default(), String::new()))
                }
                _ => {}
            },
            Ok(Event::Text(piece)) if reading.is_some() => {
                if let Ok(piece) = piece.decode() {
                    text.push_str(&piece);
                }
            }
            Ok(Event::GeneralRef(reference)) if reading.is_some() => {
                if let Ok(name) = reference.decode() {
                    text.push('&');
                    text.push_str(&name);
                    text.push(';');
                }
            }
            Ok(Event::CData(piece)) if reading.is_some() => {
                if let Ok(piece) = piece.decode() {
                    text.push_str(&quick_xml::escape::escape(piece));
                }
            }
            Ok(Event::End(element)) => {
                if element.local_name().as_ref() == b"EventData" {
                    in_data = false;
                }
                if let Some(done) = reading.take() {
                    let value = unescaped(&text);
                    match done {
                        Reading::Id => id = value.trim().parse::<u32>().ok(),
                        Reading::Channel => event.channel = value.trim().to_string(),
                        Reading::Data(name) => event.data.push((name, value)),
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
    }

    event.id = id?;
    (!event.provider.is_empty()).then_some(event)
}

fn attribute(element: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    let attribute = element.try_get_attribute(name).ok()??;
    let raw = std::str::from_utf8(&attribute.value).ok()?;
    Some(unescaped(raw))
}

fn unescaped(raw: &str) -> String {
    quick_xml::escape::unescape(raw)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| raw.to_string())
}

/// The fields' values in hexadecimal, as Windows writes them, "0x1018"
/// (seen on this PC, 4 November 2026). Read as numbers, so a change of case
/// or of leading zeros can't stop a match.
fn hex(value: Option<&str>) -> Option<u64> {
    let value = value?.trim();
    let digits = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")).unwrap_or(value);
    u64::from_str_radix(digits, 16).ok()
}

/// Whether an event is about the emulator's own process: an Application
/// Error or an Application Hang naming its process id and its start time.
/// `created` is the creation time GetProcessTimes gives for the process, the
/// FILETIME as one number of 100-nanosecond steps since 1601.
///
/// Both are needed, and never the program's name: a process id is used
/// again once its process has gone, while the two together name one process
/// (GetProcessTimes, Microsoft Learn; Raymond Chen, "When does a process ID
/// become available for reuse?", devblogs.microsoft.com/oldnewthing/
/// 20110107-00/?p=11803; both read 4 November 2026). The id in the event's
/// System part is the process that wrote it, Windows Error Reporting, not
/// the one that crashed (seen on this PC, 4 November 2026), so only the
/// fields are read. Application Error's event 1005, for a program Windows
/// closed when its disk went away, names the program but holds neither
/// field (its message, read on this PC, 4 November 2026), so it can't be
/// matched to a process and is left out.
pub fn is_about(event: &WinEvent, pid: u32, created: u64) -> bool {
    let (id_field, start_field) = match (event.provider.as_str(), event.id) {
        (APPLICATION_ERROR, CRASH_EVENT) => ("ProcessId", "ProcessCreationTime"),
        (APPLICATION_HANG, HANG_EVENT) => ("ProcessId", "StartTime"),
        _ => return false,
    };
    hex(event.field(id_field)) == Some(u64::from(pid)) && hex(event.field(start_field)) == Some(created)
}

/// Whether an event is the display driver's. These name no process (see
/// DISPLAY_SAYS), so they are taken by their time alone, as a fact about the
/// machine during the game, and never make a crash on their own.
pub fn is_display_event(event: &WinEvent) -> bool {
    event.provider == DISPLAY && DISPLAY_EVENTS.contains(&event.id)
}

fn is_crash_event(event: &WinEvent) -> bool {
    event.provider == APPLICATION_ERROR && event.id == CRASH_EVENT
}

fn is_hang_event(event: &WinEvent) -> bool {
    event.provider == APPLICATION_HANG && event.id == HANG_EVENT
}

/// A line holds a path when it names a drive (`C:\`, `C:/`), a share
/// (`\\server`) or a link. Players share the kept log with Copy full log,
/// and the interface never shows a raw path.
fn holds_path(line: &str) -> bool {
    line.contains("\\\\")
        || line.contains("://")
        || line
            .as_bytes()
            .windows(3)
            .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && (w[2] == b'\\' || w[2] == b'/'))
}

/// A path cut to its file name. AppPath, ModulePath and ExeFileName hold
/// the player's own folders (seen on this PC, 4 November 2026).
fn file_name(value: &str) -> String {
    if !value.contains('\\') && !holds_path(value) {
        return value.to_string();
    }
    let trimmed = value.trim_end_matches(['\\', '/']);
    trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).to_string()
}

/// "2026-10-29T16:05:12.6290049Z" as "2026-10-29 16:05:12 UTC"; anything
/// else as written.
fn readable_time(time: &str) -> String {
    match (time.get(..10), time.get(11..19)) {
        (Some(day), Some(clock)) if time.as_bytes().get(10) == Some(&b'T') && time.ends_with('Z') => {
            format!("{day} {clock} UTC")
        }
        _ => time.to_string(),
    }
}

/// The fields of an Application Error or Application Hang worth a note:
/// what crashed or hung, where, how, and the process it was. The rest are
/// time stamps, report ids and paths, which say nothing a player or a helper
/// can act on (the fields as this PC's own events hold them, 4 November
/// 2026).
const NOTED_FIELDS: &[&str] = &[
    "AppName",
    "AppVersion",
    "ModuleName",
    "ModuleVersion",
    "ExceptionCode",
    "FaultingOffset",
    "ProcessId",
    "HangType",
];

/// Omoio's note of one of Windows' events, in Windows' own words: the
/// event's own field names and values, which aren't translated, or for a
/// display event its sentence. Only `NOTED_FIELDS` are kept when the event
/// has them, empty fields are left out, and a path is cut to its file name.
pub fn event_note(event: &WinEvent) -> String {
    let display = DISPLAY_SAYS
        .iter()
        .find(|(id, _)| event.provider == DISPLAY && *id == event.id);
    let said = match display {
        Some((_, says)) => {
            let first = event.data.first().map(|(_, value)| value.as_str()).unwrap_or("");
            says.replace("%1", &file_name(first))
        }
        None => event
            .data
            .iter()
            .filter(|(name, value)| {
                let known = event.data.iter().any(|(name, _)| NOTED_FIELDS.contains(&name.as_str()));
                !value.is_empty() && (!known || NOTED_FIELDS.contains(&name.as_str()))
            })
            .map(|(name, value)| {
                if name.is_empty() {
                    file_name(value)
                } else {
                    format!("{name}={}", file_name(value))
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    };
    let log = if event.channel.is_empty() {
        String::new()
    } else {
        format!(" in its {} log", event.channel)
    };
    let head = format!(
        "Omoio's note: Windows logged event {} ({}){log} at {}",
        event.id,
        event.provider,
        readable_time(&event.time)
    );
    if said.is_empty() {
        format!("{head}.")
    } else {
        format!("{head}: {said}")
    }
}

// ---------------------------------------------------------------------------
// The emulators' own marks
// ---------------------------------------------------------------------------

/// What RPCS3 writes in front of a line, U+00B7 and a letter (rpcs3/util/
/// logs.cpp lines 822 to 832, RPCS3 master b2e52d6, read 4 November 2026).
const RPCS3_MARK: char = '·';
const RPCS3_FATAL: &str = "·F";
const RPCS3_ERROR: &str = "·E";

/// Cemu's crashlog starts with this, with no timestamp, on a line of its
/// own, and the version after it is a number or, for Omoio's own build, a
/// commit hash ("Crashlog for Cemu c170da5"). Then come a stack trace, the
/// exception, the registers and the game (createCrashlog,
/// `src/Common/ExceptionHandler/ExceptionHandler_win32.cpp` lines 156 to
/// 244, fork v2.6-2, read 4 November 2026). A wxWidgets assert writes a
/// stack trace too, but never this line, so only this line proves a crash
/// (`src/gui/CemuApp.cpp` lines 375 to 389).
const CEMU_CRASHLOG: &str = "Crashlog for Cemu ";
/// "Exception 0x%08x at 0x%I64x(+0x%I64x) in module %s", or without the
/// module for an address in recompiled code (ExceptionHandler_win32.cpp
/// lines 197 and 202).
const CEMU_EXCEPTION: &str = "Exception 0x";
/// The crashlog's game info (ExceptionHandler.cpp lines 51 to 70).
const CEMU_GAME: &str = "Game: ";
/// The line after an unrecoverable renderer error, and two lines after an
/// OSPanic, under its "File:" line (VulkanRenderer.cpp lines 1660 to 1665,
/// SwapchainInfoVk.cpp lines 254 to 259, coreinit.cpp lines 273 to 282).
const CEMU_MSG: &str = "Msg: ";
const CEMU_BEFORE_MSG: [(&str, usize); 3] = [
    ("Unrecoverable error in Vulkan renderer", 1),
    ("Unrecoverable error in Vulkan swapchain", 1),
    ("OSPanic!", 2),
];

/// The lines of Cemu's log worth reading, by how they start once the
/// timestamp is off. Cemu marks no level on a line (CemuLogging.cpp lines
/// 121 to 136), so these are its words for giving up or for a game in
/// trouble, each where Cemu writes it (fork v2.6-2, read 4 November 2026).
/// The lines that carry a path, such as `Failed to load "<path>"` and the
/// cache corruption lines, are left out.
const CEMU_PROBLEMS: &[&str] = &[
    "Unrecoverable error in Vulkan renderer",                        // VulkanRenderer.cpp L1662
    "Unrecoverable error in Vulkan swapchain",                       // SwapchainInfoVk.cpp L256
    "vkWaitForFences: Returned",                                     // VulkanRenderer.cpp L1963, L1967
    "Unable to allocate image memory chunk",                         // VKRMemoryManager.cpp L316
    "Ran out of system memory",                                      // coreinit_MEM.cpp L26
    "Failed to allocate memory for recompiler",                      // PPCRecompiler.cpp L356
    "OSPanic!",                                                      // coreinit.cpp L275
    "The title terminated the process by calling coreinit.exit",     // coreinit.cpp L248
    "Detected error in GPU command buffer",                          // LatteCommandProcessor.cpp L1000
    "Encountered wxWidgets assert!",                                 // CemuApp.cpp L378
    "Error when initializing Vulkan renderer",                       // VulkanCanvas.cpp L40
    "can't initialize tv audio",                                     // ax_out.cpp L414
    "can't initialize pad audio",                                    // ax_out.cpp L429
    "can't initialize audio input",                                  // mic.cpp L170
    "Unable to find RPX executable",                                 // CafeSystem.cpp L146
    "Attempting to recompile function outside of allowed code area", // PPCRecompiler.cpp L135
];

/// Dolphin's warnings and errors that every normal Wii session has, seen in
/// the sessions Omoio kept on this PC (4 November 2026). The SSL warning
/// also carries a path and a link to where the files it wants come from,
/// so it is never shown in any form (Core/IOS/Network/SSL.cpp lines 165 to
/// 168 and 180, Dolphin 2609a). The rest are the Wii's channels asking
/// Nintendo's old servers (Common/HttpRequest.cpp line 283), shader warnings
/// (VideoBackends/D3DCommon/Shader.cpp line 290) and a file Dolphin looks
/// for and does without (Common/DirectIOFile.cpp line 109).
const DOLPHIN_NOISE: &[&str] = &[
    "Warning: IOS: Could not read a file required for SSL services",
    "Warning: IOS: A file required for SSL services",
    "Failed to GET ",
    "ps_5_0 compilation succeeded with warnings",
    "CreateFile: ",
];

/// The line of the emulator's log that proves a crash, if it holds one.
/// Give it the emulator's own log, before Omoio's notes are added.
pub fn log_crash(console: Console, log: &str) -> Option<String> {
    match console {
        // RPCS3's crash handler and its emergency exit both write a fatal
        // line (Utilities/Thread.cpp lines 2405 to 2530 and 3468 to 3566,
        // master b2e52d6, read 4 November 2026).
        Console::Ps3 => {
            let lines: Vec<&str> = log.lines().collect();
            let at = lines.iter().position(|line| line.starts_with(RPCS3_FATAL))?;
            Some(rpcs3_entry(&lines, at))
        }
        Console::WiiU => {
            let mut lines = log.lines();
            let crashlog = lines.find(|line| line.starts_with(CEMU_CRASHLOG))?;
            let exception = lines.find(|line| line.starts_with(CEMU_EXCEPTION));
            Some(exception.unwrap_or(crashlog).trim().to_string())
        }
        // Dolphin's log never proves a crash. Its only exception handler on
        // Windows is the one for its fast memory, which writes nothing and
        // passes every other exception on to Windows, and it sets no crash
        // handler of its own (Core/MemTools.cpp lines 49 to 108; a search of
        // Source/Core for SetUnhandledExceptionFilter, MiniDumpWriteDump and
        // set_terminate; Dolphin 2609a, read 4 November 2026). It plays on
        // after an invalid read (Core/PowerPC/MMU.cpp lines 1229 to 1234).
        // So a Dolphin crash shows in its exit code and in Windows'
        // Application Error, and its log holds only what came before.
        Console::Wii | Console::GameCube => None,
    }
}

/// The lines of the emulator's log worth reading, in order, each once, up
/// to `limit`. Give it the emulator's own log, before Omoio's notes are
/// added.
pub fn log_problems(console: Console, log: &str, limit: usize) -> Vec<String> {
    match console {
        Console::Ps3 => rpcs3_problems(log, limit),
        Console::WiiU => cemu_problems(log, limit),
        Console::Wii | Console::GameCube => dolphin_problems(log, limit),
    }
}

/// RPCS3's line at `at`, with the text after it when the marked line has
/// none of its own. Its crash handler writes `sys_log.fatal("\n%s", msg)`,
/// so the marked line ends at "SYS: " and the exception follows on unmarked
/// lines (Utilities/Thread.cpp line 2527, master b2e52d6, read 4 November
/// 2026). One or two of them say what happened.
fn rpcs3_entry(lines: &[&str], at: usize) -> String {
    let mut entry = lines[at].trim().to_string();
    if entry.ends_with(':') {
        for next in lines.iter().skip(at + 1).take(2) {
            let next = next.trim();
            if next.is_empty() || next.starts_with(RPCS3_MARK) {
                break;
            }
            entry.push(' ');
            entry.push_str(next);
        }
    }
    entry
}

/// Fatals and errors, in order, deduplicated. RPCS3 repeats the same failure
/// for every frame it happens on, and fifty copies of one line is not fifty
/// problems.
fn rpcs3_problems(log: &str, limit: usize) -> Vec<String> {
    let lines: Vec<&str> = log.lines().collect();
    let mut seen = HashSet::new();
    let mut problems = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if !line.starts_with(RPCS3_FATAL) && !line.starts_with(RPCS3_ERROR) {
            continue;
        }
        let entry = rpcs3_entry(&lines, at);
        // The timestamp differs every time; compare on what was said.
        let said = entry.splitn(3, ' ').nth(2).unwrap_or(&entry).to_string();
        if seen.insert(said) {
            problems.push(entry);
            if problems.len() >= limit {
                break;
            }
        }
    }
    problems
}

/// A Cemu line without its "[HH:MM:SS.mmm] " (CemuLogging.cpp line 127,
/// fork v2.6-2). The crashlog's lines have none.
fn cemu_text(line: &str) -> &str {
    match line.strip_prefix('[').and_then(|rest| rest.split_once("] ")) {
        Some((stamp, text)) if stamp.len() == 12 => text,
        _ => line,
    }
}

fn cemu_problems(log: &str, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut problems = Vec::new();
    let mut in_crashlog = false;
    // How many lines on a "Msg: " line still belongs to the one before it.
    let mut msg_within: usize = 0;
    for line in log.lines() {
        let text = cemu_text(line);
        if text.starts_with(CEMU_CRASHLOG) {
            in_crashlog = true;
        }
        let wanted = CEMU_PROBLEMS.iter().any(|start| text.starts_with(start))
            || (in_crashlog && (text.starts_with(CEMU_EXCEPTION) || text.starts_with(CEMU_GAME)))
            || (msg_within > 0 && text.starts_with(CEMU_MSG));
        msg_within = CEMU_BEFORE_MSG
            .iter()
            .find(|(start, _)| text.starts_with(start))
            .map(|(_, lines)| *lines)
            .unwrap_or(msg_within.saturating_sub(1));
        if !wanted || holds_path(line) {
            continue;
        }
        if seen.insert(text.trim().to_string()) {
            problems.push(line.trim().to_string());
            if problems.len() >= limit {
                break;
            }
        }
    }
    problems
}

/// A Dolphin line: "MM:SS:mmm <file>:<line> <L>[<kind>]: <message>", the
/// level one of N, E, W, I or D (Common/Logging/LogManager.cpp lines 211 to
/// 235, Common/Logging/Log.h lines 79 to 86, Dolphin 2609a, read 4 November
/// 2026). The rest of a message with line breaks in it runs on below with
/// no timestamp, so those lines are not entries.
fn dolphin_entry(line: &str) -> Option<(char, &str, &str)> {
    let (stamp, rest) = line.split_once(' ')?;
    let b = stamp.as_bytes();
    if b.len() != 9 || b[2] != b':' || b[5] != b':' {
        return None;
    }
    let (_source, rest) = rest.split_once(' ')?;
    let level = rest.chars().next()?;
    let (kind, message) = rest.strip_prefix(level)?.strip_prefix('[')?.split_once("]: ")?;
    Some((level, kind, message))
}

/// Errors, and warnings in MASTER and Video: every panic alert is a warning
/// in MASTER, a critical one an error there, and the renderer's asserts and
/// a failed present are warnings in Video (Common/MsgHandler.h lines 105 to
/// 136, Common/MsgHandler.cpp lines 77 to 88, VideoBackends/D3DCommon/
/// SwapChain.cpp lines 305 to 309, Dolphin 2609a, read 4 November 2026).
/// Only the first line of each.
fn dolphin_problems(log: &str, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut problems = Vec::new();
    for line in log.lines() {
        let Some((level, kind, message)) = dolphin_entry(line) else {
            continue;
        };
        let wanted = level == 'E' || (level == 'W' && (kind == "MASTER" || kind == "Video"));
        if !wanted || DOLPHIN_NOISE.iter().any(|noise| message.starts_with(noise)) || holds_path(line) {
            continue;
        }
        if seen.insert(message.trim().to_string()) {
            problems.push(line.trim().to_string());
            if problems.len() >= limit {
                break;
            }
        }
    }
    problems
}

// ---------------------------------------------------------------------------
// How it ended
// ---------------------------------------------------------------------------

/// What Omoio learnt about how a session ended.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// Why the emulator wouldn't start the game, in the player's words,
    /// when it refused it.
    pub refusal: Option<String>,
    /// Omoio ended it, because the player asked.
    pub stopped_by_us: bool,
    /// The emulator's exit code, when Omoio read one.
    pub exit_code: Option<u32>,
    /// What `log_crash` found in the emulator's own log.
    pub crash_line: Option<String>,
    /// Windows' events for this session: those `is_about` matched to the
    /// emulator's process, and display events by their time.
    pub events: Vec<WinEvent>,
}

/// How the session ended. A refusal first, then Omoio's own stop, so a game
/// Omoio ended is never called a crash. Then a crash, only on evidence: an
/// exception exit code, the emulator's own crash mark, or Windows' Application
/// Error for its process. Cemu's crash handler ends Cemu with exit(0) once it
/// has written its crashlog and never hands the crash to Windows, so on the
/// Wii U the crashlog is the only evidence of the crashes Cemu catches
/// (ExceptionHandler_win32.cpp lines 241 and 276 to 287, fork v2.6-2, read
/// 4 November 2026).
///
/// Anything else is closed normally, an emulator ended from Task Manager
/// included, whose exit code the note still gives. So is one Windows closed
/// after it stopped responding (Application Hang), and a display driver's
/// reset, which names no process, never makes a crash alone.
pub fn decide(evidence: &Evidence) -> Ending {
    if evidence.refusal.is_some() {
        Ending::Refused
    } else if evidence.stopped_by_us {
        Ending::Stopped
    } else if evidence.exit_code.is_some_and(is_exception)
        || evidence.crash_line.is_some()
        || evidence.events.iter().any(is_crash_event)
    {
        Ending::Crashed
    } else {
        Ending::Closed
    }
}

/// Omoio's notes about how the session ended, a line each, for the kept log:
/// the exit code and Windows' events. None when Omoio ended the game itself,
/// since then the exit code is the one Omoio's own stop left.
pub fn notes(emulator: &str, evidence: &Evidence) -> Vec<String> {
    if evidence.refusal.is_some() || evidence.stopped_by_us {
        return Vec::new();
    }
    let mut notes = vec![match evidence.exit_code {
        Some(code) => exit_note(emulator, code),
        None => format!("Omoio's note: Omoio couldn't read {emulator}'s exit code."),
    }];
    notes.extend(evidence.events.iter().map(event_note));
    notes
}

/// What the crash evidence says, strongest first: the emulator's own mark,
/// then Windows' report, then the exit code alone.
fn crash_words(console: Console, emulator: &str, evidence: &Evidence) -> String {
    if evidence.crash_line.is_some() {
        match console {
            Console::Ps3 => format!("{emulator} logged a fatal error."),
            Console::WiiU => format!("{emulator} caught a crash and closed."),
            Console::Wii | Console::GameCube => format!("{emulator}'s log shows a crash."),
        }
    } else if evidence.events.iter().any(is_crash_event) {
        format!("Windows reported that {emulator} crashed.")
    } else {
        format!("{emulator} ended unexpectedly.")
    }
}

/// RPCS3's are its error and fatal lines. Cemu's and Dolphin's are lines
/// picked by their words and levels, Dolphin's warnings among them, so they
/// are counted as problems, not errors.
fn errors_logged(console: Console, count: usize) -> String {
    let what = if console == Console::Ps3 { "error" } else { "problem" };
    match count {
        1 => format!("1 {what} logged."),
        n => format!("{n} {what}s logged."),
    }
}

/// The line the Logs page shows for a session, in plain words: never a path,
/// a code or an exception, which are in the kept log and behind Show errors.
/// `emulator` is the emulator's own name, `problems` what `log_problems`
/// found.
///
/// "{emulator} logged no errors." is said only of an emulator whose errors
/// Omoio can read. Cemu marks no level on its lines (CemuLogging.cpp lines
/// 121 to 136, fork v2.6-2), so for Cemu it says what Omoio found, or that
/// its log shows no crash.
pub fn summary(console: Console, emulator: &str, ending: Ending, evidence: &Evidence, problems: &[String]) -> String {
    if ending == Ending::Refused {
        return evidence
            .refusal
            .clone()
            .unwrap_or_else(|| format!("{emulator} didn't start the game."));
    }
    let crashlog = console == Console::WiiU && evidence.crash_line.is_some();
    let mut said = Vec::new();
    if ending == Ending::Crashed {
        said.push(crash_words(console, emulator, evidence));
    } else if crashlog {
        said.push(format!("{emulator} caught a crash."));
    } else if evidence.events.iter().any(is_hang_event) {
        said.push(format!("Windows reported that {emulator} stopped responding and was closed."));
    }
    if console == Console::WiiU {
        // The crashlog's own lines are its problems, and "no crash" beside a
        // crash Windows reported would read as if the two disagreed.
        if !crashlog && ending != Ending::Crashed {
            said.push(format!("{emulator}'s log shows no crash."));
        }
        if !crashlog && !problems.is_empty() {
            said.push(errors_logged(console, problems.len()));
        }
    } else if problems.is_empty() {
        said.push(format!("{emulator} logged no errors."));
    } else {
        said.push(errors_logged(console, problems.len()));
    }
    if evidence.events.iter().any(is_display_event) {
        said.push("Windows reported a display driver problem during the game.".to_string());
    }
    said.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- exit codes -------------------------------------------------------

    #[test]
    fn exception_codes_are_crashes_and_other_endings_are_not() {
        for code in [0xC000_0005, 0xC000_0409, 0xC000_00FD, 0xC000_0374, 0x8000_0003, 0xE06D_7363, 0xC000_0135] {
            assert!(is_exception(code), "{code:#X}");
        }
        // 1 from taskkill and Task Manager, 3 from abort without Windows
        // Error Reporting, 259 read after the end, -1, Ctrl+C, an HRESULT and
        // a code a program made up itself.
        for code in [0, 1, 3, 259, 0xFFFF_FFFF, 0xC000_013A, 0x8000_4005, 0xE000_0001] {
            assert!(!is_exception(code), "{code:#X}");
        }
    }

    #[test]
    fn every_named_code_is_an_exception() {
        for (code, _) in NAMED {
            assert!(is_exception(*code), "{code:#X}");
        }
    }

    #[test]
    fn notes_an_exception_in_plain_words() {
        assert_eq!(
            exit_note("Cemu", 0xC000_0005),
            "Omoio's note: Cemu ended with exit code 0xC0000005, an access violation: it used memory it had no right to."
        );
        assert!(exit_note("Cemu", 0xC000_0409).contains("fail-fast"));
        assert!(exit_note("Dolphin", 0xC000_0123).contains("0xC0000123, an exception code Omoio has no name for."));
    }

    #[test]
    fn notes_an_ending_from_outside_without_calling_it_a_crash() {
        let note = exit_note("Cemu", 1);
        assert!(note.starts_with("Omoio's note: Cemu ended with exit code 1, "));
        assert!(note.contains("Task Manager"));
        assert!(exit_note("RPCS3", 0xFFFF_FFFF).contains("exit code 0xFFFFFFFF, or -1,"));
        assert!(exit_note("Dolphin", 0).contains("exit code 0, "));
        assert!(exit_note("Dolphin", 42).contains("exit code 42, a code of its own"));
    }

    #[test]
    fn notes_read_plainly() {
        let codes = NAMED.iter().map(|(code, _)| *code).chain([0, 1, 3, 259, 42, 0xFFFF_FFFF, 0xC000_013A]);
        for code in codes {
            let note = exit_note("Cemu", code);
            assert!(note.ends_with('.') && !note.ends_with(".."), "{note}");
            assert!(!note.contains('\u{2014}') && !note.contains('!'), "{note}");
        }
    }

    // ---- Windows' events --------------------------------------------------

    /// An Application Error as Windows wrote it on this PC (4 November 2026),
    /// shape only.
    const APP_ERROR: &str = "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'>\
 <System><Provider Name='Application Error' Guid='{a0e9b465-b939-57d7-b27d-95d8e925ff57}'/>\
  <EventID>1000</EventID><Version>0</Version><Level>2</Level><Task>100</Task><Opcode>0</Opcode>\
  <Keywords>0x8000000000000000</Keywords><TimeCreated SystemTime='2026-10-29T16:05:12.6290049Z'/>\
  <EventRecordID>31913</EventRecordID><Correlation/><Execution ProcessID='10572' ThreadID='6440'/>\
  <Channel>Application</Channel><Computer>...</Computer><Security UserID='S-1-5-18'/></System>\
 <EventData>\
  <Data Name='AppName'>ShadowLogger.exe</Data><Data Name='AppVersion'>2.2.5.0</Data>\
  <Data Name='AppTimeStamp'>6ab4e6ce</Data><Data Name='ModuleName'>unknown</Data>\
  <Data Name='ModuleVersion'>0.0.0.0</Data><Data Name='ModuleTimeStamp'>00000000</Data>\
  <Data Name='ExceptionCode'>c0000005</Data><Data Name='FaultingOffset'>00007ffdc8a81855</Data>\
  <Data Name='ProcessId'>0x1018</Data><Data Name='ProcessCreationTime'>0x1dd67b64620c924</Data>\
  <Data Name='AppPath'>C:\\Program Files\\Shadow\\ShadowLogger\\ShadowLogger.exe</Data>\
  <Data Name='ModulePath'>unknown</Data><Data Name='IntegratorReportId'>19912abb-...</Data>\
  <Data Name='PackageFullName'></Data><Data Name='PackageRelativeAppId'></Data>\
 </EventData></Event>";

    /// An Application Hang as Windows wrote it on this PC (4 November 2026).
    const APP_HANG: &str = "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
<Provider Name='Application Hang' Guid='{c631c3dc-c676-59e4-2db3-5c0af00f9675}'/><EventID>1002</EventID>\
<Level>2</Level><Task>101</Task><TimeCreated SystemTime='2026-10-21T08:00:01.0000000Z'/><Channel>Application</Channel></System>\
<EventData><Data Name='AppName'>conhost.exe</Data><Data Name='AppVersion'>10.0.22621.3672</Data>\
 <Data Name='ProcessId'>0x4e50</Data><Data Name='StartTime'>0x1dd61fc37e945e4</Data>\
 <Data Name='TerminationTime'>4294967295</Data><Data Name='ExeFileName'>C:\\Windows\\System32\\conhost.exe</Data>\
 <Data Name='ReportId'>49ae43de-...</Data><Data Name='PackageFullName'></Data>\
 <Data Name='PackageRelativeAppId'></Data><Data Name='HangType'>Quiesce</Data></EventData></Event>";

    /// No display event was on this PC, so this one is built the way a
    /// source with no manifest writes one: unnamed fields, the driver's name
    /// first, and the event id with its qualifiers.
    const DISPLAY_RESET: &str = "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
<Provider Name='Display'/><EventID Qualifiers='0'>4101</EventID><Level>3</Level>\
<TimeCreated SystemTime='2026-11-04T19:31:38.1234567Z'/><Execution ProcessID='0' ThreadID='0'/>\
<Channel>System</Channel><Computer>pc</Computer><Security/></System>\
<EventData><Data>amdkmdag</Data><Data></Data></EventData></Event>";

    const CEMU_PID: u32 = 0x1018;
    const CEMU_CREATED: u64 = 0x1dd6_7b64_620c_924;

    #[test]
    fn reads_an_application_error_field_by_field() {
        let event = parse_event(APP_ERROR).unwrap();
        assert_eq!(event.channel, "Application");
        assert_eq!(event.provider, "Application Error");
        assert_eq!(event.id, 1000);
        assert_eq!(event.time, "2026-10-29T16:05:12.6290049Z");
        assert_eq!(event.data.len(), 15);
        assert_eq!(event.data[0], ("AppName".to_string(), "ShadowLogger.exe".to_string()));
        assert_eq!(event.field("ExceptionCode"), Some("c0000005"));
        assert_eq!(event.field("PackageFullName"), Some(""));
        assert_eq!(event.data[14].0, "PackageRelativeAppId");
    }

    #[test]
    fn reads_an_event_with_unnamed_fields() {
        let event = parse_event(DISPLAY_RESET).unwrap();
        assert_eq!((event.provider.as_str(), event.id, event.channel.as_str()), ("Display", 4101, "System"));
        assert_eq!(event.data, vec![(String::new(), "amdkmdag".to_string()), (String::new(), String::new())]);
    }

    #[test]
    fn unescapes_what_the_xml_escaped() {
        let xml = "<Event><System><Provider Name='Application Error'/><EventID>1000</EventID></System>\
<EventData><Data Name='AppPath'>D:\\Games &amp; more\\Cemu.exe</Data><Data Name='Odd'>&lt;x&gt;&#65;</Data></EventData></Event>";
        let event = parse_event(xml).unwrap();
        assert_eq!(event.field("AppPath"), Some("D:\\Games & more\\Cemu.exe"));
        assert_eq!(event.field("Odd"), Some("<x>A"));
    }

    #[test]
    fn something_that_isnt_an_event_is_nothing() {
        assert!(parse_event("").is_none());
        assert!(parse_event("not xml at all").is_none());
        assert!(parse_event("<Event><System><Provider Name='Display'/></System></Event>").is_none());
        assert!(parse_event("<Event><System><EventID>1000</EventID></System></Event>").is_none());
    }

    #[test]
    fn an_event_is_about_a_process_only_by_its_id_and_start_time() {
        let crash = parse_event(APP_ERROR).unwrap();
        assert!(is_about(&crash, CEMU_PID, CEMU_CREATED));
        assert!(!is_about(&crash, CEMU_PID + 4, CEMU_CREATED));
        assert!(!is_about(&crash, CEMU_PID, CEMU_CREATED + 1));
        // The System part names the process that wrote the event.
        assert!(!is_about(&crash, 10572, CEMU_CREATED));

        let hang = parse_event(APP_HANG).unwrap();
        assert!(is_about(&hang, 0x4e50, 0x1dd_61fc_37e9_45e4));
        assert!(!is_about(&hang, 0x4e50, CEMU_CREATED));

        // A display event names no process at all.
        let display = parse_event(DISPLAY_RESET).unwrap();
        assert!(!is_about(&display, 0, 0));
        assert!(is_display_event(&display));
        assert!(!is_display_event(&crash));
    }

    #[test]
    fn hex_fields_match_whatever_their_case_or_zeros() {
        let mut crash = parse_event(APP_ERROR).unwrap();
        for (name, value) in crash.data.iter_mut() {
            if name == "ProcessId" {
                *value = "0x00001018".into();
            } else if name == "ProcessCreationTime" {
                *value = "0X1DD67B64620C924".into();
            }
        }
        assert!(is_about(&crash, CEMU_PID, CEMU_CREATED));
    }

    #[test]
    fn an_events_note_is_in_windows_words_and_keeps_only_what_says_something() {
        let note = event_note(&parse_event(APP_ERROR).unwrap());
        assert!(note.starts_with(
            "Omoio's note: Windows logged event 1000 (Application Error) in its Application log at 2026-10-29 16:05:12 UTC: "
        ));
        assert!(note.contains("AppName=ShadowLogger.exe, AppVersion=2.2.5.0"));
        assert!(note.contains("ExceptionCode=c0000005"));
        assert!(note.contains("ProcessId=0x1018"));
        // Time stamps, report ids and paths are left out.
        for left_out in ["ProcessCreationTime", "AppTimeStamp", "IntegratorReportId", "AppPath", "ModulePath"] {
            assert!(!note.contains(left_out), "{left_out}");
        }
        assert!(!note.contains("Program Files") && !note.contains(":\\"));

        let hang = event_note(&parse_event(APP_HANG).unwrap());
        assert!(hang.contains("AppName=conhost.exe, AppVersion=10.0.22621.3672, ProcessId=0x4e50"), "{hang}");
        assert!(!hang.contains("System32") && !hang.contains("ExeFileName"));

        assert_eq!(
            event_note(&parse_event(DISPLAY_RESET).unwrap()),
            "Omoio's note: Windows logged event 4101 (Display) in its System log at 2026-11-04 19:31:38 UTC: \
             Display driver amdkmdag stopped responding and has successfully recovered."
        );
    }

    // ---- RPCS3's marks ----------------------------------------------------

    #[test]
    fn keeps_errors_and_fatals_and_leaves_the_noise() {
        let log = "·! 0:00:01.0 SYS: starting\n\
·W 0:00:02.0 sys_fs: a warning\n\
·E 0:00:03.0 sys_fs: could not open thing\n\
·T 0:00:04.0 trace noise\n\
·F 0:00:05.0 RSX: it fell over\n";
        let problems = log_problems(Console::Ps3, log, 20);
        assert_eq!(problems.len(), 2);
        assert!(problems[0].contains("could not open thing"));
        assert!(problems[1].contains("it fell over"));
    }

    #[test]
    fn one_failure_repeated_every_frame_is_still_one_problem() {
        let log = "·E 0:00:01.0 RSX: same thing\n\
·E 0:00:02.5 RSX: same thing\n\
·E 0:00:09.9 RSX: same thing\n\
·E 0:00:11.0 RSX: a different thing\n";
        assert_eq!(log_problems(Console::Ps3, log, 20).len(), 2);
    }

    #[test]
    fn stops_collecting_at_the_limit() {
        let log: String = (0..50).map(|i| format!("·E 0:00:0{i}.0 problem {i}\n")).collect();
        assert_eq!(log_problems(Console::Ps3, &log, 10).len(), 10);
    }

    #[test]
    fn a_crash_is_only_claimed_when_something_fatal_was_logged() {
        assert_eq!(
            log_crash(Console::Ps3, "·! fine\n·F 0:00:01.0 RSX: it fell over\n").as_deref(),
            Some("·F 0:00:01.0 RSX: it fell over")
        );
        // A session full of errors that still ended cleanly is not a crash.
        assert!(log_crash(Console::Ps3, "·E 0:00:01.0 sys_fs: missing optional file\n·W 0:00:02.0 warning\n").is_none());
        assert!(log_crash(Console::Ps3, "").is_none());
    }

    /// The shape RPCS3's crash handler writes, after the research of
    /// 4 November 2026; the pointers are made up.
    const RPCS3_CRASH: &str = "·! 0:12:30.000000 SYS: fine so far\n\
·F 0:12:34.567890 {PPU[0x1000000] Thread (main_thread) [0x00abcdef]} SYS: \n\
Unhandled Win32 exception 0xC0000005.\n\
Segfault reading location 0000000000000000 at 00007FF6A1B2C3D4.\n\
Thread id = 1234.\n\
·F 0:12:34.570001 SIG: Thread terminated due to fatal error: Unhandled Win32 exception 0xC0000005.\n";

    #[test]
    fn a_fatal_line_with_nothing_after_its_colon_takes_the_lines_below() {
        let joined = "·F 0:12:34.567890 {PPU[0x1000000] Thread (main_thread) [0x00abcdef]} SYS: \
                      Unhandled Win32 exception 0xC0000005. \
                      Segfault reading location 0000000000000000 at 00007FF6A1B2C3D4.";
        assert_eq!(log_crash(Console::Ps3, RPCS3_CRASH).as_deref(), Some(joined));
        let problems = log_problems(Console::Ps3, RPCS3_CRASH, 20);
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0], joined);
        assert!(problems[1].contains("SIG: Thread terminated"));
    }

    // ---- Cemu's marks -----------------------------------------------------

    /// A crash Cemu caught: its format exactly, the values made up (the
    /// research of 4 November 2026).
    const CEMU_CRASH: &str = "[20:14:02.511] ------- Init Cemu c170da5 -------\n\
[20:14:02.512] CPU: AMD Ryzen 5 5625U with Radeon Graphics\n\
[20:14:02.512] RAM: 5951MB\n\
[20:14:03.390] Using GPU: AMD Radeon(TM) Graphics\n\
[20:14:05.004] ------- Loaded title -------\n\
[20:14:05.004] TitleVersion: v32\n\
[20:14:06.120] Skylanders portal IPC: listening on 127.0.0.1:28411\n\
\n\
Crashlog for Cemu c170da5\n\
Date: 04-11-2026 19:31:40\n\
\n\
-----------------------------------------\n\
   Stack trace\n\
-----------------------------------------\n\
0x00007ff6d1a3b2c0 +0x0013b2c0 Cemu.exe         DumpThreadStackTrace\n\
0x00007ffb8c2e1a10 +0x000e1a10 KERNELBASE.dll   UnhandledExceptionFilter\n\
Exception 0xc0000005 at 0x7ffb61a2c3d4(+0x2c3d4) in module amdvlk64.dll\n\
cemu.exe at 0x7ff6d1900000\n\
\n\
RAX=0000000000000000 RBX=000001f8213f0000 RCX=0000000000000001 RDX=0000000000000000\n\
\n\
-----------------------------------------\n\
   Game info\n\
-----------------------------------------\n\
Game: Skylanders Imaginators\n\
TitleId: 0005000010abcdef\n\
RPXHash: 1a2b3c4d (Update: 5e6f7a8b)\n";

    /// The head of a Cemu session kept on this PC.
    const CEMU_NORMAL: &str = "[01:49:13.994] ------- Init Cemu c170da5 -------\n\
[01:49:14.007] IOSU_CRYPTO: No otp.bin found. Online mode cannot be used\n\
[01:49:20.606] Shader cache loaded with 269 shaders. Commited mem 93MB. Took 2614ms\n\
[01:49:21.139] Game attempting to re-initialize existing thread\n";

    #[test]
    fn cemus_crashlog_is_a_crash_and_names_its_module() {
        assert_eq!(
            log_crash(Console::WiiU, CEMU_CRASH).as_deref(),
            Some("Exception 0xc0000005 at 0x7ffb61a2c3d4(+0x2c3d4) in module amdvlk64.dll")
        );
        assert_eq!(
            log_problems(Console::WiiU, CEMU_CRASH, 40),
            vec![
                "Exception 0xc0000005 at 0x7ffb61a2c3d4(+0x2c3d4) in module amdvlk64.dll".to_string(),
                "Game: Skylanders Imaginators".to_string(),
            ]
        );
    }

    #[test]
    fn a_crashlog_cut_short_is_still_a_crash() {
        let log = "[20:14:05.004] ------- Loaded title -------\n\nCrashlog for Cemu c170da5\nDate: 04-11-2026 19:31:40\n";
        assert_eq!(log_crash(Console::WiiU, log).as_deref(), Some("Crashlog for Cemu c170da5"));
    }

    #[test]
    fn an_assert_that_ran_out_of_memory_leaves_its_line_above_the_crashlog() {
        let log = "[20:31:39.880] Ran out of system memory\n\nCrashlog for Cemu c170da5\n\
Exception 0x80000003 at 0x7ff6d1a01234(+0x101234) in module Cemu.exe\n";
        assert!(log_crash(Console::WiiU, log).unwrap().starts_with("Exception 0x80000003"));
        let problems = log_problems(Console::WiiU, log, 40);
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0], "[20:31:39.880] Ran out of system memory");
    }

    #[test]
    fn a_renderer_error_without_a_crashlog_is_trouble_but_not_a_crash() {
        let log = "[20:31:39.880] Unrecoverable error in Vulkan renderer\n\
[20:31:39.880] Msg: failed to submit command buffer. Error -4\n";
        assert!(log_crash(Console::WiiU, log).is_none());
        assert_eq!(log_problems(Console::WiiU, log, 40).len(), 2);
    }

    #[test]
    fn a_wx_assert_writes_a_stack_trace_but_is_no_crash() {
        let log = "[20:40:00.000] Encountered wxWidgets assert!\n\
[20:40:00.000] File: ..\\src\\common\\wincmn.cpp Line: 123\n\
-----------------------------------------\n\
   Stack trace\n\
-----------------------------------------\n\
0x00007ff6d1a3b2c0 +0x0013b2c0 Cemu.exe         DumpThreadStackTrace\n";
        assert!(log_crash(Console::WiiU, log).is_none());
        assert_eq!(log_problems(Console::WiiU, log, 40), vec!["[20:40:00.000] Encountered wxWidgets assert!".to_string()]);
    }

    #[test]
    fn a_normal_cemu_session_holds_nothing_worth_reading() {
        assert!(log_crash(Console::WiiU, CEMU_NORMAL).is_none());
        assert!(log_problems(Console::WiiU, CEMU_NORMAL, 40).is_empty());
    }

    #[test]
    fn cemus_msg_and_game_lines_count_only_where_they_belong() {
        let log = "[20:00:00.000] Msg: a line on its own\n\
[20:00:01.000] Game: not in a crashlog\n\
[20:00:02.000] OSPanic!\n\
[20:00:02.000] File: game.cpp:12\n\
[20:00:02.000] Msg: the game gave up\n\
[20:00:03.000] OSPanic!\n\
[20:00:03.000] File: game.cpp:40\n\
[20:00:03.000] Msg: could not open C:\\Users\\Someone\\save.dat\n";
        assert_eq!(
            log_problems(Console::WiiU, log, 40),
            vec!["[20:00:02.000] OSPanic!".to_string(), "[20:00:02.000] Msg: the game gave up".to_string()]
        );
    }

    // ---- Dolphin's marks --------------------------------------------------

    /// What comes before a Dolphin crash, in its format exactly (the research
    /// of 4 November 2026; the SSL warning's path and link are left out here
    /// too).
    const DOLPHIN_TROUBLE: &str = "20:34:177 Core\\ConfigManager.cpp:279 N[CORE]: Active title: Skylanders: Spyro's Adventure (SSPP52)\n\
20:39:085 Core\\IOS\\Network\\SSL.cpp:168 W[MASTER]: Warning: IOS: Could not read a file required for SSL services (<path>). Please refer to <url> for instructions on setting up Wii networking.\n\
22:36:814 Common\\HttpRequest.cpp:283 E[COMMON]: Failed to GET http://example.invalid/forecast.bin: Could not resolve host: example.invalid\n\
31:07:077 Core\\PowerPC\\MMU.cpp:1228 W[MASTER]: Warning: Invalid read from 0x00000004, PC = 0x80234567.\n\
\n\
The game probably would have crashed on real hardware. Enable MMU in advanced settings to accurately emulate game crashes.\n\
31:07:090 VideoCommon\\CommandProcessor.cpp:642 E[Video]: FIFO: Unknown Opcode 0x18 @ 0x1f2a3b4c5d6, preprocessing = no, CPBase: 0x00000000\n\
31:07:091 VideoCommon\\CommandProcessor.cpp:713 W[MASTER]: Warning: GFX FIFO: Unknown Opcode (0x18 @ 0x1f2a3b4c5d6, preprocess=false).\n\
\n\
This error is usually caused by the emulated GPU desyncing with the emulated CPU. Turn off the \"Dual Core\" setting to avoid this.\n\
\n\
Further errors will be sent to the Video Backend log and Dolphin will now likely crash or hang.\n";

    #[test]
    fn dolphin_keeps_its_errors_and_alerts_and_leaves_what_every_session_has() {
        let problems = log_problems(Console::Wii, DOLPHIN_TROUBLE, 40);
        assert_eq!(problems.len(), 3);
        assert!(problems[0].ends_with("W[MASTER]: Warning: Invalid read from 0x00000004, PC = 0x80234567."));
        assert!(problems[1].contains("E[Video]: FIFO: Unknown Opcode"));
        assert!(problems[2].contains("W[MASTER]: Warning: GFX FIFO"));
        assert!(problems.iter().all(|p| !p.contains("SSL") && !p.contains("Failed to GET")));
        // Its log never proves a crash.
        assert!(log_crash(Console::Wii, DOLPHIN_TROUBLE).is_none());
        assert!(log_crash(Console::GameCube, DOLPHIN_TROUBLE).is_none());
    }

    #[test]
    fn dolphin_keeps_a_lost_device() {
        let log = "12:00:001 VideoBackends\\Vulkan\\CommandBufferManager.cpp:433 E[Video]: (SubmitCommandBuffer:433) vkQueueSubmit failed:  (-4: VK_ERROR_DEVICE_LOST)\n\
12:00:001 VideoBackends\\Vulkan\\CommandBufferManager.cpp:435 W[MASTER]: Warning: Failed to submit command buffer: VK_ERROR_DEVICE_LOST (-4)\n\
12:00:002 VideoBackends\\D3DCommon\\Shader.cpp:290 W[Video]: ps_5_0 compilation succeeded with warnings:\n\
12:00:003 Common\\DirectIOFile.cpp:109 W[COMMON]: CreateFile: The system cannot find the file specified.\n\
12:00:004 VideoBackends\\D3D\\D3DBase.cpp:162 N[Video]: Successfully released all device references!\n";
        assert_eq!(log_problems(Console::GameCube, log, 40).len(), 2);
    }

    #[test]
    fn dolphin_never_shows_a_line_with_a_path() {
        let log = "12:00:001 Core\\Boot\\Boot.cpp:100 E[BOOT]: Could not open D:\\Games\\Someone\\game.iso\n";
        assert!(log_problems(Console::Wii, log, 40).is_empty());
    }

    #[test]
    fn omoios_notes_never_pass_for_a_crash() {
        let mut notes: Vec<String> = [0xC000_0005, 1, 0].iter().map(|code| exit_note("Cemu", *code)).collect();
        notes.push(event_note(&parse_event(APP_ERROR).unwrap()));
        notes.push(event_note(&parse_event(DISPLAY_RESET).unwrap()));
        notes.push("Omoio, 1:02 into the game: asked the emulator to close".to_string());
        let text = notes.join("\n");
        for console in Console::ALL {
            assert!(log_crash(console, &text).is_none(), "{console:?}");
            assert!(log_problems(console, &text, 40).is_empty(), "{console:?}");
        }
    }

    // ---- how it ended -----------------------------------------------------

    fn crash_event() -> WinEvent {
        parse_event(APP_ERROR).unwrap()
    }

    #[test]
    fn a_refusal_comes_first_then_omoios_own_stop() {
        let refused = Evidence {
            refusal: Some("Cemu couldn't find the game's files.".into()),
            stopped_by_us: true,
            exit_code: Some(0xC000_0005),
            ..Default::default()
        };
        assert_eq!(decide(&refused), Ending::Refused);
        let stopped = Evidence { stopped_by_us: true, exit_code: Some(0xC000_0005), events: vec![crash_event()], ..Default::default() };
        assert_eq!(decide(&stopped), Ending::Stopped);
    }

    #[test]
    fn a_crash_needs_evidence() {
        let exception = Evidence { exit_code: Some(0xC000_0409), ..Default::default() };
        assert_eq!(decide(&exception), Ending::Crashed);
        let crashlog = Evidence { exit_code: Some(0), crash_line: Some("Crashlog for Cemu c170da5".into()), ..Default::default() };
        assert_eq!(decide(&crashlog), Ending::Crashed);
        let reported = Evidence { exit_code: None, events: vec![crash_event()], ..Default::default() };
        assert_eq!(decide(&reported), Ending::Crashed);
    }

    #[test]
    fn task_manager_a_hang_and_a_driver_reset_are_closed_normally() {
        assert_eq!(decide(&Evidence { exit_code: Some(1), ..Default::default() }), Ending::Closed);
        assert_eq!(decide(&Evidence { exit_code: Some(0xFFFF_FFFF), ..Default::default() }), Ending::Closed);
        let hang = Evidence { exit_code: Some(1), events: vec![parse_event(APP_HANG).unwrap()], ..Default::default() };
        assert_eq!(decide(&hang), Ending::Closed);
        let reset = Evidence { exit_code: Some(0), events: vec![parse_event(DISPLAY_RESET).unwrap()], ..Default::default() };
        assert_eq!(decide(&reset), Ending::Closed);
        assert_eq!(decide(&Evidence::default()), Ending::Closed);
    }

    #[test]
    fn notes_come_only_after_an_end_omoio_didnt_cause() {
        let ended = Evidence { exit_code: Some(0xC000_0005), events: vec![crash_event()], ..Default::default() };
        let kept = notes("Cemu", &ended);
        assert_eq!(kept.len(), 2);
        assert!(kept[0].contains("0xC0000005"));
        assert!(kept[1].contains("event 1000"));
        assert_eq!(notes("Cemu", &Evidence::default()), vec!["Omoio's note: Omoio couldn't read Cemu's exit code.".to_string()]);
        assert!(notes("Cemu", &Evidence { stopped_by_us: true, exit_code: Some(1), ..Default::default() }).is_empty());
        assert!(notes("RPCS3", &Evidence { refusal: Some("x".into()), exit_code: Some(1), ..Default::default() }).is_empty());
    }

    fn summary_of(console: Console, emulator: &str, evidence: &Evidence, problems: &[String]) -> String {
        summary(console, emulator, decide(evidence), evidence, problems)
    }

    #[test]
    fn cemus_summary_says_what_omoio_found() {
        let crashlog = Evidence { exit_code: Some(0), crash_line: Some("Exception 0xc0000005 at 0x1 in module amdvlk64.dll".into()), ..Default::default() };
        let lines = vec!["Exception 0xc0000005 at 0x1 in module amdvlk64.dll".to_string(), "Game: Skylanders Imaginators".to_string()];
        assert_eq!(summary_of(Console::WiiU, "Cemu", &crashlog, &lines), "Cemu caught a crash and closed.");

        let reported = Evidence { exit_code: Some(0xC000_0409), events: vec![crash_event()], ..Default::default() };
        assert_eq!(summary_of(Console::WiiU, "Cemu", &reported, &[]), "Windows reported that Cemu crashed.");
        let trouble = vec!["[20:31:39.880] Unrecoverable error in Vulkan renderer".to_string(), "[20:31:39.880] Msg: x".to_string()];
        assert_eq!(summary_of(Console::WiiU, "Cemu", &reported, &trouble), "Windows reported that Cemu crashed. 2 problems logged.");

        let exception = Evidence { exit_code: Some(0xC000_0374), ..Default::default() };
        assert_eq!(summary_of(Console::WiiU, "Cemu", &exception, &[]), "Cemu ended unexpectedly.");

        let closed = Evidence { exit_code: Some(0), ..Default::default() };
        assert_eq!(summary_of(Console::WiiU, "Cemu", &closed, &[]), "Cemu's log shows no crash.");
        assert_eq!(summary_of(Console::WiiU, "Cemu", &closed, &trouble), "Cemu's log shows no crash. 2 problems logged.");

        // Stopped by the player while Cemu was stuck after its crashlog.
        let stopped = Evidence { stopped_by_us: true, crash_line: Some("Crashlog for Cemu c170da5".into()), ..Default::default() };
        assert_eq!(summary_of(Console::WiiU, "Cemu", &stopped, &[]), "Cemu caught a crash.");
    }

    #[test]
    fn rpcs3_and_dolphin_summaries_count_errors_as_the_page_did() {
        let closed = Evidence { exit_code: Some(0), ..Default::default() };
        assert_eq!(summary_of(Console::Ps3, "RPCS3", &closed, &[]), "RPCS3 logged no errors.");
        assert_eq!(summary_of(Console::Ps3, "RPCS3", &closed, &["·E a".to_string()]), "1 error logged.");
        assert_eq!(summary_of(Console::Wii, "Dolphin", &closed, &[]), "Dolphin logged no errors.");

        let fatal = Evidence { exit_code: Some(0xC000_0409), crash_line: Some("·F 0:00:01.0 RSX: it fell over".into()), ..Default::default() };
        let three = vec!["·E a".to_string(), "·E b".to_string(), "·F c".to_string()];
        assert_eq!(summary_of(Console::Ps3, "RPCS3", &fatal, &three), "RPCS3 logged a fatal error. 3 errors logged.");

        let reported = Evidence { exit_code: Some(0xC000_0005), events: vec![crash_event()], ..Default::default() };
        assert_eq!(
            summary_of(Console::GameCube, "Dolphin", &reported, &[]),
            "Windows reported that Dolphin crashed. Dolphin logged no errors."
        );
        let exception = Evidence { exit_code: Some(0xC000_0005), ..Default::default() };
        assert_eq!(summary_of(Console::Wii, "Dolphin", &exception, &three), "Dolphin ended unexpectedly. 3 problems logged.");
    }

    #[test]
    fn a_hang_and_a_driver_reset_are_said_beside_the_rest() {
        let hang = Evidence { exit_code: Some(1), events: vec![parse_event(APP_HANG).unwrap()], ..Default::default() };
        assert_eq!(
            summary_of(Console::WiiU, "Cemu", &hang, &[]),
            "Windows reported that Cemu stopped responding and was closed. Cemu's log shows no crash."
        );
        let reset = Evidence { exit_code: Some(0xC000_0409), events: vec![crash_event(), parse_event(DISPLAY_RESET).unwrap()], ..Default::default() };
        assert_eq!(
            summary_of(Console::WiiU, "Cemu", &reset, &[]),
            "Windows reported that Cemu crashed. Windows reported a display driver problem during the game."
        );
    }

    #[test]
    fn a_refusal_is_said_in_omoios_own_words() {
        let refused = Evidence { refusal: Some("Dolphin couldn't read that disc image.".into()), ..Default::default() };
        assert_eq!(summary_of(Console::Wii, "Dolphin", &refused, &[]), "Dolphin couldn't read that disc image.");
    }

    #[test]
    fn a_summary_never_holds_a_code_a_path_or_an_exception() {
        let line = "Exception 0xc0000005 at 0x1 in module amdvlk64.dll".to_string();
        let cases = [
            Evidence { exit_code: Some(0xC000_0005), crash_line: Some(line.clone()), ..Default::default() },
            Evidence { exit_code: Some(0xC000_0409), events: vec![crash_event(), parse_event(DISPLAY_RESET).unwrap()], ..Default::default() },
            Evidence { exit_code: Some(0xC000_0374), ..Default::default() },
            Evidence { exit_code: Some(1), events: vec![parse_event(APP_HANG).unwrap()], ..Default::default() },
        ];
        for evidence in &cases {
            for console in Console::ALL {
                let said = summary_of(console, "Emulator", evidence, &[line.clone()]);
                assert!(!said.contains("0x") && !said.contains(":\\") && !said.contains("Exception"), "{said}");
                assert!(!said.contains('\u{2014}') && !said.contains('!'), "{said}");
            }
        }
    }
}
