//! Working an emulator's Qt windows from outside, for its Skylanders portal.
//!
//! RPCS3 and Dolphin are both built with Qt, whose buttons and boxes are not
//! windows of their own, so window messages can't reach them. Qt answers UI
//! Automation for every widget instead, and gives each an id made from the
//! object names of the widgets it sits in. A button is pressed by focusing
//! it and sending its window the space bar: UI Automation's own press makes
//! Qt hold the button down for a tenth of a second first. Nothing here moves
//! the mouse. How this was proven is in docs/what-we-verified.md,
//! "Skylanders".

use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};
use windows::core::{Interface, BOOL, BSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationElementArray, IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_AutomationIdPropertyId, UIA_ControlTypePropertyId, UIA_ExpandCollapsePatternId, UIA_InvokePatternId,
    UIA_MenuItemControlTypeId, UIA_NamePropertyId, UIA_SelectionItemPatternId, UIA_ValuePatternId,
    UIA_CONTROLTYPE_ID, UIA_PROPERTY_ID,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC_EX, VIRTUAL_KEY, VK_SPACE};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    SetWindowPos, ShowWindow, GW_OWNER, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, WM_KEYDOWN, WM_KEYUP,
};

/// How long a window or a widget may take to show.
pub const WAIT: Duration = Duration::from_secs(5);

/// UI Automation for the thread that asks. Each thread starts COM for itself;
/// one that already has it keeps the kind it has.
pub struct Automation(IUIAutomation);

impl Automation {
    pub fn new() -> Option<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()?;
            // A window that stops answering is given up on rather than waited for.
            if let Ok(newer) = automation.cast::<IUIAutomation2>() {
                let _ = newer.SetConnectionTimeout(2000);
                let _ = newer.SetTransactionTimeout(5000);
            }
            Some(Self(automation))
        }
    }

    pub fn window(&self, window: HWND) -> Option<IUIAutomationElement> {
        unsafe { self.0.ElementFromHandle(window) }.ok()
    }

    /// The widgets in `within` with this id, in the order Qt made them.
    pub fn all(&self, within: &IUIAutomationElement, id: &str) -> Vec<IUIAutomationElement> {
        self.matching(within, UIA_AutomationIdPropertyId, id)
    }

    /// The widgets in `within` of one kind, such as every radio button, in
    /// the order Qt made them.
    pub fn of_kind(&self, within: &IUIAutomationElement, kind: UIA_CONTROLTYPE_ID) -> Vec<IUIAutomationElement> {
        let found = self
            .number_is(UIA_ControlTypePropertyId, kind.0)
            .and_then(|condition| unsafe { within.FindAll(TreeScope_Descendants, &condition) });
        listed(found)
    }

    fn matching(&self, within: &IUIAutomationElement, property: UIA_PROPERTY_ID, text: &str) -> Vec<IUIAutomationElement> {
        let found = self
            .text_is(property, text)
            .and_then(|condition| unsafe { within.FindAll(TreeScope_Descendants, &condition) });
        listed(found)
    }

    /// The first widget in `within` with this id, waited for: Qt shows a
    /// window a moment before its widgets answer.
    pub fn first(&self, within: &IUIAutomationElement, id: &str) -> Option<IUIAutomationElement> {
        soon(WAIT, || self.all(within, id).into_iter().next())
    }

    pub fn menu_item(&self, within: &IUIAutomationElement, label: &str) -> Option<IUIAutomationElement> {
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

fn listed(found: windows::core::Result<IUIAutomationElementArray>) -> Vec<IUIAutomationElement> {
    let Ok(found) = found else {
        return Vec::new();
    };
    let count = unsafe { found.Length() }.unwrap_or(0);
    (0..count).filter_map(|at| unsafe { found.GetElement(at) }.ok()).collect()
}

/// Whether a widget takes input, as a slot's name box, which shows a
/// figure's name, does not.
pub fn enabled(element: &IUIAutomationElement) -> bool {
    unsafe { element.CurrentIsEnabled() }.is_ok_and(|on| on.as_bool())
}

pub fn name(element: &IUIAutomationElement) -> String {
    unsafe { element.CurrentName() }.map(|name| name.to_string()).unwrap_or_default()
}

pub fn value(element: &IUIAutomationElement) -> String {
    unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .and_then(|pattern| unsafe { pattern.CurrentValue() })
        .map(|value| value.to_string())
        .unwrap_or_default()
}

pub fn set_value(element: &IUIAutomationElement, text: &str) -> Option<()> {
    let text = BSTR::from(text);
    unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .and_then(|pattern| unsafe { pattern.SetValue(&text) })
        .ok()
}

/// UI Automation's own press. For a Qt button it lands a tenth of a second
/// late, but it leaves the keyboard where it is.
pub fn invoke(element: &IUIAutomationElement) -> Option<()> {
    unsafe { element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
        .and_then(|pattern| unsafe { pattern.Invoke() })
        .ok()
}

/// Opens a menu, or a menu item that holds one.
pub fn expand(element: &IUIAutomationElement) -> Option<()> {
    unsafe { element.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId) }
        .and_then(|menu| unsafe { menu.Expand() })
        .ok()
}

/// Whether a radio button or a list item is the one picked.
pub fn is_selected(element: &IUIAutomationElement) -> bool {
    unsafe { element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId) }
        .and_then(|item| unsafe { item.CurrentIsSelected() })
        .is_ok_and(|selected| selected.as_bool())
}

/// Presses a button the way the keyboard does: focused, then the space bar
/// sent to its window. Posted, so a press that opens a window waiting for an
/// answer doesn't wait here.
pub fn press(button: &IUIAutomationElement, window: HWND) -> Option<()> {
    unsafe { button.SetFocus() }.ok()?;
    key(window, VK_SPACE);
    Some(())
}

/// A key going down and up in `window`, for whichever of its widgets has the
/// keyboard. Qt reads the scan code too, so it comes along as Windows gives it.
pub fn key(window: HWND, key: VIRTUAL_KEY) {
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
pub fn soon<T>(limit: Duration, mut ready: impl FnMut() -> Option<T>) -> Option<T> {
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

/// The process's windows, shown or hidden, in the order Windows keeps them.
pub fn all_windows(pid: u32) -> Vec<HWND> {
    let mut all: Vec<HWND> = Vec::new();
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize)) };
    all.into_iter().filter(|&window| process_of(window) == pid).collect()
}

pub fn showing(pid: u32) -> Vec<HWND> {
    all_windows(pid).into_iter().filter(|&window| visible(window)).collect()
}

pub fn visible(window: HWND) -> bool {
    unsafe { IsWindowVisible(window) }.as_bool()
}

pub fn title(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetWindowTextW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

pub fn class(window: HWND) -> String {
    let mut buffer = [0u16; 128];
    let length = unsafe { GetClassNameW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

/// Far off the screen, where it still works but nobody sees it.
pub fn out_of_sight(window: HWND) {
    let _ = unsafe { SetWindowPos(window, None, -32000, -32000, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
}

pub fn hide(window: HWND) {
    let _ = unsafe { ShowWindow(window, SW_HIDE) };
}

/// Waits until a window has closed, and says whether it did.
pub fn gone(window: HWND) -> bool {
    soon(WAIT, || (!visible(window)).then_some(())).is_some()
}

/// The process's window titled `wanted`, put out of sight as soon as it
/// shows. Looked for every millisecond, so it is gone before it can be seen.
pub fn arrives(pid: u32, wanted: &str) -> Option<HWND> {
    soon(WAIT, || {
        let window = showing(pid).into_iter().find(|&w| title(w) == wanted)?;
        out_of_sight(window);
        Some(window)
    })
}

/// Qt's menus each open as a window of their own, over everything else.
pub fn open_menus(pid: u32) -> Vec<HWND> {
    showing(pid).into_iter().filter(|&window| class(window).contains("Popup")).collect()
}

pub fn process_of(window: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    pid
}

/// Whether another of the process's windows owns this one, as it owns its
/// dialogs. The game's window has no owner until it sits in Omoio's, which
/// then owns it.
pub fn owned_by(pid: u32, window: HWND) -> bool {
    unsafe { GetWindow(window, GW_OWNER) }.is_ok_and(|owner| process_of(owner) == pid)
}
