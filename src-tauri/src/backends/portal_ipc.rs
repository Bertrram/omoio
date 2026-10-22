//! Talking to an emulator's Skylanders portal server, which puts figures on
//! the portal and takes them off without the emulator's windows. RPCS3 has
//! one (rpcs3/portal_ipc.rs), and so does the build of Cemu made for Omoio
//! (github.com/Bertrram/Cemu, branch omoio-portal-ipc; cemu/portal.rs),
//! which takes RPCS3's commands the same way and adds a few of its own. Each
//! emulator says where its server listens and how many slots its portal has;
//! everything else is here. Facts below are from RPCS3 build 0.0.43-20344,
//! read 20 October 2026 (https://github.com/RPCS3/rpcs3/blob/58317c9350983d71f18df2b0a346433161d6b13c/...).
//!
//! A server listens on 127.0.0.1 only, bound for itself alone
//! (`3rdparty/pine/pine_server.h` lines 82 to 114). Each connection carries
//! one command: the server reads one line, at most 4096 bytes within a
//! second, answers and closes (`SkylanderPortalIPC.cpp` lines 49 and 50, 202
//! to 232, 283 to 315).

use std::io::{BufRead, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

/// A refused connection on this machine can take Windows a second or more to
/// report, so a server that isn't there is given up on sooner than that.
const CONNECT_WAIT: Duration = Duration::from_millis(200);
/// A server answers at once, unless another connection holds its one
/// listener for up to the second it allows each.
const ANSWER_WAIT: Duration = Duration::from_secs(2);
/// More than any answer: sixteen status lines and `ok`.
const LONGEST_ANSWER: u64 = 8192;

/// Why a command didn't do what it asked.
#[derive(Debug, PartialEq, Eq)]
pub enum Failed {
    /// Nothing answered on the port, so nothing was done.
    Unreached,
    /// The command went, but no answer a server would give came back.
    NoAnswer,
    /// The server turned it down, in its own words after "error ", such as
    /// "slot empty" or "cannot open file".
    Refused(String),
}

/// What a slot holds: the figure's id and variant, as the emulator reads
/// them from the figure (RPCS3's `Skylander.cpp` lines 229 to 245), or
/// nothing.
pub type Slot = Option<(u16, u16)>;

/// One emulator's server: where it listens and how many slots its portal has.
pub struct Server {
    port: u16,
    slots: usize,
}

impl Server {
    pub fn new(port: u16, slots: usize) -> Self {
        Self { port, slots }
    }

    /// What each slot holds, counted from 0.
    pub fn status(&self) -> Result<Vec<Slot>, Failed> {
        status_slots(&self.ask("status")?, self.slots).ok_or(Failed::NoAnswer)
    }

    /// Puts the figure in `file` in `slot`, which must be empty. The path
    /// goes as it is after the slot, spaces and all, and must start with a
    /// drive (`SkylanderPortalIPC.cpp` lines 52 to 93, 167 to 180).
    pub fn load(&self, slot: usize, file: &str) -> Result<usize, Failed> {
        loaded_slot(&self.ask(&format!("load {slot} {file}"))?, self.slots).ok_or(Failed::NoAnswer)
    }

    /// Takes the figure in `slot` off; "slot empty" when there was none
    /// (`SkylanderPortalIPC.cpp` lines 95 to 104).
    pub fn remove(&self, slot: usize) -> Result<(), Failed> {
        self.ask(&format!("remove {slot}")).map(drop)
    }

    /// Sends one command on a connection of its own and gives back the
    /// server's answer, up to and with its last line.
    ///
    /// Omoio ends its side first, once the command is sent, and reads until
    /// the server has closed. A server bound for itself alone can't bind its
    /// port again while a connection it closed first is winding down, which
    /// Microsoft's "Using SO_REUSEADDR and SO_EXCLUSIVEADDRUSE" says and
    /// answers this way; RPCS3 binds again each time a game boots.
    pub fn ask(&self, command: &str) -> Result<Vec<String>, Failed> {
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

/// `own` when nothing holds it, else a port Windows has free. Binding it for
/// a moment is the test: a server bound for itself alone makes the same bind
/// fail while any program has it, an emulator still on its way out included.
/// The emulator would otherwise fail to start its server ("Cannot start
/// server on port", `SkylanderPortalIPC.cpp` line 256) and the portal would
/// go back to its windows.
pub fn free_port(own: u16) -> u16 {
    if TcpListener::bind(("127.0.0.1", own)).is_ok() {
        return own;
    }
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .ok()
        // RPCS3 takes 1025 to 65535 (`SkylanderPortalIPC_config.h` line 8).
        .filter(|&port| port >= 1025)
        .unwrap_or(own)
}

/// The lines of an answer up to the one that ends it: "ok", "ok <x>" or
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

/// `status`'s line for each of the `slots`, "slot<N> loaded <id> <variant>"
/// or "slot<N> empty", in order and then "ok" (`SkylanderPortalIPC.cpp`
/// lines 106 to 121). Anything else isn't the server's answer.
fn status_slots(lines: &[String], slots: usize) -> Option<Vec<Slot>> {
    let (last, held) = lines.split_last()?;
    if last != "ok" || held.len() != slots {
        return None;
    }
    held.iter()
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
fn loaded_slot(lines: &[String], slots: usize) -> Option<usize> {
    let slot: usize = lines.last()?.strip_prefix("ok ")?.parse().ok()?;
    (slot < slots).then_some(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    /// RPCS3's portal has eight slots.
    const RPCS3_STATUS: &str = "slot0 loaded 16 0\nslot1 empty\nslot2 loaded 212 12302\nslot3 empty\n\
                                slot4 empty\nslot5 empty\nslot6 empty\nslot7 loaded 680 20996\nok\n";

    /// Cemu's has sixteen (`MAX_SKYLANDERS`, `Skylander.h` line 48, v2.6).
    fn cemu_status() -> String {
        let mut text: String = (0..16)
            .map(|slot| match slot {
                0 => "slot0 loaded 16 0\n".to_string(),
                9 => "slot9 loaded 3004 14337\n".to_string(),
                15 => "slot15 loaded 601 20480\n".to_string(),
                _ => format!("slot{slot} empty\n"),
            })
            .collect();
        text.push_str("ok\n");
        text
    }

    #[test]
    fn the_status_reads_slot_by_slot() {
        let slots = status_slots(&lines(RPCS3_STATUS), 8).unwrap();
        assert_eq!(slots.len(), 8);
        assert_eq!(slots[0], Some((16, 0)));
        assert_eq!(slots[1], None);
        assert_eq!(slots[2], Some((212, 0x300E)));
        assert_eq!(slots[7], Some((680, 0x5204)));
    }

    #[test]
    fn a_portal_of_sixteen_reads_all_sixteen() {
        let slots = status_slots(&lines(&cemu_status()), 16).unwrap();
        assert_eq!(slots.len(), 16);
        assert_eq!(slots[0], Some((16, 0)));
        assert_eq!(slots[9], Some((3004, 0x3801)));
        assert_eq!(slots[15], Some((601, 0x5000)));
        assert_eq!(slots.iter().filter(|slot| slot.is_none()).count(), 13);
        // Each portal's answer has its own number of lines.
        assert_eq!(status_slots(&lines(&cemu_status()), 8), None);
        assert_eq!(status_slots(&lines(RPCS3_STATUS), 16), None);
        assert_eq!(status_slots(&lines(&cemu_status().replace("slot15", "slot16")), 16), None);
        assert_eq!(loaded_slot(&lines("ok 15"), 16), Some(15));
        assert_eq!(loaded_slot(&lines("ok 16"), 16), None);
    }

    #[test]
    fn anything_but_rpcs3s_status_is_no_answer() {
        assert_eq!(status_slots(&lines("ok\n"), 8), None);
        assert_eq!(status_slots(&lines(&RPCS3_STATUS.replace("slot3", "slot9")), 8), None);
        assert_eq!(status_slots(&lines(&RPCS3_STATUS.replace("212 12302", "212")), 8), None);
        assert_eq!(status_slots(&lines(&RPCS3_STATUS.replace("ok\n", "done\n")), 8), None);
        assert_eq!(reply(b"HTTP/1.1 400 Bad Request\r\n\r\n"), Err(Failed::NoAnswer));
    }

    #[test]
    fn answers_end_on_ok_or_error() {
        assert_eq!(reply(b"ok 3\n"), Ok(vec!["ok 3".to_string()]));
        assert_eq!(loaded_slot(&lines("ok 3"), 8), Some(3));
        assert_eq!(loaded_slot(&lines("ok"), 8), None);
        assert_eq!(loaded_slot(&lines("ok 8"), 8), None);
        assert_eq!(reply(b"error slot empty\n"), Err(Failed::Refused("slot empty".to_string())));
        assert_eq!(reply(b"error cannot open file\n"), Err(Failed::Refused("cannot open file".to_string())));
        assert_eq!(reply(b"slot0 empty\n"), Err(Failed::NoAnswer));
        assert_eq!(reply(b""), Err(Failed::NoAnswer));
    }

    #[test]
    fn a_held_port_is_traded_for_a_free_one() {
        let holder = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let held = holder.local_addr().unwrap().port();
        let port = free_port(held);
        assert_ne!(port, held);
        assert!(port >= 1025);
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
        drop(holder);
        assert_eq!(free_port(held), held);
    }
}
