//! Figures on the Infinity Base of a Disney Infinity game in Cemu.
//!
//! The base goes through the portal server of the build of Cemu made for
//! Omoio (portal.rs), in commands of its own that start with "infinity", so
//! none can be taken for a Skylanders one: `infinity status`, `load`,
//! `remove`, `clear`, `create` and `list` (its SkylanderPortalIPC.cpp,
//! `HandleInfinity`, branch infinity-toypad). Each is answered in the shape
//! the portal's commands are (portal_ipc.rs). Cemu's own Emulated USB
//! Devices window is never opened for the base: the trait's `ready_toy`,
//! which does nothing, is left as it is, and the portal's `ready`, which
//! opens that window, is for a Skylanders game only (session.rs).
//!
//! A build without the base's commands answers "error unknown command" to
//! each (`ProcessCommand`, SkylanderPortalIPC.cpp line 251, v2.6-2), and
//! v2.6-2 starts no server at all in a game without the Skylanders portal
//! (`portal::never_answered`; both read 4 November 2026). Either way the
//! menu says that it needs a newer Cemu.
//!
//! Omoio reads no figure file for the base: a figure's number is encrypted
//! in it (`LoadFigure`, Infinity.cpp lines 856 to 878, v2.6-2). The numbers
//! here are the ones Cemu reads as it puts a figure on, and the names the
//! ones its figure maker's list gives them.

use super::portal;
use crate::backends::portal_ipc::{Failed, Server};
use crate::core::infinity::PLACES;
use crate::core::toys::ToyFigure;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

const OLD_CEMU: &str = "The Infinity Base menu needs a newer version of Omoio's Cemu.";
const NOT_READY: &str = "Cemu's base isn't ready yet. Try again in a moment.";
const NOT_ANSWERING: &str = "Cemu's base isn't answering. Try again.";
const NO_BASE: &str = "Cemu started this game without its Infinity Base. Quit the game and start it again.";
const NO_PLACE: &str = "The base has no place there.";
const NOT_A_FIGURE: &str = "That file isn't a Disney Infinity figure Cemu can read.";
const ON_ALREADY: &str = "That figure is on the base already.";
const LOAD_FAILED: &str = "Cemu couldn't put that figure on the base.";
const REMOVE_FAILED: &str = "Cemu couldn't take that figure off the base.";
const MAKE_FAILED: &str = "Cemu couldn't make that figure.";

/// A figure whose number isn't in Cemu's list, named as RPCS3 and Dolphin
/// name one in their own Infinity windows (RPCS3's
/// `rpcs3qt/infinity_dialog.cpp` line 887, master b2e52d6; Dolphin's
/// `FindFigure`, `Core/IOS/USB/Emulated/Infinity.cpp` line 750, 2609a; read
/// 4 November 2026), so the menu has one name for it whichever emulator
/// runs the game. Cemu's window adds the number (`FindFigure`, Infinity.cpp
/// lines 802 to 812, v2.6-2).
const UNKNOWN: &str = "Unknown Figure";

/// What a place holds: the figure's number and its series, as the server
/// reports them, or nothing.
type Place = Option<(u32, u8)>;

/// Cemu's figures as `infinity list` gave them, and the Cemu that gave
/// them. The server reports a place by its figure's number alone.
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
            "base not emulated" => NO_BASE,
            "invalid place" => NO_PLACE,
            "not an infinity file" | "file too small" => NOT_A_FIGURE,
            "already on the base" => ON_ALREADY,
            _ => otherwise,
        },
        Failed::Unreached | Failed::NoAnswer => NOT_ANSWERING,
    };
    words.to_string()
}

/// `infinity status`'s line for each of the nine places, "place<N> loaded
/// <number> <series>" or "place<N> empty", in order and then "ok". The
/// series is the one Cemu's list gives the number, 0 for a number it
/// doesn't have. Anything else isn't the server's answer.
fn status_places(lines: &[String]) -> Option<Vec<Place>> {
    let (last, held) = lines.split_last()?;
    if last != "ok" || held.len() != PLACES {
        return None;
    }
    held.iter()
        .enumerate()
        .map(|(place, line)| {
            let words: Vec<&str> = line.split(' ').collect();
            if words.first() != Some(&format!("place{place}").as_str()) {
                return None;
            }
            match words[1..] {
                ["empty"] => Some(None),
                ["loaded", number, series] => Some(Some((number.parse().ok()?, series.parse().ok()?))),
                _ => None,
            }
        })
        .collect()
}

/// `infinity list`'s line for each figure Cemu's figure maker makes,
/// "figure <number> <series> <name>", and then "ok". The name comes last,
/// so the spaces, colons and apostrophes in it need no quoting.
fn listed(lines: &[String]) -> Option<Vec<ToyFigure>> {
    let (last, figures) = lines.split_last()?;
    if last != "ok" || figures.is_empty() {
        return None;
    }
    figures
        .iter()
        .map(|line| {
            let mut words = line.strip_prefix("figure ")?.splitn(3, ' ');
            let number = words.next()?.parse().ok()?;
            let series = words.next()?.parse().ok()?;
            let name = words.next().filter(|name| !name.trim().is_empty())?;
            Some(ToyFigure { name: name.to_string(), number, series: Some(series) })
        })
        .collect()
}

/// Each place's name, empty where it holds nothing.
fn named(held: &[Place], figures: &[ToyFigure]) -> Vec<String> {
    held.iter()
        .map(|place| match place {
            None => String::new(),
            Some((number, _)) => figures
                .iter()
                .find(|figure| figure.number == *number)
                .map_or_else(|| UNKNOWN.to_string(), |figure| figure.name.clone()),
        })
        .collect()
}

fn ask_list(server: &Server) -> Result<Vec<ToyFigure>, String> {
    let lines = server.ask("infinity list").map_err(|failed| refusal(failed, NOT_ANSWERING))?;
    listed(&lines).ok_or_else(|| NOT_ANSWERING.to_string())
}

/// Cemu's list, asked of it the first time a figure is to be named and
/// kept for as long as that Cemu runs, since a running Cemu's list never
/// changes (`s_listFigures`, Infinity.cpp lines 21 to 343, v2.6-2).
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

/// What the base holds, as the server reports it.
fn on_base(server: &Server, pid: u32) -> Result<Vec<String>, String> {
    let lines = server.ask("infinity status").map_err(|failed| refusal(failed, NOT_ANSWERING))?;
    let held = status_places(&lines).ok_or_else(|| NOT_ANSWERING.to_string())?;
    if held.iter().all(Option::is_none) {
        return Ok(vec![String::new(); PLACES]);
    }
    Ok(named(&held, &known(server, pid)?))
}

/// Puts the figure in `file` on in `place`. The server takes off what the
/// place held only once it has read the new file, as Cemu's window does
/// (`LoadFigurePath`, EmulatedUSBDeviceFrame.cpp lines 473 to 494, v2.6-2),
/// so a file that won't go on leaves the place as it was. It opens the file
/// for writing, as the window does (line 475): the game keeps its progress
/// on the figure, and Cemu saves each block the game writes into the file
/// (`WriteBlock`, Infinity.cpp lines 761 to 781).
fn load_through(server: &Server, place: usize, file: &Path) -> Result<(), String> {
    let path = file.to_str().ok_or(LOAD_FAILED)?;
    server
        .ask(&format!("infinity load {place} {path}"))
        .map(drop)
        .map_err(|failed| refusal(failed, LOAD_FAILED))
}

/// Takes the figure in `place` off; a place already empty is as good.
fn remove_through(server: &Server, place: usize) -> Result<(), String> {
    match server.ask(&format!("infinity remove {place}")) {
        Ok(_) => Ok(()),
        Err(Failed::Refused(why)) if why == "place empty" => Ok(()),
        Err(failed) => Err(refusal(failed, REMOVE_FAILED)),
    }
}

/// Has the server make a figure of `figure` into `file`, as Cemu's figure
/// maker would (`CreateFigure`, Infinity.cpp lines 957 to 1022, v2.6-2),
/// then puts it on in `place`, which the maker's window does itself once
/// it has made one (EmulatedUSBDeviceFrame.cpp lines 496 to 505). The
/// series goes with it as the window sends it, the one Cemu's list gives
/// the figure (lines 563 and 574), and Cemu dates the figure by it
/// (`GenerateBlankFigureData`, Infinity.cpp lines 1041 to 1077). The server
/// won't make it over a file that is there, and none is: the menu picks a
/// name not taken.
fn create_through(server: &Server, place: usize, figure: &ToyFigure, file: &Path) -> Result<(), String> {
    let series = figure.series.ok_or(MAKE_FAILED)?;
    let path = file.to_str().ok_or(MAKE_FAILED)?;
    server
        .ask(&format!("infinity create {} {series} {path}", figure.number))
        .map_err(|failed| refusal(failed, MAKE_FAILED))?;
    load_through(server, place, file)
}

/// The name in each of the base's nine places, in place order, empty where
/// there is nothing.
pub fn places(pid: u32) -> Result<Vec<String>, String> {
    on_base(&server(pid)?, pid)
}

/// Puts the figure in `file` on in `place`, and returns what the base holds
/// afterwards.
pub fn load(pid: u32, place: usize, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    let server = server(pid)?;
    load_through(&server, place, file)?;
    on_base(&server, pid)
}

/// Takes the figure in `place` off, and returns what is left.
pub fn remove(pid: u32, place: usize) -> Result<Vec<String>, String> {
    check(place)?;
    let server = server(pid)?;
    remove_through(&server, place)?;
    on_base(&server, pid)
}

/// Every figure Cemu's figure maker makes, with its number and series.
pub fn list(pid: u32) -> Result<Vec<ToyFigure>, String> {
    let figures = ask_list(&server(pid)?)?;
    keep(pid, figures.clone());
    Ok(figures)
}

/// Makes a figure of `figure` into `file` and puts it on in `place`.
/// Returns what the base holds afterwards.
pub fn create(pid: u32, place: usize, figure: &ToyFigure, file: &Path) -> Result<Vec<String>, String> {
    check(place)?;
    let server = server(pid)?;
    create_through(&server, place, figure, file)?;
    on_base(&server, pid)
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

    /// A few lines of Cemu's list, as `infinity list` sends them
    /// (`s_listFigures`, Infinity.cpp lines 22, 33, 42, 78, 156 and 243,
    /// v2.6-2).
    const LIST_ANSWER: &str = "figure 1000001 1 Mr. Incredible\n\
                               figure 1000012 1 Hector Barbossa\n\
                               figure 1000021 1 Sorcerer's Apprentice Mickey\n\
                               figure 1000118 2 Stitch\n\
                               figure 2000201 3 Star Wars: Twilight of the Republic Play Set\n\
                               figure 4000041 1 Hangin' Ten Stitch With Surfboard\n\
                               ok\n";

    /// Stitch on Player One, an unknown figure on Player Two and a play set
    /// on the hexagon.
    const STATUS_ANSWER: &str = "place0 loaded 2000201 3\nplace1 empty\nplace2 empty\n\
                                 place3 loaded 1000118 2\nplace4 empty\nplace5 empty\n\
                                 place6 loaded 1999999 0\nplace7 empty\nplace8 empty\nok\n";

    #[test]
    fn the_status_reads_place_by_place() {
        let held = status_places(&lines(STATUS_ANSWER)).unwrap();
        assert_eq!(held.len(), 9);
        assert_eq!(held[0], Some((2_000_201, 3)));
        assert_eq!(held[3], Some((1_000_118, 2)));
        // A number no u16 holds, and the series of one not in the list.
        assert_eq!(held[6], Some((1_999_999, 0)));
        assert_eq!(held.iter().filter(|place| place.is_none()).count(), 6);
    }

    #[test]
    fn anything_but_the_bases_status_is_no_answer() {
        assert_eq!(status_places(&lines("ok\n")), None);
        // Eight places, or ten.
        assert_eq!(status_places(&lines(&STATUS_ANSWER.replace("place8 empty\n", ""))), None);
        assert_eq!(status_places(&lines(&STATUS_ANSWER.replace("ok\n", "place9 empty\nok\n"))), None);
        // The portal's status, or the places out of order.
        assert_eq!(status_places(&lines(&STATUS_ANSWER.replace("place", "slot"))), None);
        assert_eq!(status_places(&lines(&STATUS_ANSWER.replace("place1 ", "place2 "))), None);
        assert_eq!(status_places(&lines(&STATUS_ANSWER.replace("1000118 2", "1000118"))), None);
        assert_eq!(status_places(&lines(&STATUS_ANSWER.replace("1000118 2", "1000118 300"))), None);
        assert_eq!(status_places(&lines(&STATUS_ANSWER.replace("ok\n", "done\n"))), None);
    }

    #[test]
    fn the_list_reads_each_figure_with_its_whole_name() {
        let figures = listed(&lines(LIST_ANSWER)).unwrap();
        assert_eq!(figures.len(), 6);
        assert_eq!(figures[0], ToyFigure { name: "Mr. Incredible".into(), number: 1_000_001, series: Some(1) });
        assert_eq!(figures[2].name, "Sorcerer's Apprentice Mickey");
        assert_eq!(figures[4].name, "Star Wars: Twilight of the Republic Play Set");
        assert_eq!(figures[4].series, Some(3));
        assert_eq!(figures[5].number, 4_000_041);

        assert_eq!(listed(&lines("ok\n")), None);
        assert_eq!(listed(&lines("figure 1000001 1\nok\n")), None);
        assert_eq!(listed(&lines("figure 1000001 one Mr. Incredible\nok\n")), None);
        assert_eq!(listed(&lines("slot0 empty\nok\n")), None);
        assert_eq!(listed(&lines(&LIST_ANSWER.replace("ok\n", ""))), None);
    }

    #[test]
    fn a_place_is_named_from_cemus_list() {
        let held = status_places(&lines(STATUS_ANSWER)).unwrap();
        let names = named(&held, &listed(&lines(LIST_ANSWER)).unwrap());
        assert_eq!(names[0], "Star Wars: Twilight of the Republic Play Set");
        assert_eq!(names[3], "Stitch");
        assert_eq!(names[6], "Unknown Figure");
        assert_eq!(names[1], "");
        assert_eq!(names.len(), 9);
    }

    #[test]
    fn what_the_server_turns_down_is_said_plainly() {
        let refused = |why: &str| refusal(Failed::Refused(why.to_string()), LOAD_FAILED);
        assert_eq!(refused("unknown command"), "The Infinity Base menu needs a newer version of Omoio's Cemu.");
        assert_eq!(refused("base not emulated"), NO_BASE);
        assert_eq!(refused("invalid place"), NO_PLACE);
        assert_eq!(refused("not an infinity file"), NOT_A_FIGURE);
        assert_eq!(refused("file too small"), NOT_A_FIGURE);
        assert_eq!(refused("already on the base"), ON_ALREADY);
        assert_eq!(refused("cannot open file"), LOAD_FAILED);
        assert_eq!(refused("path must be absolute"), LOAD_FAILED);
        assert_eq!(refusal(Failed::Unreached, LOAD_FAILED), NOT_ANSWERING);
        assert_eq!(refusal(Failed::NoAnswer, MAKE_FAILED), NOT_ANSWERING);
        assert_eq!(check(9), Err(NO_PLACE.to_string()));
        assert_eq!(check(8), Ok(()));
    }

    /// A server that answers as the build of Cemu made for Omoio is to
    /// answer the base's commands, on a base of nine places, with Cemu's
    /// list cut down to `LIST_ANSWER`. A figure it made is known by the
    /// number it was made with; any other file of 320 bytes reads as a
    /// figure Cemu's list doesn't have. It takes `connections` commands, one
    /// to a connection, and each line it read comes back through `heard`.
    fn fork_like(connections: usize) -> (Server, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tell, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let mut base: [Place; PLACES] = [None; PLACES];
            let mut made: HashMap<String, (u32, u8)> = HashMap::new();
            for _ in 0..connections {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let line = line.trim_end().to_string();
                let answer = fork_answer(&mut base, &mut made, &line);
                let _ = tell.send(line);
                (&stream).write_all(answer.as_bytes()).unwrap();
            }
        });
        (Server::new(port, portal::SLOTS), heard)
    }

    fn fork_answer(base: &mut [Place; PLACES], made: &mut HashMap<String, (u32, u8)>, line: &str) -> String {
        let Some(rest) = line.strip_prefix("infinity ") else {
            return "error unknown command\n".to_string();
        };
        let (command, rest) = rest.split_once(' ').unwrap_or((rest, ""));
        match command {
            "status" => {
                let mut text: String = base
                    .iter()
                    .enumerate()
                    .map(|(place, held)| match held {
                        Some((number, series)) => format!("place{place} loaded {number} {series}\n"),
                        None => format!("place{place} empty\n"),
                    })
                    .collect();
                text.push_str("ok\n");
                text
            }
            "list" => LIST_ANSWER.to_string(),
            "load" => {
                let (place, path) = rest.split_once(' ').unwrap();
                let Some(place) = place.parse::<usize>().ok().filter(|&place| place < PLACES) else {
                    return "error invalid place\n".to_string();
                };
                if std::fs::metadata(path).map(|file| file.len()).ok() != Some(320) {
                    return "error not an infinity file\n".to_string();
                }
                let figure = made.get(path).copied().unwrap_or((1_999_999, 0));
                base[place] = Some(figure);
                format!("ok {} {}\n", figure.0, figure.1)
            }
            "remove" => match base.get_mut(rest.parse::<usize>().unwrap()).map(Option::take) {
                Some(Some(_)) => "ok\n".to_string(),
                Some(None) => "error place empty\n".to_string(),
                None => "error invalid place\n".to_string(),
            },
            "create" => {
                let words: Vec<&str> = rest.splitn(3, ' ').collect();
                if Path::new(words[2]).exists() {
                    return "error file exists\n".to_string();
                }
                std::fs::write(words[2], [0u8; 320]).unwrap();
                made.insert(words[2].to_string(), (words[0].parse().unwrap(), words[1].parse().unwrap()));
                "ok\n".to_string()
            }
            _ => "error unknown command\n".to_string(),
        }
    }

    /// Answers every command as a build without the base's commands does.
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

    #[test]
    fn figures_go_on_and_off_the_base_through_a_server_like_the_forks() {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-infinity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (stitch, brought, notes) = (dir.join("Stitch.bin"), dir.join("Brought.bin"), dir.join("notes.txt"));
        std::fs::write(&brought, [0u8; 320]).unwrap();
        std::fs::write(&notes, "not a figure").unwrap();
        // A pid of its own, so no other test's list is taken for this Cemu's.
        let pid = 4_000_001;
        let (server, heard) = fork_like(13);

        let empty = on_base(&server, pid).unwrap();
        assert_eq!(empty, vec![String::new(); 9], "an empty base wants no list");
        let figure = ToyFigure { name: "Stitch".into(), number: 1_000_118, series: Some(2) };
        create_through(&server, 3, &figure, &stitch).unwrap();
        assert_eq!(on_base(&server, pid).unwrap()[3], "Stitch");
        load_through(&server, 6, &brought).unwrap();
        assert_eq!(on_base(&server, pid).unwrap()[6], "Unknown Figure", "the list was asked for once");
        assert_eq!(load_through(&server, 0, &notes), Err(NOT_A_FIGURE.to_string()));
        remove_through(&server, 3).unwrap();
        remove_through(&server, 3).unwrap();
        let after = on_base(&server, pid).unwrap();
        assert_eq!((after[3].as_str(), after[6].as_str()), ("", "Unknown Figure"));
        let made_again = create_through(&server, 3, &figure, &stitch);
        assert_eq!(made_again, Err(MAKE_FAILED.to_string()), "not made over a file that is there");
        assert_eq!(ask_list(&server).unwrap().len(), 6);

        let heard: Vec<String> = heard.iter().take(13).collect();
        let (stitch, brought, notes) = (stitch.to_str().unwrap(), brought.to_str().unwrap(), notes.to_str().unwrap());
        let said = [
            "infinity status".to_string(),
            format!("infinity create 1000118 2 {stitch}"),
            format!("infinity load 3 {stitch}"),
            "infinity status".to_string(),
            "infinity list".to_string(),
            format!("infinity load 6 {brought}"),
            "infinity status".to_string(),
            format!("infinity load 0 {notes}"),
            "infinity remove 3".to_string(),
            "infinity remove 3".to_string(),
            "infinity status".to_string(),
            format!("infinity create 1000118 2 {stitch}"),
            "infinity list".to_string(),
        ];
        assert_eq!(heard, said);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_cemu_is_told_apart_by_its_answer() {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-infinity-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Stitch.bin");
        let figure = ToyFigure { name: "Stitch".into(), number: 1_000_118, series: Some(2) };
        let server = older(5);
        assert_eq!(on_base(&server, 4_000_002), Err(OLD_CEMU.to_string()));
        assert_eq!(load_through(&server, 3, &file), Err(OLD_CEMU.to_string()));
        assert_eq!(remove_through(&server, 3), Err(OLD_CEMU.to_string()));
        assert_eq!(ask_list(&server), Err(OLD_CEMU.to_string()));
        assert_eq!(create_through(&server, 3, &figure, &file), Err(OLD_CEMU.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_figure_with_no_series_isnt_sent_to_be_made() {
        let server = older(0);
        let figure = ToyFigure { name: "Stitch".into(), number: 1_000_118, series: None };
        assert_eq!(create_through(&server, 3, &figure, Path::new("C:\\Stitch.bin")), Err(MAKE_FAILED.to_string()));
    }
}
