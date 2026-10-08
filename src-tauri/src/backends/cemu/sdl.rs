//! How Cemu's SDL input knows a pad that is not an XInput one.
//!
//! Cemu v2.6 names an SDL pad `<n>_<guid>` (`SDLController.cpp`): the GUID
//! SDL gives the joystick, as 32 hex digits, after how many joysticks with
//! that same GUID come before it. A profile finds its pad by that GUID byte
//! for byte (`SDLControllerProvider::get_index`), so it has to be the one SDL
//! makes, which is not something gilrs reports.
//!
//! Cemu v2.6 is built with SDL 2.30.3 (its `vcpkg.json`) and turns on SDL's
//! own HID drivers for PlayStation and Switch pads. Such a driver makes the
//! GUID from what the pad reports over HID (`SDL_CreateJoystickGUID`,
//! `HIDAPI_AddDevice`), in 16 bytes, numbers little-endian:
//!
//! - the bus, always USB (0x0003), Bluetooth too, which SDL 2.30 cannot tell
//! - a CRC-16 of a name, see below
//! - the vendor id, two zero bytes, the product id, two zero bytes
//! - the version, the HID release number
//! - `h`, which marks SDL's HID drivers, and a last byte that is 0 but for
//!   Switch pads
//!
//! A pad over Bluetooth gets its GUID the same way, from SDL 2.30.3's source
//! as read on 8 October 2026. On Windows SDL does not know a HID device's
//! bus: its `SDL_hid_device_info` has no field for one (`SDL_hidapi.h`), and
//! `hid_enumerate` (`hidapi/windows/hid.c`) reads the ids, the version and
//! the strings with `HidD_GetAttributes` and `HidD_Get*String` whatever the
//! connection. `HIDAPI_AddDevice` then writes USB into every GUID, with a note
//! that it has no way to tell Bluetooth. The PS4 and PS5 drivers learn they
//! are on Bluetooth only afterwards, from the pad's reports, which changes how
//! they read the pad and not its name (`HIDAPI_DriverPS4_InitDevice`,
//! `HIDAPI_DriverPS5_InitDevice`). So over Bluetooth a pad's GUID can differ
//! from its USB one only in the version, where Windows reports another for
//! that connection, and Omoio reads the version the way SDL does.
//!
//! Which name the CRC is of is up to each driver. The PS5 one renames a Sony
//! pad "DualSense Wireless Controller", or "DualSense Edge Wireless
//! Controller", and the PS4 one a Sony pad "PS4 Controller", each taking the
//! CRC of the new name (`HIDAPI_SetDeviceName`, which leaves the CRC alone
//! when the name was that already). The Switch one makes the whole GUID again
//! from the pad's own manufacturer and product strings, and puts the kind of
//! pad it reports, 3 for a Pro Controller, in the last byte
//! (`UpdateDeviceIdentity`, `HIDAPI_SetDeviceProduct`).
//!
//! So Omoio reads the same HID devices the way SDL does on Windows
//! (`hid_enumerate` in SDL's `hidapi/windows/hid.c`) and works the GUID out
//! here. A Cemu built with SDL 3 makes its GUIDs differently, and this would
//! need reading again then.

/// The pads SDL's HID drivers take in Cemu, by vendor and product id, with
/// which driver it is: Sony's PS5 and PS4 pads and Nintendo's Pro Controller,
/// which is also what an 8BitDo pad in its Switch mode reports.
const KNOWN: [(u16, u16, Kind); 6] = [
    (SONY, 0x0CE6, Kind::Ps5("DualSense Wireless Controller")),
    (SONY, 0x0DF2, Kind::Ps5("DualSense Edge Wireless Controller")),
    (SONY, 0x05C4, Kind::Ps4),
    (SONY, 0x09CC, Kind::Ps4),
    (SONY, 0x0BA0, Kind::Ps4),
    (NINTENDO, 0x2009, Kind::SwitchPro),
];

const SONY: u16 = 0x054C;
const NINTENDO: u16 = 0x057E;

/// What pads.rs calls each of those pads, before its number: the name gilrs
/// 0.11.2 gives it on Windows. Its Windows.Gaming.Input backend makes a pad's
/// GUID from the vendor and product ids alone (`Gamepad::new` in gilrs-core
/// 0.6.8's `windows_wgi/gamepad.rs`), and the SDL controller list gilrs ships
/// names those GUIDs (`SDL_GameControllerDB/gamecontrollerdb.txt`, read 8
/// October 2026). The DualSense Edge shares the DualSense's name there.
const GILRS_NAMES: [(u16, u16, &str); 6] = [
    (SONY, 0x0CE6, "PS5 Controller"),
    (SONY, 0x0DF2, "PS5 Controller"),
    (SONY, 0x05C4, "PS4 Controller"),
    (SONY, 0x09CC, "PS4 Controller"),
    (SONY, 0x0BA0, "PS4 Controller"),
    (NINTENDO, 0x2009, "Nintendo Switch Pro Controller"),
];

/// What SDL marks its HID drivers' GUIDs with.
const HIDAPI: u8 = b'h';
/// What a Pro Controller reports itself as (`k_eSwitchDeviceInfoControllerType_ProController`).
const SWITCH_PRO: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Ps5(&'static str),
    Ps4,
    SwitchPro,
}

/// A HID device as SDL's enumeration on Windows sees it: its attributes and
/// strings, each string `None` where Windows had none to give. With it,
/// whether Windows has it over Bluetooth, which SDL 2.30 never asks (`pick`).
#[derive(Debug, Clone, PartialEq)]
pub struct HidPad {
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub manufacturer: Option<String>,
    pub product_name: Option<String>,
    pub bluetooth: bool,
}

/// Cemu's name for a pad, and whether its face buttons go by the letters on
/// them rather than where they sit.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub uuid: String,
    pub by_label: bool,
}

/// The `ordinal`th pad, counted from 0, of this vendor and product among the
/// HID devices plugged in, as Cemu names it. `None` for a pad SDL's HID
/// drivers do not take in Cemu, or one that is not there.
pub fn find(vendor: u16, product: u16, ordinal: usize, hid: &[HidPad]) -> Option<Found> {
    let ours: Vec<&HidPad> = hid
        .iter()
        .filter(|pad| pad.vendor == vendor && pad.product == product)
        .collect();
    pick(&ours, ordinal)
}

/// The `ordinal`th pad, counted from 0, of those pads.rs calls `name`, found
/// among the HID devices plugged in rather than through gilrs, as Cemu names
/// it. This is for a pad gilrs has no ids for while Windows still has it
/// plugged in. pads.rs numbers the pads of one name in the order gilrs found
/// them, so with one such pad plugged in it is the same pad either way.
/// `None` for a name none of the pads SDL's HID drivers take in Cemu has.
pub fn find_named(name: &str, ordinal: usize, hid: &[HidPad]) -> Option<Found> {
    let ours: Vec<&HidPad> = hid
        .iter()
        .filter(|pad| gilrs_name(pad.vendor, pad.product) == Some(name))
        .collect();
    pick(&ours, ordinal)
}

/// The `ordinal`th of `ours`, which are in the order SDL finds them, counting
/// the pads on a cable first.
///
/// SDL gives a pad on Bluetooth no joystick while the same pad is on USB too
/// ("Prefer the USB device over the Bluetooth device", with
/// `HIDAPI_HasConnectedUSBDevice`, in the PS4, PS5 and Switch drivers'
/// `InitDevice`, release-2.30.3, read 8 October 2026). It knows the same pad
/// by a serial number it asks the pad for, and asking a DualSense over
/// Bluetooth switches it to reports other programs cannot read until it is
/// turned off (`HIDAPI_DriverPS5_InitDevice`, `SDL_hints.h`). So Omoio does
/// not ask, and cannot tell one pad on both from two pads. Counted cable
/// first, a pad on both is found as its USB self, the one SDL plays, and a
/// pad on Bluetooth alone is found as before.
fn pick(ours: &[&HidPad], ordinal: usize) -> Option<Found> {
    let mut ours = ours.to_vec();
    ours.sort_by_key(|pad| pad.bluetooth);
    let pad = ours.get(ordinal)?;
    let guid = guid(pad)?;
    // Joysticks of one GUID are counted in the order SDL found them, and the
    // only ones that can share it are the same model, read by the same driver.
    let before = ours[..ordinal].iter().filter(|other| self::guid(other) == Some(guid)).count();
    Some(Found {
        uuid: format!("{before}_{}", hex(&guid)),
        by_label: kind(pad.vendor, pad.product) == Some(Kind::SwitchPro),
    })
}

fn gilrs_name(vendor: u16, product: u16) -> Option<&'static str> {
    GILRS_NAMES
        .iter()
        .find(|(v, p, _)| *v == vendor && *p == product)
        .map(|(_, _, name)| *name)
}

fn kind(vendor: u16, product: u16) -> Option<Kind> {
    KNOWN
        .iter()
        .find(|(v, p, _)| *v == vendor && *p == product)
        .map(|(_, _, kind)| *kind)
}

/// The GUID SDL 2.30 gives this pad in Cemu, or `None` for one its HID
/// drivers do not take there.
pub fn guid(pad: &HidPad) -> Option<[u8; 16]> {
    let kind = kind(pad.vendor, pad.product)?;
    let manufacturer = pad.manufacturer.as_deref();
    let product = pad.product_name.as_deref();
    let mut guid = create_guid(pad.vendor, pad.product, pad.version, manufacturer, product);
    match kind {
        Kind::Ps5(name) => rename(&mut guid, pad, name),
        Kind::Ps4 => rename(&mut guid, pad, "PS4 Controller"),
        Kind::SwitchPro => guid[15] = SWITCH_PRO,
    }
    Some(guid)
}

/// `HIDAPI_SetDeviceName`: a new name brings the CRC of that name with it.
fn rename(guid: &mut [u8; 16], pad: &HidPad, name: &str) {
    let before = joystick_name(pad.vendor, pad.product, pad.manufacturer.as_deref(), pad.product_name.as_deref());
    if before != name {
        guid[2..4].copy_from_slice(&crc16(name.as_bytes()).to_le_bytes());
    }
}

/// `SDL_CreateJoystickGUID` for a HID pad, which always has both ids.
fn create_guid(vendor: u16, product: u16, version: u16, manufacturer: Option<&str>, name: Option<&str>) -> [u8; 16] {
    let crc = match (manufacturer, name) {
        (Some(m), Some(p)) if !m.is_empty() && !p.is_empty() => crc16(format!("{m} {p}").as_bytes()),
        (_, Some(p)) => crc16(p.as_bytes()),
        _ => 0,
    };
    let mut guid = [0u8; 16];
    guid[0..2].copy_from_slice(&0x0003u16.to_le_bytes());
    guid[2..4].copy_from_slice(&crc.to_le_bytes());
    guid[4..6].copy_from_slice(&vendor.to_le_bytes());
    guid[8..10].copy_from_slice(&product.to_le_bytes());
    guid[12..14].copy_from_slice(&version.to_le_bytes());
    guid[14] = HIDAPI;
    guid
}

/// `SDL_crc16`: CRC-16 with the reversed polynomial 0xA001, from 0.
fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |crc, &byte| {
        let mut r = (crc as u8) ^ byte;
        let mut low = 0u16;
        for _ in 0..8 {
            low = (if (low ^ r as u16) & 1 != 0 { 0xA001 } else { 0 }) ^ (low >> 1);
            r >>= 1;
        }
        low ^ (crc >> 8)
    })
}

/// `SDL_GUIDToString`: lowercase hex, two digits a byte.
fn hex(guid: &[u8; 16]) -> String {
    guid.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `SDL_CreateJoystickName` for the pads in `KNOWN`, none of which has a
/// name of its own in SDL's list of controllers, so the name comes from the
/// pad's strings or, failing those, from its kind.
fn joystick_name(vendor: u16, product: u16, manufacturer: Option<&str>, name: Option<&str>) -> String {
    const REPLACEMENTS: [(&str, &str); 12] = [
        ("ASTRO Gaming", "ASTRO"),
        ("Bensussen Deutsch & Associates,Inc.(BDA)", "BDA"),
        ("Guangzhou Chicken Run Network Technology Co., Ltd.", "GameSir"),
        ("HORI CO.,LTD", "HORI"),
        ("HORI CO.,LTD.", "HORI"),
        ("Mad Catz Inc.", "Mad Catz"),
        ("Nintendo Co., Ltd.", "Nintendo"),
        ("NVIDIA Corporation ", ""),
        ("Performance Designed Products", "PDP"),
        ("QANBA USA, LLC", "Qanba"),
        ("QANBA USA,LLC", "Qanba"),
        ("Unknown ", ""),
    ];
    let manufacturer = manufacturer.unwrap_or("").trim_start_matches(' ');
    let name = name.unwrap_or("").trim_start_matches(' ');
    let mut full: Vec<u8> = if !manufacturer.is_empty() && !name.is_empty() {
        format!("{manufacturer} {name}").into_bytes()
    } else if !name.is_empty() {
        name.as_bytes().to_vec()
    } else {
        match kind(vendor, product) {
            Some(Kind::Ps5(_)) => "DualSense Wireless Controller",
            Some(Kind::Ps4) => "PS4 Controller",
            Some(Kind::SwitchPro) => "Nintendo Switch Pro Controller",
            None => return format!("0x{vendor:04x}/0x{product:04x}"),
        }
        .as_bytes()
        .to_vec()
    };

    while full.last() == Some(&b' ') {
        full.pop();
    }
    let mut at = 0;
    while at + 1 < full.len() {
        if full[at] == b' ' && full[at + 1] == b' ' {
            full.remove(at);
        } else {
            at += 1;
        }
    }
    for (prefix, replacement) in REPLACEMENTS {
        if full.len() >= prefix.len() && full[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes()) {
            full.splice(..prefix.len(), replacement.bytes());
            break;
        }
    }
    // A name that says its maker twice, "Razer Razer Raiju", loses the first.
    for start in 1..full.len().saturating_sub(1) {
        let mut matched = full
            .iter()
            .zip(&full[start..])
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count();
        while matched > 0 {
            if full[matched] == b' ' || full[matched] == b'-' {
                full.drain(..=matched);
                break;
            }
            matched -= 1;
        }
        if matched > 0 {
            break;
        }
    }
    String::from_utf8_lossy(&full).into_owned()
}

/// The HID devices SDL would look at, in the order it finds them: present
/// ones with a driver, not XInput (whose paths hold `&ig_`), that open for
/// reading and writing, and whose top usage is a joystick, a gamepad or a
/// multi-axis controller (`hid_enumerate`, SDL's `hidapi/windows/hid.c`).
#[cfg(windows)]
pub fn hid_pads() -> Vec<HidPad> {
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_Registry_PropertyW, CM_Get_Parent, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
        SetupDiGetDeviceRegistryPropertyW, CM_DRP_COMPATIBLEIDS, CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        SPDRP_CLASS, SPDRP_DRIVER, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    };
    use windows::Win32::Devices::HumanInterfaceDevice::{
        HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetManufacturerString,
        HidD_GetPreparsedData, HidD_GetProductString, HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS,
        PHIDP_PREPARSED_DATA,
    };
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    const GENERIC_DESKTOP: u16 = 0x01;
    const JOYSTICK: u16 = 0x04;
    const GAMEPAD: u16 = 0x05;
    const MULTI_AXIS: u16 = 0x08;

    /// A string the way SDL keeps it: up to the first nul of 512 wide
    /// characters, or `None` when Windows gives none.
    fn read_string(get: impl FnOnce(*mut core::ffi::c_void, u32) -> bool) -> Option<String> {
        let mut wide = [0u16; 512];
        if !get(wide.as_mut_ptr().cast(), std::mem::size_of_val(&wide) as u32) {
            return None;
        }
        wide[511] = 0;
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        Some(String::from_utf16_lossy(&wide[..end]))
    }

    /// Whether the device has a driver and is of the HID class. SDL asks this
    /// of the device at the interface's index, which is how it reads.
    fn is_hid_class(set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO, index: u32) -> bool {
        let mut info = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        // Calls into Windows with buffers sized as asked.
        unsafe {
            if SetupDiEnumDeviceInfo(set, index, &mut info).is_err() {
                return false;
            }
            let mut class = [0u8; 512];
            if SetupDiGetDeviceRegistryPropertyW(set, &info, SPDRP_CLASS, None, Some(&mut class[..]), None).is_err() {
                return false;
            }
            let wide: Vec<u16> = class.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
            if String::from_utf16_lossy(&wide[..end]) != "HIDClass" {
                return false;
            }
            let mut driver = [0u8; 512];
            SetupDiGetDeviceRegistryPropertyW(set, &info, SPDRP_DRIVER, None, Some(&mut driver[..]), None).is_ok()
        }
    }

    /// Whether Windows has the device over Bluetooth, told by the compatible
    /// ids of the device it hangs from, as hidapi 0.14 tells a HID device's
    /// bus (`hid_internal_get_info` in its `windows/hid.c`, read 8 October
    /// 2026): the first id naming USB makes it USB, the first naming BTHENUM
    /// or BTHLEDEVICE Bluetooth.
    fn on_bluetooth(device: u32) -> bool {
        let mut parent = 0u32;
        let mut ids = [0u16; 1024];
        let mut length = std::mem::size_of_val(&ids) as u32;
        // Calls into Windows with a buffer of the size handed over.
        let read = unsafe {
            CM_Get_Parent(&mut parent, device, 0) == CR_SUCCESS
                && CM_Get_DevNode_Registry_PropertyW(
                    parent,
                    CM_DRP_COMPATIBLEIDS,
                    None,
                    Some(ids.as_mut_ptr().cast()),
                    &mut length,
                    0,
                ) == CR_SUCCESS
        };
        if !read {
            return false;
        }
        for id in ids.split(|&c| c == 0).take_while(|id| !id.is_empty()) {
            let id = String::from_utf16_lossy(id).to_uppercase();
            if id.contains("USB") {
                return false;
            }
            if id.contains("BTHENUM") || id.contains("BTHLEDEVICE") {
                return true;
            }
        }
        false
    }

    let mut found = Vec::new();
    // Calls into Windows. Every buffer handed over is sized as the call asks,
    // and every handle opened is closed.
    unsafe {
        let guid = HidD_GetHidGuid();
        let Ok(set) = SetupDiGetClassDevsW(Some(&guid as *const _), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
        else {
            return found;
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
            let at = index;
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
            let mut device = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiGetDeviceInterfaceDetailW(set, &interface, Some(detail), size, None, Some(&mut device)).is_err() {
                continue;
            }
            let path = std::ptr::addr_of!((*detail).DevicePath).cast::<u16>();
            let length = (0..).take_while(|&i| *path.add(i) != 0).count();
            let wide = std::slice::from_raw_parts(path, length);
            if String::from_utf16_lossy(wide).contains("&ig_") || !is_hid_class(set, at) {
                continue;
            }

            // For reading and writing, shared, as SDL opens it, so the
            // devices that would not open for SDL are passed over here too.
            let Ok(file) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
                .open(std::ffi::OsString::from_wide(wide))
            else {
                continue;
            };
            let handle = HANDLE(file.as_raw_handle());
            let mut attributes = HIDD_ATTRIBUTES {
                Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32,
                ..Default::default()
            };
            let _ = HidD_GetAttributes(handle, &mut attributes);
            let mut preparsed = PHIDP_PREPARSED_DATA::default();
            let mut caps: HIDP_CAPS = std::mem::zeroed();
            let usable = HidD_GetPreparsedData(handle, &mut preparsed) && {
                let status = HidP_GetCaps(preparsed, &mut caps);
                let _ = HidD_FreePreparsedData(preparsed);
                status == HIDP_STATUS_SUCCESS
            };
            if usable
                && caps.UsagePage == GENERIC_DESKTOP
                && [JOYSTICK, GAMEPAD, MULTI_AXIS].contains(&caps.Usage)
            {
                found.push(HidPad {
                    vendor: attributes.VendorID,
                    product: attributes.ProductID,
                    version: attributes.VersionNumber,
                    manufacturer: read_string(|buffer, length| HidD_GetManufacturerString(handle, buffer, length)),
                    product_name: read_string(|buffer, length| HidD_GetProductString(handle, buffer, length)),
                    bluetooth: on_bluetooth(device.DevInst),
                });
            }
            drop(file);
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    found
}

#[cfg(not(windows))]
pub fn hid_pads() -> Vec<HidPad> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(vendor: u16, product: u16, version: u16, manufacturer: Option<&str>, name: Option<&str>) -> HidPad {
        HidPad {
            vendor,
            product,
            version,
            manufacturer: manufacturer.map(str::to_string),
            product_name: name.map(str::to_string),
            bluetooth: false,
        }
    }

    fn dualsense() -> HidPad {
        pad(SONY, 0x0CE6, 0x0100, Some("Sony Interactive Entertainment"), Some("Wireless Controller"))
    }

    #[test]
    fn crc16_is_sdls() {
        // The check value of CRC-16/ARC, which SDL_crc16 is.
        assert_eq!(crc16(b"123456789"), 0xBB3D);
        assert_eq!(crc16(b""), 0);
    }

    #[test]
    fn a_dualsense_is_known_by_its_new_name() {
        let guid = guid(&dualsense()).unwrap();
        // CRC-16 of "DualSense Wireless Controller" is 0x5657.
        assert_eq!(hex(&guid), "030057564c050000e60c000000016800");
    }

    #[test]
    fn a_dualsense_edge_and_a_ps4_pad_have_names_of_their_own() {
        let edge = pad(SONY, 0x0DF2, 0x0100, Some("Sony Interactive Entertainment"), Some("DualSense Edge Wireless Controller"));
        assert_eq!(hex(&guid(&edge).unwrap())[4..8], *"e027");
        let ps4 = pad(SONY, 0x09CC, 0x0100, Some("Sony Interactive Entertainment"), Some("Wireless Controller"));
        assert_eq!(hex(&guid(&ps4).unwrap()), "03008fe54c050000cc09000000016800");
    }

    #[test]
    fn a_name_that_was_already_right_keeps_the_crc_of_the_strings() {
        // With no strings the name comes from the kind, which is the name
        // the driver gives, so SDL leaves the CRC as it was: none.
        let bare = pad(SONY, 0x0CE6, 0x0100, None, None);
        assert_eq!(hex(&guid(&bare).unwrap()), "030000004c050000e60c000000016800");
    }

    #[test]
    fn a_pad_over_bluetooth_differs_from_usb_only_in_what_windows_reports() {
        // SDL 2.30 writes USB into every GUID it makes from HID and takes the
        // version and strings Windows reports for the connection, which the
        // PS4 and PS5 drivers then rename (`HIDAPI_AddDevice`,
        // `HIDAPI_SetDeviceName`). Whatever strings come over Bluetooth, the
        // name is the driver's, so only another version tells the two apart.
        for (product, usb) in [
            (0x0CE6, "030057564c050000e60c000000016800"),
            (0x05C4, "03008fe54c050000c405000000016800"),
            (0x09CC, "03008fe54c050000cc09000000016800"),
        ] {
            let cable = pad(SONY, product, 0x0100, Some("Sony Interactive Entertainment"), Some("Wireless Controller"));
            assert_eq!(hex(&guid(&cable).unwrap()), usb);
            let same_version = pad(SONY, product, 0x0100, None, Some("Wireless Controller"));
            assert_eq!(guid(&same_version), guid(&cable), "other strings, the same name once renamed");
            let other_version = hex(&guid(&pad(SONY, product, 0x0211, None, Some("Wireless Controller"))).unwrap());
            assert_eq!(other_version[..4], *"0300", "still USB");
            assert_eq!(other_version[..24], usb[..24]);
            assert_eq!(other_version[24..28], *"1102", "its own version, little-endian");
            assert_eq!(other_version[28..], *"6800");
        }
    }

    fn over_bluetooth(mut pad: HidPad, version: u16) -> HidPad {
        pad.bluetooth = true;
        pad.version = version;
        pad
    }

    #[test]
    fn a_pad_on_cable_and_bluetooth_at_once_is_found_as_its_usb_self() {
        // SDL plays only the USB one of a pad on both, and Windows may list
        // the Bluetooth one first.
        let hid = [over_bluetooth(dualsense(), 0x0000), dualsense()];
        assert_eq!(find(SONY, 0x0CE6, 0, &hid).unwrap().uuid, "0_030057564c050000e60c000000016800");
        assert_eq!(find(SONY, 0x0CE6, 1, &hid).unwrap().uuid, "0_030057564c050000e60c000000006800");
        assert_eq!(find_named("PS5 Controller", 0, &hid), find(SONY, 0x0CE6, 0, &hid));

        let ps4 = pad(SONY, 0x09CC, 0x0100, Some("Sony Interactive Entertainment"), Some("Wireless Controller"));
        let hid = [over_bluetooth(ps4.clone(), 0x0000), ps4];
        assert_eq!(find(SONY, 0x09CC, 0, &hid).unwrap().uuid, "0_03008fe54c050000cc09000000016800");
    }

    #[test]
    fn a_pad_on_bluetooth_alone_is_found_as_it_is() {
        let hid = [over_bluetooth(dualsense(), 0x0000)];
        assert_eq!(find(SONY, 0x0CE6, 0, &hid).unwrap().uuid, "0_030057564c050000e60c000000006800");
        assert_eq!(find_named("PS5 Controller", 0, &hid), find(SONY, 0x0CE6, 0, &hid));
    }

    #[test]
    fn a_ps4_pad_with_no_strings_keeps_the_crc_it_had() {
        // The name then comes from the kind, "PS4 Controller", which is what
        // the driver renames it to, so SDL leaves the CRC as it was: none.
        let bare = pad(SONY, 0x09CC, 0x0100, None, None);
        assert_eq!(hex(&guid(&bare).unwrap()), "030000004c050000cc09000000016800");
    }

    #[test]
    fn a_pad_is_found_by_the_name_gilrs_gives_it() {
        let edge = pad(SONY, 0x0DF2, 0x0100, Some("Sony Interactive Entertainment"), Some("DualSense Edge Wireless Controller"));
        let pro = pad(NINTENDO, 0x2009, 0x0210, Some("Nintendo Co., Ltd."), Some("Pro Controller"));
        let hid = [pro, edge, dualsense()];
        assert_eq!(find_named("PS5 Controller", 0, &hid), find(SONY, 0x0DF2, 0, &hid), "an Edge has a DualSense's name");
        assert_eq!(find_named("PS5 Controller", 1, &hid), find(SONY, 0x0CE6, 0, &hid));
        assert_eq!(find_named("PS5 Controller", 2, &hid), None, "only two are plugged in");
        let found = find_named("Nintendo Switch Pro Controller", 0, &hid).unwrap();
        assert_eq!(found.uuid, "0_0300bb977e0500000920000010026803");
        assert!(found.by_label);
        assert_eq!(find_named("Wireless Controller", 0, &hid), None, "not a name gilrs gives any of them");
    }

    #[test]
    fn a_pro_controller_keeps_its_own_strings_and_says_what_it_is() {
        let pro = pad(NINTENDO, 0x2009, 0x0210, Some("Nintendo Co., Ltd."), Some("Pro Controller"));
        // CRC-16 of "Nintendo Co., Ltd. Pro Controller" is 0x97BB.
        assert_eq!(hex(&guid(&pro).unwrap()), "0300bb977e0500000920000010026803");
    }

    #[test]
    fn other_pads_are_not_guessed_at() {
        assert_eq!(guid(&pad(0x2DC8, 0x6101, 0x0100, Some("8BitDo"), Some("8BitDo Pro 2"))), None);
        assert_eq!(guid(&pad(0x045E, 0x028E, 0x0110, None, Some("Controller"))), None);
    }

    #[test]
    fn names_are_tidied_as_sdl_tidies_them() {
        assert_eq!(joystick_name(NINTENDO, 0x2009, Some("Nintendo Co., Ltd."), Some("Pro Controller")), "Nintendo Pro Controller");
        assert_eq!(joystick_name(SONY, 0x0CE6, Some("  Razer"), Some("Razer  Raiju ")), "Razer Raiju");
        assert_eq!(joystick_name(SONY, 0x0CE6, Some(""), Some("Wireless Controller")), "Wireless Controller");
        assert_eq!(joystick_name(SONY, 0x05C4, None, Some("")), "PS4 Controller");
    }

    #[test]
    fn the_same_model_twice_is_counted_in_the_order_found() {
        let other = pad(NINTENDO, 0x2009, 0x0210, Some("Nintendo Co., Ltd."), Some("Pro Controller"));
        let hid = [dualsense(), other, dualsense()];
        let first = find(SONY, 0x0CE6, 0, &hid).unwrap();
        let second = find(SONY, 0x0CE6, 1, &hid).unwrap();
        assert_eq!(first.uuid, "0_030057564c050000e60c000000016800");
        assert_eq!(second.uuid, "1_030057564c050000e60c000000016800");
        assert!(!first.by_label);
        let pro = find(NINTENDO, 0x2009, 0, &hid).unwrap();
        assert_eq!(pro.uuid, "0_0300bb977e0500000920000010026803");
        assert!(pro.by_label, "Switch pads report their buttons by letter");
        assert_eq!(find(SONY, 0x0CE6, 2, &hid), None, "only two are plugged in");
    }

    #[test]
    fn the_same_model_on_another_version_is_counted_apart() {
        let mut newer = dualsense();
        newer.version = 0x0200;
        let hid = [dualsense(), newer];
        assert!(find(SONY, 0x0CE6, 1, &hid).unwrap().uuid.starts_with("0_"));
    }
}
