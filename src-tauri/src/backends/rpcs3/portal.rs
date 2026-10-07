//! Figures on RPCS3's Skylanders portal, put there from outside RPCS3.
//!
//! RPCS3 keeps its portal in memory and fills it only from its Skylanders
//! Manager, which opens from the main window's Manage menu. There is no file
//! or command-line option for it, and `--no-gui` leaves the main window out
//! altogether. So a Skylanders game starts with the main window, which is
//! hidden the moment it shows, and the manager is opened once behind it and
//! kept out of sight for the rest of the game.
//!
//! RPCS3 is built with Qt, whose buttons and boxes are not windows of their
//! own, so the window messages that work Cemu's portal can't reach them. Qt
//! answers UI Automation for every widget instead. A button is pressed by
//! focusing it and sending its window the space bar: UI Automation's own
//! press makes Qt hold the button down for a tenth of a second first, three
//! times over for a new figure. Qt is told to use its own file windows rather
//! than Windows' (`dialogs=none`), which open faster and answer UI Automation
//! like the rest. Nothing moves the mouse. How this was proven is in
//! docs/what-we-verified.md, "Skylanders".

use crate::core::figures::{self, Character};
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use windows::core::{Interface, BOOL, BSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_AutomationIdPropertyId, UIA_ControlTypePropertyId, UIA_ExpandCollapsePatternId, UIA_InvokePatternId,
    UIA_MenuItemControlTypeId, UIA_NamePropertyId, UIA_ValuePatternId, UIA_PROPERTY_ID,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC_EX, VIRTUAL_KEY, VK_DOWN, VK_ESCAPE, VK_LMENU, VK_RETURN, VK_SPACE, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, PostMessageW, SetForegroundWindow, SetWindowPos, ShowWindow, GW_OWNER, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, WM_CLOSE, WM_KEYDOWN, WM_KEYUP,
};

/// How many figures RPCS3's portal window holds (`UI_SKY_NUM`).
pub const SLOTS: usize = 8;

/// What Qt's Windows platform is told, so RPCS3 opens Qt's own file windows.
pub const QT_PLATFORM: &str = "windows:dialogs=none";

/// The start of RPCS3's main window title, which goes on with the version.
pub const MAIN_TITLE: &str = "RPCS3 ";
/// How the game window's title starts, from the Window Title Format in
/// RPCS3's config.yml ("FPS: %F | %R | %V | %T [%t]"), which Omoio leaves as
/// it comes; the window has it from the moment it opens (`gs_frame.cpp`).
/// RPCS3's other windows without an owner never start so, such as the one
/// that shows progress while a game's code is compiled on its first start.
pub const GAME_TITLE: &str = "FPS:";

/// RPCS3's windows, as `rpcs3qt/skylander_dialog.cpp` titles them.
const MANAGER: &str = "Skylanders Manager";
const CREATOR: &str = "Skylander Creator";
const OPEN_FIGURE: &str = "Select Skylander File";
const SAVE_FIGURE: &str = "Create Skylander File";

/// The way to the manager through the main window's menus (`main_window.ui`).
const MENU: [&str; 3] = ["Manage", "Portals and Gates", "Skylanders Portal"];

/// Qt makes each widget's UI Automation id from where it sits.
const SLOT_NAME: &str = "gui_application.skylanders_manager.QGroupBox.QLineEdit";
const SLOT_BUTTON: &str = "gui_application.skylanders_manager.QGroupBox.QPushButton";
const MAKER_BOX: &str = "gui_application.skylanders_creator.QLineEdit";
const MAKER_BUTTON: &str = "gui_application.skylanders_creator.QPushButton";
const MAKER_LIST: &str = "gui_application.skylanders_creator.QComboBox";
const MAKER_TYPING: &str = "gui_application.skylanders_creator.QComboBox.QLineEdit";
const FILE_NAME: &str = "gui_application.QFileDialog.fileNameEdit";
const FILE_BUTTON: &str = "gui_application.QFileDialog.buttonBox.QPushButton";

const LOOKS_DIFFERENT: &str = "RPCS3's portal looks different from what Omoio knows.";
const NOT_READY: &str = "RPCS3's portal isn't ready yet. Try again in a moment.";
const LOAD_FAILED: &str = "RPCS3 couldn't put that figure on the portal. It may be on it already.";
const MAKE_FAILED: &str = "RPCS3 couldn't make that figure. Try again.";

const WAIT: Duration = Duration::from_secs(5);
/// How long RPCS3 may take to show its main window after it starts.
const START_WAIT: Duration = Duration::from_secs(30);
/// How long one step through the figure maker's list may take to show. The
/// list stops moving at either end, which is how its ends are found, so a
/// step that hasn't shown by then gets `END_WAIT` more before it counts as
/// the end: a game starting can keep RPCS3 too busy to answer at once, and
/// a list cut short would be kept until RPCS3 updates.
const STEP_WAIT: Duration = Duration::from_millis(300);
const END_WAIT: Duration = Duration::from_secs(1);

/// One errand in RPCS3's windows at a time: the manager opening while a game
/// starts and a figure the user picked meanwhile would each open windows the
/// other is waiting for, and one clears away the windows another left open.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn turn() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

/// UI Automation for the thread that asks. Each thread starts COM for itself;
/// one that already has it keeps the kind it has.
struct Automation(IUIAutomation);

impl Automation {
    fn new() -> Result<Self, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .map_err(|_| "Couldn't reach RPCS3's windows.".to_string())?;
            // A window that stops answering is given up on rather than waited for.
            if let Ok(newer) = automation.cast::<IUIAutomation2>() {
                let _ = newer.SetConnectionTimeout(2000);
                let _ = newer.SetTransactionTimeout(5000);
            }
            Ok(Self(automation))
        }
    }

    fn window(&self, window: HWND) -> Result<IUIAutomationElement, String> {
        unsafe { self.0.ElementFromHandle(window) }.map_err(|_| LOOKS_DIFFERENT.to_string())
    }

    /// The widgets in `within` with this id, in the order Qt made them.
    fn all(&self, within: &IUIAutomationElement, id: &str) -> Vec<IUIAutomationElement> {
        let found = self
            .text_is(UIA_AutomationIdPropertyId, id)
            .and_then(|condition| unsafe { within.FindAll(TreeScope_Descendants, &condition) });
        let Ok(found) = found else {
            return Vec::new();
        };
        let count = unsafe { found.Length() }.unwrap_or(0);
        (0..count).filter_map(|at| unsafe { found.GetElement(at) }.ok()).collect()
    }

    /// The first widget in `within` with this id, waited for: Qt shows a
    /// window a moment before its widgets answer.
    fn first(&self, within: &IUIAutomationElement, id: &str) -> Result<IUIAutomationElement, String> {
        soon(WAIT, || self.all(within, id).into_iter().next()).ok_or_else(|| LOOKS_DIFFERENT.to_string())
    }

    fn menu_item(&self, within: &IUIAutomationElement, label: &str) -> Option<IUIAutomationElement> {
        let by_name = self.text_is(UIA_NamePropertyId, label).ok()?;
        let by_kind = self.number_is(UIA_ControlTypePropertyId, UIA_MenuItemControlTypeId.0).ok()?;
        let both = unsafe { self.0.CreateAndCondition(&by_name, &by_kind) }.ok()?;
        unsafe { within.FindFirst(TreeScope_Descendants, &both) }.ok()
    }

    fn text_is(&self, property: UIA_PROPERTY_ID, text: &str) -> windows::core::Result<IUIAutomationCondition> {
        let mut value = VARIANT::default();
        unsafe {
            let inner = &mut *value.Anonymous.Anonymous;
            inner.vt = VT_BSTR;
            inner.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(text));
            let condition = self.0.CreatePropertyCondition(property, &value);
            let _ = VariantClear(&mut value);
            condition
        }
    }

    fn number_is(&self, property: UIA_PROPERTY_ID, number: i32) -> windows::core::Result<IUIAutomationCondition> {
        let mut value = VARIANT::default();
        unsafe {
            let inner = &mut *value.Anonymous.Anonymous;
            inner.vt = VT_I4;
            inner.Anonymous.lVal = number;
            self.0.CreatePropertyCondition(property, &value)
        }
    }
}

fn name(element: &IUIAutomationElement) -> String {
    unsafe { element.CurrentName() }.map(|name| name.to_string()).unwrap_or_default()
}

fn value(element: &IUIAutomationElement) -> String {
    unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .and_then(|pattern| unsafe { pattern.CurrentValue() })
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn set_value(element: &IUIAutomationElement, text: &str) -> Result<(), String> {
    let text = BSTR::from(text);
    unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .and_then(|pattern| unsafe { pattern.SetValue(&text) })
        .map_err(|_| LOOKS_DIFFERENT.to_string())
}

/// UI Automation's own press. For a Qt button it lands a tenth of a second
/// late, but it leaves the keyboard where it is.
fn invoke(element: &IUIAutomationElement) -> Result<(), String> {
    unsafe { element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
        .and_then(|pattern| unsafe { pattern.Invoke() })
        .map_err(|_| LOOKS_DIFFERENT.to_string())
}

/// Presses a button the way the keyboard does: focused, then the space bar
/// sent to its window. Posted, so a press that opens a window waiting for an
/// answer doesn't wait here.
fn press(button: &IUIAutomationElement, window: HWND) -> Result<(), String> {
    unsafe { button.SetFocus() }.map_err(|_| LOOKS_DIFFERENT.to_string())?;
    key(window, VK_SPACE);
    Ok(())
}

/// A key going down and up in `window`, for whichever of its widgets has the
/// keyboard. Qt reads the scan code too, so it comes along as Windows gives it.
fn key(window: HWND, key: VIRTUAL_KEY) {
    let scan = unsafe { MapVirtualKeyW(u32::from(key.0), MAPVK_VK_TO_VSC_EX) } as isize;
    let extended = isize::from(scan & 0xFF00 == 0xE000) << 24;
    let down = 1 | ((scan & 0xFF) << 16) | extended;
    let up = down | 0xC000_0000;
    unsafe {
        let _ = PostMessageW(Some(window), WM_KEYDOWN, WPARAM(usize::from(key.0)), LPARAM(down));
        let _ = PostMessageW(Some(window), WM_KEYUP, WPARAM(usize::from(key.0)), LPARAM(up));
    }
}

/// Asks `ready` every millisecond until it has an answer or `limit` is up.
fn soon<T>(limit: Duration, mut ready: impl FnMut() -> Option<T>) -> Option<T> {
    let until = Instant::now() + limit;
    loop {
        if let Some(found) = ready() {
            return Some(found);
        }
        if Instant::now() >= until {
            return None;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
    let list = unsafe { &mut *(list.0 as *mut Vec<HWND>) };
    list.push(window);
    BOOL(1)
}

/// RPCS3's windows, shown or hidden, in the order Windows keeps them.
fn all_windows(pid: u32) -> Vec<HWND> {
    let mut all: Vec<HWND> = Vec::new();
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize)) };
    all.into_iter().filter(|&window| process_of(window) == pid).collect()
}

fn showing(pid: u32) -> Vec<HWND> {
    all_windows(pid).into_iter().filter(|&window| visible(window)).collect()
}

fn visible(window: HWND) -> bool {
    unsafe { IsWindowVisible(window) }.as_bool()
}

pub fn title(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetWindowTextW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

fn class(window: HWND) -> String {
    let mut buffer = [0u16; 128];
    let length = unsafe { GetClassNameW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

/// Far off the screen, where it still works but nobody sees it.
fn out_of_sight(window: HWND) {
    let _ = unsafe {
        SetWindowPos(window, None, -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
    };
}

fn hide(window: HWND) {
    let _ = unsafe { ShowWindow(window, SW_HIDE) };
}

/// Waits until a window has closed, and says whether it did.
fn gone(window: HWND) -> bool {
    soon(WAIT, || (!visible(window)).then_some(())).is_some()
}

/// RPCS3's window titled `wanted`, put out of sight as soon as it shows.
/// Looked for every millisecond, so it is gone before it can be seen.
fn arrives(pid: u32, wanted: &str) -> Option<HWND> {
    soon(WAIT, || {
        let window = showing(pid).into_iter().find(|&w| title(w) == wanted)?;
        out_of_sight(window);
        Some(window)
    })
}

fn main_window(pid: u32) -> Option<HWND> {
    all_windows(pid).into_iter().find(|&window| title(window).starts_with(MAIN_TITLE))
}

/// Qt's menus each open as a window of their own, over everything else.
fn open_menus(pid: u32) -> Vec<HWND> {
    showing(pid).into_iter().filter(|&window| class(window).contains("Popup")).collect()
}

fn process_of(window: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    pid
}

/// Whether another of RPCS3's windows owns this one, as it owns its dialogs.
/// The game's window has no owner until it sits in Omoio's, which then owns it.
fn owned_by_rpcs3(pid: u32, window: HWND) -> bool {
    unsafe { GetWindow(window, GW_OWNER) }.is_ok_and(|owner| process_of(owner) == pid)
}

/// A window RPCS3 put up by itself, such as an error: one of its dialogs
/// other than those Omoio opens. Qt's menus, titled "RPCS3" like the
/// program, are windows of their own kind.
fn is_message(pid: u32, window: HWND) -> bool {
    let title = title(window);
    owned_by_rpcs3(pid, window)
        && !title.is_empty()
        && !class(window).contains("Popup")
        && ![MANAGER, CREATOR, OPEN_FIGURE, SAVE_FIGURE].contains(&title.as_str())
}

/// Hands the keyboard back to the game's window when one of RPCS3's own
/// windows took it, as a file window does when it opens: RPCS3's game hears
/// the pad only while its window is in front. Windows lets a program change
/// which window is in front only once it has had the last input, so a press
/// of Alt, which no window acts on alone, goes first, the way Tauri brings
/// its own windows forward.
fn give_back_to_game(pid: u32) {
    let front = unsafe { GetForegroundWindow() };
    if process_of(front) != pid {
        return;
    }
    let game = showing(pid)
        .into_iter()
        .find(|&window| !owned_by_rpcs3(pid, window) && title(window).starts_with(GAME_TITLE));
    let Some(game) = game.filter(|&game| game != front) else {
        return;
    };
    if unsafe { SetForegroundWindow(game) }.as_bool() {
        return;
    }
    let alt = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_LMENU, dwFlags: flags, ..Default::default() } },
    };
    let presses = [alt(KEYBD_EVENT_FLAGS(0)), alt(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&presses, std::mem::size_of::<INPUT>() as i32);
        let _ = SetForegroundWindow(game);
    }
}

/// Closes a message RPCS3 put up, so it doesn't sit over the game, and says
/// whether there was one.
fn dismiss_message(pid: u32) -> bool {
    let Some(message) = showing(pid).into_iter().find(|&w| is_message(pid, w)) else {
        return false;
    };
    out_of_sight(message);
    key(message, VK_RETURN);
    gone(message);
    true
}

/// Closes what a try that gave up left open: a message, a file window, the
/// figure maker. Each is modal, and the manager's buttons would open another
/// over it. A message that won't close is left after a few tries.
fn put_away_leftovers(pid: u32) {
    for _ in 0..3 {
        if !dismiss_message(pid) {
            break;
        }
    }
    for leftover in [OPEN_FIGURE, SAVE_FIGURE, CREATOR] {
        for window in showing(pid).into_iter().filter(|&w| title(w) == leftover) {
            key(window, VK_ESCAPE);
            gone(window);
        }
    }
}

/// The manager, opened through the main window's menus if it isn't open,
/// and kept out of sight.
fn manager(automation: &Automation, pid: u32) -> Result<HWND, String> {
    if let Some(window) = showing(pid).into_iter().find(|&w| title(w) == MANAGER) {
        out_of_sight(window);
        return Ok(window);
    }
    let main = main_window(pid).ok_or(NOT_READY)?;
    hide(main);
    let menus = automation.window(main)?;
    let opened = open_manager(automation, pid, &menus);
    if opened.is_err() {
        for menu in open_menus(pid) {
            key(menu, VK_ESCAPE);
        }
    }
    opened?;
    arrives(pid, MANAGER).ok_or_else(|| "RPCS3's portal didn't open. Try again.".to_string())
}

/// Manage, then Portals and Gates, then Skylanders Portal. Each menu goes out
/// of sight as soon as it opens, and they all close when the item is chosen.
fn open_manager(automation: &Automation, pid: u32, menus: &IUIAutomationElement) -> Result<(), String> {
    for label in &MENU[..2] {
        let item = find_menu_item(automation, pid, menus, label).ok_or(LOOKS_DIFFERENT)?;
        unsafe { item.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId) }
            .and_then(|menu| unsafe { menu.Expand() })
            .map_err(|_| LOOKS_DIFFERENT.to_string())?;
        for menu in open_menus(pid) {
            out_of_sight(menu);
        }
    }
    invoke(&find_menu_item(automation, pid, menus, MENU[2]).ok_or(LOOKS_DIFFERENT)?)
}

/// A menu item, in the main window or in one of the menus open over it.
fn find_menu_item(automation: &Automation, pid: u32, menus: &IUIAutomationElement, label: &str) -> Option<IUIAutomationElement> {
    soon(WAIT, || {
        automation.menu_item(menus, label).or_else(|| {
            open_menus(pid)
                .into_iter()
                .find_map(|menu| automation.window(menu).ok().and_then(|menu| automation.menu_item(&menu, label)))
        })
    })
}

/// Gets the portal ready while a Skylanders game starts: RPCS3's main window
/// is hidden the moment it shows, and the manager opened behind it, so the
/// first figure goes on as quickly as any other.
pub fn prepare(pid: u32) {
    std::thread::spawn(move || {
        let Some(main) = soon(START_WAIT, || main_window(pid).filter(|&window| visible(window))) else {
            return;
        };
        hide(main);
        let _turn = turn();
        if let Ok(automation) = Automation::new() {
            if let Ok(window) = manager(&automation, pid) {
                warm_up(&automation, pid, window);
            }
        }
    });
}

/// Opens one of Qt's file windows and closes it again, unseen, while the game
/// starts. Qt sets its file windows up the first time one opens, which made a
/// game's first figure take a second against a third of one after. Pressed
/// with UI Automation's own press, which leaves the keyboard alone: the
/// game's window may be up by now.
fn warm_up(automation: &Automation, pid: u32, window: HWND) {
    let Ok(manager) = automation.window(window) else {
        return;
    };
    if slot_button(automation, &manager, "Load", 0).and_then(|load| invoke(&load)).is_err() {
        return;
    }
    let Some(picker) = arrives(pid, OPEN_FIGURE) else {
        return;
    };
    let cancel = automation.window(picker).ok().and_then(|dialog| {
        automation.all(&dialog, FILE_BUTTON).into_iter().find(|button| name(button) == "Cancel")
    });
    if cancel.is_some_and(|cancel| invoke(&cancel).is_ok()) {
        gone(picker);
    }
    give_back_to_game(pid);
}

/// Keeps RPCS3's main window hidden while the game runs, and closes RPCS3
/// once the game's window `game` has gone: with its main window there, RPCS3
/// would otherwise carry on, out of sight, after the game.
pub fn tidy(pid: u32, game: isize) {
    let Some(main) = main_window(pid) else {
        return;
    };
    if !unsafe { IsWindow(Some(HWND(game as *mut _))) }.as_bool() {
        let _ = unsafe { PostMessageW(Some(main), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    } else if visible(main) {
        hide(main);
    }
}

fn check(slot: usize) -> Result<(), String> {
    if slot < SLOTS {
        Ok(())
    } else {
        Err("The portal has no slot there.".to_string())
    }
}

/// A slot's name as the menu knows it: empty for none, and for one of the
/// figures whose variant RPCS3's list has wrong, the figure's name where
/// RPCS3 says "Unknown (Id:212 Var:12302)".
fn shown(name: &str) -> String {
    if name == "None" {
        return String::new();
    }
    unknown(name)
        .and_then(|(id, variant)| figures::fixed_name(id, variant))
        .map_or_else(|| name.to_string(), str::to_string)
}

fn unknown(name: &str) -> Option<(u16, u16)> {
    let (id, variant) = name.strip_prefix("Unknown (Id:")?.strip_suffix(')')?.split_once(" Var:")?;
    Some((id.parse().ok()?, variant.parse().ok()?))
}

/// What each slot holds, empty where it holds nothing. Qt answers on the
/// thread that loads a figure, so a read after a figure went on sees it.
fn read(automation: &Automation, window: HWND) -> Result<Vec<String>, String> {
    let manager = automation.window(window)?;
    let names: Vec<String> = automation.all(&manager, SLOT_NAME).iter().map(|slot| shown(&value(slot))).collect();
    if names.len() == SLOTS {
        Ok(names)
    } else {
        Err(LOOKS_DIFFERENT.to_string())
    }
}

/// What the portal holds once RPCS3 has finished, or `failed` when it put up
/// a message instead. RPCS3's own words name the file's whole path, so they
/// stay behind.
fn settled(automation: &Automation, pid: u32, window: HWND, failed: &str) -> Result<Vec<String>, String> {
    let names = read(automation, window)?;
    if dismiss_message(pid) {
        return Err(failed.to_string());
    }
    Ok(names)
}

fn slot_button(automation: &Automation, manager: &IUIAutomationElement, label: &str, slot: usize) -> Result<IUIAutomationElement, String> {
    automation
        .all(manager, SLOT_BUTTON)
        .into_iter()
        .filter(|button| name(button) == label)
        .nth(slot)
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// Puts a figure's file in one of Qt's file windows and presses Enter, which
/// presses its Open or Save button, then waits for it to close.
fn choose_file(automation: &Automation, picker: HWND, file: &Path) -> Result<(), String> {
    let window = automation.window(picker)?;
    let name_box = automation.first(&window, FILE_NAME)?;
    set_value(&name_box, &file.to_string_lossy())?;
    key(picker, VK_RETURN);
    if gone(picker) {
        Ok(())
    } else {
        key(picker, VK_ESCAPE);
        Err("RPCS3 didn't take the figure's file. Try again.".to_string())
    }
}

/// The figures on the portal, by slot.
pub fn figures(pid: u32) -> Result<Vec<String>, String> {
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    read(&automation, window)
}

/// Puts the figure in `file` on the portal in `slot`, counted from 0, and
/// returns what the portal holds afterwards.
pub fn load(pid: u32, slot: usize, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(pid);
    let buttons = automation.window(window)?;
    press(&slot_button(&automation, &buttons, "Load", slot)?, window)?;
    let picker = arrives(pid, OPEN_FIGURE).ok_or("RPCS3 didn't ask for the figure. Try again.")?;
    choose_file(&automation, picker, file)?;
    settled(&automation, pid, window, LOAD_FAILED)
}

/// Takes the figure in `slot` off the portal, and returns what is left.
pub fn clear(pid: u32, slot: usize) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    let manager = automation.window(window)?;
    let held = automation.all(&manager, SLOT_NAME).into_iter().nth(slot).ok_or(LOOKS_DIFFERENT)?;
    if value(&held) != "None" {
        press(&slot_button(&automation, &manager, "Clear", slot)?, window)?;
        // The press is posted, and RPCS3 may answer a read before it gets to it.
        let _ = soon(WAIT, || (value(&held) == "None").then_some(()));
    }
    read(&automation, window)
}

/// Has RPCS3's figure maker make a figure of `character` into `file`. RPCS3
/// then puts it on the portal in `slot` itself. Returns what the portal
/// holds afterwards.
pub fn create(pid: u32, slot: usize, character: &Character, file: &Path) -> Result<Vec<String>, String> {
    check(slot)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(pid);
    let buttons = automation.window(window)?;
    press(&slot_button(&automation, &buttons, "Create", slot)?, window)?;
    let maker = arrives(pid, CREATOR).ok_or("RPCS3's figure maker didn't open. Try again.")?;
    if let Err(problem) = make(&automation, pid, maker, character, file) {
        put_away_leftovers(pid);
        return Err(problem);
    }
    settled(&automation, pid, window, MAKE_FAILED)
}

/// Fills in the figure maker and has it save the figure. Its ID and Variant
/// boxes are what it makes the figure from; its list only fills them in.
fn make(automation: &Automation, pid: u32, maker: HWND, character: &Character, file: &Path) -> Result<(), String> {
    let window = automation.window(maker)?;
    let boxes = soon(WAIT, || Some(automation.all(&window, MAKER_BOX)).filter(|found| found.len() == 2))
        .ok_or(LOOKS_DIFFERENT)?;
    set_value(&boxes[0], &character.id.to_string())?;
    set_value(&boxes[1], &character.variant.to_string())?;
    let make = automation
        .all(&window, MAKER_BUTTON)
        .into_iter()
        .find(|button| name(button) == "Create")
        .ok_or(LOOKS_DIFFERENT)?;
    press(&make, maker)?;
    let saver = arrives(pid, SAVE_FIGURE).ok_or("RPCS3 didn't ask where to keep the figure. Try again.")?;
    choose_file(automation, saver, file)?;
    // RPCS3 writes the figure, closes the maker and puts the figure on, or
    // says why it couldn't.
    let done = soon(WAIT, || {
        if !visible(maker) {
            Some(true)
        } else {
            showing(pid).into_iter().any(|window| is_message(pid, window)).then_some(false)
        }
    });
    if done == Some(true) {
        Ok(())
    } else {
        Err(MAKE_FAILED.to_string())
    }
}

/// Every character RPCS3's figure maker offers. The maker is closed again
/// without making anything.
pub fn characters(pid: u32) -> Result<Vec<Character>, String> {
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(pid);
    let buttons = automation.window(window)?;
    // Any slot's maker will do, since nothing is made.
    press(&slot_button(&automation, &buttons, "Create", SLOTS - 1)?, window)?;
    let maker = arrives(pid, CREATOR).ok_or("RPCS3's figure maker didn't open. Try again.")?;
    let found = list(&automation, maker);
    key(maker, VK_ESCAPE);
    gone(maker);
    match found {
        Ok(found) if !found.is_empty() => Ok(found),
        _ => Err("Couldn't read RPCS3's list of characters.".to_string()),
    }
}

/// The figure maker's list, read one step at a time. Each step fills the ID
/// and Variant boxes for the character it lands on, and the arrow keys take
/// the steps. The list is sorted by name and opens on Whirlwind, so it is
/// read down to the end, then up to the top; an end is where a step changes
/// nothing.
fn list(automation: &Automation, maker: HWND) -> Result<Vec<Character>, String> {
    let window = automation.window(maker)?;
    let shown = automation.first(&window, MAKER_LIST)?;
    let boxes = automation.all(&window, MAKER_BOX);
    let (Some(id_box), Some(variant_box)) = (boxes.first(), boxes.get(1)) else {
        return Err(LOOKS_DIFFERENT.to_string());
    };
    // The arrows go to the widget with the keyboard, which has to be the
    // list's own typing box rather than one of the others.
    let typing = automation.first(&window, MAKER_TYPING)?;
    unsafe { typing.SetFocus() }.map_err(|_| LOOKS_DIFFERENT.to_string())?;
    let read = || (value(&shown), value(id_box), value(variant_box));
    let mut found: Vec<Character> = Vec::new();
    let mut now = read();
    keep(&mut found, &now);
    for arrow in [VK_DOWN, VK_UP] {
        loop {
            key(maker, arrow);
            // A read can land half way through a step, between the list and
            // the boxes, so a change counts once two reads agree on it.
            let moved = || {
                let first = read();
                (first != now && read() == first).then_some(first)
            };
            let Some(next) = soon(STEP_WAIT, moved).or_else(|| soon(END_WAIT, moved)) else {
                break;
            };
            keep(&mut found, &next);
            now = next;
        }
    }
    Ok(found)
}

/// Adds what one step showed, once. "--Unknown--" leaves the boxes as the
/// character before it had them, so it is left out.
fn keep(found: &mut Vec<Character>, (name, id, variant): &(String, String, String)) {
    let (Ok(id), Ok(variant)) = (id.parse(), variant.parse()) else {
        return;
    };
    if name.is_empty() || name.starts_with("--") || found.iter().any(|known| known.name == *name) {
        return;
    }
    found.push(Character { name: name.clone(), id, variant });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_reads_as_the_menu_names_it() {
        assert_eq!(shown("None"), "");
        assert_eq!(shown("Spyro"), "Spyro");
        // Tempest Timer made with Trap Team's own variant, which RPCS3's
        // list has as 0x300D.
        assert_eq!(shown("Unknown (Id:212 Var:12302)"), "Tempest Timer");
        // A figure RPCS3 doesn't know stays as RPCS3 put it.
        assert_eq!(shown("Unknown (Id:999 Var:0)"), "Unknown (Id:999 Var:0)");
        assert_eq!(unknown("Unknown (Id:16 Var:6145)"), Some((16, 0x1801)));
        assert_eq!(unknown("Unknown Spyro"), None);
    }

    #[test]
    fn each_character_is_kept_once_and_the_unknown_line_not_at_all() {
        let mut found = Vec::new();
        let step = |name: &str, id: &str, variant: &str| (name.to_string(), id.to_string(), variant.to_string());
        keep(&mut found, &step("Whirlwind", "0", "0"));
        keep(&mut found, &step("Wildfire", "458", "0"));
        keep(&mut found, &step("Whirlwind", "0", "0"));
        keep(&mut found, &step("--Unknown--", "607", "0"));
        keep(&mut found, &step("Polar Whirlwind", "0", "7170"));
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Whirlwind", "Wildfire", "Polar Whirlwind"]);
        assert_eq!((found[2].id, found[2].variant), (0, 0x1C02));
    }

    /// Needs an RPCS3 running with its Skylanders Manager reachable and a
    /// figure file, so it runs only by hand:
    /// `OMOIO_RPCS3_PID=<pid> OMOIO_FIGURE=<file> cargo test portal_on_a_running_rpcs3 -- --ignored`
    #[test]
    #[ignore]
    fn portal_on_a_running_rpcs3() {
        let pid: u32 = std::env::var("OMOIO_RPCS3_PID").unwrap().parse().unwrap();
        let figure = std::env::var("OMOIO_FIGURE").unwrap();
        let before = figures(pid).unwrap();
        assert_eq!(before.len(), SLOTS);
        let started = Instant::now();
        let loaded = load(pid, 0, Path::new(&figure)).unwrap();
        println!("load took {:?}: {loaded:?}", started.elapsed());
        assert!(!loaded[0].is_empty(), "slot 1 holds the figure: {loaded:?}");
        let started = Instant::now();
        let cleared = clear(pid, 0).unwrap();
        println!("clear took {:?}", started.elapsed());
        assert!(cleared[0].is_empty(), "slot 1 is empty again: {cleared:?}");
        let started = Instant::now();
        let list = characters(pid).unwrap();
        println!("{} characters in {:?}", list.len(), started.elapsed());
        assert!(list.iter().any(|c| c.name == "Spyro" && c.id == 16 && c.variant == 0));
        // Beside the figure, in a small folder like Omoio's own: Qt's save
        // window reads the folder it opens in.
        let made = Path::new(&figure).with_file_name(format!("omoio-made-{}.sky", std::process::id()));
        let started = Instant::now();
        let after = create(pid, 1, &Character { name: "Snap Shot".into(), id: 462, variant: 0 }, &made).unwrap();
        println!("create took {:?}: {after:?}", started.elapsed());
        assert_eq!(after[1], "Snap Shot");
        clear(pid, 1).unwrap();
        let _ = std::fs::remove_file(made);
    }
}
