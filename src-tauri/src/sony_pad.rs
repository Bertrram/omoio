//! Sony's DualSense, DualSense Edge and DualShock 4, read from their own
//! reports.
//!
//! Over Bluetooth these pads start out sending a simple report (0x01), laid
//! out as their HID description says, which is what gilrs reads through
//! Windows.Gaming.Input. An emulator's SDL and RPCS3's own handlers
//! switch them to a full report instead, 0x31 on a DualSense and 0x11 to 0x19
//! on a DualShock 4, which the HID description doesn't lay out, so gilrs
//! hears nothing more: the pad still shows as plugged in, but nothing it
//! presses reaches Omoio's Controller screen or its menus (tickets #11 and
//! #12, 12 October 2026). The switch lasts until the pad is turned off. SDL
//! calls it "a one-way ticket" (SDL release-3.4.0, `SDL_hidapi_ps5.c` line
//! 863, `SDL_hidapi_ps4.c` line 748; release-2.30.3, the SDL Cemu 2.6 is
//! built with, says "a one-way trip", `SDL_hidapi_ps5.c` line 830,
//! `SDL_hidapi_ps4.c` line 722). How each one switches a pad, read 14
//! October 2026:
//!
//! - SDL sends the pad an effects report, and asking a DualSense for its
//!   serial number or firmware is enough on its own (release-3.4.0,
//!   `SDL_hidapi_ps5.c` lines 419 to 431, `HIDAPI_DriverPS5_SetEnhancedMode`
//!   at 844; `SDL_hidapi_ps4.c`, `HIDAPI_DriverPS4_UpdateEffects` at 656).
//!   The SDL 2.30.3 in Cemu 2.6 (its vcpkg.json, tag v2.6) switches one on
//!   Bluetooth once the PS4 or PS5 rumble hint is on (release-2.30.3,
//!   `SDL_hidapi_ps5.c` lines 400 to 409, `SDL_hidapi_ps4.c` lines 772 to
//!   778), and Cemu turns both on (`SDL_HINT_JOYSTICK_HIDAPI_PS4_RUMBLE` and
//!   `_PS5_RUMBLE`, src/input/api/SDL/SDLControllerProvider.cpp lines 24
//!   and 25, tag v2.6).
//! - RPCS3 asks a DualSense for feature report 0x09, which "will also enable
//!   enhanced feature reports for bluetooth", and a DualShock 4 on Bluetooth
//!   for feature report 0x02, which "tells controller to send 0x11 reports"
//!   (RPCS3 commit 222754bf, `rpcs3/Input/dualsense_pad_handler.cpp` lines
//!   128 to 133, `rpcs3/Input/ds4_pad_handler.cpp` lines 712 to 720).
//!
//! Over a cable there is no switch: a pad sends its full report (0x01, 64
//! bytes) from the moment it is plugged in, and gilrs reads it as before.
//! SDL takes a DualSense whose first report is 64 bytes long, and any
//! DualShock 4 on a cable, as sending full reports already, with nothing to
//! switch (release-3.4.0, `SDL_hidapi_ps5.c` lines 409 to 411,
//! `SDL_hidapi_ps4.c` lines 334 to 337). RPCS3 reads a DualSense's 0x01
//! report of any length but the Bluetooth one as its full report
//! (`dualsense_pad_handler.cpp` lines 302 to 315), and a DualShock 4 on a
//! cable by its 64-byte 0x01 report, asking for report 0x02 to switch it
//! only on Bluetooth (`ds4_pad_handler.cpp` lines 712 to 720 and 742).
//!
//! So Omoio reads each of these pads' reports itself as well, and pads.rs
//! takes what it holds from them while gilrs can't hear it (`pads.rs`,
//! `sony_held`). Like switch_pro.rs it only listens: it never sends the pad
//! anything, not even a request for a feature report, which on a DualSense
//! is itself enough to switch it. The layouts are SDL's (release-3.4.0,
//! zlib licensed): `PS5SimpleStatePacket_t` and `PS5StatePacketCommon_t`
//! (`SDL_hidapi_ps5.c` lines 78 to 108) read as
//! `HIDAPI_DriverPS5_HandleSimpleStatePacket` (1137) and
//! `HIDAPI_DriverPS5_HandleStatePacketCommon` (1229) read them, and
//! `PS4StatePacket_t` (`SDL_hidapi_ps4.c` lines 83 to 107) as
//! `HIDAPI_DriverPS4_HandleStatePacket` (1004) reads it.
//!
//! A full report over Bluetooth ends in a CRC-32 of the report after the
//! Bluetooth header byte 0xA1, and a report whose CRC is wrong is left out,
//! as SDL leaves it out (`VerifyCRC` and `HIDAPI_DriverPS5_IsPacketValid`,
//! `SDL_hidapi_ps5.c` lines 1448 to 1490) and RPCS3 does
//! (`dualsense_pad_handler.cpp` lines 323 to 333, `ds4_pad_handler.cpp`
//! lines 731 to 740). On a DualShock 4 SDL keeps a report with a wrong CRC
//! until it has had three right ones (`HIDAPI_DriverPS4_IsPacketValid`,
//! `SDL_hidapi_ps4.c` lines 1224 to 1236), and so does this.

use std::time::Instant;

pub const SONY: u16 = 0x054C;

/// How a pad lays out its reports, by its USB product id: the DualSense and
/// the DualSense Edge one way, both models of the DualShock 4 and Sony's
/// wireless adapter for it another. The ids are SDL's
/// (`USB_PRODUCT_SONY_DS5`, `_DS5_EDGE`, `_DS4`, `_DS4_SLIM`, `_DS4_DONGLE`
/// in `src/joystick/usb_ids.h`, release-3.4.0) and RPCS3's
/// (`SONY_DUALSENSE_ID_0` and `_1`, `dualsense_pad_handler.cpp` lines 28 and
/// 29; `SONY_DS4_ID_0` to `_2`, `ds4_pad_handler.cpp` lines 11 to 13).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Model {
    DualSense,
    DualShock4,
}

fn model(product: u16) -> Option<Model> {
    match product {
        0x0CE6 | 0x0DF2 => Some(Model::DualSense),
        0x05C4 | 0x09CC | 0x0BA0 => Some(Model::DualShock4),
        _ => None,
    }
}

/// The product id of a pad this reads, given the ids gilrs has for it.
pub fn reads(vendor: Option<u16>, product: Option<u16>) -> Option<u16> {
    product.filter(|product| vendor == Some(SONY) && model(*product).is_some())
}

/// What one report says is held, by place, and whether it was a full
/// report over Bluetooth, which gilrs can't read.
#[derive(Debug, Clone, PartialEq)]
struct Report {
    held: Vec<&'static str>,
    full: bool,
}

/// How far a stick has to go from the middle, and a trigger be pulled, to
/// count: about half way and a quarter, as for the other pads (pads.rs,
/// `decode` and `sdl_held`). Both are counted on 0 to 255.
const PUSHED: i32 = 64;
const PULLED: u8 = 64;

/// The inputs held, by place, in the order pads.rs's `sdl_held` gives them,
/// from a report's sticks (left x and y, right x and y), its three bytes of
/// buttons and its triggers (left, right). Byte 0 is the d-pad as a hat,
/// 0 for up going round clockwise to 7 and 8 for none, under Square, Cross,
/// Circle and Triangle; byte 1 is L1, R1, L2 and R2 pulled at all, Share,
/// Options, L3 and R3; bit 0x01 of byte 2 is the PS button
/// (`HIDAPI_DriverPS5_HandleStatePacketCommon`, `SDL_hidapi_ps5.c` lines
/// 1229 to 1300, and `HIDAPI_DriverPS4_HandleStatePacket`, `SDL_hidapi_ps4.c`
/// lines 1027 to 1095, release-3.4.0). A stick counts up as it falls, the
/// opposite of what "LS Y+" means. The touchpad's click, the DualSense's
/// microphone button and the Edge's back buttons have no place in a layout
/// (core/pad_layout.rs, `INPUTS`), as gilrs leaves them out too.
fn held_in(sticks: [u8; 4], buttons: [u8; 3], triggers: [u8; 2]) -> Vec<&'static str> {
    let face = buttons[0] >> 4;
    let hat = buttons[0] & 0x0F;
    // SDL counts a trigger that reports only that it is pulled as pulled all
    // the way (`HIDAPI_DriverPS5_HandleStatePacketCommon`, lines 1302 to
    // 1313).
    let pulled = |value: u8, bit: u8| value >= PULLED || (value == 0 && buttons[1] & bit != 0);
    let all = [
        (face & 0x02 != 0, "South"),
        (face & 0x04 != 0, "East"),
        (face & 0x01 != 0, "West"),
        (face & 0x08 != 0, "North"),
        (buttons[1] & 0x01 != 0, "LB"),
        (buttons[1] & 0x02 != 0, "RB"),
        (pulled(triggers[0], 0x04), "LT"),
        (pulled(triggers[1], 0x08), "RT"),
        (buttons[1] & 0x40 != 0, "LS"),
        (buttons[1] & 0x80 != 0, "RS"),
        (matches!(hat, 7 | 0 | 1), "Up"),
        (matches!(hat, 3..=5), "Down"),
        (matches!(hat, 5..=7), "Left"),
        (matches!(hat, 1..=3), "Right"),
        (buttons[1] & 0x10 != 0, "Back"),
        (buttons[1] & 0x20 != 0, "Start"),
        (buttons[2] & 0x01 != 0, "Guide"),
    ];
    let mut held: Vec<&'static str> = all.iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect();
    // From the middle, on 0 to 255, with up counted upwards.
    let off = |value: u8| i32::from(value) - 128;
    let [lx, ly, rx, ry] = sticks;
    for (value, plus, minus) in [
        (off(lx), "LS X+", "LS X-"),
        (-off(ly), "LS Y+", "LS Y-"),
        (off(rx), "RS X+", "RS X-"),
        (-off(ry), "RS Y+", "RS Y-"),
    ] {
        if value >= PUSHED {
            held.push(plus);
        } else if value <= -PUSHED {
            held.push(minus);
        }
    }
    held
}

/// The CRC-32 SDL and RPCS3 check a Bluetooth report with, the one zlib
/// computes, carried on from `crc` (`SDL_crc32`, `src/stdlib/SDL_crc32.c`,
/// release-3.4.0).
fn crc32(crc: u32, bytes: &[u8]) -> u32 {
    let mut crc = !crc;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Whether the last four of a Bluetooth report's `length` bytes are the
/// CRC-32 of the rest, the header byte 0xA1 first (`VerifyCRC`,
/// `SDL_hidapi_ps5.c` lines 1448 to 1461).
fn crc_ok(report: &[u8], length: usize) -> bool {
    let Some(body) = report.get(..length) else {
        return false;
    };
    let (data, sent) = body.split_at(length - 4);
    crc32(crc32(0, &[0xA1]), data) == u32::from_le_bytes([sent[0], sent[1], sent[2], sent[3]])
}

/// The length of a full report over Bluetooth, with its CRC at the end:
/// SDL checks a DualShock 4's first 78 bytes (`SDL_hidapi_ps4.c` line 1225)
/// and RPCS3 reads a DualSense's as 78 bytes (`dualsense_input_report_bt`,
/// `dualsense_pad_handler.h`), the length of report 0x31 Windows reads.
const BLUETOOTH_LENGTH: usize = 78;

/// Reads one pad's reports, keeping what a DualShock 4's CRCs have shown.
struct Decoder {
    model: Model,
    /// How many full reports in a row, give or take, came with a right CRC.
    good_crcs: u32,
}

impl Decoder {
    fn new(model: Model) -> Self {
        Self { model, good_crcs: 0 }
    }

    /// What a report says, or `None` for one that isn't the pad's state.
    /// `report` is as Windows reads it: the report id, then the report, then
    /// zeros up to the longest report the pad has.
    fn read(&mut self, report: &[u8]) -> Option<Report> {
        let at = |start: usize, simple: bool| -> Option<Report> {
            // Sticks, buttons and triggers; a full report has a counter too.
            let state = report.get(start..start + if simple { 9 } else { 10 })?;
            let sticks = [state[0], state[1], state[2], state[3]];
            // A simple report has its buttons before the triggers, a full one
            // after them and a counter (`PS5SimpleStatePacket_t` against
            // `PS5StatePacketCommon_t`, `SDL_hidapi_ps5.c` lines 78 to 108).
            let (buttons, triggers) = if simple {
                ([state[4], state[5], state[6]], [state[7], state[8]])
            } else {
                ([state[7], state[8], state[9]], [state[4], state[5]])
            };
            Some(Report {
                held: held_in(sticks, buttons, triggers),
                full: false,
            })
        };
        let full = |mut found: Report| {
            found.full = true;
            found
        };
        match (self.model, *report.first()?) {
            // A DualSense's 0x01 is the simple report when it comes with the
            // Bluetooth report's length, 78 bytes, or alone, 10; else it is
            // the full report sent on a cable (`HIDAPI_DriverPS5_UpdateDevice`,
            // `SDL_hidapi_ps5.c` line 1526; RPCS3's `get_data`,
            // `dualsense_pad_handler.cpp` line 304).
            (Model::DualSense, 0x01) => at(1, report.len() == 10 || report.len() == BLUETOOTH_LENGTH),
            // Report 0x31 has a byte of its own before the state
            // (`SDL_hidapi_ps5.c` line 1543).
            (Model::DualSense, 0x31) => crc_ok(report, BLUETOOTH_LENGTH).then(|| at(2, false).map(full))?,
            // A DualShock 4's simple report is the first bytes of its full
            // one (`PS4StatePacket_t`, `SDL_hidapi_ps4.c` lines 83 to 107,
            // both read by `HIDAPI_DriverPS4_HandleStatePacket`, line 1287).
            // Sony's wireless adapter marks a long report sent with no pad
            // connected to it with bit 0x04 of byte 31 (lines 1205 to 1212).
            (Model::DualShock4, 0x01) => {
                if report.len() >= 64 && report[31] & 0x04 != 0 {
                    return None;
                }
                // Laid out as a simple report, buttons before triggers.
                at(1, true)
            }
            // Reports 0x11 to 0x19 have two bytes of their own before the
            // state, the first saying with bit 0x80 that the state is there
            // (`HIDAPI_DriverPS4_IsPacketValid`, `SDL_hidapi_ps4.c` lines
            // 1214 to 1236; `HIDAPI_DriverPS4_UpdateDevice`, line 1290).
            (Model::DualShock4, 0x11..=0x19) => {
                if report.len() < BLUETOOTH_LENGTH || report[1] & 0x80 == 0 {
                    return None;
                }
                if crc_ok(report, BLUETOOTH_LENGTH) {
                    self.good_crcs += 1;
                } else {
                    self.good_crcs = self.good_crcs.saturating_sub(1);
                    if self.good_crcs >= 3 {
                        return None;
                    }
                }
                at(3, true).map(full)
            }
            _ => None,
        }
    }
}

/// The buttons among inputs held, sorted, leaving out the sticks and
/// triggers. gilrs and the reports agree on these exactly when gilrs hears
/// the pad, while a stick or trigger just at the line may count for one and
/// not the other.
pub fn buttons(held: &[&'static str]) -> Vec<&'static str> {
    let mut buttons: Vec<&'static str> =
        held.iter().copied().filter(|name| !name.contains(' ') && *name != "LT" && *name != "RT").collect();
    buttons.sort_unstable();
    buttons
}

/// What a pad's reports say now.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    /// What is held, by place.
    pub held: Vec<&'static str>,
    /// Whether it came in a full report over Bluetooth, which gilrs can't
    /// read.
    pub full: bool,
    /// When the last report came.
    pub at: Instant,
    /// How many times the buttons held (`buttons`) have changed since the
    /// pad was found, and when they last did.
    pub changes: u64,
    pub changed: Instant,
}

impl Heard {
    /// `last` heard, with the report `report` come in at `now`.
    fn after(last: Option<&Heard>, report: Report, now: Instant) -> Heard {
        let (changes, changed) = match last {
            Some(last) if buttons(&last.held) != buttons(&report.held) => (last.changes + 1, now),
            Some(last) => (last.changes, last.changed),
            None => (0, now),
        };
        Heard {
            held: report.held,
            full: report.full,
            at: now,
            changes,
            changed,
        }
    }
}

/// What the reports of the `nth` pad found of USB product `product`,
/// counted from zero, last said. `None` before one has come, or for a pad
/// that isn't there. pads.rs decides whether it is recent enough to count.
pub fn heard(product: u16, nth: usize) -> Option<Heard> {
    imp::heard(product, nth)
}

/// Starts looking for Sony's pads to read, once.
pub fn watch() {
    imp::watch();
}

#[cfg(windows)]
mod imp {
    use super::{Decoder, Heard, SONY};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    /// How often new pads are looked for.
    const LOOK_EVERY: Duration = Duration::from_secs(2);

    /// One pad being read, by its device path, with what it last said.
    struct Reading {
        path: String,
        product: u16,
        last: Option<Heard>,
    }

    /// In the order found.
    fn readings() -> &'static Mutex<Vec<Reading>> {
        static READINGS: OnceLock<Mutex<Vec<Reading>>> = OnceLock::new();
        READINGS.get_or_init(|| Mutex::new(Vec::new()))
    }

    pub fn heard(product: u16, nth: usize) -> Option<Heard> {
        watch();
        let readings = readings().lock().unwrap();
        readings.iter().filter(|r| r.product == product).nth(nth)?.last.clone()
    }

    pub fn watch() {
        static STARTED: OnceLock<()> = OnceLock::new();
        STARTED.get_or_init(|| {
            std::thread::spawn(|| loop {
                for path in crate::switch_pro::hid_paths() {
                    // A device's path names its ids, in a form that differs
                    // between USB and Bluetooth, so this only spares opening
                    // every other device.
                    let lower = path.to_lowercase();
                    let named = |id: &str| lower.contains(id);
                    if !named("054c") || !["0ce6", "0df2", "05c4", "09cc", "0ba0"].into_iter().any(named) {
                        continue;
                    }
                    if readings().lock().unwrap().iter().any(|r| r.path == path) {
                        continue;
                    }
                    let wanted = |vendor: u16, product: u16| vendor == SONY && super::model(product).is_some();
                    let Some((file, length, product)) = crate::switch_pro::open_shared(&path, wanted) else {
                        continue;
                    };
                    let Some(model) = super::model(product).filter(|_| length >= 10) else {
                        continue;
                    };
                    readings().lock().unwrap().push(Reading {
                        path: path.clone(),
                        product,
                        last: None,
                    });
                    std::thread::spawn(move || read(path, file, length, Decoder::new(model)));
                }
                std::thread::sleep(LOOK_EVERY);
            });
        });
    }

    /// Reads one pad's reports until it goes, then forgets it.
    fn read(path: String, mut file: std::fs::File, length: usize, mut decoder: Decoder) {
        use std::io::Read;
        let mut report = vec![0u8; length];
        while let Ok(read) = file.read(&mut report) {
            if read == 0 {
                break;
            }
            if let Some(found) = decoder.read(&report[..read]) {
                if let Some(reading) = readings().lock().unwrap().iter_mut().find(|r| r.path == path) {
                    reading.last = Some(Heard::after(reading.last.as_ref(), found, Instant::now()));
                }
            }
        }
        readings().lock().unwrap().retain(|r| r.path != path);
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Heard;

    pub fn heard(_product: u16, _nth: usize) -> Option<Heard> {
        None
    }

    pub fn watch() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sticks at rest, nothing pressed: the d-pad's hat says 8, none.
    const REST_STICKS: [u8; 4] = [0x80, 0x80, 0x80, 0x80];

    /// A DualSense's full report over a cable, 64 bytes.
    fn dualsense_usb(sticks: [u8; 4], buttons: [u8; 3], triggers: [u8; 2]) -> Vec<u8> {
        let mut report = vec![0x01];
        report.extend(sticks);
        report.extend(triggers);
        report.push(0x2A); // counter
        report.extend(buttons);
        report.push(0x00); // the fourth byte of buttons
        report.extend([0x5Au8; 52]); // sequence, motion, touchpad and the rest
        report.truncate(64);
        report
    }

    /// The simple report a DualSense sends over Bluetooth until it is
    /// switched, as Windows reads it: padded to 78 bytes.
    fn dualsense_simple(sticks: [u8; 4], buttons: [u8; 3], triggers: [u8; 2]) -> Vec<u8> {
        let mut report = vec![0x01];
        report.extend(sticks);
        report.extend(buttons);
        report.extend(triggers);
        report.resize(BLUETOOTH_LENGTH, 0);
        report
    }

    /// Puts the right CRC at the end of a 78-byte Bluetooth report.
    fn sealed(mut report: Vec<u8>) -> Vec<u8> {
        report.resize(BLUETOOTH_LENGTH, 0);
        let crc = crc32(crc32(0, &[0xA1]), &report[..BLUETOOTH_LENGTH - 4]);
        report[BLUETOOTH_LENGTH - 4..].copy_from_slice(&crc.to_le_bytes());
        report
    }

    /// A DualSense's full report over Bluetooth, 0x31.
    fn dualsense_full(sticks: [u8; 4], buttons: [u8; 3], triggers: [u8; 2]) -> Vec<u8> {
        let mut report = vec![0x31, 0x10];
        report.extend(sticks);
        report.extend(triggers);
        report.push(0x2A);
        report.extend(buttons);
        report.push(0x00);
        report.extend([0x5Au8; 40]);
        sealed(report)
    }

    /// A DualShock 4's report 0x01, simple over Bluetooth or full on a cable,
    /// with `rest` bytes of motion and the rest after it.
    fn dualshock_01(sticks: [u8; 4], buttons: [u8; 3], triggers: [u8; 2], rest: usize) -> Vec<u8> {
        let mut report = vec![0x01];
        report.extend(sticks);
        report.extend(buttons);
        report.extend(triggers);
        report.extend(vec![0u8; rest]);
        report
    }

    /// A DualShock 4's full report over Bluetooth, 0x11.
    fn dualshock_full(sticks: [u8; 4], buttons: [u8; 3], triggers: [u8; 2]) -> Vec<u8> {
        let mut report = vec![0x11, 0xC0, 0x00];
        report.extend(sticks);
        report.extend(buttons);
        report.extend(triggers);
        report.extend([0x33u8; 30]);
        sealed(report)
    }

    fn read(model: Model, report: &[u8]) -> Option<Report> {
        Decoder::new(model).read(report)
    }

    #[test]
    fn the_crc_is_zlibs() {
        // The check value every CRC-32 of this kind gives "123456789".
        assert_eq!(crc32(0, b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(crc32(0, b"1234"), b"56789"), 0xCBF4_3926, "carried on");
    }

    #[test]
    fn a_pad_at_rest_holds_nothing() {
        let rest = [0x08, 0x00, 0x00];
        for report in [
            dualsense_usb(REST_STICKS, rest, [0, 0]),
            dualsense_simple(REST_STICKS, rest, [0, 0]),
            dualsense_full(REST_STICKS, rest, [0, 0]),
        ] {
            assert_eq!(read(Model::DualSense, &report).unwrap().held, Vec::<&str>::new());
        }
        for report in [
            dualshock_01(REST_STICKS, rest, [0, 0], 0),
            dualshock_01(REST_STICKS, rest, [0, 0], 54),
            dualshock_full(REST_STICKS, rest, [0, 0]),
        ] {
            assert_eq!(read(Model::DualShock4, &report).unwrap().held, Vec::<&str>::new());
        }
    }

    #[test]
    fn only_bluetooth_full_reports_are_ones_gilrs_cant_read() {
        let rest = [0x08, 0x00, 0x00];
        assert!(!read(Model::DualSense, &dualsense_usb(REST_STICKS, rest, [0, 0])).unwrap().full);
        assert!(!read(Model::DualSense, &dualsense_simple(REST_STICKS, rest, [0, 0])).unwrap().full);
        assert!(read(Model::DualSense, &dualsense_full(REST_STICKS, rest, [0, 0])).unwrap().full);
        assert!(!read(Model::DualShock4, &dualshock_01(REST_STICKS, rest, [0, 0], 54)).unwrap().full);
        assert!(read(Model::DualShock4, &dualshock_full(REST_STICKS, rest, [0, 0])).unwrap().full);
    }

    #[test]
    fn face_buttons_go_by_where_they_sit() {
        // Square 0x10, Cross 0x20, Circle 0x40, Triangle 0x80, over the hat.
        let held = |face: u8| {
            let report = dualsense_full(REST_STICKS, [face | 0x08, 0, 0], [0, 0]);
            read(Model::DualSense, &report).unwrap().held
        };
        assert_eq!(held(0x20), ["South"]);
        assert_eq!(held(0x40), ["East"]);
        assert_eq!(held(0x10), ["West"]);
        assert_eq!(held(0x80), ["North"]);
    }

    #[test]
    fn the_ps_button_is_guide_in_every_layout() {
        let ps = [0x08, 0x00, 0x01];
        assert_eq!(read(Model::DualSense, &dualsense_usb(REST_STICKS, ps, [0, 0])).unwrap().held, ["Guide"]);
        assert_eq!(read(Model::DualSense, &dualsense_simple(REST_STICKS, ps, [0, 0])).unwrap().held, ["Guide"]);
        assert_eq!(read(Model::DualSense, &dualsense_full(REST_STICKS, ps, [0, 0])).unwrap().held, ["Guide"]);
        assert_eq!(read(Model::DualShock4, &dualshock_01(REST_STICKS, ps, [0, 0], 54)).unwrap().held, ["Guide"]);
        assert_eq!(read(Model::DualShock4, &dualshock_full(REST_STICKS, ps, [0, 0])).unwrap().held, ["Guide"]);
    }

    #[test]
    fn the_middle_and_shoulder_buttons_are_read() {
        let held = read(Model::DualShock4, &dualshock_full(REST_STICKS, [0x08, 0x01 | 0x02 | 0x10 | 0x20, 0], [0, 0]))
            .unwrap()
            .held;
        assert_eq!(held, ["LB", "RB", "Back", "Start"]);
        let held = read(Model::DualSense, &dualsense_usb(REST_STICKS, [0x08, 0x40 | 0x80, 0], [0, 0])).unwrap().held;
        assert_eq!(held, ["LS", "RS"]);
    }

    #[test]
    fn the_touchpad_microphone_and_edge_buttons_are_left_out() {
        let report = dualsense_full(REST_STICKS, [0x08, 0, 0x02 | 0x04 | 0xF0], [0, 0]);
        assert!(read(Model::DualSense, &report).unwrap().held.is_empty());
    }

    #[test]
    fn the_d_pad_is_a_hat() {
        let held = |hat: u8| read(Model::DualSense, &dualsense_full(REST_STICKS, [hat, 0, 0], [0, 0])).unwrap().held;
        assert_eq!(held(0), ["Up"]);
        assert_eq!(held(1), ["Up", "Right"]);
        assert_eq!(held(2), ["Right"]);
        assert_eq!(held(3), ["Down", "Right"]);
        assert_eq!(held(4), ["Down"]);
        assert_eq!(held(5), ["Down", "Left"]);
        assert_eq!(held(6), ["Left"]);
        assert_eq!(held(7), ["Up", "Left"]);
        assert!(held(8).is_empty());
    }

    #[test]
    fn triggers_count_once_pulled_a_quarter() {
        let held = |triggers, bits| {
            read(Model::DualSense, &dualsense_full(REST_STICKS, [0x08, bits, 0], triggers)).unwrap().held
        };
        assert_eq!(held([200, 0], 0x04), ["LT"]);
        assert_eq!(held([0, 64], 0x08), ["RT"]);
        assert!(held([30, 40], 0x0C).is_empty(), "pulled a little");
        assert_eq!(held([0, 0], 0x0C), ["LT", "RT"], "a trigger that only says it is pulled");
    }

    #[test]
    fn sticks_count_once_pushed_half_way_with_up_upwards() {
        let held = |sticks| read(Model::DualShock4, &dualshock_full(sticks, [0x08, 0, 0], [0, 0])).unwrap().held;
        assert_eq!(held([0xFF, 0x00, 0x00, 0xFF]), ["LS X+", "LS Y+", "RS X-", "RS Y-"]);
        assert!(held([0xA0, 0x60, 0x90, 0x70]).is_empty(), "resting slightly off centre");
        // The simple layout reads the sticks from the same bytes.
        let report = dualsense_simple([0x80, 0x00, 0x80, 0x80], [0x08, 0, 0], [0, 0]);
        assert_eq!(read(Model::DualSense, &report).unwrap().held, ["LS Y+"]);
    }

    #[test]
    fn the_simple_and_full_dualsense_layouts_are_told_apart() {
        // The same bytes after the id: a simple report reads byte 5 as
        // buttons, a full one as the left trigger.
        let bytes = [0x80, 0x80, 0x80, 0x80, 0x28, 0x00, 0x00, 0x00, 0x00];
        let mut simple = vec![0x01];
        simple.extend(bytes);
        // Read as a simple report: Cross over a hat of 8, none.
        assert_eq!(read(Model::DualSense, &simple).unwrap().held, ["South"], "10 bytes");
        let mut usb = simple.clone();
        usb.resize(64, 0);
        // Read as a full report: left trigger 0x28, the hat byte 0, up.
        assert_eq!(read(Model::DualSense, &usb).unwrap().held, ["Up"]);
        let mut padded = simple.clone();
        padded.resize(BLUETOOTH_LENGTH, 0);
        assert_eq!(read(Model::DualSense, &padded).unwrap().held, ["South"], "the simple report, padded");
    }

    #[test]
    fn a_full_report_with_a_wrong_crc_is_left_out() {
        let mut report = dualsense_full(REST_STICKS, [0x08, 0, 0x01], [0, 0]);
        report[10] ^= 0x01;
        assert_eq!(read(Model::DualSense, &report), None);
    }

    #[test]
    fn a_dualshock_with_wrong_crcs_is_read_until_it_has_sent_right_ones() {
        let good = dualshock_full(REST_STICKS, [0x08, 0, 0x01], [0, 0]);
        let mut bad = good.clone();
        bad[BLUETOOTH_LENGTH - 1] ^= 0xFF;
        let mut decoder = Decoder::new(Model::DualShock4);
        assert!(decoder.read(&bad).is_some(), "no right CRC seen yet");
        for _ in 0..4 {
            assert!(decoder.read(&good).is_some());
        }
        assert_eq!(decoder.read(&bad), None, "after right ones, a wrong one is left out");
    }

    #[test]
    fn a_dualshock_report_without_its_state_is_left_out() {
        let mut report = dualshock_full(REST_STICKS, [0x08, 0, 0x01], [0, 0]);
        report[1] = 0x00;
        assert_eq!(read(Model::DualShock4, &sealed(report)), None);
    }

    #[test]
    fn the_wireless_adapter_with_no_pad_is_left_out() {
        let mut report = dualshock_01(REST_STICKS, [0x08, 0, 0x01], [0, 0], 54);
        report[31] = 0x04;
        assert_eq!(read(Model::DualShock4, &report), None);
    }

    #[test]
    fn other_reports_and_short_ones_are_left_out() {
        assert_eq!(read(Model::DualSense, &[0x05, 1, 2, 3]), None);
        assert_eq!(read(Model::DualSense, &[0x01, 0x80, 0x80]), None);
        assert_eq!(read(Model::DualShock4, &[0x11, 0xC0, 0x00, 0x80]), None);
        assert_eq!(read(Model::DualShock4, &[]), None);
    }

    #[test]
    fn only_sonys_pads_are_read() {
        assert_eq!(reads(Some(SONY), Some(0x0CE6)), Some(0x0CE6));
        assert_eq!(reads(Some(SONY), Some(0x0DF2)), Some(0x0DF2));
        assert_eq!(reads(Some(SONY), Some(0x05C4)), Some(0x05C4));
        assert_eq!(reads(Some(SONY), Some(0x09CC)), Some(0x09CC));
        assert_eq!(reads(Some(SONY), Some(0x0268)), None, "a DualShock 3");
        assert_eq!(reads(Some(0x057E), Some(0x2009)), None);
        assert_eq!(reads(None, Some(0x0CE6)), None);
    }

    #[test]
    fn buttons_leave_out_sticks_and_triggers_and_are_sorted() {
        assert_eq!(buttons(&["South", "LT", "LS X+", "Guide", "RT", "RS Y-"]), ["Guide", "South"]);
    }

    #[test]
    fn changes_are_counted_by_the_buttons_alone() {
        let start = Instant::now();
        let later = start + std::time::Duration::from_millis(30);
        let report = |held: Vec<&'static str>| Report { held, full: true };
        let first = Heard::after(None, report(vec![]), start);
        assert_eq!((first.changes, first.changed), (0, start));
        let stick = Heard::after(Some(&first), report(vec!["LS X+"]), later);
        assert_eq!((stick.changes, stick.changed), (0, start), "a stick is no button");
        let pressed = Heard::after(Some(&stick), report(vec!["Guide"]), later);
        assert_eq!((pressed.changes, pressed.changed, pressed.at), (1, later, later));
    }
}
