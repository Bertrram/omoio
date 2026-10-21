//! A pad Cemu reads over the network, played by Omoio from the keyboard and
//! mouse (keyboard_pad.rs).
//!
//! Cemu reads pads from servers that speak the "cemuhook" protocol, the
//! one DS4Windows and BetterJoy serve: UDP on this machine, Cemu asking and
//! the server answering with the pad's state. Everything here is written
//! from Cemu's own client, `src/input/api/DSU/` at v2.6 (read 20 October
//! 2026), with the protocol's own description (v1993.github.io/
//! cemuhook-protocol, read the same day) where Cemu leaves something out:
//!
//! - Every packet starts with a 16-byte header: "DSUS" from a server,
//!   "DSUC" from Cemu, the protocol version 1001, the length after the
//!   header, a CRC-32 of the whole packet taken with its own four bytes at
//!   zero, and the sender's id. Then the message type, 0x100000 the
//!   version, 0x100001 a port's information, 0x100002 a pad's data
//!   (DSUMessages.h lines 53 to 85, DSUMessages.cpp lines 4 to 27).
//! - Cemu checks the CRC over the size it expects each answer to be: 24
//!   bytes for the version, 32 for a port, 100 for data (`IsValid`,
//!   DSUMessages.cpp lines 82 to 95; the structs in DSUMessages.h lines
//!   148 to 271). Anything else is dropped.
//! - Cemu takes the slot from the packet and uses it to index its eight
//!   pads with no check (`reader_thread`, DSUControllerProvider.cpp lines
//!   338 to 372), so the slot sent is always one of those.
//! - After every port or data packet it takes, Cemu asks for the next data
//!   (line 378). It asks first as the profile loads (`DSUController::load`
//!   and `connect`, DSUController.cpp lines 31 to 53) and never again on its
//!   own, so the server has to be listening before Cemu starts.
//! - It has no timeout: the last state it got stays (lines 249 to 380). So
//!   the pad at rest is the last thing sent.
//! - The first state Cemu sees once the pad is there is taken as the pad
//!   at rest, sticks centred (`ControllerBase::calibrate`, Controller.cpp
//!   lines 162 to 167). A port answer carries no stick, and one taken first
//!   would leave Cemu thinking the sticks rest at their far corner, so a
//!   data packet at rest goes before every port answer.

use crate::keyboard_pad::{self, Aim};
use crate::raw_input;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE, POINT, RECT, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};

const SERVER: &[u8; 4] = b"DSUS";
const CLIENT: &[u8; 4] = b"DSUC";
const VERSION: u16 = 1001;
const VERSION_MESSAGE: u32 = 0x10_0000;
const PORTS_MESSAGE: u32 = 0x10_0001;
const DATA_MESSAGE: u32 = 0x10_0002;

/// Where the server listens. Not 26760, the protocol's own port, which
/// DS4Windows and BetterJoy take by default and Cemu asks at as it starts
/// (`DSUProviderSettings`, DSUControllerProvider.h line 21): a pad of
/// theirs would answer for Omoio's, or Omoio couldn't listen at all. Cemu's
/// profile names the port (`<port>`, DSUController.cpp lines 25 to 39).
pub const PORT: u16 = 26762;

/// The one slot Omoio's pad is in.
pub const SLOT: u8 = 0;

/// How often the pad's state goes to Cemu. Cemu looks at its pads every
/// millisecond (`InputManager::update_thread`, InputManager.cpp lines 932 to
/// 953, v2.6) and a game once a frame, so 250 a second has a press there
/// within 4 ms, a quarter of a frame at 60, for 25 kB a second that never
/// leaves the machine.
const EVERY: Duration = Duration::from_millis(4);

/// Cemu asks again after each packet. When it hasn't for this long it has
/// gone, and nothing more is sent to it, as the protocol asks of servers.
const FORGET_AFTER: Duration = Duration::from_secs(5);

/// The pad at rest, gravity as Cemu's motion reader takes a pad held still
/// in its starting pose, (0, -1, 0) in g (`MahonySensorFusion`'s starting
/// pose, motion/Mahony.h lines 10 to 14, against `integrate_motion`,
/// DSUControllerProvider.cpp lines 435 to 441, which turns y and z round).
const REST_ACCEL: [f32; 3] = [0.0, -1.0, 0.0];

/// A pad's state as one packet carries it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadState {
    /// Sixteen buttons, numbered as `keyboard_pad::Output::Button`: the
    /// first packet byte from bit 0, then the second from bit 8.
    pub buttons: u16,
    /// Left stick across and up, right stick across and up, 128 at rest.
    pub sticks: [u8; 4],
    /// Turning, pitch, yaw and roll, in degrees a second.
    pub gyro: [f32; 3],
}

impl PadState {
    pub fn rest() -> Self {
        PadState {
            buttons: 0,
            sticks: [128; 4],
            gyro: [0.0; 3],
        }
    }
}

/// CRC-32 as the protocol and Cemu take it: the common one, polynomial
/// 0xEDB88320, reflected, starting and ending inverted (util/crypto/
/// crc32.cpp lines 281 to 349, v2.6).
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Writes the header and message type at the start of `packet`, the length
/// and the CRC last of all.
fn seal(packet: &mut [u8], id: u32, message: u32) {
    packet[0..4].copy_from_slice(SERVER);
    packet[4..6].copy_from_slice(&VERSION.to_le_bytes());
    let length = (packet.len() - 16) as u16;
    packet[6..8].copy_from_slice(&length.to_le_bytes());
    packet[8..12].copy_from_slice(&[0; 4]);
    packet[12..16].copy_from_slice(&id.to_le_bytes());
    packet[16..20].copy_from_slice(&message.to_le_bytes());
    let crc = crc32(packet);
    packet[8..12].copy_from_slice(&crc.to_le_bytes());
}

pub fn version_answer(id: u32) -> [u8; 24] {
    let mut packet = [0u8; 24];
    packet[20..22].copy_from_slice(&VERSION.to_le_bytes());
    seal(&mut packet, id, VERSION_MESSAGE);
    packet
}

/// A port's information, from offset 20: slot, state (2 connected), model
/// (2 a DualShock 4), connection (1 USB), six bytes of address, battery
/// (5 full) and whether it is active (`PortInfoData`, DSUMessages.h lines
/// 160 to 169).
fn port_info(packet: &mut [u8], slot: u8, connected: bool) {
    packet[20] = slot;
    if connected {
        packet[21..24].copy_from_slice(&[2, 2, 1]);
        packet[30] = 5;
        packet[31] = 1;
    }
}

pub fn port_answer(id: u32, slot: u8) -> [u8; 32] {
    let mut packet = [0u8; 32];
    port_info(&mut packet, slot, slot == SLOT);
    seal(&mut packet, id, PORTS_MESSAGE);
    packet
}

/// The pad's data (`DataResponseData`, DSUMessages.h lines 199 to 232): the
/// port, the packet's number, the buttons, sticks, each button again from 0
/// to 255, two touches left untouched, then the motion with its time in
/// microseconds, which Cemu reads only when it moves on
/// (`integrate_motion`, DSUControllerProvider.cpp lines 418 to 444).
pub fn data_answer(id: u32, number: u32, state: &PadState, micros: u64) -> [u8; 100] {
    let mut packet = [0u8; 100];
    port_info(&mut packet, SLOT, true);
    packet[32..36].copy_from_slice(&number.to_le_bytes());
    let [first, second] = state.buttons.to_le_bytes();
    packet[36] = first;
    packet[37] = second;
    packet[40..44].copy_from_slice(&state.sticks);
    let full = |held: bool| if held { 255 } else { 0 };
    let bit = |n: u16| state.buttons & 1 << n != 0;
    // D-pad left, down, right, up; Square, Cross, Circle, Triangle; R1, L1,
    // R2, L2.
    let analog = [7, 6, 5, 4, 15, 14, 13, 12, 11, 10, 9, 8].map(|n| full(bit(n)));
    packet[44..56].copy_from_slice(&analog);
    packet[68..76].copy_from_slice(&micros.to_le_bytes());
    for (at, value) in REST_ACCEL.iter().chain(&state.gyro).enumerate() {
        packet[76 + at * 4..80 + at * 4].copy_from_slice(&value.to_le_bytes());
    }
    seal(&mut packet, id, DATA_MESSAGE);
    packet
}

/// What Cemu asked for.
#[derive(Debug, PartialEq)]
pub enum Request {
    Version,
    /// The slots it wants to know about.
    Ports(Vec<u8>),
    Data,
}

/// A request from Cemu, `None` for anything that isn't one, as Cemu writes
/// them (DSUMessages.cpp lines 41 to 75): the header, the type, and for
/// ports a count and four slots.
pub fn read_request(packet: &[u8]) -> Option<Request> {
    if packet.len() < 20 || &packet[0..4] != CLIENT {
        return None;
    }
    let length = 16 + usize::from(u16::from_le_bytes([packet[6], packet[7]]));
    let packet = packet.get(..length)?;
    let mut zeroed = packet.to_vec();
    zeroed[8..12].copy_from_slice(&[0; 4]);
    if crc32(&zeroed) != u32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]) {
        return None;
    }
    match u32::from_le_bytes([packet[16], packet[17], packet[18], packet[19]]) {
        VERSION_MESSAGE => Some(Request::Version),
        PORTS_MESSAGE => {
            let count = i32::from_le_bytes(packet.get(20..24)?.try_into().ok()?).clamp(0, 4) as usize;
            Some(Request::Ports(packet.get(24..24 + count)?.to_vec()))
        }
        DATA_MESSAGE => Some(Request::Data),
        _ => None,
    }
}

struct Running {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// The emulator the pad plays to, once it has started, 0 before.
static GAME: AtomicU32 = AtomicU32::new(0);

/// Whether the game's window was in front with no menu of Omoio's over it,
/// and when that was last said (`hear`), in milliseconds since `EPOCH`.
static IN_FRONT: AtomicBool = AtomicBool::new(false);
static HEARD_AT: AtomicU64 = AtomicU64::new(0);
static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

/// Longer than this since `hear` was last called, the game is taken as not
/// in front: whatever watches the front has stopped.
const HEARD_WITHIN: Duration = Duration::from_millis(250);

/// The port Cemu's profile should name while the pad is being served,
/// `None` when it isn't, and the player plays on Cemu's own keyboard
/// controller instead.
pub fn port() -> Option<u16> {
    // The thread ends by itself when the game it played to does (`follow`).
    let running = RUNNING.lock().unwrap();
    running.as_ref().is_some_and(|running| !running.thread.is_finished()).then_some(PORT)
}

/// Starts serving `player`'s pad with the mouse turning what `aim` says,
/// for the game about to start. Before Cemu does, since it asks only as it
/// starts. Any pad served before stops first. Says why when it can't: the
/// player then plays on Cemu's keyboard controller, as before there was
/// this.
pub fn start(player: &crate::core::pad_layout::Player, aim: Aim) -> Result<(), String> {
    stop();
    let socket = UdpSocket::bind(("127.0.0.1", PORT))
        .map_err(|err| format!("Couldn't listen on port {PORT} for Cemu's pad: {err}"))?;
    // Read between sends, never waited on: the loop keeps time with sleep.
    socket.set_nonblocking(true).map_err(|err| err.to_string())?;
    let reader = crate::raw_input::start()?;
    GAME.store(0, Ordering::Relaxed);
    IN_FRONT.store(false, Ordering::Relaxed);
    RESTING.store(false, Ordering::Relaxed);
    let stop = Arc::new(AtomicBool::new(false));
    let pad = keyboard_pad::Pad::new(player, aim);
    let stopping = stop.clone();
    let thread = std::thread::spawn(move || {
        serve(&socket, pad, &stopping);
        reader.stop();
    });
    *RUNNING.lock().unwrap() = Some(Running { stop, thread });
    Ok(())
}

/// The game the pad plays to has started as `pid`. The pad stops when that
/// process ends.
pub fn follow(pid: u32) {
    GAME.store(pid, Ordering::Relaxed);
}

/// Whether the game hears the pad now: its window is on the screen and in
/// front, with no menu of Omoio's over it. Said every few moments by
/// cursor.rs, which keeps the mouse in the game by the same test, so the
/// pad and the cursor always agree. Otherwise the pad is at rest, which
/// keeps the game as deaf as the portal menu and Big Picture need
/// (`Features::quiet_behind`) and leaves another program the player went
/// to with every key their own.
pub fn hear(in_front: bool) {
    IN_FRONT.store(in_front, Ordering::Relaxed);
    HEARD_AT.store(EPOCH.elapsed().as_millis() as u64, Ordering::Relaxed);
}

/// Whether the cursor rests on Omoio's resting window (`rest`).
static RESTING: AtomicBool = AtomicBool::new(false);

/// Where the cursor is kept while the game is in front: one pixel in the
/// middle of the game, on a window of Omoio's, rather than anywhere in the
/// game's window, or `None` to keep it in the window as before.
///
/// Cemu takes a left click in its game window as a finger on the GamePad's
/// screen where the cursor is (`MainWindow::OnMouseLeft`, MainWindow.cpp
/// lines 1372 to 1384, bound to the picture's window at lines 1574 and
/// 1575; `InputManager::get_left_down_mouse_info`, InputManager.cpp lines
/// 838 to 883; all Cemu v2.6, read 21 October 2026). The left button attacks
/// in the keyboard layout, so in a game that uses the GamePad's screen,
/// such as SWAP Force, every attack also tapped it. Nothing the pad sends
/// stops that: Cemu tries the pad's own touch first, but a pad touches the
/// screen whenever its touch pad says it is touched (`has_position`,
/// DSUController.cpp lines 65 to 69, and `VPADController::update_touch`,
/// lines 185 to 235), and a click outside the game's picture still counts,
/// pulled in to its nearest edge (the same lines). And Cemu has no setting
/// for it. So the click must not reach Cemu at all.
///
/// The player's mouse is read through Raw Input, which hears it wherever
/// the cursor is (raw_input.rs), so the game doesn't need the cursor. It
/// rests on a window of Omoio's a few pixels big (`raw_input::rest_at`),
/// which takes the clicks without taking the front from the game and shows
/// no cursor, so the cursor is hidden over the game as well. Only once
/// Windows says that window is the one at that pixel, and only once no
/// button is held: Cemu never captures the mouse (no `CaptureMouse` in its
/// src/gui at v2.6), so a button's going up goes "to the window beneath
/// the cursor" ("WM_LBUTTONUP message", Microsoft Learn, read 21 October
/// 2026), and one that went down in Cemu's window and up on Omoio's would
/// keep a finger on the screen. The middle, since a pixel at the screen's edge could bring up
/// a taskbar that hides itself.
pub fn rest(game: Option<RECT>) -> Option<RECT> {
    let Some(game) = game else {
        raw_input::rest_at(None);
        RESTING.store(false, Ordering::Relaxed);
        return None;
    };
    let at = middle(&game);
    let ours = raw_input::rest_at(Some(at));
    let resting = may_rest(RESTING.load(Ordering::Relaxed), raw_input::buttons(), ours);
    RESTING.store(resting, Ordering::Relaxed);
    resting.then(|| RECT { left: at.x, top: at.y, right: at.x + 1, bottom: at.y + 1 })
}

/// The left and right buttons, the ones Cemu reads, whichever way round
/// Windows has them (raw input reports the buttons as they are on the
/// mouse).
const CEMUS_BUTTONS: u8 = 0b11;

fn may_rest(resting: bool, buttons: u8, ours: bool) -> bool {
    ours && (resting || buttons & CEMUS_BUTTONS == 0)
}

fn middle(rect: &RECT) -> POINT {
    POINT { x: rect.left + (rect.right - rect.left) / 2, y: rect.top + (rect.bottom - rect.top) / 2 }
}

fn heard() -> bool {
    let since = EPOCH.elapsed().saturating_sub(Duration::from_millis(HEARD_AT.load(Ordering::Relaxed)));
    IN_FRONT.load(Ordering::Relaxed) && since < HEARD_WITHIN
}

/// Stops serving, with the pad left at rest, if it is being served.
pub fn stop() {
    let running = RUNNING.lock().unwrap().take();
    if let Some(running) = running {
        running.stop.store(true, Ordering::Relaxed);
        let _ = running.thread.join();
    }
}

/// A game that hasn't started by then never will: the start failed after
/// the pad was set up.
const START_WITHIN: Duration = Duration::from_secs(60);

/// The emulator's process, watched for its end.
struct Game(HANDLE);

impl Game {
    fn open(pid: u32) -> Option<Game> {
        if pid == 0 {
            return None;
        }
        unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok().map(Game)
    }

    fn ended(&self) -> bool {
        let waited = unsafe { WaitForSingleObject(self.0, 0) };
        waited == WAIT_OBJECT_0
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn serve(socket: &UdpSocket, mut pad: keyboard_pad::Pad, stopping: &AtomicBool) {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos() ^ since.as_secs() as u32);
    let started = Instant::now();
    let mut game: Option<Game> = None;
    let mut client: Option<(SocketAddr, Instant)> = None;
    let mut number = 0u32;
    let mut send = |socket: &UdpSocket, to: SocketAddr, state: &PadState| {
        number = number.wrapping_add(1);
        let micros = started.elapsed().as_micros() as u64;
        let _ = socket.send_to(&data_answer(id, number, state, micros), to);
    };
    let mut last = Instant::now();
    let mut buffer = [0u8; 100];
    loop {
        // Everything Cemu sent since the last packet went out. A data
        // request is answered by the next packet, not at once: Cemu asks
        // again after each one it gets, and answering each would send as
        // fast as the two could go.
        while let Ok((length, from)) = socket.recv_from(&mut buffer) {
            match read_request(&buffer[..length]) {
                Some(Request::Version) => {
                    let _ = socket.send_to(&version_answer(id), from);
                }
                Some(Request::Ports(slots)) => {
                    send(socket, from, &PadState::rest());
                    for slot in slots.into_iter().filter(|&slot| slot < 8) {
                        let _ = socket.send_to(&port_answer(id, slot), from);
                    }
                }
                Some(Request::Data) => client = Some((from, Instant::now())),
                None => {}
            }
        }
        std::thread::sleep(EVERY.saturating_sub(last.elapsed()));
        let dt = last.elapsed().as_secs_f32();
        last = Instant::now();
        if game.is_none() {
            game = Game::open(GAME.load(Ordering::Relaxed));
        }
        let over = stopping.load(Ordering::Relaxed)
            || game.as_ref().map_or(started.elapsed() > START_WITHIN, Game::ended);
        let state = pad.step(&crate::raw_input::take(), dt, !over && heard());
        client = client.filter(|(_, asked)| asked.elapsed() < FORGET_AFTER);
        if let Some((to, _)) = client {
            send(socket, to, &state);
        }
        if over {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_is_the_one_cemu_checks_with() {
        // Cemu's own check of its CRC-32: bytes 0 to 88 (`CRCTest`,
        // util/crypto/crc32.cpp lines 352 to 359, v2.6).
        let bytes: Vec<u8> = (0..89).collect();
        assert_eq!(crc32(&bytes), 0x3fc6_1683);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926, "the common check value");
    }

    /// Whether Cemu would take `packet` as a server's answer of `size`
    /// bytes: its magic, version, length and CRC.
    fn cemu_takes(packet: &[u8], size: usize, message: u32) {
        assert_eq!(packet.len(), size);
        assert_eq!(&packet[0..4], b"DSUS");
        assert_eq!(u16::from_le_bytes([packet[4], packet[5]]), 1001);
        assert_eq!(usize::from(u16::from_le_bytes([packet[6], packet[7]])), size - 16);
        let mut zeroed = packet.to_vec();
        zeroed[8..12].copy_from_slice(&[0; 4]);
        assert_eq!(crc32(&zeroed).to_le_bytes(), packet[8..12]);
        assert_eq!(u32::from_le_bytes(packet[16..20].try_into().unwrap()), message);
    }

    #[test]
    fn answers_are_the_sizes_cemu_checks() {
        cemu_takes(&version_answer(7), 24, 0x100000);
        let port = port_answer(7, 0);
        cemu_takes(&port, 32, 0x100001);
        assert_eq!(port[20..24], [0, 2, 2, 1], "slot 0, connected");
        assert_eq!(port_answer(7, 3)[20..22], [3, 0], "another slot is not connected");
        cemu_takes(&data_answer(7, 1, &PadState::rest(), 0), 100, 0x100002);
    }

    #[test]
    fn data_sits_where_cemu_reads_it() {
        let state = PadState {
            buttons: 1 << 14 | 1 << 4,
            sticks: [255, 0, 128, 200],
            gyro: [1.5, -2.0, 0.0],
        };
        let packet = data_answer(9, 42, &state, 123_456);
        assert_eq!(packet[20..22], [0, 2], "slot 0, connected");
        assert_eq!(u32::from_le_bytes(packet[32..36].try_into().unwrap()), 42);
        assert_eq!(packet[36], 1 << 4, "d-pad up in the first byte");
        assert_eq!(packet[37], 1 << 6, "Cross in the second");
        assert_eq!(packet[40..44], [255, 0, 128, 200], "the sticks at 0x28");
        assert_eq!(packet[47], 255, "d-pad up again, from 0 to 255");
        assert_eq!(packet[49], 255, "and Cross");
        assert_eq!(u64::from_le_bytes(packet[68..76].try_into().unwrap()), 123_456, "time at 0x44");
        let float = |at: usize| f32::from_le_bytes(packet[at..at + 4].try_into().unwrap());
        assert_eq!([float(76), float(80), float(84)], REST_ACCEL, "gravity at 0x4C");
        assert_eq!([float(88), float(92), float(96)], [1.5, -2.0, 0.0], "turning at 0x58");
    }

    /// A request as Cemu builds one (DSUMessages.cpp lines 41 to 75).
    fn cemu_asks(message: u32, body: &[u8]) -> Vec<u8> {
        let mut packet = b"DSUC".to_vec();
        packet.extend(1001u16.to_le_bytes());
        packet.extend(((4 + body.len()) as u16).to_le_bytes());
        packet.extend([0; 4]);
        packet.extend(1234u32.to_le_bytes());
        packet.extend(message.to_le_bytes());
        packet.extend(body);
        let crc = crc32(&packet);
        packet[8..12].copy_from_slice(&crc.to_le_bytes());
        packet
    }

    /// Serves on the real port and reads the keyboard and mouse, so it is
    /// run by hand, not with the rest: `cargo test --lib dsu -- --ignored`.
    #[test]
    #[ignore]
    fn a_client_asking_as_cemu_does_gets_the_pad_at_its_pace() {
        let player = crate::core::pad_layout::Player::on(crate::core::pad_layout::Pad::keyboard());
        start(&player, Aim::Layout).unwrap();
        assert_eq!(port(), Some(PORT));
        let cemu = UdpSocket::bind("127.0.0.1:0").unwrap();
        cemu.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let server = ("127.0.0.1", PORT);
        cemu.send_to(&cemu_asks(0x100001, &[4, 0, 0, 0, 0, 1, 2, 3]), server).unwrap();
        let mut buffer = [0u8; 200];
        let got = |buffer: &mut [u8; 200]| cemu.recv(buffer).unwrap();
        assert_eq!(got(&mut buffer), 100, "a data packet at rest before the ports");
        assert_eq!(buffer[40..44], [128; 4]);
        for slot in 0..4 {
            assert_eq!(got(&mut buffer), 32);
            cemu_takes(&buffer[..32], 32, 0x100001);
            assert_eq!(buffer[20..22], [slot, if slot == 0 { 2 } else { 0 }]);
        }
        cemu.send_to(&cemu_asks(0x100002, &[1, 0, 0, 0, 0, 0, 0, 0]), server).unwrap();
        let begun = Instant::now();
        let mut numbers = Vec::new();
        while numbers.len() < 50 {
            assert_eq!(got(&mut buffer), 100);
            cemu_takes(&buffer[..100], 100, 0x100002);
            numbers.push(u32::from_le_bytes(buffer[32..36].try_into().unwrap()));
            assert_eq!(buffer[36..38], [0, 0], "at rest: nothing says the game is in front");
            // Cemu asks again after each packet it takes.
            cemu.send_to(&cemu_asks(0x100002, &[1, 0, 0, 0, 0, 0, 0, 0]), server).unwrap();
        }
        let took = begun.elapsed();
        assert!(numbers.windows(2).all(|pair| pair[1] == pair[0] + 1), "{numbers:?}");
        assert!(took > Duration::from_millis(150) && took < Duration::from_millis(400), "50 packets in {took:?}");
        stop();
        assert_eq!(port(), None);
    }

    #[test]
    fn the_cursor_rests_only_on_omoios_window_and_never_mid_click() {
        assert!(may_rest(false, 0, true));
        assert!(!may_rest(false, 0, false), "Windows doesn't show Omoio's window there yet");
        assert!(!may_rest(false, 0b01, true), "the left button went down in Cemu's window");
        assert!(!may_rest(false, 0b10, true), "and the right one");
        assert!(may_rest(false, 0b100, true), "Cemu doesn't read the middle button");
        assert!(may_rest(true, 0b01, true), "already resting, a click lands on Omoio's window");
        assert!(!may_rest(true, 0, false));
    }

    #[test]
    fn the_cursor_rests_in_the_middle_of_the_game() {
        let game = RECT { left: 212, top: 40, right: 1912, bottom: 1000 };
        assert_eq!(middle(&game), POINT { x: 1062, y: 520 });
        let tiny = RECT { left: 5, top: 5, right: 6, bottom: 6 };
        assert_eq!(middle(&tiny), POINT { x: 5, y: 5 }, "still inside");
    }

    #[test]
    fn cemus_requests_are_read() {
        assert_eq!(read_request(&cemu_asks(0x100000, &[])), Some(Request::Version));
        let ports = cemu_asks(0x100001, &[4, 0, 0, 0, 0, 1, 2, 3]);
        assert_eq!(read_request(&ports), Some(Request::Ports(vec![0, 1, 2, 3])));
        let data = cemu_asks(0x100002, &[1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(read_request(&data), Some(Request::Data));
        let mut broken = data.clone();
        broken[22] ^= 1;
        assert_eq!(read_request(&broken), None, "a wrong CRC");
        assert_eq!(read_request(&data_answer(1, 1, &PadState::rest(), 0)), None, "a server's packet");
        assert_eq!(read_request(&data[..10]), None);
    }
}
