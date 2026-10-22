//! RPCS3's Skylanders portal server, which puts figures on the portal and
//! takes them off without RPCS3's windows. It came with RPCS3 build
//! 0.0.43-20280 (commit dfc486f, 10 October 2026); an older RPCS3 doesn't
//! answer, and portal.rs then works the portal through the Skylanders
//! Manager as before. Facts below are from build 0.0.43-20344, read 20
//! October 2026 (https://github.com/RPCS3/rpcs3/blob/58317c9350983d71f18df2b0a346433161d6b13c/...).
//! How Omoio talks to it is shared with Cemu's (backends/portal_ipc.rs).
//!
//! The server is switched on in `config\sky_ipc.yml` beside RPCS3's other
//! settings (`SkylanderPortalIPC_config.cpp` lines 43 to 46, with
//! `fs::get_config_dir(true)`, `Utilities/File.cpp` lines 2518 to 2614: the
//! program's folder, then `config/`, as Omoio's RPCS3 has no `portable`
//! folder). RPCS3 reads the file each time it starts the emulator, at its own
//! start and again as a game boots (`System.cpp` lines 828 to 835, `Init`,
//! which `Load` calls at line 1284), and stops the server with the game
//! (`g_fxo->reset()`, lines 494 and 4230).

use crate::backends::portal_ipc::free_port;
pub use crate::backends::portal_ipc::{Failed, Server, Slot};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};

/// RPCS3's own port for the server (`SkylanderPortalIPC_config.h` line 8).
const DEFAULT_PORT: u16 = 28013;

/// The port written for the game now starting.
static PORT: AtomicU16 = AtomicU16::new(DEFAULT_PORT);

/// Slots on the portal; `status` names each of them (`SkylanderPortalIPC.cpp`
/// lines 106 to 121).
const SLOTS: usize = 8;

/// Writes `sky_ipc.yml` in RPCS3's config folder `config` before a game, with
/// the server on for a Skylanders game and off for any other, so no port is
/// held for a game that has no portal. RPCS3's own port is used unless
/// another program has it, when Windows picks a free one (`free_port`).
pub fn get_ready(config: &Path, on: bool) -> std::io::Result<()> {
    let file = config.join("sky_ipc.yml");
    if !on && !file.exists() {
        // Off is RPCS3's default (`SkylanderPortalIPC_config.h` line 7).
        return Ok(());
    }
    let port = if on { free_port(DEFAULT_PORT) } else { DEFAULT_PORT };
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

/// The server of the RPCS3 now running.
pub fn ours() -> Server {
    Server::new(PORT.load(Ordering::Relaxed), SLOTS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

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
        let port = free_port(DEFAULT_PORT);
        if holder.is_ok() {
            assert_ne!(port, DEFAULT_PORT);
            assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
        }
        assert!(port >= 1025);
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
        (Server::new(port, SLOTS), heard)
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
        assert_eq!(Server::new(port, SLOTS).status(), Err(Failed::Unreached));
    }
}
