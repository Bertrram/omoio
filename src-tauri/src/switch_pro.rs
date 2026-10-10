//! Nintendo's Pro Controller, read from its full reports.
//!
//! A Pro Controller sends one of two kinds of report. It starts with a
//! simple one (0x3F), laid out as its HID description says, which is what
//! gilrs reads through Windows.Gaming.Input. Steam, and the SDL in each
//! emulator, switch it to its full report (0x30) instead, and it stays so
//! until it is unplugged or turned off. Its HID description gives 0x30 the
//! simple report's layout, which the full report doesn't have, so read that
//! way its timer and battery come out as buttons that flicker and its motion
//! sensors as sticks. On Omoio's Controller screen nearly every button showed
//! as held, on and off, with nothing pressed (issue #13).
//!
//! So Omoio reads each Pro Controller's reports itself as well, and while one
//! sends full reports, what it holds is taken from them. It only listens: it
//! never switches a pad from one report to the other, which is left to Steam
//! and the emulators. The layout is that of Nintendo's own report
//! (dekuNukem's Nintendo_Switch_Reverse_Engineering, "bluetooth_hid_notes"
//! and "USB-HID-Notes", read 10 October 2026), which SDL reads too
//! (`HIDAPI_DriverSwitch_HandleFullControllerState`).

/// What is held on the `nth` Pro Controller found, counted from zero, while
/// it sends full reports. `None` while it sends only simple ones, which
/// gilrs reads right, or isn't there.
pub fn held(nth: usize) -> Option<Vec<&'static str>> {
    imp::held(nth)
}

/// Starts looking for Pro Controllers to read, once.
pub fn watch() {
    imp::watch();
}

/// Report ids whose first bytes are the pad's state as the full report has
/// it: the full report, and a reply to a command, which Steam sends now and
/// then.
const FULL: [u8; 2] = [0x30, 0x21];

/// The middle of a stick's 12-bit range, and how far from it a stick counts
/// as pushed: about half its travel, as for the other pads.
const CENTRE: i32 = 2048;
const PUSHED: i32 = 700;

/// What a full report says is held, by place. `None` for any other report.
fn decode(report: &[u8]) -> Option<Vec<&'static str>> {
    if report.len() < 12 || !FULL.contains(&report[0]) {
        return None;
    }
    // Byte 3 is the right side's buttons, 4 those in the middle and 5 the
    // left side's. B is the bottom face button, A the right one.
    const RIGHT: [(u8, &str); 6] = [
        (0x01, "West"),
        (0x02, "North"),
        (0x04, "South"),
        (0x08, "East"),
        (0x40, "RB"),
        (0x80, "RT"),
    ];
    const MIDDLE: [(u8, &str); 5] = [(0x01, "Back"), (0x02, "Start"), (0x04, "RS"), (0x08, "LS"), (0x10, "Guide")];
    const LEFT: [(u8, &str); 6] = [
        (0x01, "Down"),
        (0x02, "Up"),
        (0x04, "Right"),
        (0x08, "Left"),
        (0x40, "LB"),
        (0x80, "LT"),
    ];
    let mut held: Vec<&'static str> = Vec::new();
    for (byte, buttons) in [(report[3], &RIGHT[..]), (report[4], &MIDDLE[..]), (report[5], &LEFT[..])] {
        held.extend(buttons.iter().filter(|(bit, _)| byte & bit != 0).map(|(_, name)| *name));
    }
    // Each stick is two 12-bit numbers in three bytes, up counting upwards.
    let stick = |b: &[u8]| {
        let x = i32::from(b[0]) | (i32::from(b[1] & 0x0F) << 8);
        let y = i32::from(b[1] >> 4) | (i32::from(b[2]) << 4);
        (x - CENTRE, y - CENTRE)
    };
    let (lx, ly) = stick(&report[6..9]);
    let (rx, ry) = stick(&report[9..12]);
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
    Some(held)
}

#[cfg(windows)]
mod imp {
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    const NINTENDO: u16 = 0x057E;
    const PRO_CONTROLLER: u16 = 0x2009;
    /// A pad sending full reports sends one every few milliseconds, so one
    /// that has sent none for this long sends simple ones again, or is gone.
    const FRESH: Duration = Duration::from_millis(500);
    /// How often new pads are looked for.
    const LOOK_EVERY: Duration = Duration::from_secs(2);

    /// One Pro Controller being read, by its device path, with what its last
    /// full report said and when.
    struct Reading {
        path: String,
        last: Option<(Instant, Vec<&'static str>)>,
    }

    /// In the order found.
    fn readings() -> &'static Mutex<Vec<Reading>> {
        static READINGS: OnceLock<Mutex<Vec<Reading>>> = OnceLock::new();
        READINGS.get_or_init(|| Mutex::new(Vec::new()))
    }

    pub fn held(nth: usize) -> Option<Vec<&'static str>> {
        watch();
        let readings = readings().lock().unwrap();
        let (at, held) = readings.get(nth)?.last.as_ref()?;
        (at.elapsed() < FRESH).then(|| held.clone())
    }

    pub fn watch() {
        static STARTED: OnceLock<()> = OnceLock::new();
        STARTED.get_or_init(|| {
            std::thread::spawn(|| {
                loop {
                    for path in hid_paths() {
                        // A device's path names its ids, in a form that
                        // differs between USB and Bluetooth, so this only
                        // spares opening every other device.
                        let lower = path.to_lowercase();
                        if !lower.contains("057e") || !lower.contains("2009") {
                            continue;
                        }
                        if readings().lock().unwrap().iter().any(|r| r.path == path) {
                            continue;
                        }
                        if let Some((file, length)) = open(&path) {
                            readings().lock().unwrap().push(Reading { path: path.clone(), last: None });
                            std::thread::spawn(move || read(path, file, length));
                        }
                    }
                    std::thread::sleep(LOOK_EVERY);
                }
            });
        });
    }

    /// Reads one pad's reports until it goes, then forgets it.
    fn read(path: String, mut file: std::fs::File, length: usize) {
        use std::io::Read;
        let mut report = vec![0u8; length];
        while let Ok(read) = file.read(&mut report) {
            if read == 0 {
                break;
            }
            if let Some(held) = super::decode(&report[..read]) {
                if let Some(reading) = readings().lock().unwrap().iter_mut().find(|r| r.path == path) {
                    reading.last = Some((Instant::now(), held));
                }
            }
        }
        readings().lock().unwrap().retain(|r| r.path != path);
    }

    /// Opens a HID device for reading, shared, when it is a Pro Controller,
    /// with the length of its input reports, which each read has to be.
    fn open(path: &str) -> Option<(std::fs::File, usize)> {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Devices::HumanInterfaceDevice::{
            HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetPreparsedData, HidP_GetCaps, HIDD_ATTRIBUTES,
            HIDP_CAPS, HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
        };
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
            .open(path)
            .ok()?;
        let handle = HANDLE(file.as_raw_handle());
        let mut attributes = HIDD_ATTRIBUTES {
            Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32,
            ..Default::default()
        };
        // Each call fills in what it is handed, and the preparsed data is
        // freed once read.
        let length = unsafe {
            if !HidD_GetAttributes(handle, &mut attributes)
                || attributes.VendorID != NINTENDO
                || attributes.ProductID != PRO_CONTROLLER
            {
                return None;
            }
            let mut preparsed = PHIDP_PREPARSED_DATA::default();
            if !HidD_GetPreparsedData(handle, &mut preparsed) {
                return None;
            }
            let mut caps: HIDP_CAPS = std::mem::zeroed();
            let status = HidP_GetCaps(preparsed, &mut caps);
            let _ = HidD_FreePreparsedData(preparsed);
            if status != HIDP_STATUS_SUCCESS {
                return None;
            }
            usize::from(caps.InputReportByteLength)
        };
        (length >= 12).then_some((file, length))
    }

    /// The paths of the HID devices plugged in.
    fn hid_paths() -> Vec<String> {
        use windows::core::PCWSTR;
        use windows::Win32::Devices::DeviceAndDriverInstallation::{
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
            SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA,
            SP_DEVICE_INTERFACE_DETAIL_DATA_W,
        };
        use windows::Win32::Devices::HumanInterfaceDevice::HidD_GetHidGuid;

        let mut paths = Vec::new();
        // Calls into Windows with every buffer sized as the call asks, and the
        // list freed at the end.
        unsafe {
            let guid = HidD_GetHidGuid();
            let Ok(set) =
                SetupDiGetClassDevsW(Some(&guid as *const _), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
            else {
                return paths;
            };
            let mut index = 0u32;
            loop {
                let mut interface = SP_DEVICE_INTERFACE_DATA {
                    cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                    ..Default::default()
                };
                if SetupDiEnumDeviceInterfaces(set, None, &guid, index, &mut interface).is_err() {
                    break;
                }
                index += 1;
                let mut size = 0u32;
                let _ = SetupDiGetDeviceInterfaceDetailW(set, &interface, None, 0, Some(&mut size as *mut u32), None);
                if (size as usize) < std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() {
                    continue;
                }
                // u32s keep the buffer aligned for the struct laid over it.
                let mut buffer = vec![0u32; (size as usize).div_ceil(4)];
                let detail = buffer.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
                (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(set, &interface, Some(detail), size, None, None).is_err() {
                    continue;
                }
                let path = std::ptr::addr_of!((*detail).DevicePath).cast::<u16>();
                let length = (0..).take_while(|&i| *path.add(i) != 0).count();
                paths.push(String::from_utf16_lossy(std::slice::from_raw_parts(path, length)));
            }
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
        paths
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn held(_nth: usize) -> Option<Vec<&'static str>> {
        None
    }

    pub fn watch() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full report with these buttons and sticks, the rest as a pad at rest
    /// sends it: a timer, the battery, and motion data after the sticks.
    fn report(id: u8, right: u8, middle: u8, left: u8, sticks: [(i32, i32); 2]) -> Vec<u8> {
        let mut report = vec![id, 0x5A, 0x8E, right, middle, left];
        for (x, y) in sticks {
            let (x, y) = ((x + CENTRE) as u16, (y + CENTRE) as u16);
            report.extend([(x & 0xFF) as u8, ((x >> 8) as u8 & 0x0F) | ((y & 0x0F) << 4) as u8, (y >> 4) as u8]);
        }
        report.extend([0x0B; 52]);
        report
    }

    #[test]
    fn a_pad_at_rest_holds_nothing_whatever_its_timer_and_motion_say() {
        assert_eq!(decode(&report(0x30, 0, 0, 0, [(0, 0), (0, 0)])), Some(vec![]));
        assert_eq!(decode(&report(0x30, 0, 0, 0, [(150, -200), (-90, 60)])), Some(vec![]));
    }

    #[test]
    fn face_buttons_go_by_where_they_sit() {
        let held = |right| decode(&report(0x30, right, 0, 0, [(0, 0), (0, 0)])).unwrap();
        assert_eq!(held(0x04), ["South"]);
        assert_eq!(held(0x08), ["East"]);
        assert_eq!(held(0x01), ["West"]);
        assert_eq!(held(0x02), ["North"]);
        assert_eq!(held(0x40 | 0x80), ["RB", "RT"]);
    }

    #[test]
    fn the_middle_and_left_buttons_are_read() {
        let held = decode(&report(0x30, 0, 0x01 | 0x02 | 0x10, 0x02 | 0x40 | 0x80, [(0, 0), (0, 0)])).unwrap();
        assert_eq!(held, ["Back", "Start", "Guide", "Up", "LB", "LT"]);
        let held = decode(&report(0x30, 0, 0x04 | 0x08, 0x01 | 0x04 | 0x08, [(0, 0), (0, 0)])).unwrap();
        assert_eq!(held, ["RS", "LS", "Down", "Right", "Left"]);
    }

    #[test]
    fn the_capture_button_and_the_rail_buttons_are_left_out() {
        assert_eq!(decode(&report(0x30, 0x10 | 0x20, 0x20, 0x10 | 0x20, [(0, 0), (0, 0)])), Some(vec![]));
    }

    #[test]
    fn a_stick_counts_once_pushed_about_half_way() {
        let held = decode(&report(0x30, 0, 0, 0, [(1200, 900), (-1100, -1300)])).unwrap();
        assert_eq!(held, ["LS X+", "LS Y+", "RS X-", "RS Y-"]);
        let held = decode(&report(0x30, 0, 0, 0, [(-800, -50), (20, 750)])).unwrap();
        assert_eq!(held, ["LS X-", "RS Y+"]);
    }

    #[test]
    fn a_reply_to_a_command_is_read_like_a_full_report() {
        assert_eq!(decode(&report(0x21, 0x04, 0, 0, [(0, 0), (0, 0)])), Some(vec!["South"]));
    }

    #[test]
    fn a_simple_report_is_left_to_gilrs() {
        assert_eq!(decode(&[0x3F, 0x01, 0x00, 0x08, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80]), None);
        assert_eq!(decode(&[0x30, 0x00, 0x8E]), None);
    }
}
