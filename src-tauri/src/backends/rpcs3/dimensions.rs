//! Tags on RPCS3's Toy Pad, put there from outside RPCS3.
//!
//! RPCS3 always emulates one Toy Pad (`get_num_emu_devices`,
//! Emu/Io/Dimensions.cpp lines 573 to 576) and fills it only from its
//! Dimensions Manager, which opens from the main window's Manage menu, then
//! Portals and Gates, then Dimensions Toypad (rpcs3qt/main_window.ui lines
//! 300, 330 to 339 and 1332 to 1334; main_window.cpp lines 3217 to 3221).
//! Its portal server knows Skylanders only (Emu/Io/SkylanderPortalIPC.cpp
//! lines 151 to 182). So a LEGO Dimensions game starts with RPCS3's main
//! window, hidden, as a Skylanders game does (launch.rs), and the manager is
//! opened once behind it, kept out of sight for the rest of the game and
//! worked by UI Automation the way infinity.rs works the Infinity Manager,
//! which RPCS3 builds the same way: a group box of places, each a name box
//! and the buttons Move, Clear, Create and Load (dimensions_dialog.cpp
//! lines 570 to 603 and 618 to 681). All RPCS3/rpcs3 b2e52d6, read 4
//! November 2026.
//!
//! Omoio reads nothing out of a tag's file. RPCS3 names a tag in the
//! manager once it has put it on the Toy Pad (lines 759 to 778), its figure
//! maker's list gives each minifigure's number, and RPCS3's list of vehicles
//! and gadgets is kept here (`TOKENS`). RPCS3 keeps none of the lights the
//! game sets on the pads (Emu/Io/Dimensions.cpp lines 647 to 659), so the
//! menu has none to show here.

use super::portal::{
    dismiss_message, free_turn, give_back_to_game, is_message, main_window, put_away, turn, END_WAIT, FILE_BUTTON,
    FILE_NAME, START_WAIT, STEP_WAIT,
};
use crate::backends::qt::{
    self, arrives, arrives_over, gone, hide, key, name, out_of_sight, showing, soon, title, value, visible,
    Automation as Qt, WAIT,
};
use crate::core::dimensions::{Piece, PLACES};
use crate::core::toys::ToyFigure;
use std::path::Path;
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::IUIAutomationElement;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN};

/// RPCS3's windows for the Toy Pad, as `rpcs3qt/dimensions_dialog.cpp`
/// titles them (lines 573, 383, 503, 708 and 464). The figure maker and its
/// save window have the same titles as the Infinity Manager's
/// (infinity_dialog.cpp lines 409 and 582), so they are looked for over the
/// window that opens them (`arrives_over`).
const MANAGER: &str = "Dimensions Manager";
const CREATOR: &str = "Figure Creator";
const MOVER: &str = "Figure Mover";
const OPEN_FIGURE: &str = "Select Dimensions File";
const SAVE_FIGURE: &str = "Create Figure File";

/// The windows above, which portal.rs never takes for one of RPCS3's
/// messages.
pub(super) const WINDOWS: [&str; 5] = [MANAGER, CREATOR, MOVER, OPEN_FIGURE, SAVE_FIGURE];

/// What a try that gave up may leave open: a file window before the figure
/// maker it opened over, and the mover, whose Escape puts the tag back
/// where it was (`move_figure`, lines 735 to 738).
const LEFTOVERS: [&str; 4] = [OPEN_FIGURE, SAVE_FIGURE, CREATOR, MOVER];

/// The way to the manager through the main window's menus.
const MENU: [&str; 3] = ["Manage", "Portals and Gates", "Dimensions Toypad"];

/// Qt makes each widget's UI Automation id from the object names of the
/// widgets it sits in: the manager is `dimensions_manager`, its figure
/// maker `figure_creator` and its mover `figure_mover` (lines 574, 384 and
/// 504), built as the Skylanders windows whose ids were proven
/// (docs/what-we-verified.md, "Driving RPCS3's portal from outside works").
///
/// The manager's seven name boxes and 28 buttons are made without a parent
/// and become its group box's own when the group box takes its layout
/// (line 600), in the layout's order: place 0, 1 and 2 in the top row, 3 to
/// 6 in the bottom one, each its name box, Move, Clear, Create and Load
/// (lines 585 to 599 and 671 to 678). So the nth name box, and the nth
/// button of a label, is place n.
///
/// The maker has one number box of its own, Figure Number, where the
/// Infinity Manager's has two (lines 417 to 424; infinity_dialog.cpp lines
/// 506 to 516); its list's typing box sits inside the list.
///
/// The mover's seven buttons are made with the mover as their parent, in
/// place order (lines 509 to 519 and 542), and read "Move Here", or "Pick
/// up and Place" at the tag's own place (lines 543 to 546). Only a
/// minifigure's place has a label beside it (lines 530 to 541), so they
/// are told apart by their order.
const PLACE_NAME: &str = "gui_application.dimensions_manager.QGroupBox.QLineEdit";
const PLACE_BUTTON: &str = "gui_application.dimensions_manager.QGroupBox.QPushButton";
const MAKER_BOX: &str = "gui_application.figure_creator.QLineEdit";
const MAKER_BUTTON: &str = "gui_application.figure_creator.QPushButton";
const MAKER_LIST: &str = "gui_application.figure_creator.QComboBox";
const MAKER_TYPING: &str = "gui_application.figure_creator.QComboBox.QLineEdit";
const MOVER_BUTTON: &str = "gui_application.figure_mover.QPushButton";

/// What a name box says when its place holds nothing (lines 648, 691 and
/// 732).
const NONE: &str = "None";

/// The title of RPCS3's message for a file shorter than a tag (lines 750 to
/// 755). Its other message for a file it won't take, "Failed to open the
/// figure file!", also stands for one on the Toy Pad already, which it
/// holds open against writers (`fs::lock`, lines 743 to 748).
const TOO_SHORT: &str = "Failed to read the figure file!";

/// How long a tag stays lifted in a move before it goes down in its new
/// place. RPCS3 tells the game it was lifted as the mover opens and put down
/// when a place is picked (lines 721 to 726), and hands the game one such
/// change each time it asks the Toy Pad (`interrupt_transfer`,
/// Emu/Io/Dimensions.cpp lines 583 to 611). How often the game asks is in
/// neither emulator's source; Cemu's Toy Pad, waiting for a change to hand
/// over, looks again every 100 ms (`DimensionsUSB::GetStatus`,
/// nsyshid/Dimensions.cpp lines 511 to 538, Bertrram/Cemu v2.6-2, read 4
/// November 2026). A player takes a moment in the mover, and Omoio waits a
/// little longer than that look, so the lift is read before the put-down.
/// Whether the game needs the wait at all only a play can say.
const LIFTED: Duration = Duration::from_millis(150);

/// RPCS3's vehicles and gadgets, as its `list_tokens` has them and in its
/// order (rpcs3qt/dimensions_dialog.cpp lines 112 to 378). No window of
/// RPCS3's lists them: the manager only names a tag by them as it puts the
/// tag on (lines 769 to 772), and the figure maker lists the minifigures
/// alone (lines 389 to 402). They are kept here so the menu knows a tag
/// RPCS3 names by one of them as that vehicle or gadget, such as a Blank
/// Tag the game has built one onto (toy_menu.rs, `learned`). Cemu's list is
/// the same, entry for entry (`s_listTokens`, nsyshid/Dimensions.cpp lines
/// 114 to 380, Bertrram/Cemu v2.6-2), the space in front of the second
/// Gyrosphere's name included, which keeps it apart from the first. Read 4
/// November 2026.
const TOKENS: [(u32, &str); 266] = [
    (1000, "Police Car"),
    (1001, "Aerial Squad Car"),
    (1002, "Missile Striker"),
    (1003, "Gravity Sprinter"),
    (1004, "Street Shredder"),
    (1005, "Sky Clobberer"),
    (1006, "Batmobile"),
    (1007, "Batblaster"),
    (1008, "Sonic Batray"),
    (1009, "Benny's Spaceship"),
    (1010, "Lasercraft"),
    (1011, "The Annihilator"),
    (1012, "DeLorean Time Machine"),
    (1013, "Electric Time Machine"),
    (1014, "Ultra Time Machine"),
    (1015, "Hoverboard"),
    (1016, "Cyclone Board"),
    (1017, "Ultimate Hoverjet"),
    (1018, "Eagle Interceptor"),
    (1019, "Eagle Sky Blazer"),
    (1020, "Eagle Swoop Diver"),
    (1021, "Swamp Skimmer"),
    (1022, "Cragger's Fireship"),
    (1023, "Croc Command Sub"),
    (1024, "Cyber-Guard"),
    (1025, "Cyber-Wrecker"),
    (1026, "Laser Robot Walker"),
    (1027, "K-9"),
    (1028, "K-9 Ruff Rover"),
    (1029, "K-9 Laser Cutter"),
    (1030, "TARDIS"),
    (1031, "Laser-Pulse TARDIS"),
    (1032, "Energy-Burst TARDIS"),
    (1033, "Emmet's Excavator"),
    (1034, "Destroy Dozer"),
    (1035, "Destruct-o-Mech"),
    (1036, "Winged Monkey"),
    (1037, "Battle Monkey"),
    (1038, "Commander Monkey"),
    (1039, "Axe Chariot"),
    (1040, "Axe Hurler"),
    (1041, "Soaring Chariot"),
    (1042, "Shelob the Great"),
    (1043, "8-Legged Stalker"),
    (1044, "Poison Slinger"),
    (1045, "Homer's Car"),
    (1047, "The SubmaHomer"),
    (1046, "The Homercraft"),
    (1048, "Taunt-o-Vision"),
    (1050, "The MechaHomer"),
    (1049, "Blast Cam"),
    (1051, "Velociraptor"),
    (1053, "Venom Raptor"),
    (1052, "Spike Attack Raptor"),
    (1054, "Gyrosphere"),
    (1055, "Sonic Beam Gyrosphere"),
    (1056, " Gyrosphere"),
    (1057, "Clown Bike"),
    (1058, "Cannon Bike"),
    (1059, "Anti-Gravity Rocket Bike"),
    (1060, "Mighty Lion Rider"),
    (1061, "Lion Blazer"),
    (1062, "Fire Lion"),
    (1063, "Arrow Launcher"),
    (1064, "Seeking Shooter"),
    (1065, "Triple Ballista"),
    (1066, "Mystery Machine"),
    (1067, "Mystery Tow & Go"),
    (1068, "Mystery Monster"),
    (1069, "Boulder Bomber"),
    (1070, "Boulder Blaster"),
    (1071, "Cyclone Jet"),
    (1072, "Storm Fighter"),
    (1073, "Lightning Jet"),
    (1074, "Electro-Shooter"),
    (1075, "Blade Bike"),
    (1076, "Flight Fire Bike"),
    (1077, "Blades of Fire"),
    (1078, "Samurai Mech"),
    (1079, "Samurai Shooter"),
    (1080, "Soaring Samurai Mech"),
    (1081, "Companion Cube"),
    (1082, "Laser Deflector"),
    (1083, "Gold Heart Emitter"),
    (1084, "Sentry Turret"),
    (1085, "Turret Striker"),
    (1086, "Flight Turret Carrier"),
    (1087, "Scooby Snack"),
    (1088, "Scooby Fire Snack"),
    (1089, "Scooby Ghost Snack"),
    (1090, "Cloud Cuckoo Car"),
    (1091, "X-Stream Soaker"),
    (1092, "Rainbow Cannon"),
    (1093, "Invisible Jet"),
    (1094, "Laser Shooter"),
    (1095, "Torpedo Bomber"),
    (1096, "NinjaCopter"),
    (1097, "Glaciator"),
    (1098, "Freeze Fighter"),
    (1099, "Travelling Time Train"),
    (1100, "Flight Time Train"),
    (1101, "Missile Blast Time Train"),
    (1102, "Aqua Watercraft"),
    (1103, "Seven Seas Speeder"),
    (1104, "Trident of Fire"),
    (1105, "Drill Driver"),
    (1106, "Bane Dig 'n' Drill"),
    (1107, "Bane Drill 'n' Blast"),
    (1108, "Quinn Mobile"),
    (1109, "Quinn Ultra Racer"),
    (1110, "Missile Launcher"),
    (1111, "The Joker's Chopper"),
    (1112, "Mischievous Missile Blaster"),
    (1113, "Lock 'n' Laser Jet"),
    (1114, "Hover Pod"),
    (1115, "Krypton Striker"),
    (1116, "Super Stealth Pod"),
    (1117, "Dalek"),
    (1118, "Fire 'n' Ride Dalek"),
    (1119, "Silver Shooter Dalek"),
    (1120, "Ecto-1"),
    (1121, "Ecto-1 Blaster"),
    (1122, "Ecto-1 Water Diver"),
    (1123, "Ghost Trap"),
    (1124, "Ghost Stun 'n' Trap"),
    (1125, "Proton Zapper"),
    (1126, "Unknown"),
    (1127, "Unknown"),
    (1128, "Unknown"),
    (1129, "Unknown"),
    (1130, "Unknown"),
    (1131, "Unknown"),
    (1132, "Lloyd's Golden Dragon"),
    (1133, "Sword Projector Dragon"),
    (1134, "Unknown"),
    (1135, "Unknown"),
    (1136, "Unknown"),
    (1137, "Unknown"),
    (1138, "Unknown"),
    (1139, "Unknown"),
    (1140, "Unknown"),
    (1141, "Unknown"),
    (1142, "Unknown"),
    (1143, "Unknown"),
    (1144, "Mega Flight Dragon"),
    (1145, "Unknown"),
    (1146, "Unknown"),
    (1147, "Unknown"),
    (1148, "Unknown"),
    (1149, "Unknown"),
    (1150, "Unknown"),
    (1151, "Unknown"),
    (1152, "Unknown"),
    (1153, "Unknown"),
    (1154, "Unknown"),
    (1155, "Flying White Dragon"),
    (1156, "Golden Fire Dragon"),
    (1157, "Ultra Destruction Dragon"),
    (1158, "Arcade Machine"),
    (1159, "8-Bit Shooter"),
    (1160, "The Pixelator"),
    (1161, "G-6155 Spy Hunter"),
    (1162, "Interdiver"),
    (1163, "Aerial Spyhunter"),
    (1164, "Slime Shooter"),
    (1165, "Slime Exploder"),
    (1166, "Slime Streamer"),
    (1167, "Terror Dog"),
    (1168, "Terror Dog Destroyer"),
    (1169, "Soaring Terror Dog"),
    (1170, "Ancient Psychic Tandem War Elephant"),
    (1171, "Cosmic Squid"),
    (1172, "Psychic Submarine"),
    (1173, "BMO"),
    (1174, "DOGMO"),
    (1175, "SNAKEMO"),
    (1176, "Jakemobile"),
    (1177, "Snail Dude Jake"),
    (1178, "Hover Jake"),
    (1179, "Lumpy Car"),
    (1181, "Lumpy Land Whale"),
    (1180, "Lumpy Truck"),
    (1182, "Lunatic Amp"),
    (1183, "Shadow Scorpion"),
    (1184, "Heavy Metal Monster"),
    (1185, "B.A.'s Van"),
    (1186, "Fool Smasher"),
    (1187, "Pain Plane"),
    (1188, "Phone Home"),
    (1189, "Mobile Uplink"),
    (1190, "Super-Charged Satellite"),
    (1191, "Niffler"),
    (1192, "Sinister Scorpion"),
    (1193, "Vicious Vulture"),
    (1194, "Swooping Evil"),
    (1195, "Brutal Bloom"),
    (1196, "Crawling Creeper"),
    (1197, "Ecto-1 (2016)"),
    (1198, "Ectozer"),
    (1199, "PerfEcto"),
    (1200, "Flash 'n' Finish"),
    (1201, "Rampage Record Player"),
    (1202, "Stripe's Throne"),
    (1203, "R.C. Racer"),
    (1204, "Gadget-O-Matic"),
    (1205, "Scarlet Scorpion"),
    (1206, "Hogwarts Express"),
    (1208, "Steam Warrior"),
    (1207, "Soaring Steam Plane"),
    (1209, "Enchanted Car"),
    (1210, "Shark Sub"),
    (1211, "Monstrous Mouth"),
    (1212, "IMF Scrambler"),
    (1213, "Shock Cycle"),
    (1214, "IMF Covert Jet"),
    (1215, "IMF Sports Car"),
    (1216, "IMF Tank"),
    (1217, "IMF Splorer"),
    (1218, "Sonic Speedster"),
    (1219, "Blue Typhoon"),
    (1220, "Moto Bug"),
    (1221, "The Tornado"),
    (1222, "Crabmeat"),
    (1223, "Eggcatcher"),
    (1224, "K.I.T.T."),
    (1225, "Goliath Armored Semi"),
    (1226, "K.I.T.T. Jet"),
    (1227, "Police Helicopter"),
    (1228, "Police Hovercraft"),
    (1229, "Police Plane"),
    (1230, "Bionic Steed"),
    (1231, "Bat-Raptor"),
    (1232, "Ultrabat"),
    (1233, "Batwing"),
    (1234, "The Black Thunder"),
    (1235, "Bat-Tank"),
    (1236, "Skeleton Organ"),
    (1237, "Skeleton Jukebox"),
    (1238, "Skele-Turkey"),
    (1239, "One-Eyed Willy's Pirate Ship"),
    (1240, "Fanged Fortune"),
    (1241, "Inferno Cannon"),
    (1242, "Buckbeak"),
    (1243, "Giant Owl"),
    (1244, "Fierce Falcon"),
    (1245, "Saturn's Sandworm"),
    (1247, "Haunted Vacuum"),
    (1246, "Spooky Spider"),
    (1248, "PPG Smartphone"),
    (1249, "PPG Hotline"),
    (1250, "Powerpuff Mag-Net"),
    (1253, "Mega Blast Bot"),
    (1251, "Ka-Pow Cannon"),
    (1252, "Slammin' Guitar"),
    (1254, "Octi"),
    (1255, "Super Skunk"),
    (1256, "Sonic Squid"),
    (1257, "T-Car"),
    (1258, "T-Forklift"),
    (1259, "T-Plane"),
    (1260, "Spellbook of Azarath"),
    (1261, "Raven Wings"),
    (1262, "Giant Hand"),
    (1263, "Titan Robot"),
    (1264, "T-Rocket"),
    (1265, "Robot Retriever"),
];

const UNREACHABLE: &str = "Couldn't reach RPCS3's windows.";
const LOOKS_DIFFERENT: &str = "RPCS3's Toy Pad looks different from what Omoio knows.";
const NOT_READY: &str = "RPCS3's Toy Pad isn't ready yet. Try again in a moment.";
const DIDNT_OPEN: &str = "RPCS3's Toy Pad didn't open. Try again.";
const NO_PLACE: &str = "The Toy Pad has no place there.";
const LOAD_FAILED: &str = "RPCS3 couldn't put that figure on the Toy Pad. It may be on it already.";
const NOT_A_TAG: &str = "That file isn't a LEGO Dimensions figure RPCS3 can read.";
const MAKE_FAILED: &str = "RPCS3 couldn't make that figure. Try again.";
const MAKER_DIDNT_OPEN: &str = "RPCS3's figure maker didn't open. Try again.";
const NO_LIST: &str = "Couldn't read RPCS3's list of figures.";
const NOTHING_TO_MOVE: &str = "There's no figure there to move.";
const PLACE_TAKEN: &str = "Another figure is on that place. Take it off first.";
const MOVE_FAILED: &str = "RPCS3 didn't move that figure. Try again.";

/// The same words the menu and Cemu's Toy Pad give (toy_menu.rs,
/// `before_create`; cemu/toypad.rs).
const NO_VEHICLE: &str =
    "Vehicles and gadgets go on a Blank Tag. Put one on the centre pad and build the vehicle in the game.";

fn automation() -> Result<Qt, String> {
    Qt::new().ok_or_else(|| UNREACHABLE.to_string())
}

fn check(place: usize) -> Result<(), String> {
    if place < PLACES {
        Ok(())
    } else {
        Err(NO_PLACE.to_string())
    }
}

/// Gets the Toy Pad ready while a LEGO Dimensions game starts: RPCS3's
/// main window is hidden the moment it shows, and the manager is opened
/// behind it, so the first tag goes on as quickly as any other.
pub fn prepare(pid: u32) {
    std::thread::spawn(move || {
        let Some(main) = soon(START_WAIT, || main_window(pid).filter(|&window| visible(window))) else {
            return;
        };
        hide(main);
        let _turn = turn();
        let Ok(automation) = automation() else {
            return;
        };
        if let Ok(window) = manager(&automation, pid) {
            warm_up(&automation, pid, window);
        }
    });
}

/// Opens the manager as the Toy Pad menu opens, if the game's start didn't,
/// so the menu's first errand doesn't wait for it. Nothing when an errand
/// already has the turn, which opens it anyway.
pub fn ready(pid: u32) {
    let Some(_turn) = free_turn() else {
        return;
    };
    if showing(pid).into_iter().any(|window| title(window) == MANAGER) {
        return;
    }
    if let Ok(automation) = automation() {
        let _ = manager(&automation, pid);
    }
}

/// The manager, opened through the main window's menus if it isn't open,
/// and kept out of sight. RPCS3 makes one at a time (`get_dlg`, lines 610
/// to 616).
fn manager(automation: &Qt, pid: u32) -> Result<HWND, String> {
    if let Some(window) = showing(pid).into_iter().find(|&w| title(w) == MANAGER) {
        out_of_sight(window);
        return Ok(window);
    }
    let main = main_window(pid).ok_or(NOT_READY)?;
    hide(main);
    let menus = automation.window(main).ok_or(LOOKS_DIFFERENT)?;
    if !qt::choose_from_menus(automation, pid, &menus, &MENU) {
        return Err(LOOKS_DIFFERENT.to_string());
    }
    arrives(pid, MANAGER).ok_or_else(|| DIDNT_OPEN.to_string())
}

/// Opens one of Qt's file windows and closes it again, unseen, while the
/// game starts, as portal.rs does for the Skylanders Manager: Qt sets its
/// file windows up the first time one opens. Pressed with UI Automation's
/// own press, which leaves the keyboard alone. Cancel in Load's file window
/// leaves the place as it was (lines 708 to 712).
fn warm_up(automation: &Qt, pid: u32, window: HWND) {
    let Some(manager) = automation.window(window) else {
        return;
    };
    if place_button(automation, &manager, "Load", 0).ok().and_then(|load| qt::invoke(&load)).is_none() {
        return;
    }
    let Some(picker) = arrives(pid, OPEN_FIGURE) else {
        return;
    };
    let cancel = automation
        .window(picker)
        .and_then(|dialog| automation.all(&dialog, FILE_BUTTON).into_iter().find(|button| name(button) == "Cancel"));
    if cancel.is_some_and(|cancel| qt::invoke(&cancel).is_some()) {
        gone(picker);
    }
    give_back_to_game(pid);
}

fn place_button(automation: &Qt, manager: &IUIAutomationElement, label: &str, place: usize) -> Result<IUIAutomationElement, String> {
    automation
        .all(manager, PLACE_BUTTON)
        .into_iter()
        .filter(|button| name(button) == label)
        .nth(place)
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// Presses a button as the keyboard does (qt.rs, `press`), which Qt answers
/// at once, where UI Automation's own press lands a tenth of a second late.
fn press(button: &IUIAutomationElement, window: HWND) -> Result<(), String> {
    qt::press(button, window).ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// What each name box says, in place order, as RPCS3 put it.
fn boxes(automation: &Qt, window: HWND) -> Result<Vec<String>, String> {
    let manager = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    let said: Vec<String> = automation.all(&manager, PLACE_NAME).iter().map(value).collect();
    if said.len() == PLACES {
        Ok(said)
    } else {
        Err(LOOKS_DIFFERENT.to_string())
    }
}

/// Each place's tag as the menu knows it, empty where there is none. The
/// rest stays as RPCS3 says it, which isn't always what the tag is now.
/// RPCS3 names a tag once, as it goes on: from its list of minifigures,
/// else its list of vehicles and gadgets, else "Blank Tag" (lines 759 to
/// 778), so a tag whose number neither list has says "Blank Tag" too, and a
/// Blank Tag the game has since written a vehicle to says it until it goes
/// on again. A manager made again names only minifigures and calls every
/// other tag "Unknown Figure" (lines 634 to 645); Omoio keeps the manager
/// open for the whole game, so that is rare. A place it cleared holds
/// nothing in a manager made again (lines 646 to 649 and 683 to 691),
/// unlike one of the Infinity Manager's (infinity.rs, `FILLED`).
fn named(said: &[String]) -> Vec<String> {
    said.iter().map(|said| if said == NONE { String::new() } else { said.clone() }).collect()
}

/// Omoio's words for what RPCS3's message titled `title` says went wrong
/// with a file, `otherwise` for any but the one for a short file.
fn refusal(title: &str, otherwise: &'static str) -> &'static str {
    if title == TOO_SHORT {
        NOT_A_TAG
    } else {
        otherwise
    }
}

/// What the Toy Pad holds once RPCS3 has put a tag on, or why it didn't
/// when it put up a message instead, `failed` unless the message says the
/// file is too short. RPCS3's own words name the file's whole path, so they
/// stay behind. The read comes first: RPCS3 answers it only once it has
/// finished with the tag, as it does for the Skylanders Manager (portal.rs,
/// `settled`).
fn settled(automation: &Qt, pid: u32, window: HWND, failed: &'static str) -> Result<Vec<String>, String> {
    let said = boxes(automation, window)?;
    let message = showing(pid).into_iter().find(|&w| is_message(pid, w)).map(title);
    if dismiss_message(pid) {
        return Err(refusal(&message.unwrap_or_default(), failed).to_string());
    }
    Ok(named(&said))
}

/// Puts a tag's file in one of Qt's file windows and presses Enter, which
/// presses its Open or Save button, then waits for it to close.
fn choose_file(automation: &Qt, picker: HWND, file: &Path) -> Result<(), String> {
    let window = automation.window(picker).ok_or(LOOKS_DIFFERENT)?;
    let name_box = automation.first(&window, FILE_NAME).ok_or(LOOKS_DIFFERENT)?;
    qt::set_value(&name_box, &file.to_string_lossy()).ok_or(LOOKS_DIFFERENT)?;
    key(picker, VK_RETURN);
    if gone(picker) {
        Ok(())
    } else {
        key(picker, VK_ESCAPE);
        Err("RPCS3 didn't take the figure's file. Try again.".to_string())
    }
}

/// The tags on the Toy Pad, by place: 0, 3 and 4 on the left pad, 1 on the
/// centre one, 2, 5 and 6 on the right (core/dimensions.rs).
pub fn places(pid: u32) -> Result<Vec<String>, String> {
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    Ok(named(&boxes(&automation, window)?))
}

/// Puts the tag in `file` on the Toy Pad in `place`, and returns what the
/// Toy Pad holds afterwards. RPCS3 takes off what the place held first, and
/// only once it has read the new file (`load_figure_path`, lines 741 to
/// 759), so a file it can't open leaves the place as it was. It holds a
/// tag's file open against writers while it is on (`fs::lock`, line 743),
/// so a file already on the Toy Pad is turned down, as is one shorter than
/// a tag (lines 744 to 755).
pub fn load(pid: u32, place: usize, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    put_away(pid, &LEFTOVERS);
    let buttons = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    press(&place_button(&automation, &buttons, "Load", place)?, window)?;
    let picker = arrives(pid, OPEN_FIGURE).ok_or("RPCS3 didn't ask for the figure. Try again.")?;
    choose_file(&automation, picker, file)?;
    settled(&automation, pid, window, LOAD_FAILED)
}

/// Takes the tag in `place` off the Toy Pad, and returns what is left.
/// RPCS3's Clear saves the tag to its file and closes it as it goes
/// (`remove_figure`, Emu/Io/Dimensions.cpp lines 394 to 429), and does
/// nothing for a place that holds none (dimensions_dialog.cpp lines 683 to
/// 693).
pub fn remove(pid: u32, place: usize) -> Result<Vec<String>, String> {
    check(place)?;
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    let manager = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    let held = automation.all(&manager, PLACE_NAME).into_iter().nth(place).ok_or(LOOKS_DIFFERENT)?;
    if value(&held) != NONE {
        press(&place_button(&automation, &manager, "Clear", place)?, window)?;
        // The press is posted, and RPCS3 may answer a read before it gets to it.
        let _ = soon(WAIT, || (value(&held) == NONE).then_some(()));
    }
    Ok(named(&boxes(&automation, window)?))
}

/// Moves the tag in `from` to `to` with the manager's Move and its Figure
/// Mover, as a player does: the game sees the tag lifted, then put down in
/// its new place, its file going along with it (`move_figure`, lines 719
/// to 739; Emu/Io/Dimensions.cpp lines 470 to 494). `to` must be empty
/// unless it is `from`, which puts the tag back down where it was, as the
/// mover's "Pick up and Place" does. RPCS3 would take a tag in `to` off
/// first, saving it (Emu/Io/Dimensions.cpp lines 481 to 483), which the
/// player didn't ask for. Returns what the Toy Pad holds afterwards.
pub fn move_figure(pid: u32, from: usize, to: usize) -> Result<Vec<String>, String> {
    check(from)?;
    check(to)?;
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    put_away(pid, &LEFTOVERS);
    let said = boxes(&automation, window)?;
    // Move does nothing for a place that holds no tag (lines 663 to 669).
    if said[from] == NONE {
        return Err(NOTHING_TO_MOVE.to_string());
    }
    if to != from && said[to] != NONE {
        return Err(PLACE_TAKEN.to_string());
    }
    let buttons = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    press(&place_button(&automation, &buttons, "Move", from)?, window)?;
    let mover = arrives(pid, MOVER).ok_or(MOVE_FAILED)?;
    if let Err(problem) = put_down(&automation, mover, to) {
        put_away(pid, &LEFTOVERS);
        return Err(problem);
    }
    Ok(named(&boxes(&automation, window)?))
}

/// Picks `to` in the mover, once the game has had a moment to read the
/// lift (`LIFTED`), and waits for the mover to close.
fn put_down(automation: &Qt, mover: HWND, to: usize) -> Result<(), String> {
    let window = automation.window(mover).ok_or(LOOKS_DIFFERENT)?;
    let places = soon(WAIT, || Some(automation.all(&window, MOVER_BUTTON)).filter(|found| found.len() == PLACES))
        .ok_or(LOOKS_DIFFERENT)?;
    std::thread::sleep(LIFTED);
    press(&places[to], mover)?;
    if gone(mover) {
        Ok(())
    } else {
        Err(MOVE_FAILED.to_string())
    }
}

/// Has RPCS3's figure maker make `figure`, a minifigure or the Blank Tag,
/// into `file`, a file that isn't there yet. RPCS3 then puts it on the Toy
/// Pad in `place` itself (`create_figure`, lines 695 to 703). Returns what
/// the Toy Pad holds afterwards. A vehicle or gadget isn't made: RPCS3's
/// maker would encrypt its number as a minifigure's, where the game keeps a
/// vehicle's in plain, so the tag would read as no vehicle at all
/// (`create_blank_character` and `get_figure_id`, Emu/Io/Dimensions.cpp
/// lines 496 to 529 and 233 to 247). It is built in the game onto a Blank
/// Tag instead, which RPCS3 makes for the number 0 (the same file, lines
/// 522 to 526).
pub fn create(pid: u32, place: usize, figure: &ToyFigure, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    if matches!(Piece::of_id(figure.number), Piece::Vehicle) {
        return Err(NO_VEHICLE.to_string());
    }
    // Qt's save window would ask before replacing it, and RPCS3 would then
    // write over its start rather than make it anew (`fs::create`, line 470).
    if file.exists() {
        return Err("There's a figure file of that name already. Try again.".to_string());
    }
    // RPCS3 makes the file but not the folder it goes in (line 470).
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    put_away(pid, &LEFTOVERS);
    let buttons = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    press(&place_button(&automation, &buttons, "Create", place)?, window)?;
    let maker = arrives_over(pid, CREATOR, window).ok_or(MAKER_DIDNT_OPEN)?;
    if let Err(problem) = make(&automation, pid, maker, figure.number, file) {
        put_away(pid, &LEFTOVERS);
        return Err(problem);
    }
    // The manager names the new tag as it puts it on (lines 759 to 778), so
    // it can be read at once.
    settled(&automation, pid, window, MAKE_FAILED)
}

/// The maker's Figure Number box, the one box that is its own
/// (`MAKER_BOX`). Two would be the Infinity Manager's maker.
fn number_box(automation: &Qt, window: &IUIAutomationElement) -> Result<IUIAutomationElement, String> {
    soon(WAIT, || Some(automation.all(window, MAKER_BOX)).filter(|found| !found.is_empty()))
        .filter(|found| found.len() == 1)
        .and_then(|found| found.into_iter().next())
        .ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// Fills in the figure maker and has it save the tag. Its Figure Number box
/// is what it makes the tag from; its list only fills it in (lines 436 to
/// 448).
fn make(automation: &Qt, pid: u32, maker: HWND, number: u32, file: &Path) -> Result<(), String> {
    let window = automation.window(maker).ok_or(LOOKS_DIFFERENT)?;
    let number_box = number_box(automation, &window)?;
    qt::set_value(&number_box, &number.to_string()).ok_or(LOOKS_DIFFERENT)?;
    // Beside Cancel (lines 427 and 428).
    let make = automation
        .all(&window, MAKER_BUTTON)
        .into_iter()
        .find(|button| name(button) == "Create")
        .ok_or(LOOKS_DIFFERENT)?;
    press(&make, maker)?;
    let saver = arrives_over(pid, SAVE_FIGURE, maker).ok_or("RPCS3 didn't ask where to keep the figure. Try again.")?;
    choose_file(automation, saver, file)?;
    // RPCS3 writes the tag and closes the maker, or says why it couldn't and
    // leaves the maker open (lines 470 to 484).
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

/// Every tag RPCS3 knows, with its number: what its figure maker makes, the
/// minifigures and the Blank Tag, which is all the maker's list holds
/// (lines 389 to 402), and then its vehicles and gadgets (`TOKENS`), which
/// only name a tag the game wrote one to. The maker's list is read from the
/// maker of place 0, which lists the same whichever place opened it, since
/// it is given none (lines 380 and 698), and closed again with Escape,
/// which makes nothing (lines 698 to 702). The menu keeps what this gives
/// for each version of RPCS3 (toy_menu.rs, `listed`), since reading it
/// takes a few seconds.
pub fn figures(pid: u32) -> Result<Vec<ToyFigure>, String> {
    let _turn = turn();
    let automation = automation()?;
    let window = manager(&automation, pid)?;
    put_away(pid, &LEFTOVERS);
    let buttons = automation.window(window).ok_or(LOOKS_DIFFERENT)?;
    press(&place_button(&automation, &buttons, "Create", 0)?, window)?;
    let maker = arrives_over(pid, CREATOR, window).ok_or(MAKER_DIDNT_OPEN)?;
    let steps = read_maker(&automation, maker);
    key(maker, VK_ESCAPE);
    if !gone(maker) {
        put_away(pid, &LEFTOVERS);
        return Err(NO_LIST.to_string());
    }
    with_vehicles(kept(&steps?))
}

/// The maker's minifigures, then every vehicle and gadget. A maker that
/// showed none was not read, and gives no list.
fn with_vehicles(mut found: Vec<ToyFigure>) -> Result<Vec<ToyFigure>, String> {
    if found.is_empty() {
        return Err(NO_LIST.to_string());
    }
    found.extend(TOKENS.iter().map(|&(number, name)| ToyFigure { name: name.to_string(), number, series: None }));
    Ok(found)
}

/// Each step through the maker's list as (the list's text, Figure Number).
/// Each step fills the box for the minifigure it lands on (lines 436 to
/// 443). The list is sorted by name, with "--Unknown--" among them (lines
/// 399 and 402).
fn read_maker(automation: &Qt, maker: HWND) -> Result<Vec<(String, String)>, String> {
    let window = automation.window(maker).ok_or(LOOKS_DIFFERENT)?;
    let list = automation.first(&window, MAKER_LIST).ok_or(LOOKS_DIFFERENT)?;
    let number_box = number_box(automation, &window)?;
    let typing = automation.first(&window, MAKER_TYPING).ok_or(LOOKS_DIFFERENT)?;
    let read = || (value(&list), value(&number_box));
    qt::step_through(maker, &typing, STEP_WAIT, END_WAIT, read).ok_or_else(|| LOOKS_DIFFERENT.to_string())
}

/// The figures the steps showed, once for each number. The box opens on 0
/// (line 419) and is filled only by a step, so what the maker showed as it
/// opened may pair the name the list opens on with a number that isn't its
/// own: it counts last, and only for a number no step gave.
fn kept(steps: &[(String, String)]) -> Vec<ToyFigure> {
    let mut found = Vec::new();
    for step in steps.iter().skip(1).chain(steps.first()) {
        keep(&mut found, step);
    }
    found
}

/// Adds what one step showed, once for each number. "--Unknown--" leaves
/// the box as the figure before it had it (lines 399 and 439), so it is
/// left out.
fn keep(found: &mut Vec<ToyFigure>, (text, number): &(String, String)) {
    let Ok(number) = number.trim().parse::<u32>() else {
        return;
    };
    let text = text.trim();
    if text.is_empty() || text.starts_with("--") || found.iter().any(|known| known.number == number) {
        return;
    }
    found.push(ToyFigure { name: text.to_string(), number, series: None });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(text: &str, number: &str) -> (String, String) {
        (text.to_string(), number.to_string())
    }

    fn figure(name: &str, number: u32) -> ToyFigure {
        ToyFigure { name: name.to_string(), number, series: None }
    }

    #[test]
    fn a_place_reads_as_the_menu_names_it() {
        let said: Vec<String> =
            ["None", "Batman", "Blank Tag", "Unknown Figure", "Batmobile", "None", ""].map(String::from).to_vec();
        assert_eq!(named(&said), ["", "Batman", "Blank Tag", "Unknown Figure", "Batmobile", "", ""]);
    }

    #[test]
    fn each_figure_is_kept_once_and_the_unknown_line_not_at_all() {
        // RPCS3's list sorted by name, as the maker shows it (lines 23 to
        // 110 and 399 to 402), the box filled by each step.
        let steps = [
            step("Blank Tag", "0"),
            step("--Unknown--", "0"),
            step("Aquaman", "4"),
            step("Bad Cop", "5"),
            step("Bane", "6"),
            step("--Unknown--", "6"),
            step("Batman", "1"),
            step("Blank Tag", "0"),
            step("Supergirl Red Lantern", "769"),
            step("", "2"),
            step("Gandalf", ""),
            step("Bane", "6"),
        ];
        let found = kept(&steps);
        let names: Vec<(&str, u32)> = found.iter().map(|f| (f.name.as_str(), f.number)).collect();
        assert_eq!(
            names,
            [("Aquaman", 4), ("Bad Cop", 5), ("Bane", 6), ("Batman", 1), ("Blank Tag", 0), ("Supergirl Red Lantern", 769)]
        );
        assert!(found.iter().all(|f| f.series.is_none()), "the Toy Pad's figures have no series");
    }

    #[test]
    fn what_the_maker_showed_as_it_opened_gives_way_to_the_steps() {
        // Opened on a name the box doesn't hold the number of yet.
        let steps = [step("Aquaman", "0"), step("Bad Cop", "5"), step("Blank Tag", "0"), step("Aquaman", "4")];
        assert_eq!(kept(&steps), [figure("Bad Cop", 5), figure("Blank Tag", 0), figure("Aquaman", 4)]);
        // At the end of the list already, so no step lands on it.
        let steps = [step("Wyldstyle", "3"), step("Wonder Woman", "15"), step("Batman", "1")];
        assert_eq!(kept(&steps), [figure("Wonder Woman", 15), figure("Batman", 1), figure("Wyldstyle", 3)]);
        assert_eq!(kept(&[]), []);
    }

    #[test]
    fn every_vehicle_and_gadget_comes_after_the_makers_figures() {
        let listed = with_vehicles(vec![figure("Batman", 1), figure("Blank Tag", 0)]).unwrap();
        assert_eq!(listed.len(), 2 + 266);
        assert_eq!(listed[..2], [figure("Batman", 1), figure("Blank Tag", 0)]);
        assert!(listed[2..].iter().all(|f| matches!(Piece::of_id(f.number), Piece::Vehicle)));
        assert!(listed.contains(&figure("Batmobile", 1006)));
        assert!(listed.contains(&figure("Robot Retriever", 1265)));
        // Nothing read from the maker is no list, whatever Omoio knows.
        assert_eq!(with_vehicles(Vec::new()), Err(NO_LIST.to_string()));
    }

    #[test]
    fn the_vehicles_are_rpcs3s_ids_1000_to_1265_each_once() {
        let mut ids: Vec<u32> = TOKENS.iter().map(|&(id, _)| id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1000..=1265).collect::<Vec<u32>>());
        // A few lines of RPCS3's list, the order it keeps included
        // (dimensions_dialog.cpp lines 113, 119, 158 to 160 and 378).
        assert_eq!(TOKENS[0], (1000, "Police Car"));
        assert_eq!(TOKENS[6], (1006, "Batmobile"));
        assert_eq!(TOKENS[45..48], [(1045, "Homer's Car"), (1047, "The SubmaHomer"), (1046, "The Homercraft")]);
        assert_eq!(TOKENS[265], (1265, "Robot Retriever"));
        // The two Gyrospheres stay apart by RPCS3's own space.
        assert_eq!(TOKENS.iter().filter(|&&(_, name)| name.trim() == "Gyrosphere").count(), 2);
        assert_eq!(TOKENS.iter().filter(|&&(_, name)| name == "Gyrosphere").count(), 1);
    }

    #[test]
    fn a_file_too_short_is_told_apart_from_one_rpcs3_wont_open() {
        // RPCS3's two messages for a file it won't take (lines 746 and 753).
        assert_eq!(refusal("Failed to read the figure file!", LOAD_FAILED), NOT_A_TAG);
        assert_eq!(refusal("Failed to open the figure file!", LOAD_FAILED), LOAD_FAILED);
        assert_eq!(refusal("", MAKE_FAILED), MAKE_FAILED);
    }

    #[test]
    fn a_vehicle_or_a_place_the_toy_pad_lacks_is_turned_down_before_rpcs3_is_asked() {
        let file = Path::new("C:\\nowhere\\Batmobile.bin");
        // The first build of the Batmobile and the last gadget in both lists.
        assert_eq!(create(0, 1, &figure("Batmobile", 1006), file), Err(NO_VEHICLE.to_string()));
        assert_eq!(create(0, 1, &figure("Robot Retriever", 1265), file), Err(NO_VEHICLE.to_string()));
        assert_eq!(create(0, PLACES, &figure("Batman", 1), file), Err(NO_PLACE.to_string()));
        assert_eq!(load(0, PLACES, file), Err(NO_PLACE.to_string()));
        assert_eq!(remove(0, PLACES), Err(NO_PLACE.to_string()));
        assert_eq!(move_figure(0, PLACES, 0), Err(NO_PLACE.to_string()));
        assert_eq!(move_figure(0, 0, PLACES), Err(NO_PLACE.to_string()));
    }

    #[test]
    fn its_windows_are_the_dimensions_managers() {
        let titles = ["Dimensions Manager", "Figure Creator", "Figure Mover", "Select Dimensions File", "Create Figure File"];
        assert_eq!(WINDOWS, titles);
        for leftover in LEFTOVERS {
            assert!(WINDOWS.contains(&leftover), "{leftover}");
        }
        assert!(!LEFTOVERS.contains(&MANAGER), "the manager stays open for the whole game");
        assert!(!WINDOWS.contains(&TOO_SHORT), "RPCS3's message is one of its own");
    }

    /// Needs an RPCS3 running a LEGO Dimensions game with its main window
    /// (started by Omoio, or without `--no-gui`), a tag file and a folder
    /// to make one in, so it runs only by hand:
    /// `OMOIO_RPCS3_PID=<pid> OMOIO_TOYPAD_TAG=<file> cargo test toy_pad_on_a_running_rpcs3 -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn toy_pad_on_a_running_rpcs3() {
        use std::time::Instant;
        let pid: u32 = std::env::var("OMOIO_RPCS3_PID").unwrap().parse().unwrap();
        let tag = std::env::var("OMOIO_TOYPAD_TAG").unwrap();
        let before = places(pid).unwrap();
        assert_eq!(before.len(), PLACES);
        let started = Instant::now();
        let loaded = load(pid, 1, Path::new(&tag)).unwrap();
        println!("load took {:?}: {loaded:?}", started.elapsed());
        assert!(!loaded[1].is_empty(), "the centre pad holds the tag: {loaded:?}");
        // RPCS3 holds the file open while it is on, so it won't go on twice.
        assert!(load(pid, 0, Path::new(&tag)).is_err());
        let started = Instant::now();
        let moved = move_figure(pid, 1, 3).unwrap();
        println!("move took {:?}: {moved:?}", started.elapsed());
        assert!(moved[1].is_empty() && moved[3] == loaded[1], "the tag is on the left pad: {moved:?}");
        let again = move_figure(pid, 3, 3).unwrap();
        assert_eq!(again[3], loaded[1], "picked up and put back: {again:?}");
        let cleared = remove(pid, 3).unwrap();
        assert!(cleared[3].is_empty(), "the left pad is empty again: {cleared:?}");
        let started = Instant::now();
        let listed = figures(pid).unwrap();
        println!("{} figures in {:?}", listed.len(), started.elapsed());
        assert!(listed.contains(&figure("Batman", 1)), "{listed:?}");
        assert!(listed.contains(&figure("Blank Tag", 0)), "{listed:?}");
        let makes = listed.iter().filter(|f| f.number < 1000).count();
        assert_eq!(listed.len() - makes, TOKENS.len(), "the maker lists no vehicles: {listed:?}");
        // Beside the tag, in a small folder like Omoio's own: Qt's save
        // window reads the folder it opens in.
        for (made, name) in [(figure("Batman", 1), "Batman"), (figure("Blank Tag", 0), "Blank Tag")] {
            let file = Path::new(&tag).with_file_name(format!("omoio-made-{}-{}.bin", made.number, std::process::id()));
            let started = Instant::now();
            let after = create(pid, 1, &made, &file).unwrap();
            println!("create took {:?}: {after:?}", started.elapsed());
            assert_eq!(after[1], name);
            assert_eq!(std::fs::metadata(&file).unwrap().len(), 180);
            remove(pid, 1).unwrap();
            let _ = std::fs::remove_file(file);
        }
    }
}
