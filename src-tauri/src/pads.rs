//! The pads plugged in, and what is held on them right now.
//!
//! Xbox-style pads are read through XInput by slot, the same slots the
//! emulators use. The Guide button is left out of `XInputGetState`. RPCS3
//! reads it through `XInputGetStateEx`, which xinput1_4.dll exports by
//! ordinal 100 only, with Guide as bit 0x0400, and so does this. Where that
//! export is missing, everything but Guide still works.
//!
//! Every other pad, a DualSense or a Switch Pro or an 8BitDo, is read with
//! gilrs, which names buttons by place the way layouts do. gilrs keeps its
//! picture of each pad current as events arrive, so one thread owns it for as
//! long as Omoio runs and publishes what it sees, starting it again whenever a
//! pad leaves and before a game starts (`watch`, `rescan`).

use crate::core::pad_layout::Pad;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// USB vendor ids, which say who made a pad and so how its buttons are
/// marked.
const MICROSOFT: u16 = 0x045E;
const SONY: u16 = 0x054C;
const NINTENDO: u16 = 0x057E;

/// One of the four XInput slots, counted from zero. What sits in a slot is not
/// known, only that it is laid out like an Xbox pad, which is what XInput
/// describes.
pub fn xinput_slot(slot: usize) -> Pad {
    Pad {
        device: format!("XInput Pad #{}", slot + 1),
        name: format!("Controller {}", slot + 1),
        handler: "XInput".to_string(),
        family: "xbox".to_string(),
    }
}

/// The four XInput slots, which a player can be given before anything is in
/// them.
pub fn xinput_slots() -> Vec<Pad> {
    (0..4).map(xinput_slot).collect()
}

fn family_of(vendor: Option<u16>) -> &'static str {
    match vendor {
        Some(SONY) => "playstation",
        Some(NINTENDO) => "nintendo",
        Some(MICROSOFT) => "xbox",
        _ => "generic",
    }
}

/// The inputs held in one XInput state, by place. A stick counts once it is
/// pushed about half way and a trigger once it is pulled a quarter, so a pad
/// resting slightly off centre never records.
fn decode(buttons: u16, left_trigger: u8, right_trigger: u8, sticks: [i16; 4]) -> Vec<&'static str> {
    const BUTTONS: [(u16, &str); 15] = [
        (0x0001, "Up"),
        (0x0002, "Down"),
        (0x0004, "Left"),
        (0x0008, "Right"),
        (0x0010, "Start"),
        (0x0020, "Back"),
        (0x0040, "LS"),
        (0x0080, "RS"),
        (0x0100, "LB"),
        (0x0200, "RB"),
        (0x0400, "Guide"),
        (0x1000, "South"),
        (0x2000, "East"),
        (0x4000, "West"),
        (0x8000, "North"),
    ];
    const PUSHED: i16 = 16_000;
    const PULLED: u8 = 64;

    let mut held: Vec<&'static str> = BUTTONS
        .iter()
        .filter(|(bit, _)| buttons & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if left_trigger > PULLED {
        held.push("LT");
    }
    if right_trigger > PULLED {
        held.push("RT");
    }
    // XInput counts up as positive, which is also what "LS Y+" means.
    let [lx, ly, rx, ry] = sticks;
    for (value, plus, minus) in [
        (lx, "LS X+", "LS X-"),
        (ly, "LS Y+", "LS Y-"),
        (rx, "RS X+", "RS X-"),
        (ry, "RS Y+", "RS Y-"),
    ] {
        if value > PUSHED {
            held.push(plus);
        } else if value < -PUSHED {
            held.push(minus);
        }
    }
    held
}

#[cfg(windows)]
type GetStateEx =
    unsafe extern "system" fn(u32, *mut windows::Win32::UI::Input::XboxController::XINPUT_STATE) -> u32;

/// Looked up once. The library stays loaded for as long as Omoio runs, as it
/// would had XInput been linked the ordinary way.
#[cfg(windows)]
fn get_state_ex() -> Option<GetStateEx> {
    use windows::core::{w, PCSTR};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    static FOUND: OnceLock<Option<GetStateEx>> = OnceLock::new();
    *FOUND.get_or_init(|| {
        // Calls into Windows, which Rust cannot check. Asking by ordinal is
        // passing the number where a name would go, as the API allows.
        let library = unsafe { LoadLibraryW(w!("xinput1_4.dll")) }.ok()?;
        let found = unsafe { GetProcAddress(library, PCSTR(100 as *const u8)) }?;
        // The export has XInputGetState's shape, with Guide filled in too.
        Some(unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, GetStateEx>(found) })
    })
}

/// What is held on the pad in XInput slot `slot`, counted from zero. `None`
/// when no pad answers there.
#[cfg(windows)]
fn xinput_held(slot: u32) -> Option<Vec<&'static str>> {
    use windows::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};

    let mut state = XINPUT_STATE::default();
    // Both only fill in the struct handed over. Zero means a pad answered.
    let answered = match get_state_ex() {
        Some(get_state) => unsafe { get_state(slot, &mut state) == 0 },
        None => unsafe { XInputGetState(slot, &mut state) == 0 },
    };
    if !answered {
        return None;
    }
    let pad = state.Gamepad;
    Some(decode(
        pad.wButtons.0,
        pad.bLeftTrigger,
        pad.bRightTrigger,
        [pad.sThumbLX, pad.sThumbLY, pad.sThumbRX, pad.sThumbRY],
    ))
}

#[cfg(not(windows))]
fn xinput_held(_slot: u32) -> Option<Vec<&'static str>> {
    None
}

/// What a pad gilrs reads has held, by place. Triggers are buttons on some
/// pads and axes on others, so both are looked at.
fn sdl_held(pad: &gilrs::Gamepad) -> Vec<&'static str> {
    use gilrs::{Axis, Button};
    const BUTTONS: [(Button, &str); 17] = [
        (Button::South, "South"),
        (Button::East, "East"),
        (Button::West, "West"),
        (Button::North, "North"),
        (Button::LeftTrigger, "LB"),
        (Button::RightTrigger, "RB"),
        (Button::LeftTrigger2, "LT"),
        (Button::RightTrigger2, "RT"),
        (Button::LeftThumb, "LS"),
        (Button::RightThumb, "RS"),
        (Button::DPadUp, "Up"),
        (Button::DPadDown, "Down"),
        (Button::DPadLeft, "Left"),
        (Button::DPadRight, "Right"),
        (Button::Select, "Back"),
        (Button::Start, "Start"),
        (Button::Mode, "Guide"),
    ];
    let mut held: Vec<&'static str> = BUTTONS
        .iter()
        .filter(|(button, _)| pad.is_pressed(*button))
        .map(|(_, name)| *name)
        .collect();
    for (axis, name) in [(Axis::LeftZ, "LT"), (Axis::RightZ, "RT")] {
        if !held.contains(&name) && pad.value(axis) > 0.25 {
            held.push(name);
        }
    }
    for (axis, plus, minus) in [
        (Axis::LeftStickX, "LS X+", "LS X-"),
        (Axis::LeftStickY, "LS Y+", "LS Y-"),
        (Axis::RightStickX, "RS X+", "RS X-"),
        (Axis::RightStickY, "RS Y+", "RS Y-"),
    ] {
        let value = pad.value(axis);
        if value > 0.5 {
            held.push(plus);
        } else if value < -0.5 {
            held.push(minus);
        }
    }
    held
}

/// A pad as gilrs last saw it.
struct Seen {
    pad: Pad,
    vendor: Option<u16>,
    product: Option<u16>,
    /// Whether Windows takes it for an Xbox-style pad, which XInput reads
    /// too.
    xbox_like: bool,
    held: Vec<&'static str>,
}

/// Whether a pad gilrs reads is one XInput also answers for. Microsoft's own
/// are, and so is any pad Windows.Gaming.Input takes for a gamepad, such as a
/// PowerA or PDP Xbox pad: gilrs gives exactly those no uuid
/// (`Gamepad::new`, `Uuid::nil()` when `WgiGamepad::FromGameController`
/// succeeds, gilrs-core 0.6.8's `windows_wgi/gamepad.rs`, read 8 October
/// 2026). Listed twice, the second one took a player of its own.
fn answers_through_xinput(vendor: Option<u16>, xbox_like: bool) -> bool {
    vendor == Some(MICROSOFT) || xbox_like
}

/// `None` until gilrs has had its first look.
fn seen() -> &'static Mutex<Option<Vec<Seen>>> {
    static SEEN: OnceLock<Mutex<Option<Vec<Seen>>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(None))
}

/// Set by `rescan` to have gilrs started again before its next look, and
/// cleared once that look is published.
static LOOK_AGAIN: AtomicBool = AtomicBool::new(false);
/// Set when gilrs couldn't start at all, so nothing waits for it to look.
static NO_GILRS: AtomicBool = AtomicBool::new(false);

/// Starts the thread that owns gilrs the first time a pad is asked about, and
/// waits a moment for its first look so that question gets a real answer.
///
/// gilrs keeps every pad it has seen for as long as it lives. One that leaves
/// stays on its list, unconnected, and when that connection comes back it is
/// matched by the id Windows gives the connection and keeps the controller
/// object it had the first time, which its ids are read from (`handle_event`
/// and `Gamepad::vendor_id` in gilrs-core 0.6.8's `windows_wgi/gamepad.rs`).
/// So once a pad leaves, or `rescan` asks, gilrs is started again, and every
/// pad plugged in is seen as new, as it is when Omoio starts. The old one
/// stops its own thread as it goes (its `Drop`). Rumble is left off: Omoio
/// never rumbles a pad, and gilrs 0.11.2's rumble thread runs on after its
/// gilrs is gone (`ff/server.rs`).
fn watch() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            let start = || gilrs::GilrsBuilder::new().with_force_feedback(false).build();
            let Ok(mut gilrs) = start() else {
                NO_GILRS.store(true, Ordering::SeqCst);
                *seen().lock().unwrap() = Some(Vec::new());
                return;
            };
            loop {
                let asked = LOOK_AGAIN.load(Ordering::SeqCst);
                let mut left = false;
                while let Some(event) = gilrs.next_event() {
                    left |= matches!(event.event, gilrs::EventType::Disconnected);
                }
                if left || asked {
                    if let Ok(fresh) = start() {
                        gilrs = fresh;
                    }
                }
                // Pads of the same name are numbered from zero, which tells
                // two identical controllers apart in the saved layout. RPCS3
                // names them its own way; backends/rpcs3/controllers.rs turns
                // this name into RPCS3's.
                let mut named: HashMap<String, u32> = HashMap::new();
                let now = gilrs
                    .gamepads()
                    .map(|(_, pad)| {
                        let name = pad.map_name().unwrap_or(pad.name()).to_string();
                        let at = named.entry(name.clone()).or_insert(0);
                        let device = format!("{name} {at}");
                        *at += 1;
                        let xbox_like = pad.uuid() == [0; 16];
                        Seen {
                            pad: Pad {
                                device,
                                name,
                                handler: "SDL".to_string(),
                                family: if xbox_like { "xbox" } else { family_of(pad.vendor_id()) }.to_string(),
                            },
                            vendor: pad.vendor_id(),
                            product: pad.product_id(),
                            xbox_like,
                            held: sdl_held(&pad),
                        }
                    })
                    .collect();
                *seen().lock().unwrap() = Some(now);
                if asked {
                    LOOK_AGAIN.store(false, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(16));
            }
        });
    });
    for _ in 0..50 {
        if seen().lock().unwrap().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Has gilrs look at the pads plugged in afresh, as it does when Omoio
/// starts, and waits a moment for that look. Asked before a game starts:
/// gilrs hears of pads coming and going through Windows.Gaming.Input, whose
/// events gilrs notes need a window of Omoio's in focus (gilrs 0.11.2,
/// `lib.rs`, on Windows), so a pad that came or went while a game was in
/// front may not have been heard of.
pub fn rescan() {
    watch();
    if NO_GILRS.load(Ordering::SeqCst) {
        return;
    }
    LOOK_AGAIN.store(true, Ordering::SeqCst);
    for _ in 0..50 {
        if !LOOK_AGAIN.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Which XInput slots have a pad in them.
fn xinput_in_use() -> Vec<usize> {
    (0..4).filter(|&slot| xinput_held(slot as u32).is_some()).collect()
}

/// The pads plugged in right now, XInput ones first since that is what most
/// people have.
pub fn connected() -> Vec<Pad> {
    let slots = xinput_in_use();
    let mut found: Vec<Pad> = slots.iter().map(|&slot| xinput_slot(slot)).collect();
    watch();
    if let Some(seen) = seen().lock().unwrap().as_ref() {
        // Pads XInput answers for were listed through it already, so they are
        // not listed a second time.
        found.extend(
            seen.iter()
                .filter(|s| slots.is_empty() || !answers_through_xinput(s.vendor, s.xbox_like))
                .map(|s| s.pad.clone()),
        );
    }
    found
}

/// What is held on a pad right now, by place. `None` when it does not
/// answer, which is how a pad being switched on or off shows.
pub fn held(device: &str) -> Option<Vec<&'static str>> {
    if let Some(slot) = device
        .strip_prefix("XInput Pad #")
        .and_then(|number| number.parse::<u32>().ok())
        .filter(|number| (1..=4).contains(number))
    {
        return xinput_held(slot - 1);
    }
    watch();
    seen()
        .lock()
        .unwrap()
        .as_ref()?
        .iter()
        .find(|s| s.pad.device == device)
        .map(|s| s.held.clone())
}

/// A pad's USB vendor and product ids, with how many pads of that same model
/// gilrs lists before it, which Cemu needs to find it among the HID devices.
/// `None` for a pad gilrs does not have plugged in, or one whose ids it does
/// not know.
pub fn usb_ids(device: &str) -> Option<(u16, u16, usize)> {
    usb_ids_counting(device, |a, b| a == b)
}

/// `usb_ids`, counting the pads before it that `same` takes, by their
/// vendor and product ids, for one of a kind with it. For an emulator that
/// numbers pads by something two models can share, as Dolphin numbers them
/// by the name SDL gives them.
pub fn usb_ids_counting(device: &str, same: impl Fn((u16, u16), (u16, u16)) -> bool) -> Option<(u16, u16, usize)> {
    watch();
    let seen = seen().lock().unwrap();
    let listed: Vec<(&str, Option<(u16, u16)>)> =
        seen.as_ref()?.iter().map(|s| (s.pad.device.as_str(), s.vendor.zip(s.product))).collect();
    ids_before(&listed, device, same)
}

/// `usb_ids_counting` among the pads `listed`, with their ids, in the order
/// gilrs lists them.
fn ids_before(
    listed: &[(&str, Option<(u16, u16)>)],
    device: &str,
    same: impl Fn((u16, u16), (u16, u16)) -> bool,
) -> Option<(u16, u16, usize)> {
    let at = listed.iter().position(|(name, _)| *name == device)?;
    let ids = listed[at].1?;
    let before = listed[..at].iter().filter_map(|(_, other)| *other).filter(|other| same(*other, ids)).count();
    Some((ids.0, ids.1, before))
}

/// Everything held on any pad plugged in, for a menu any player may use.
pub fn held_anywhere() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = (0..4).filter_map(xinput_held).flatten().collect();
    watch();
    if let Some(seen) = seen().lock().unwrap().as_ref() {
        for pad in seen {
            all.extend(pad.held.iter().copied());
        }
    }
    all.sort_unstable();
    all.dedup();
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_come_out_by_their_place() {
        assert_eq!(decode(0x1000 | 0x0400, 0, 0, [0; 4]), ["Guide", "South"]);
        assert_eq!(decode(0x0020 | 0x0010, 0, 0, [0; 4]), ["Start", "Back"]);
    }

    #[test]
    fn a_resting_pad_holds_nothing() {
        assert!(decode(0, 20, 30, [3_000, -4_000, 1_200, -900]).is_empty());
    }

    #[test]
    fn sticks_and_triggers_count_once_pushed() {
        assert_eq!(decode(0, 200, 0, [0, 30_000, 0, 0]), ["LT", "LS Y+"]);
        assert_eq!(decode(0, 0, 255, [0, 0, -32_768, 0]), ["RT", "RS X-"]);
    }

    #[test]
    fn who_made_a_pad_says_what_is_printed_on_it() {
        assert_eq!(family_of(Some(0x054C)), "playstation");
        assert_eq!(family_of(Some(0x057E)), "nintendo");
        assert_eq!(family_of(Some(0x045E)), "xbox");
        assert_eq!(family_of(Some(0x2DC8)), "generic");
        assert_eq!(family_of(None), "generic");
    }

    #[test]
    fn a_pad_xinput_answers_for_is_known_whoever_made_it() {
        assert!(answers_through_xinput(Some(MICROSOFT), false));
        assert!(answers_through_xinput(Some(0x20D6), true), "a PowerA pad Windows takes for a gamepad");
        assert!(!answers_through_xinput(Some(SONY), false));
        assert!(!answers_through_xinput(None, false));
    }

    #[test]
    fn pads_before_one_are_counted_by_what_makes_them_alike() {
        // Two models of the DualShock 4, then a DualSense.
        let listed = [
            ("PS4 Controller 0", Some((SONY, 0x05C4))),
            ("Mystery Pad 0", None),
            ("PS4 Controller 1", Some((SONY, 0x09CC))),
            ("DualSense Wireless Controller 0", Some((SONY, 0x0CE6))),
        ];
        let model = |a: (u16, u16), b: (u16, u16)| a == b;
        let maker = |a: (u16, u16), b: (u16, u16)| a.0 == b.0;
        assert_eq!(ids_before(&listed, "PS4 Controller 1", model), Some((SONY, 0x09CC, 0)));
        assert_eq!(ids_before(&listed, "PS4 Controller 1", maker), Some((SONY, 0x09CC, 1)));
        assert_eq!(ids_before(&listed, "DualSense Wireless Controller 0", maker), Some((SONY, 0x0CE6, 2)));
        assert_eq!(ids_before(&listed, "Mystery Pad 0", model), None, "no ids");
        assert_eq!(ids_before(&listed, "Unplugged 0", model), None);
    }

    #[test]
    fn xinput_slots_are_named_plainly() {
        let slot = xinput_slot(2);
        assert_eq!(slot.device, "XInput Pad #3");
        assert_eq!(slot.name, "Controller 3");
        assert_eq!(slot.family, "xbox");
    }
}
