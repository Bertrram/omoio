//! Figures on Dolphin's Infinity Base, put there from outside Dolphin.
//!
//! Dolphin keeps the base in memory and fills it only from its Infinity
//! Manager, a window of its own that opens from the main window's Tools
//! menu, under Emulated USB Devices, beside its Skylanders Manager
//! (`MenuBar::AddToolsMenu`, DolphinQt/MenuBar.cpp lines 278 to 292;
//! `MainWindow::ShowInfinityBase`, DolphinQt/MainWindow.cpp lines 1472 to
//! 1482). So a Disney Infinity game starts with the main window, hidden the
//! moment it shows, and the manager is opened once behind it and kept out of
//! sight for the rest of the game, as the portal's is (portal.rs).
//!
//! The manager (`InfinityBaseWindow`, DolphinQt/InfinityBase/
//! InfinityBaseWindow.cpp) lists the base's nine places in the order Omoio
//! counts them (core/infinity.rs), each a box with the name of the figure on
//! it, "None" when empty, and buttons of its own: Clear, Create and Load
//! (`AddFigureSlot`, lines 109 to 134). Load asks for a figure's file
//! ("Select Figure File"). Create opens the Infinity Figure Creator, whose
//! Create makes a figure from the number in its Figure Number box alone,
//! asks where to keep it ("Create Infinity File") and puts it on the place
//! (lines 159 to 166 and 197 to 313). Clear takes the figure off. All in
//! Dolphin 2609a, read 4 November 2026. Dolphin is Qt, and its windows are
//! worked as the portal's are (backends/qt.rs).
//!
//! While a figure is on the base Dolphin holds its file open, and nothing
//! else can open it then, not even to read it: Dolphin opens it with
//! `_wfopen_s` (`LoadFigurePath`, line 170; `IOFile::Open`,
//! Common/IOFile.cpp line 73), which "can't open a file for sharing"
//! (Microsoft Learn, fopen_s, _wfopen_s, Remarks, read 4 November 2026). So
//! nothing here reads a figure's file, and it is backed up before it goes on
//! (commands.rs, `toy_load`). Which figure a file holds, Omoio knows from its
//! own record of the figures it had made, or from the name Dolphin shows in
//! the place's box once it is on: the figure's number is encrypted in its
//! file, and Omoio decrypts nothing.

use super::portal::{self, choose_file, turn};
use crate::backends::qt::{
    self, arrives, gone, hide, key, name, out_of_sight, showing, soon, step_through, title, value, visible,
    Automation as Qt, WAIT,
};
use crate::backends::Listed;
use crate::core::infinity::PLACES;
use crate::core::toys::ToyFigure;
use std::path::Path;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, UIA_ButtonControlTypeId, UIA_ComboBoxControlTypeId, UIA_EditControlTypeId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;

/// Dolphin's windows, as InfinityBaseWindow.cpp titles them (lines 41, 199,
/// 147 and 295).
const MANAGER: &str = "Infinity Manager";
const CREATOR: &str = "Infinity Figure Creator";
const OPEN_FIGURE: &str = "Select Figure File";
const SAVE_FIGURE: &str = "Create Infinity File";

/// The windows Omoio opens for the base, none of them one of Dolphin's
/// messages (portal.rs, `is_message`).
pub(super) const WINDOWS: [&str; 4] = [MANAGER, CREATOR, OPEN_FIGURE, SAVE_FIGURE];

/// The way to the manager through the main window's menus (MenuBar.cpp
/// lines 278, 287 and 289), as UI Automation names them, without the `&`
/// that marks a shortcut.
const MENU: [&str; 3] = ["Tools", "Emulated USB Devices", "Infinity Base"];

/// A place's buttons, by their words (`AddFigureSlot`, lines 116 to 118).
/// The figure maker's button that makes the figure has the same words as
/// the place's, renamed from OK (line 257).
const LOAD: &str = "Load";
const CLEAR: &str = "Clear";
const CREATE: &str = "Create";

const LOOKS_DIFFERENT: &str = "Dolphin's Infinity Base looks different from what Omoio knows.";
const NOT_READY: &str = "Dolphin's Infinity Base isn't ready yet. Try again in a moment.";
const LOAD_FAILED: &str = "Dolphin couldn't put that figure on the base. It may be on it already.";
const MAKE_FAILED: &str = "Dolphin couldn't make that figure. Try again.";
const MAKER_DIDNT_OPEN: &str = "Dolphin's figure maker didn't open. Try again.";
const NO_LIST: &str = "Couldn't read Dolphin's list of figures.";


/// The places whose figure makers offer, between them, every figure on
/// Dolphin's list: the hexagon's first place its play sets and hexagonal
/// discs, Player One its characters, and the place under Player One its
/// round discs. Each maker lists only what fits its place
/// (`CreateFigureDialog`, lines 207 to 229).
const LISTED_IN: [usize; 3] = [0, 3, 4];

/// UI Automation, with Dolphin's words for what goes wrong in the base's
/// windows.
struct Automation(Qt);

impl Automation {
    fn new() -> Result<Self, String> {
        Qt::new().map(Self).ok_or_else(|| "Couldn't reach Dolphin's windows.".to_string())
    }

    fn window(&self, window: HWND) -> Result<IUIAutomationElement, String> {
        self.0.window(window).ok_or_else(|| LOOKS_DIFFERENT.to_string())
    }

    /// The buttons in `within` with these words, in the order Qt made them,
    /// waited for until there are `count`: Qt shows a window a moment before
    /// its widgets answer.
    fn buttons(
        &self,
        within: &IUIAutomationElement,
        words: &str,
        count: usize,
    ) -> Result<Vec<IUIAutomationElement>, String> {
        soon(WAIT, || {
            let found: Vec<IUIAutomationElement> = self
                .0
                .of_kind(within, UIA_ButtonControlTypeId)
                .into_iter()
                .filter(|button| name(button) == words)
                .collect();
            (found.len() == count).then_some(found)
        })
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
    }
}

/// The manager, opened through the main window's menus if it isn't open,
/// and kept out of sight. Dolphin makes it once, owned by no other window,
/// and raises it to the front whenever it shows it (`ShowInfinityBase`), so
/// it is moved away the moment it shows.
fn manager(automation: &Automation, pid: u32) -> Result<HWND, String> {
    if let Some(window) = showing(pid).into_iter().find(|&w| title(w) == MANAGER) {
        out_of_sight(window);
        return Ok(window);
    }
    let main = portal::main_window(pid).ok_or(NOT_READY)?;
    hide(main);
    let menus = automation.window(main)?;
    if !qt::choose_from_menus(&automation.0, pid, &menus, &MENU) {
        return Err(LOOKS_DIFFERENT.to_string());
    }
    arrives(pid, MANAGER).ok_or_else(|| "Dolphin's Infinity Base didn't open. Try again.".to_string())
}

/// Closes what a try that gave up left open: a message, a file window, the
/// figure maker. Each would stand in the way of the next errand. Skylanders'
/// Customize window, which Omoio knows by its look alone, is never looked
/// for here (portal.rs, `maker_window`): this figure maker has two boxes, as
/// that one has.
fn put_away_leftovers(pid: u32) {
    portal::put_away(pid, &[OPEN_FIGURE, SAVE_FIGURE, CREATOR]);
}

/// Gets the base ready while a Disney Infinity game starts: Dolphin's main
/// window is hidden the moment it shows, and the manager opened behind it,
/// so the first figure goes on as quickly as any other. The manager takes
/// the front as it opens, so the game gets it back.
pub fn prepare(pid: u32) {
    std::thread::spawn(move || {
        let Some(main) = soon(portal::START_WAIT, || portal::main_window(pid).filter(|&window| visible(window))) else {
            return;
        };
        hide(main);
        let _turn = turn();
        if let Ok(automation) = Automation::new() {
            let _ = manager(&automation, pid);
        }
        portal::give_back_to_game(pid);
    });
}

/// Gets the manager open, out of sight, without doing anything in it, so
/// the first errand from the base menu doesn't wait for it.
pub fn ready(pid: u32) {
    let _turn = turn();
    if let Ok(automation) = Automation::new() {
        let _ = manager(&automation, pid);
    }
}

fn check(place: usize) -> Result<(), String> {
    if place < PLACES {
        Ok(())
    } else {
        Err("The base has no place there.".to_string())
    }
}

/// A place's name as the menu knows it: empty for none, and otherwise the
/// name Dolphin gives the figure, from its list, or "Unknown Figure" for a
/// number its list doesn't have (`LoadFigurePath`, lines 193 to 194;
/// `InfinityBase::FindFigure`, Core/IOS/USB/Emulated/Infinity.cpp lines 741
/// to 751).
fn shown(name: &str) -> String {
    if name == "None" || name.is_empty() {
        String::new()
    } else {
        name.to_string()
    }
}

/// The places' name boxes, in place order: the manager's only boxes, made a
/// row at a time in that order and none of them taking typing
/// (`CreateMainWindow`, lines 83 to 99; `AddFigureSlot`, lines 119 to 121).
fn boxes(automation: &Automation, window: HWND) -> Result<Vec<IUIAutomationElement>, String> {
    let manager = automation.window(window)?;
    soon(WAIT, || {
        let found: Vec<IUIAutomationElement> = automation
            .0
            .of_kind(&manager, UIA_EditControlTypeId)
            .into_iter()
            .filter(|edit| !qt::enabled(edit))
            .collect();
        (found.len() == PLACES).then_some(found)
    })
    .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// The button with these words in `place`'s row. Each row has its own, and
/// no other button in the manager has their words, so the rows' come in
/// place order.
fn place_button(
    automation: &Automation,
    window: HWND,
    words: &str,
    place: usize,
) -> Result<IUIAutomationElement, String> {
    let manager = automation.window(window)?;
    automation
        .buttons(&manager, words, PLACES)?
        .into_iter()
        .nth(place)
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// What each place holds, empty where it holds nothing.
fn read(automation: &Automation, window: HWND) -> Result<Vec<String>, String> {
    Ok(boxes(automation, window)?.iter().map(|place| shown(&value(place))).collect())
}

/// What the base holds once Dolphin has finished, or `failed` when it put up
/// a message instead.
fn settled(automation: &Automation, pid: u32, window: HWND, failed: &str) -> Result<Vec<String>, String> {
    let names = read(automation, window)?;
    if portal::dismiss_message(pid) {
        return Err(failed.to_string());
    }
    Ok(names)
}

/// What is on the base, by place.
pub fn places(pid: u32) -> Result<Vec<String>, String> {
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    read(&automation, window)
}

/// Puts the figure in `file` on the base in `place`, counted from 0 as
/// core/infinity.rs counts them, and returns what the base holds afterwards.
/// Dolphin takes off a figure already there itself, once the new one's file
/// has been read (`LoadFigurePath`, lines 168 to 195).
pub fn load(pid: u32, place: usize, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(pid);
    qt::press(&place_button(&automation, window, LOAD, place)?, window).ok_or(LOOKS_DIFFERENT)?;
    let picker = arrives(pid, OPEN_FIGURE).ok_or("Dolphin didn't ask for the figure. Try again.")?;
    choose_file(&automation.0, picker, file, LOOKS_DIFFERENT)?;
    settled(&automation, pid, window, LOAD_FAILED)
}

/// Takes the figure in `place` off the base, and returns what is left.
/// Dolphin writes the figure's file a last time and lets go of it
/// (`InfinityBase::RemoveFigure`, Core/IOS/USB/Emulated/Infinity.cpp lines
/// 643 to 652).
pub fn remove(pid: u32, place: usize) -> Result<Vec<String>, String> {
    check(place)?;
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(pid);
    let held = boxes(&automation, window)?.into_iter().nth(place).ok_or(LOOKS_DIFFERENT)?;
    if !shown(&value(&held)).is_empty() {
        qt::press(&place_button(&automation, window, CLEAR, place)?, window).ok_or(LOOKS_DIFFERENT)?;
        // The press is posted, and Dolphin may answer a read before it gets
        // to it.
        let _ = soon(WAIT, || shown(&value(&held)).is_empty().then_some(()));
    }
    settled(&automation, pid, window, "Dolphin couldn't take that figure off the base. Try again.")
}

/// Whether Dolphin's maker makes a figure of this number. It writes the
/// number in three bytes and refuses one whose first byte is 0, but only
/// after it has made the file, which is left empty (`InfinityBase::CreateFigure`
/// and `GenerateBlankFigureData`, Core/IOS/USB/Emulated/Infinity.cpp lines
/// 672 to 708 and 815 to 823). So no other number is ever asked of it.
fn makes(number: u32) -> bool {
    (0x01_0000..=0xFF_FFFF).contains(&number)
}

/// Has Dolphin's figure maker make `figure` into `file`, which Dolphin then
/// puts on the base in `place` itself, and returns what the base holds
/// afterwards. Dolphin makes a figure from its number alone, with the first
/// game's series and release date in it whatever the number
/// (`GenerateBlankFigureData`, Core/IOS/USB/Emulated/Infinity.cpp lines 815
/// to 829). That is right for the Wii, which has the first game alone, so a
/// figure's series isn't asked for.
pub fn create(pid: u32, place: usize, figure: &ToyFigure, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    if !makes(figure.number) {
        return Err("Dolphin can't make that figure.".to_string());
    }
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(pid);
    qt::press(&place_button(&automation, window, CREATE, place)?, window).ok_or(LOOKS_DIFFERENT)?;
    let maker = arrives(pid, CREATOR).ok_or(MAKER_DIDNT_OPEN)?;
    if let Err(problem) = make(&automation, pid, maker, figure.number, file) {
        put_away_leftovers(pid);
        return Err(problem);
    }
    settled(&automation, pid, window, MAKE_FAILED)
}

/// The figure maker's Figure Number box: the one box that is the maker's
/// own. The other is the typing box of its list, an editable combo box,
/// which holds it (`CreateFigureDialog`, lines 204 to 254), as RPCS3's maker,
/// built the same way, shows its own as `...QComboBox.QLineEdit`
/// (backends/rpcs3/portal.rs, `MAKER_TYPING`).
fn number_box(automation: &Automation, maker: &IUIAutomationElement) -> Result<IUIAutomationElement, String> {
    soon(WAIT, || automation.0.child(maker, UIA_EditControlTypeId, None)).ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// The typing box of the figure maker's list, which takes the arrow keys
/// that step through it.
fn typing_box(automation: &Automation, maker: &IUIAutomationElement) -> Result<IUIAutomationElement, String> {
    soon(WAIT, || {
        automation
            .0
            .child(maker, UIA_ComboBoxControlTypeId, None)
            .and_then(|list| automation.0.child(&list, UIA_EditControlTypeId, None))
    })
    .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// Fills in the figure maker and has it save the figure. Dolphin reads the
/// Figure Number box alone, whatever its list shows (lines 270 to 278).
fn make(automation: &Automation, pid: u32, maker: HWND, number: u32, file: &Path) -> Result<(), String> {
    let window = automation.window(maker)?;
    qt::set_value(&number_box(automation, &window)?, &number.to_string()).ok_or(LOOKS_DIFFERENT)?;
    let create = automation.buttons(&window, CREATE, 1)?.remove(0);
    qt::press(&create, maker).ok_or(LOOKS_DIFFERENT)?;
    let saver = arrives(pid, SAVE_FIGURE).ok_or("Dolphin didn't ask where to keep the figure. Try again.")?;
    choose_file(&automation.0, saver, file, LOOKS_DIFFERENT)?;
    // Dolphin writes the figure and closes its maker, which puts the figure
    // on the place, or says why it couldn't (lines 302 to 312).
    let done = soon(WAIT, || {
        if !visible(maker) {
            Some(true)
        } else {
            showing(pid).into_iter().any(|window| portal::is_message(pid, window)).then_some(false)
        }
    });
    if done == Some(true) {
        Ok(())
    } else {
        Err(MAKE_FAILED.to_string())
    }
}

/// Every figure Dolphin's maker makes, each under the name Dolphin's list
/// gives it, which is the name a place's box shows once the figure is on:
/// 104 pieces of the first game, the only one on the Wii
/// (`list_infinity_figures`, Core/IOS/USB/Emulated/Infinity.cpp lines 23 to
/// 129). The list has no series, and so neither do these (core/toys.rs,
/// `ToyFigure`).
///
/// Each entry of the maker's list keeps its number as item data that only
/// Dolphin reads, and UI Automation gives an entry its text alone, but
/// picking one puts its number in the Figure Number box (lines 222 and 262
/// to 268). So the maker is opened from each place of `LISTED_IN`, its list
/// stepped through with the arrow keys as RPCS3's is (backends/qt.rs,
/// `step_through`), and closed again with Escape, which makes nothing. The
/// menu waits for this while it says it is getting the figures, so it is
/// done once for each Dolphin version, which keeps what it gives
/// (toy_menu.rs, `listed`). Every place's list has to be read, or a whole
/// kind of piece would be missing until Dolphin updates.
pub fn figures(pid: u32) -> Result<Listed<ToyFigure>, String> {
    let mut found: Vec<ToyFigure> = Vec::new();
    let mut whole = true;
    for place in LISTED_IN {
        let listed = listed_in(pid, place)?;
        whole &= listed.whole;
        for figure in listed.items {
            if !found.iter().any(|known| known.number == figure.number) {
                found.push(figure);
            }
        }
    }
    Ok(Listed { items: found, whole })
}

/// What the figure maker of `place` lists, a figure for each step.
fn listed_in(pid: u32, place: usize) -> Result<Listed<ToyFigure>, String> {
    let _turn = turn();
    let automation = Automation::new()?;
    let window = manager(&automation, pid)?;
    put_away_leftovers(pid);
    qt::press(&place_button(&automation, window, CREATE, place)?, window).ok_or(LOOKS_DIFFERENT)?;
    let maker = arrives(pid, CREATOR).ok_or(MAKER_DIDNT_OPEN)?;
    let steps = steps(&automation, maker);
    key(maker, VK_ESCAPE);
    gone(maker);
    let steps = steps?;
    let found: Vec<ToyFigure> = steps.seen.iter().filter_map(listed).collect();
    if found.is_empty() {
        Err(NO_LIST.to_string())
    } else {
        Ok(Listed { items: found, whole: steps.whole })
    }
}

/// Each step through the maker's list, as the list's typing box and the
/// Figure Number box show it.
fn steps(automation: &Automation, maker: HWND) -> Result<qt::Steps<(String, String)>, String> {
    let window = automation.window(maker)?;
    let number = number_box(automation, &window)?;
    let typing = typing_box(automation, &window)?;
    step_through(maker, &typing, || (value(&typing), value(&number)))
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// The figure one step showed: its name and the number it put in the box.
/// The list's last entry, "--Unknown--", puts 65535 there (line 230), a
/// number the maker doesn't make, and is left out with anything else that
/// isn't one.
fn listed((name, number): &(String, String)) -> Option<ToyFigure> {
    let number = number.trim().parse().ok().filter(|&number| makes(number))?;
    let name = name.trim();
    (!name.is_empty() && !name.starts_with("--")).then(|| ToyFigure { name: name.to_string(), number, series: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::infinity::{fits, Piece};

    #[test]
    fn a_place_reads_as_the_menu_names_it() {
        assert_eq!(shown("None"), "");
        assert_eq!(shown(""), "");
        assert_eq!(shown("Toy Story - Woody"), "Toy Story - Woody");
        assert_eq!(shown("Unknown Figure"), "Unknown Figure", "a figure Dolphin's list doesn't have is still on");
    }

    #[test]
    fn a_place_past_the_ninth_is_refused_before_dolphin_is_asked() {
        assert!(check(0).is_ok());
        assert!(check(8).is_ok());
        assert!(check(9).is_err());
        let figure = ToyFigure { name: "Mr. Incredible".into(), number: 1_000_001, series: None };
        assert!(load(0, 9, Path::new("C:\\Figures\\Woody.bin")).is_err());
        assert!(remove(0, 9).is_err());
        assert!(create(0, 9, &figure, Path::new("C:\\Figures\\Woody.bin")).is_err());
    }

    #[test]
    fn only_numbers_dolphins_maker_makes_are_asked_for() {
        assert!(!makes(0));
        assert!(!makes(0xFFFF), "the list's --Unknown--");
        assert!(makes(0x01_0000));
        assert!(makes(1_000_001), "Mr. Incredible");
        assert!(makes(4_000_070));
        assert!(makes(0xFF_FFFF));
        assert!(!makes(0x100_0000), "past the three bytes it has");
        let unlisted = ToyFigure { name: "Unknown".into(), number: 0xFFFF, series: None };
        let refused = create(0, 0, &unlisted, Path::new("C:\\Figures\\Unknown.bin"));
        assert!(refused.is_err(), "refused before Dolphin is asked");
    }

    #[test]
    fn each_step_through_the_list_gives_its_figure_and_the_unknown_line_none() {
        let step = |name: &str, number: &str| (name.to_string(), number.to_string());
        assert_eq!(
            listed(&step("The Incredibles - Mr. Incredible", "1000001")),
            Some(ToyFigure { name: "The Incredibles - Mr. Incredible".into(), number: 1_000_001, series: None })
        );
        assert_eq!(listed(&step("--Unknown--", "65535")), None);
        assert_eq!(listed(&step("--Unknown--", "1000001")), None, "a half step, the box not yet changed");
        assert_eq!(listed(&step("", "1000001")), None);
        assert_eq!(listed(&step("Cars Play Set", "")), None);
        assert_eq!(listed(&step("Cars Play Set", "two")), None);
        let trimmed = listed(&step(" Cars Play Set ", " 2000003 "));
        assert_eq!(trimmed.map(|figure| (figure.name, figure.number)), Some(("Cars Play Set".into(), 2_000_003)));
    }

    #[test]
    fn the_places_read_offer_every_kind_of_piece() {
        for piece in [Piece::Character, Piece::PlaySet, Piece::RoundDisc, Piece::HexDisc] {
            assert!(LISTED_IN.iter().any(|&place| fits(place, piece)), "{piece:?}");
        }
    }

    /// Needs a Dolphin running a Disney Infinity game with its main window
    /// there, started as `launch` starts one, and a figure file, so it runs
    /// only by hand:
    /// `OMOIO_DOLPHIN_PID=<pid> OMOIO_FIGURE=<file> cargo test base_on_a_running_dolphin -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn base_on_a_running_dolphin() {
        use std::time::Instant;
        let pid: u32 = std::env::var("OMOIO_DOLPHIN_PID").unwrap().parse().unwrap();
        let figure = std::env::var("OMOIO_FIGURE").unwrap();
        let started = Instant::now();
        let before = places(pid).unwrap();
        println!("opened and read in {:?}: {before:?}", started.elapsed());
        assert_eq!(before.len(), PLACES);
        let started = Instant::now();
        let loaded = load(pid, 3, Path::new(&figure)).unwrap();
        println!("load took {:?}: {loaded:?}", started.elapsed());
        assert!(!loaded[3].is_empty(), "Player One holds the figure: {loaded:?}");
        let started = Instant::now();
        let removed = remove(pid, 3).unwrap();
        println!("remove took {:?}", started.elapsed());
        assert!(removed[3].is_empty(), "Player One is empty again: {removed:?}");
        let started = Instant::now();
        let listed = figures(pid).unwrap();
        assert!(listed.whole, "the read reached both ends of the list");
        let listed = listed.items;
        println!("list took {:?}: {} figures", started.elapsed(), listed.len());
        assert_eq!(listed.len(), 104, "Dolphin 2609a's list: {listed:?}");
        let made = Path::new(&figure).with_file_name(format!("omoio-made-{}.bin", std::process::id()));
        let woody = ToyFigure { name: "Toy Story - Woody".into(), number: 1_000_016, series: None };
        let started = Instant::now();
        let after = create(pid, 6, &woody, &made).unwrap();
        println!("create took {:?}: {after:?}", started.elapsed());
        assert_eq!(after[6], "Toy Story - Woody");
        remove(pid, 6).unwrap();
        let _ = std::fs::remove_file(made);
    }
}
