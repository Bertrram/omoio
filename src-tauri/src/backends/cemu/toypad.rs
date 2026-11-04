//! Figures on the Toy Pad of a LEGO Dimensions game in Cemu.
//!
//! The Toy Pad goes through the portal server of the build of Cemu made for
//! Omoio (portal.rs), in commands of its own that start with "toypad", as
//! the Infinity Base's start with "infinity" (infinity.rs): `toypad status`,
//! `load`, `remove`, `move`, `create` and `list` (its SkylanderPortalIPC.cpp,
//! branch infinity-toypad). Each is answered in the shape the portal's
//! commands are (portal_ipc.rs). Cemu's own Emulated USB Devices window is
//! never opened for the Toy Pad, so the trait's `ready_toy` is left doing
//! nothing, as it is for the base.
//!
//! A build without the Toy Pad's commands answers "error unknown command"
//! to each (`ProcessCommand`, SkylanderPortalIPC.cpp line 251, v2.6-2), and
//! v2.6-2 starts no server at all in a game without the Skylanders portal
//! (`portal::never_answered`; both read 4 November 2026). Either way the
//! menu says that it needs a newer Cemu.
//!
//! Omoio reads no tag. A minifigure's id is encrypted in its tag, and Cemu
//! decrypts it as it puts the tag on, as it must to emulate the pad
//! (`LoadFigure` and `GetFigureId`, nsyshid/Dimensions.cpp lines 620 to 639
//! and 1001 to 1017, v2.6-2). The server reports the id Cemu holds for each
//! place, which follows what the game writes to the tag (`WriteBlock`, lines
//! 1050 to 1078), so a Blank Tag the game wrote a vehicle to reports that
//! vehicle. The names here are the ones Cemu's two lists give those ids.

use super::portal;
use crate::backends::portal_ipc::{Failed, Server};
use crate::core::dimensions::{self, PadLight, Piece, PLACES};
use crate::core::toys::ToyFigure;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

const OLD_CEMU: &str = "The Toy Pad menu needs a newer version of Omoio's Cemu.";
const NOT_READY: &str = "Cemu's Toy Pad isn't ready yet. Try again in a moment.";
const NOT_ANSWERING: &str = "Cemu's Toy Pad isn't answering. Try again.";
const NO_TOY_PAD: &str = "Cemu started this game without its Toy Pad. Quit the game and start it again.";
const NO_PLACE: &str = "The Toy Pad has no place there.";
const NOT_A_TAG: &str = "That file isn't a LEGO Dimensions figure Cemu can read.";
const ON_ALREADY: &str = "That figure is on the Toy Pad already.";
const NOTHING_TO_MOVE: &str = "There's no figure there to move.";
const PLACE_TAKEN: &str = "Another figure is on that place. Take it off first.";
const LOAD_FAILED: &str = "Cemu couldn't put that figure on the Toy Pad.";
const REMOVE_FAILED: &str = "Cemu couldn't take that figure off the Toy Pad.";
const MOVE_FAILED: &str = "Cemu couldn't move that figure.";
const MAKE_FAILED: &str = "Cemu couldn't make that figure.";

/// Cemu's figure maker would make a vehicle or gadget as it makes a
/// minifigure, its id encrypted where the game keeps a vehicle's in plain
/// (`CreateFigure` against `GetFigureId`, Dimensions.cpp lines 707 to 749
/// and 1001 to 1017, v2.6-2), and the game most likely takes such a tag for
/// no vehicle at all. Its window offers the minifigures and the Blank Tag
/// alone (EmulatedUSBDeviceFrame.cpp lines 698 to 703), and the Blank Tag
/// is the one meant for "gadgets from the game" (Cemu PR 1371, merged 11
/// November 2024; all read 4 November 2026): the game writes the vehicle to
/// it when it is built with the tag on the centre pad.
const NO_VEHICLE: &str =
    "Vehicles and gadgets go on a Blank Tag. Put one on the centre pad and build the vehicle in the game.";

/// A tag whose id is in neither of Cemu's lists, named as RPCS3's
/// Dimensions Manager names one (rpcs3qt/dimensions_dialog.cpp line 643,
/// master b2e52d6, read 4 November 2026), so the menu has one name for it
/// whichever emulator runs the game. Cemu's window adds the id (`FindFigure`,
/// Dimensions.cpp lines 1152 to 1169, v2.6-2).
const UNKNOWN: &str = "Unknown Figure";

/// What a place holds: the id Cemu holds for its tag, or nothing.
type Place = Option<u32>;

/// Cemu's minifigures and then its vehicles and gadgets, as `toypad list
/// minis` and `toypad list tokens` gave them, and the Cemu that gave them.
/// The server reports a place by its tag's id alone.
static LIST: Mutex<Option<(u32, Vec<ToyFigure>)>> = Mutex::new(None);

/// The server of the Cemu started as `pid`, or why there is none to ask.
fn server(pid: u32) -> Result<Server, String> {
    match portal::answered(pid) {
        Some(server) => Ok(server),
        None if portal::never_answered(pid) => Err(OLD_CEMU.to_string()),
        None => Err(NOT_READY.to_string()),
    }
}

fn check(place: usize) -> Result<(), String> {
    if place < PLACES {
        Ok(())
    } else {
        Err(NO_PLACE.to_string())
    }
}

/// What a failed command means for the person at the menu, from the
/// server's own reasons, with `otherwise` for a reason it gives no words
/// of its own here.
fn refusal(failed: Failed, otherwise: &str) -> String {
    let words = match &failed {
        Failed::Refused(why) => match why.as_str() {
            "unknown command" => OLD_CEMU,
            "toypad not emulated" => NO_TOY_PAD,
            "invalid place" => NO_PLACE,
            "not a toypad file" | "file too small" => NOT_A_TAG,
            "already on the toy pad" => ON_ALREADY,
            _ => otherwise,
        },
        Failed::Unreached | Failed::NoAnswer => NOT_ANSWERING,
    };
    words.to_string()
}

/// `toypad status`'s line for each of the seven places, "place<N> loaded
/// <id>" or "place<N> empty", in order, then a line for the light of each
/// of the three pads, "light1" to "light3" (centre, left, right), read as
/// dimensions.rs reads them, and then "ok". Anything else isn't the
/// server's answer.
fn status(lines: &[String]) -> Option<(Vec<Place>, [PadLight; 3])> {
    let (last, lines) = lines.split_last()?;
    if last != "ok" || lines.len() != PLACES + 3 {
        return None;
    }
    let (held, lit) = lines.split_at(PLACES);
    let places = held
        .iter()
        .enumerate()
        .map(|(place, line)| {
            let words: Vec<&str> = line.split(' ').collect();
            if words.first() != Some(&format!("place{place}").as_str()) {
                return None;
            }
            match words[1..] {
                ["empty"] => Some(None),
                ["loaded", id] => Some(Some(id.parse().ok()?)),
                _ => None,
            }
        })
        .collect::<Option<Vec<Place>>>()?;
    Some((places, dimensions::read_lights(lit)?))
}

/// `toypad list minis`'s or `toypad list tokens`'s line for each entry of
/// that list of Cemu's, "<id> <name>", and then "ok". The name comes last,
/// so its spaces need no quoting, and it is kept as Cemu's list has it: one
/// of the list's two Gyrospheres has a space in front of its name, which
/// keeps it apart from the other (`s_listTokens`, Dimensions.cpp lines 169
/// and 171, v2.6-2).
fn listed(lines: &[String]) -> Option<Vec<ToyFigure>> {
    let (last, entries) = lines.split_last()?;
    if last != "ok" || entries.is_empty() {
        return None;
    }
    entries
        .iter()
        .map(|line| {
            let (id, name) = line.split_once(' ')?;
            let number = id.parse().ok()?;
            (!name.trim().is_empty()).then(|| ToyFigure { name: name.to_string(), number, series: None })
        })
        .collect()
}

/// Each place's name, empty where it holds nothing.
fn named(held: &[Place], figures: &[ToyFigure]) -> Vec<String> {
    held.iter()
        .map(|place| match place {
            None => String::new(),
            Some(id) => figures
                .iter()
                .find(|figure| figure.number == *id)
                .map_or_else(|| UNKNOWN.to_string(), |figure| figure.name.clone()),
        })
        .collect()
}

/// Both of Cemu's lists, the minifigures first, the Blank Tag among them
/// as id 0, then the vehicles and gadgets (`s_listMinis` and
/// `s_listTokens`, Dimensions.cpp lines 25 to 112 and 114 to 380, v2.6-2).
fn ask_list(server: &Server) -> Result<Vec<ToyFigure>, String> {
    let mut figures = ask_one_list(server, "toypad list minis")?;
    figures.extend(ask_one_list(server, "toypad list tokens")?);
    Ok(figures)
}

fn ask_one_list(server: &Server, command: &str) -> Result<Vec<ToyFigure>, String> {
    let lines = server.ask(command).map_err(|failed| refusal(failed, NOT_ANSWERING))?;
    listed(&lines).ok_or_else(|| NOT_ANSWERING.to_string())
}

/// Cemu's lists, asked of it the first time a tag is to be named and kept
/// for as long as that Cemu runs, since a running Cemu's lists never change
/// (both are constant, Dimensions.cpp lines 25 and 114, v2.6-2).
fn known(server: &Server, pid: u32) -> Result<Vec<ToyFigure>, String> {
    if let Some((seen, figures)) = &*LIST.lock().unwrap_or_else(PoisonError::into_inner) {
        if *seen == pid {
            return Ok(figures.clone());
        }
    }
    let figures = ask_list(server)?;
    keep(pid, figures.clone());
    Ok(figures)
}

fn keep(pid: u32, figures: Vec<ToyFigure>) {
    *LIST.lock().unwrap_or_else(PoisonError::into_inner) = Some((pid, figures));
}

fn ask_status(server: &Server) -> Result<(Vec<Place>, [PadLight; 3]), String> {
    let lines = server.ask("toypad status").map_err(|failed| refusal(failed, NOT_ANSWERING))?;
    status(&lines).ok_or_else(|| NOT_ANSWERING.to_string())
}

/// What the Toy Pad holds, as the server reports it.
fn on_pad(server: &Server, pid: u32) -> Result<Vec<String>, String> {
    let (held, _) = ask_status(server)?;
    if held.iter().all(Option::is_none) {
        return Ok(vec![String::new(); PLACES]);
    }
    Ok(named(&held, &known(server, pid)?))
}

/// Puts the tag in `file` on in `place`. The server wants the place empty,
/// so one that holds a tag is emptied and the tag tried again, as Cemu's
/// window takes off what its place held before it puts a tag on
/// (`LoadMinifigPath`, EmulatedUSBDeviceFrame.cpp lines 620 to 644, v2.6-2).
/// It opens the file for writing, as the window does (line 622): the game
/// writes a vehicle to a Blank Tag and its rebuilds after, and Cemu saves
/// the whole tag into the file on every page the game writes (`WriteBlock`
/// and `Save`, Dimensions.cpp lines 1050 to 1078 and 1133 to 1140).
fn load_through(server: &Server, place: usize, file: &Path) -> Result<(), String> {
    let path = file.to_str().ok_or(LOAD_FAILED)?;
    let load = || server.ask(&format!("toypad load {place} {path}"));
    let loaded = match load() {
        Err(Failed::Refused(why)) if why == "place taken" => remove_through(server, place).and_then(|()| {
            load().map_err(|failed| refusal(failed, LOAD_FAILED))
        }),
        other => other.map_err(|failed| refusal(failed, LOAD_FAILED)),
    };
    loaded.map(drop)
}

/// Takes the tag in `place` off; a place already empty is as good.
fn remove_through(server: &Server, place: usize) -> Result<(), String> {
    match server.ask(&format!("toypad remove {place}")) {
        Ok(_) => Ok(()),
        Err(Failed::Refused(why)) if why == "place empty" => Ok(()),
        Err(failed) => Err(refusal(failed, REMOVE_FAILED)),
    }
}

/// Moves the tag in `from` to `to`, which the game sees as Cemu's window
/// lets it: lifted, and put down on its new place (`MoveMinifig`,
/// EmulatedUSBDeviceFrame.cpp lines 663 to 686; `TempRemove` and
/// `MoveFigure`, Dimensions.cpp lines 669 to 686 and 751 to 773, v2.6-2).
/// `to` the same as `from` lifts it and puts it back, the window's "Pick up
/// and Place". Cemu's own move takes off whatever was on the new place
/// without asking, and the server refuses rather than do that, so the
/// person at the menu is told and takes it off first.
fn move_through(server: &Server, from: usize, to: usize) -> Result<(), String> {
    match server.ask(&format!("toypad move {from} {to}")) {
        Ok(_) => Ok(()),
        Err(Failed::Refused(why)) if why == "place empty" => Err(NOTHING_TO_MOVE.to_string()),
        Err(Failed::Refused(why)) if why == "place taken" => Err(PLACE_TAKEN.to_string()),
        Err(failed) => Err(refusal(failed, MOVE_FAILED)),
    }
}

/// Has the server make a tag of `figure` into `file`, as Cemu's figure maker
/// would (`CreateFigure`, Dimensions.cpp lines 707 to 749, v2.6-2), then puts
/// it on in `place`, which the maker's window does itself once it has made
/// one (`CreateMinifig`, EmulatedUSBDeviceFrame.cpp lines 653 to 661). Only
/// a minifigure or a Blank Tag is made (`NO_VEHICLE`). The server won't make
/// it over a file that is there, and none is: the menu picks a name not
/// taken.
fn create_through(server: &Server, place: usize, figure: &ToyFigure, file: &Path) -> Result<(), String> {
    if Piece::of_id(figure.number) == Piece::Vehicle {
        return Err(NO_VEHICLE.to_string());
    }
    let path = file.to_str().ok_or(MAKE_FAILED)?;
    server
        .ask(&format!("toypad create {} {path}", figure.number))
        .map_err(|failed| refusal(failed, MAKE_FAILED))?;
    load_through(server, place, file)
}

/// The name on each of the Toy Pad's seven places, in place order, empty
/// where there is nothing.
pub fn places(pid: u32) -> Result<Vec<String>, String> {
    on_pad(&server(pid)?, pid)
}

/// Puts the tag in `file` on in `place`, and returns what the Toy Pad holds
/// afterwards.
pub fn load(pid: u32, place: usize, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    let server = server(pid)?;
    load_through(&server, place, file)?;
    on_pad(&server, pid)
}

/// Takes the tag in `place` off, and returns what is left.
pub fn remove(pid: u32, place: usize) -> Result<Vec<String>, String> {
    check(place)?;
    let server = server(pid)?;
    remove_through(&server, place)?;
    on_pad(&server, pid)
}

/// Moves the tag in `from` to the empty place `to`, and returns what the
/// Toy Pad holds afterwards.
pub fn move_figure(pid: u32, from: usize, to: usize) -> Result<Vec<String>, String> {
    check(from)?;
    check(to)?;
    let server = server(pid)?;
    move_through(&server, from, to)?;
    on_pad(&server, pid)
}

/// Every minifigure Cemu's figure maker makes, the Blank Tag among them,
/// and then every vehicle and gadget in its list, which name what the game
/// writes to a Blank Tag.
pub fn list(pid: u32) -> Result<Vec<ToyFigure>, String> {
    let figures = ask_list(&server(pid)?)?;
    keep(pid, figures.clone());
    Ok(figures)
}

/// Makes a tag of `figure`, a minifigure or a Blank Tag, into `file` and
/// puts it on in `place`. Returns what the Toy Pad holds afterwards.
pub fn create(pid: u32, place: usize, figure: &ToyFigure, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    let server = server(pid)?;
    create_through(&server, place, figure, file)?;
    on_pad(&server, pid)
}

/// The light the game last asked of each pad, centre, left and right, as
/// Cemu's Toy Pad kept it. `None` when the server can't be asked, or is a
/// build that keeps no lights.
pub fn lights(pid: u32) -> Option<[PadLight; 3]> {
    ask_status(&server(pid).ok()?).ok().map(|(_, lights)| lights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    /// A few lines of each of Cemu's lists, as `toypad list minis` and
    /// `toypad list tokens` send them (`s_listMinis`, Dimensions.cpp lines
    /// 26 to 28 and 112, and `s_listTokens`, lines 121, 169, 171 and 241,
    /// v2.6-2).
    const MINIS_ANSWER: &str = "0 Blank Tag\n1 Batman\n2 Gandalf\n770 Unknown 770\nok\n";
    const TOKENS_ANSWER: &str = "1006 Batmobile\n1054 Gyrosphere\n1056  Gyrosphere\n1126 Unknown\nok\n";

    /// The light lines of a status: the centre pad red at once, the left
    /// fading to blue from white for ever, and the right never lit.
    const LIGHTS: &str = "light1 solid 255 0 0 0 0 0 0 0 0 3\n\
                          light2 fade 0 0 255 255 255 255 12 0 0 7\n\
                          light3 none 0 0 0 0 0 0 0 0 0 0\n";

    /// A Blank Tag on the centre pad, Batman on the left pad's first place,
    /// a Blank Tag the game wrote a Batmobile to on the right pad, and a tag
    /// whose id neither list has.
    fn status_answer() -> String {
        format!(
            "place0 loaded 1\nplace1 loaded 0\nplace2 empty\nplace3 empty\n\
             place4 empty\nplace5 loaded 1006\nplace6 loaded 3735928559\n{LIGHTS}ok\n"
        )
    }

    #[test]
    fn the_status_reads_place_by_place_and_pad_by_pad() {
        let (held, lights) = status(&lines(&status_answer())).unwrap();
        assert_eq!(held, [Some(1), Some(0), None, None, None, Some(1006), Some(0xDEAD_BEEF)]);
        assert_eq!(lights[0].mode, "solid");
        assert_eq!(lights[0].color, [255, 0, 0]);
        assert_eq!(lights[0].changes, 3);
        assert_eq!(lights[1].mode, "fade");
        assert_eq!((lights[1].color, lights[1].from), ([0, 0, 255], [255, 255, 255]));
        assert_eq!((lights[1].time, lights[1].count, lights[1].changes), (12, 0, 7));
        assert_eq!(lights[2].mode, "none");
        // A pad's light is known by its number wherever its line stands, as
        // dimensions.rs reads them.
        let turned = status_answer()
            .replace("light1 ", "lightX ")
            .replace("light2 ", "light1 ")
            .replace("lightX ", "light2 ");
        let (_, lights) = status(&lines(&turned)).unwrap();
        assert_eq!((lights[0].mode.as_str(), lights[1].mode.as_str()), ("fade", "solid"));
    }

    #[test]
    fn anything_but_the_toy_pads_status_is_no_answer() {
        let answer = status_answer();
        assert_eq!(status(&lines("ok\n")), None);
        // Six places, or eight.
        assert_eq!(status(&lines(&answer.replace("place6 loaded 3735928559\n", ""))), None);
        assert_eq!(status(&lines(&answer.replace("light1", "place7 empty\nlight1"))), None);
        // No lights, a pad's twice, a pad's light that isn't one, or a light
        // line among the places.
        assert_eq!(status(&lines(&answer.replace(LIGHTS, ""))), None);
        assert_eq!(status(&lines(&answer.replace("light1", "light9"))), None);
        assert_eq!(status(&lines(&answer.replace("light3", "light2"))), None);
        assert_eq!(status(&lines(&answer.replace("light3 none", "light3 glow"))), None);
        let centre = "light1 solid 255 0 0 0 0 0 0 0 0 3\n";
        let early = answer.replace(centre, "").replace("place6", &format!("{centre}place6"));
        assert_eq!(status(&lines(&early)), None);
        // The places out of order, a place with an id and a variant, an id
        // that isn't a number, and the base's or the portal's status.
        assert_eq!(status(&lines(&answer.replace("place1 ", "place2 "))), None);
        assert_eq!(status(&lines(&answer.replace("loaded 1006", "loaded 1006 0"))), None);
        assert_eq!(status(&lines(&answer.replace("loaded 1006", "loaded Batmobile"))), None);
        assert_eq!(status(&lines(&answer.replace("place", "slot"))), None);
        assert_eq!(status(&lines(&answer.replace("ok\n", "done\n"))), None);
    }

    #[test]
    fn the_lists_read_each_entry_with_its_whole_name() {
        let minis = listed(&lines(MINIS_ANSWER)).unwrap();
        assert_eq!(minis[0], ToyFigure { name: "Blank Tag".into(), number: 0, series: None });
        assert_eq!(minis[3], ToyFigure { name: "Unknown 770".into(), number: 770, series: None });
        let tokens = listed(&lines(TOKENS_ANSWER)).unwrap();
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].name, "Batmobile");
        assert_eq!((tokens[2].number, tokens[2].name.as_str()), (1056, " Gyrosphere"), "as Cemu's list has it");

        assert_eq!(listed(&lines("ok\n")), None);
        assert_eq!(listed(&lines("1006\nok\n")), None);
        assert_eq!(listed(&lines("1006 \nok\n")), None);
        assert_eq!(listed(&lines("Batmobile 1006\nok\n")), None);
        assert_eq!(listed(&lines("figure 1000001 1 Mr. Incredible\nok\n")), None, "the base's list");
        assert_eq!(listed(&lines(&TOKENS_ANSWER.replace("ok\n", ""))), None);
    }

    #[test]
    fn a_place_is_named_from_cemus_lists() {
        let (held, _) = status(&lines(&status_answer())).unwrap();
        let mut figures = listed(&lines(MINIS_ANSWER)).unwrap();
        figures.extend(listed(&lines(TOKENS_ANSWER)).unwrap());
        let names = named(&held, &figures);
        assert_eq!(names, ["Batman", "Blank Tag", "", "", "", "Batmobile", "Unknown Figure"]);
    }

    #[test]
    fn what_the_server_turns_down_is_said_plainly() {
        let refused = |why: &str| refusal(Failed::Refused(why.to_string()), LOAD_FAILED);
        assert_eq!(refused("unknown command"), "The Toy Pad menu needs a newer version of Omoio's Cemu.");
        assert_eq!(refused("toypad not emulated"), NO_TOY_PAD);
        assert_eq!(refused("invalid place"), NO_PLACE);
        assert_eq!(refused("not a toypad file"), NOT_A_TAG);
        assert_eq!(refused("file too small"), NOT_A_TAG);
        assert_eq!(refused("already on the toy pad"), ON_ALREADY);
        for other in ["cannot open file", "path must be absolute", "place taken", "invalid id", "file exists"] {
            assert_eq!(refused(other), LOAD_FAILED, "{other}");
        }
        assert_eq!(refusal(Failed::Unreached, LOAD_FAILED), NOT_ANSWERING);
        assert_eq!(refusal(Failed::NoAnswer, MAKE_FAILED), NOT_ANSWERING);
        assert_eq!(check(7), Err(NO_PLACE.to_string()));
        assert_eq!(check(6), Ok(()));
        for words in [OLD_CEMU, NOT_READY, NOT_ANSWERING, NO_TOY_PAD, NO_PLACE, NOT_A_TAG, ON_ALREADY, NOTHING_TO_MOVE]
            .into_iter()
            .chain([PLACE_TAKEN, LOAD_FAILED, REMOVE_FAILED, MOVE_FAILED, MAKE_FAILED, NO_VEHICLE])
        {
            assert!(words.ends_with('.') && !words.contains('!') && !words.contains('\u{2014}'), "{words}");
        }
    }

    /// The Toy Pad of a server like the fork's: the id on each place, and
    /// the id of each file it made.
    struct Pad {
        places: [Place; PLACES],
        made: HashMap<String, u32>,
    }

    /// A server that answers as the build of Cemu made for Omoio is to
    /// answer the Toy Pad's commands, with Cemu's lists cut down to
    /// `MINIS_ANSWER` and `TOKENS_ANSWER` and the lights at `LIGHTS`. A tag
    /// it made is known by the id it was made with, and any other file of
    /// 180 bytes reads as a Blank Tag. "write <place> <id>" stands in for
    /// the game writing a vehicle to a tag. It takes `connections`
    /// commands, one to a connection, and each line it read comes back
    /// through `heard`.
    fn fork_like(connections: usize) -> (Server, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tell, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let mut pad = Pad { places: [None; PLACES], made: HashMap::new() };
            for _ in 0..connections {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let line = line.trim_end().to_string();
                let answer = fork_answer(&mut pad, &line);
                let _ = tell.send(line);
                (&stream).write_all(answer.as_bytes()).unwrap();
            }
        });
        (Server::new(port, portal::SLOTS), heard)
    }

    fn fork_answer(pad: &mut Pad, line: &str) -> String {
        if let Some(rest) = line.strip_prefix("write ") {
            let (place, id) = rest.split_once(' ').unwrap();
            pad.places[place.parse::<usize>().unwrap()] = Some(id.parse().unwrap());
            return "ok\n".to_string();
        }
        let Some(rest) = line.strip_prefix("toypad ") else {
            return "error unknown command\n".to_string();
        };
        let words: Vec<&str> = rest.splitn(3, ' ').collect();
        let place = |word: &str| word.parse::<usize>().ok().filter(|&place| place < PLACES);
        match words[..] {
            ["status"] => {
                let mut text: String = pad
                    .places
                    .iter()
                    .enumerate()
                    .map(|(place, held)| match held {
                        Some(id) => format!("place{place} loaded {id}\n"),
                        None => format!("place{place} empty\n"),
                    })
                    .collect();
                text.push_str(LIGHTS);
                text.push_str("ok\n");
                text
            }
            ["list", "minis"] => MINIS_ANSWER.to_string(),
            ["list", "tokens"] => TOKENS_ANSWER.to_string(),
            ["load", at, path] => {
                let Some(at) = place(at) else {
                    return "error invalid place\n".to_string();
                };
                if std::fs::metadata(path).map(|file| file.len()).ok() != Some(180) {
                    return "error not a toypad file\n".to_string();
                }
                if pad.places[at].is_some() {
                    return "error place taken\n".to_string();
                }
                let id = pad.made.get(path).copied().unwrap_or(0);
                pad.places[at] = Some(id);
                format!("ok {id}\n")
            }
            ["remove", at] => match place(at).map(|at| pad.places[at].take()) {
                Some(Some(_)) => "ok\n".to_string(),
                Some(None) => "error place empty\n".to_string(),
                None => "error invalid place\n".to_string(),
            },
            ["move", from, to] => {
                let (Some(from), Some(to)) = (place(from), place(to)) else {
                    return "error invalid place\n".to_string();
                };
                if pad.places[from].is_none() {
                    return "error place empty\n".to_string();
                }
                if to != from && pad.places[to].is_some() {
                    return "error place taken\n".to_string();
                }
                pad.places[to] = pad.places[from].take();
                "ok\n".to_string()
            }
            ["create", id, path] => {
                if Path::new(path).exists() {
                    return "error file exists\n".to_string();
                }
                std::fs::write(path, [0u8; 180]).unwrap();
                pad.made.insert(path.to_string(), id.parse().unwrap());
                "ok\n".to_string()
            }
            _ => "error unknown command\n".to_string(),
        }
    }

    /// Answers every command as a build without the Toy Pad's commands does.
    fn older(connections: usize) -> Server {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for _ in 0..connections {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                (&stream).write_all(b"error unknown command\n").unwrap();
            }
        });
        Server::new(port, portal::SLOTS)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-toypad-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn figures_go_on_off_and_across_the_toy_pad_through_a_server_like_the_forks() {
        let dir = scratch("errands");
        let (batman, blank, brought, notes) =
            (dir.join("Batman.bin"), dir.join("Blank Tag.bin"), dir.join("Brought.bin"), dir.join("notes.txt"));
        std::fs::write(&brought, [0u8; 180]).unwrap();
        std::fs::write(&notes, "not a tag").unwrap();
        // A pid of its own, so no other test's lists are taken for this Cemu's.
        let pid = 4_100_001;
        let (server, heard) = fork_like(23);
        let batman_figure = ToyFigure { name: "Batman".into(), number: 1, series: None };
        let blank_figure = ToyFigure { name: "Blank Tag".into(), number: 0, series: None };

        assert_eq!(on_pad(&server, pid).unwrap(), vec![String::new(); 7], "an empty pad wants no list");
        create_through(&server, 1, &blank_figure, &blank).unwrap();
        create_through(&server, 0, &batman_figure, &batman).unwrap();
        let held = on_pad(&server, pid).unwrap();
        assert_eq!((held[0].as_str(), held[1].as_str()), ("Batman", "Blank Tag"));
        // The game builds the Batmobile on the Blank Tag on the centre pad.
        server.ask("write 1 1006").unwrap();
        assert_eq!(on_pad(&server, pid).unwrap()[1], "Batmobile", "the lists were asked for once");
        // Onto a place that holds a tag: that one comes off first.
        load_through(&server, 0, &brought).unwrap();
        assert_eq!(load_through(&server, 2, &notes), Err(NOT_A_TAG.to_string()));
        move_through(&server, 1, 5).unwrap();
        assert_eq!(move_through(&server, 0, 5), Err(PLACE_TAKEN.to_string()));
        assert_eq!(move_through(&server, 3, 4), Err(NOTHING_TO_MOVE.to_string()));
        move_through(&server, 5, 5).unwrap();
        remove_through(&server, 0).unwrap();
        remove_through(&server, 0).unwrap();
        let after = on_pad(&server, pid).unwrap();
        assert_eq!(after, ["", "", "", "", "", "Batmobile", ""]);
        let made_again = create_through(&server, 3, &batman_figure, &batman);
        assert_eq!(made_again, Err(MAKE_FAILED.to_string()), "not made over a file that is there");
        assert_eq!(ask_status(&server).unwrap().1[1].mode, "fade");

        let heard: Vec<String> = heard.iter().take(23).collect();
        let path = |file: &Path| file.to_str().unwrap().to_string();
        let said = [
            "toypad status".to_string(),
            format!("toypad create 0 {}", path(&blank)),
            format!("toypad load 1 {}", path(&blank)),
            format!("toypad create 1 {}", path(&batman)),
            format!("toypad load 0 {}", path(&batman)),
            "toypad status".to_string(),
            "toypad list minis".to_string(),
            "toypad list tokens".to_string(),
            "write 1 1006".to_string(),
            "toypad status".to_string(),
            format!("toypad load 0 {}", path(&brought)),
            "toypad remove 0".to_string(),
            format!("toypad load 0 {}", path(&brought)),
            format!("toypad load 2 {}", path(&notes)),
            "toypad move 1 5".to_string(),
            "toypad move 0 5".to_string(),
            "toypad move 3 4".to_string(),
            "toypad move 5 5".to_string(),
            "toypad remove 0".to_string(),
            "toypad remove 0".to_string(),
            "toypad status".to_string(),
            format!("toypad create 1 {}", path(&batman)),
            "toypad status".to_string(),
        ];
        assert_eq!(heard, said);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_lists_come_back_as_one_minifigures_first() {
        let (server, heard) = fork_like(2);
        let figures = ask_list(&server).unwrap();
        assert_eq!(figures.len(), 8);
        assert_eq!(figures[0].name, "Blank Tag");
        assert_eq!(figures[4].name, "Batmobile");
        assert!(figures.iter().all(|figure| figure.series.is_none()));
        assert_eq!(heard.iter().take(2).collect::<Vec<_>>(), ["toypad list minis", "toypad list tokens"]);
    }

    #[test]
    fn an_older_cemu_is_told_apart_by_its_answer() {
        let dir = scratch("old");
        let file = dir.join("Batman.bin");
        let figure = ToyFigure { name: "Batman".into(), number: 1, series: None };
        let server = older(6);
        assert_eq!(on_pad(&server, 4_100_002), Err(OLD_CEMU.to_string()));
        assert_eq!(load_through(&server, 0, &file), Err(OLD_CEMU.to_string()));
        assert_eq!(remove_through(&server, 0), Err(OLD_CEMU.to_string()));
        assert_eq!(move_through(&server, 0, 3), Err(OLD_CEMU.to_string()));
        assert_eq!(ask_list(&server), Err(OLD_CEMU.to_string()));
        assert_eq!(create_through(&server, 0, &figure, &file), Err(OLD_CEMU.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_vehicle_isnt_sent_to_be_made() {
        let server = older(0);
        let batmobile = ToyFigure { name: "Batmobile".into(), number: 1006, series: None };
        assert_eq!(create_through(&server, 1, &batmobile, Path::new("C:\\Batmobile.bin")), Err(NO_VEHICLE.to_string()));
    }

    #[test]
    fn a_place_off_the_toy_pad_is_never_sent() {
        assert_eq!(load(4_100_003, 7, Path::new("C:\\Batman.bin")), Err(NO_PLACE.to_string()));
        assert_eq!(remove(4_100_003, 9), Err(NO_PLACE.to_string()));
        assert_eq!(move_figure(4_100_003, 0, 7), Err(NO_PLACE.to_string()));
        assert_eq!(move_figure(4_100_003, 7, 0), Err(NO_PLACE.to_string()));
        let figure = ToyFigure { name: "Batman".into(), number: 1, series: None };
        assert_eq!(create(4_100_003, 8, &figure, Path::new("C:\\Batman.bin")), Err(NO_PLACE.to_string()));
    }

    #[test]
    fn a_cemu_whose_server_hasnt_answered_is_asked_nothing() {
        // No `portal::prepare` saw this Cemu start.
        let pid = 4_100_004;
        assert_eq!(places(pid), Err(NOT_READY.to_string()));
        assert_eq!(move_figure(pid, 0, 3), Err(NOT_READY.to_string()));
        assert_eq!(lights(pid), None);
    }
}
