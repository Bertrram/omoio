//! RPCS3's Skylanders portal server, which puts figures on the portal and
//! takes them off without RPCS3's windows. It came with RPCS3 build
//! 0.0.43-20280 (commit dfc486f, 10 October 2026); an older RPCS3 doesn't
//! answer, and portal.rs then works the portal through the Skylanders
//! Manager as before. Facts below are from build 0.0.43-20344, read 20
//! October 2026 (https://github.com/RPCS3/rpcs3/blob/58317c9350983d71f18df2b0a346433161d6b13c/...).
//!
//! The server is switched on in `config\sky_ipc.yml` beside RPCS3's other
//! settings (`SkylanderPortalIPC_config.cpp` lines 43 to 46, with
//! `fs::get_config_dir(true)`, `Utilities/File.cpp` lines 2518 to 2614: the
//! program's folder, then `config/`, as Omoio's RPCS3 has no `portable`
//! folder). RPCS3 reads the file each time it starts the emulator, at its own
//! start and again as a game boots (`System.cpp` lines 828 to 835, `Init`,
//! which `Load` calls at line 1284), and stops the server with the game
//! (`g_fxo->reset()`, lines 494 and 4230).
//!
//! It listens on 127.0.0.1 only, bound for itself alone
//! (`3rdparty/pine/pine_server.h` lines 82 to 114). Each connection carries
//! one command: RPCS3 reads one line, at most 4096 bytes within a second,
//! answers and closes (`SkylanderPortalIPC.cpp` lines 49 and 50, 202 to 232,
//! 283 to 315).

use std::io::{BufRead, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

/// RPCS3's own port for the server (`SkylanderPortalIPC_config.h` line 8).
const DEFAULT_PORT: u16 = 28013;

/// The port written for the game now starting.
static PORT: AtomicU16 = AtomicU16::new(DEFAULT_PORT);

/// Slots on the portal; `status` names each of them (`SkylanderPortalIPC.cpp`
/// lines 106 to 121).
const SLOTS: usize = 8;

/// A refused connection on this machine can take Windows a second or more to
/// report, so a server that isn't there is given up on sooner than that.
const CONNECT_WAIT: Duration = Duration::from_millis(200);
/// RPCS3 answers at once, unless another connection holds its one listener
/// for up to the second it allows each.
const ANSWER_WAIT: Duration = Duration::from_secs(2);
/// More than any answer: eight status lines and `ok`.
const LONGEST_ANSWER: u64 = 8192;

/// Writes `sky_ipc.yml` in RPCS3's config folder `config` before a game, with
/// the server on for a Skylanders game and off for any other, so no port is
/// held for a game that has no portal. RPCS3's own port is used unless
/// another program has it, when Windows picks a free one; RPCS3 would
/// otherwise fail to start the server ("Cannot start server on port",
/// `SkylanderPortalIPC.cpp` line 256) and the portal would go back to the
/// manager.
pub fn get_ready(config: &Path, on: bool) -> std::io::Result<()> {
    let file = config.join("sky_ipc.yml");
    if !on && !file.exists() {
        // Off is RPCS3's default (`SkylanderPortalIPC_config.h` line 7).
        return Ok(());
    }
    let port = if on { free_port() } else { DEFAULT_PORT };
    let text = config_text(on, port);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        std::fs::create_dir_all(config)?;
        std::fs::write(&file, text)?;
    }
    PORT.store(port, Ordering::Relaxed);
    Ok(())
}

/// The file as RPCS3 saves it from its own Skylanders IPC window, with the
/// key names of `SkylanderPortalIPC_config.h` lines 7 and 8.
fn config_text(on: bool, port: u16) -> String {
    format!("Skylanders IPC Server enabled: {on}\nSkylanders IPC Port: {port}\n")
}

/// RPCS3's own port when nothing holds it. Binding it for a moment is the
/// test: RPCS3 binds for itself alone, so the same bind fails while any
/// program has it, an RPCS3 still on its way out included.
fn free_port() -> u16 {
    if TcpListener::bind(("127.0.0.1", DEFAULT_PORT)).is_ok() {
        return DEFAULT_PORT;
    }
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .ok()
        // RPCS3 takes 1025 to 65535 (`SkylanderPortalIPC_config.h` line 8).
        .filter(|&port| port >= 1025)
        .unwrap_or(DEFAULT_PORT)
}

/// Why a command didn't do what it asked.
#[derive(Debug, PartialEq, Eq)]
pub enum Failed {
    /// Nothing answered on the port, so nothing was done.
    Unreached,
    /// The command went, but no answer RPCS3 would give came back.
    NoAnswer,
    /// RPCS3 turned it down, in its own words after "error ", such as
    /// "slot empty" or "cannot open file".
    Refused(String),
}

/// What a slot holds: the figure's id and variant, as RPCS3 reads them from
/// the figure (`Skylander.cpp` lines 229 to 245), or nothing.
pub type Slot = Option<(u16, u16)>;

/// The server of the RPCS3 now running.
pub struct Server {
    port: u16,
}

impl Server {
    pub fn ours() -> Self {
        Self { port: PORT.load(Ordering::Relaxed) }
    }

    /// What each slot holds, counted from 0 as RPCS3's Skylanders Manager
    /// counts them: the manager shows slot `i` of the same portal
    /// (`skylander_dialog.cpp` lines 779 to 806).
    pub fn status(&self) -> Result<Vec<Slot>, Failed> {
        status_slots(&self.ask("status")?).ok_or(Failed::NoAnswer)
    }

    /// Puts the figure in `file` in `slot`, which must be empty. The path
    /// goes as it is after the slot, spaces and all, and must start with a
    /// drive (`SkylanderPortalIPC.cpp` lines 52 to 93, 167 to 180).
    pub fn load(&self, slot: usize, file: &str) -> Result<usize, Failed> {
        loaded_slot(&self.ask(&format!("load {slot} {file}"))?).ok_or(Failed::NoAnswer)
    }

    /// Takes the figure in `slot` off; "slot empty" when there was none
    /// (`SkylanderPortalIPC.cpp` lines 95 to 104).
    pub fn remove(&self, slot: usize) -> Result<(), Failed> {
        self.ask(&format!("remove {slot}")).map(drop)
    }

    /// Sends one command on a connection of its own and gives back RPCS3's
    /// answer, up to and with its last line.
    ///
    /// Omoio ends its side first, once the command is sent, and reads until
    /// RPCS3 has closed. A server bound for itself alone can't bind its port
    /// again while a connection it closed first is winding down, which
    /// Microsoft's "Using SO_REUSEADDR and SO_EXCLUSIVEADDRUSE" says and
    /// answers this way; RPCS3 binds again each time a game boots.
    fn ask(&self, command: &str) -> Result<Vec<String>, Failed> {
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_WAIT).map_err(|_| Failed::Unreached)?;
        let _ = stream.set_write_timeout(Some(ANSWER_WAIT));
        let _ = stream.set_read_timeout(Some(ANSWER_WAIT));
        // "\n" alone: a "\r" ends the line for RPCS3 as well, and the "\n"
        // after it would be left unread when it closes.
        stream.write_all(format!("{command}\n").as_bytes()).map_err(|_| Failed::NoAnswer)?;
        let _ = stream.shutdown(Shutdown::Write);
        let mut answer = Vec::new();
        // A read that waits out its time after the answer came still has it.
        let _ = stream.take(LONGEST_ANSWER).read_to_end(&mut answer);
        reply(&answer)
    }
}

/// The lines of an answer up to the one that ends it: "ok", "ok <slot>" or
/// "error <why>" (`SkylanderPortalIPC.cpp` lines 63 to 183). A refusal comes
/// back as `Refused`.
fn reply(answer: &[u8]) -> Result<Vec<String>, Failed> {
    let mut lines = Vec::new();
    for line in answer.lines() {
        let line = line.map_err(|_| Failed::NoAnswer)?;
        if let Some(why) = line.strip_prefix("error ") {
            return Err(Failed::Refused(why.to_string()));
        }
        let last = line == "ok" || line.starts_with("ok ");
        lines.push(line);
        if last {
            return Ok(lines);
        }
    }
    Err(Failed::NoAnswer)
}

/// `status`'s eight lines, "slot<N> loaded <id> <variant>" or "slot<N>
/// empty", in order and then "ok". Anything else isn't RPCS3's answer.
fn status_slots(lines: &[String]) -> Option<Vec<Slot>> {
    let (last, slots) = lines.split_last()?;
    if last != "ok" || slots.len() != SLOTS {
        return None;
    }
    slots
        .iter()
        .enumerate()
        .map(|(number, line)| {
            let words: Vec<&str> = line.split(' ').collect();
            if words.first() != Some(&format!("slot{number}").as_str()) {
                return None;
            }
            match words[1..] {
                ["empty"] => Some(None),
                ["loaded", id, variant] => Some(Some((id.parse().ok()?, variant.parse().ok()?))),
                _ => None,
            }
        })
        .collect()
}

/// The slot in "ok <slot>", where `load` put the figure.
fn loaded_slot(lines: &[String]) -> Option<usize> {
    let slot: usize = lines.last()?.strip_prefix("ok ")?.parse().ok()?;
    (slot < SLOTS).then_some(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;
    use std::sync::mpsc;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    const STATUS: &str = "slot0 loaded 16 0\nslot1 empty\nslot2 loaded 212 12302\nslot3 empty\n\
                          slot4 empty\nslot5 empty\nslot6 empty\nslot7 loaded 680 20996\nok\n";

    #[test]
    fn the_file_turns_the_server_on_as_rpcs3_writes_it() {
        assert_eq!(
            config_text(true, 28013),
            "Skylanders IPC Server enabled: true\nSkylanders IPC Port: 28013\n"
        );
        assert_eq!(
            config_text(false, 28013),
            "Skylanders IPC Server enabled: false\nSkylanders IPC Port: 28013\n"
        );
    }

    #[test]
    fn a_game_without_a_portal_leaves_no_file_behind() {
        let dir = std::env::temp_dir().join(format!("omoio-sky-ipc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        get_ready(&dir, false).unwrap();
        assert!(!dir.join("sky_ipc.yml").exists());
        get_ready(&dir, true).unwrap();
        let on = std::fs::read_to_string(dir.join("sky_ipc.yml")).unwrap();
        assert!(on.starts_with("Skylanders IPC Server enabled: true\n"), "{on}");
        get_ready(&dir, false).unwrap();
        let off = std::fs::read_to_string(dir.join("sky_ipc.yml")).unwrap();
        assert!(off.starts_with("Skylanders IPC Server enabled: false\n"), "{off}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_held_port_is_traded_for_a_free_one() {
        let holder = TcpListener::bind(("127.0.0.1", DEFAULT_PORT));
        let port = free_port();
        if holder.is_ok() {
            assert_ne!(port, DEFAULT_PORT);
            assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
        }
        assert!(port >= 1025);
    }

    #[test]
    fn the_status_reads_slot_by_slot() {
        let slots = status_slots(&lines(STATUS)).unwrap();
        assert_eq!(slots.len(), 8);
        assert_eq!(slots[0], Some((16, 0)));
        assert_eq!(slots[1], None);
        assert_eq!(slots[2], Some((212, 0x300E)));
        assert_eq!(slots[7], Some((680, 0x5204)));
    }

    #[test]
    fn anything_but_rpcs3s_status_is_no_answer() {
        assert_eq!(status_slots(&lines("ok\n")), None);
        assert_eq!(status_slots(&lines(&STATUS.replace("slot3", "slot9"))), None);
        assert_eq!(status_slots(&lines(&STATUS.replace("212 12302", "212"))), None);
        assert_eq!(status_slots(&lines(&STATUS.replace("ok\n", "done\n"))), None);
        assert_eq!(reply(b"HTTP/1.1 400 Bad Request\r\n\r\n"), Err(Failed::NoAnswer));
    }

    #[test]
    fn answers_end_on_ok_or_error() {
        assert_eq!(reply(b"ok 3\n"), Ok(vec!["ok 3".to_string()]));
        assert_eq!(loaded_slot(&lines("ok 3")), Some(3));
        assert_eq!(loaded_slot(&lines("ok")), None);
        assert_eq!(loaded_slot(&lines("ok 8")), None);
        assert_eq!(reply(b"error slot empty\n"), Err(Failed::Refused("slot empty".to_string())));
        assert_eq!(reply(b"error cannot open file\n"), Err(Failed::Refused("cannot open file".to_string())));
        assert_eq!(reply(b"slot0 empty\n"), Err(Failed::NoAnswer));
        assert_eq!(reply(b""), Err(Failed::NoAnswer));
    }

    /// A server that does what RPCS3's does with one connection: reads one
    /// line, answers it and closes. The line it read comes back through `heard`.
    fn rpcs3_like(answer: &'static str) -> (Server, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tell, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            tell.send(line).unwrap();
            (&stream).write_all(answer.as_bytes()).unwrap();
        });
        (Server { port }, heard)
    }

    #[test]
    fn the_client_talks_to_a_server_like_rpcs3s() {
        let (server, heard) = rpcs3_like(STATUS);
        let slots = server.status().unwrap();
        assert_eq!(heard.recv().unwrap(), "status\n");
        assert_eq!(slots[0], Some((16, 0)));

        let (server, heard) = rpcs3_like("ok 2\n");
        assert_eq!(server.load(2, r"C:\Users\Some One\Omoio figures\Spyro.sky"), Ok(2));
        assert_eq!(heard.recv().unwrap(), "load 2 C:\\Users\\Some One\\Omoio figures\\Spyro.sky\n");

        let (server, heard) = rpcs3_like("error slot empty\n");
        assert_eq!(server.remove(5), Err(Failed::Refused("slot empty".to_string())));
        assert_eq!(heard.recv().unwrap(), "remove 5\n");
    }

    #[test]
    fn nothing_on_the_port_is_unreached() {
        let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        assert_eq!(Server { port }.status(), Err(Failed::Unreached));
    }
}
