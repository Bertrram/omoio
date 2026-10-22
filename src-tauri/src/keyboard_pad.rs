//! The pad a keyboard-and-mouse player plays a Wii U game through.
//!
//! Cemu reads no mouse (`InputAPI::Type`, input/api/InputAPI.h lines 7 to
//! 21, v2.6), so with its own keyboard controller the mouse moving could
//! only be the arrow keys, and the mouse's buttons nothing at all. Cemu does
//! read pads over the network, through the protocol DS4Windows and
//! BetterJoy speak (dsu.rs). So Omoio reads the keyboard and mouse itself
//! (raw_input.rs) and plays them to Cemu as one such pad: each place of the
//! player's layout is a button or stick direction of that pad, and the mouse
//! moving turns a stick by how fast it moves, the way a thumb would.
//!
//! This is the part with no input or output of its own: from the keys and
//! mouse as they are, to the pad as Cemu should see it.

use crate::core::figures;
use crate::core::keys;
use crate::core::pad_layout::{Player, INPUTS};
use crate::dsu::PadState;
use std::collections::HashSet;

/// What the mouse moving turns in a game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    /// The places the player's layout puts it on, the right stick unless
    /// they moved it.
    Layout,
    LeftStick,
    RightStick,
    /// The GamePad's tilt, through the motion the pad reports.
    Tilt,
}

/// The choices the game settings sheet offers, in the words it shows.
pub const AIM_CHOICES: [&str; 3] = ["Right stick", "Left stick", "GamePad tilt"];

impl Aim {
    /// What the mouse turns in a game: what was chosen for it, else the
    /// left stick in a Skylanders game, else what the layout says.
    ///
    /// Skylanders on the Wii U walks with the left stick and gives the right
    /// stick no job at all (the Wii U column of darkspyro.net's controls for
    /// Trap Team, /trapteam/console/, read 20 October 2026), so the mouse on
    /// the right stick, where the layout puts it, would do nothing there. Its
    /// turret sections, where balloons and ships are shot down, are steered
    /// with a stick too; which one, no guide or manual found said, so the
    /// left stick is a guess to be checked on the real game.
    pub fn for_game(chosen: Option<&str>, title: &str) -> Aim {
        match chosen {
            Some("Right stick") => Aim::RightStick,
            Some("Left stick") => Aim::LeftStick,
            Some("GamePad tilt") => Aim::Tilt,
            _ if figures::game_from_title(title).is_some() => Aim::LeftStick,
            _ => Aim::Layout,
        }
    }
}

/// Where a place of the layout lands on the pad: one of its sixteen buttons,
/// numbered as Cemu numbers them, or a stick direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Output {
    Button(u8),
    /// Sticks by axis: 0 left stick across, 1 left stick up, 2 right stick
    /// across, 3 right stick up. `up` for right and up.
    Stick { axis: usize, up: bool },
}

impl Output {
    /// Cemu's number for it in a profile. A button is the bit it travels as
    /// in the packet's two button bytes, the first counting from 0 and the
    /// second from 8, and a stick direction is the `Buttons2` entry for it
    /// (`DSUController::raw_state`, input/api/DSU/DSUController.cpp lines 142
    /// to 157, and `Buttons2`, input/api/Controller.h lines 12 to 82, v2.6,
    /// read 20 October 2026). Cemu reads a stick pushed up as positive, so
    /// up is the positive entry, as for XInput.
    pub fn cemu_number(self) -> u64 {
        match self {
            Output::Button(bit) => u64::from(bit),
            Output::Stick { axis, up: true } => 38 + axis as u64,
            Output::Stick { axis, up: false } => 44 + axis as u64,
        }
    }
}

/// Where each place of a layout goes, a PlayStation pad's buttons by place,
/// as the protocol names them (cemuhook-protocol, "Controller data", read
/// 20 October 2026; the same bits in `DSUController::get_button_name`):
/// first byte Share, L3, R3, Options, then the d-pad up, right, down, left;
/// second byte L2, R2, L1, R1, Triangle, Circle, Cross, Square. Guide has
/// none: the protocol carries a Home byte, but Cemu doesn't read it.
pub fn output(place: &str) -> Option<Output> {
    let stick = |axis, up| Some(Output::Stick { axis, up });
    let button = |bit| Some(Output::Button(bit));
    match place {
        "Back" => button(0),
        "LS" => button(1),
        "RS" => button(2),
        "Start" => button(3),
        "Up" => button(4),
        "Right" => button(5),
        "Down" => button(6),
        "Left" => button(7),
        "LT" => button(8),
        "RT" => button(9),
        "LB" => button(10),
        "RB" => button(11),
        "North" => button(12),
        "East" => button(13),
        "South" => button(14),
        "West" => button(15),
        "LS X+" => stick(0, true),
        "LS X-" => stick(0, false),
        "LS Y+" => stick(1, true),
        "LS Y-" => stick(1, false),
        "RS X+" => stick(2, true),
        "RS X-" => stick(2, false),
        "RS Y+" => stick(3, true),
        "RS Y-" => stick(3, false),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Dir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Input {
    /// A key by its scan code, as core/keys.rs keeps it.
    Key(u16),
    /// A mouse button by its bit in `Snapshot::buttons`.
    Mouse(u8),
    Wheel { up: bool },
    Move(Dir),
}

fn parse(input: &str) -> Option<Input> {
    if let Some(scan) = keys::scan_code(input) {
        return Some(Input::Key(scan));
    }
    Some(match input {
        "MouseLeft" => Input::Mouse(0),
        "MouseRight" => Input::Mouse(1),
        "MouseMiddle" => Input::Mouse(2),
        "MouseBack" => Input::Mouse(3),
        "MouseForward" => Input::Mouse(4),
        "WheelUp" => Input::Wheel { up: true },
        "WheelDown" => Input::Wheel { up: false },
        "MouseMoveLeft" => Input::Move(Dir::Left),
        "MouseMoveRight" => Input::Move(Dir::Right),
        "MouseMoveUp" => Input::Move(Dir::Up),
        "MouseMoveDown" => Input::Move(Dir::Down),
        _ => return None,
    })
}

/// What one way of the mouse moving does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Turn {
    /// Presses a button while the mouse moves that way briskly.
    Press(u8),
    Stick { axis: usize, up: bool },
    /// Turns the GamePad: about its upright axis (`yaw`) or across.
    Tilt { yaw: bool, positive: bool },
}

fn turn(output: Output, aim: Aim) -> Turn {
    match (output, aim) {
        (Output::Button(bit), _) => Turn::Press(bit),
        (Output::Stick { axis, up }, Aim::Layout) => Turn::Stick { axis, up },
        (Output::Stick { axis, up }, Aim::LeftStick) => Turn::Stick { axis: axis % 2, up },
        (Output::Stick { axis, up }, Aim::RightStick) => Turn::Stick { axis: axis % 2 + 2, up },
        (Output::Stick { axis, up }, Aim::Tilt) => Turn::Tilt { yaw: axis % 2 == 0, positive: up },
    }
}

/// What the keyboard and mouse are doing, as raw_input.rs reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// The keys held, by scan code.
    pub keys: HashSet<u16>,
    /// The mouse buttons held: left, right, middle, back, forward, from bit 0.
    pub buttons: u8,
    /// Wheel notches turned since the reader started, each way.
    pub wheel_up: u32,
    pub wheel_down: u32,
    /// How far the mouse moved since the last snapshot, in its own counts,
    /// right and down positive.
    pub dx: i32,
    pub dy: i32,
}

/// How long the mouse's last movement keeps counting. Movement is added up
/// and fades by this time constant, so the stick is back at rest about a
/// seventh of a second after a brisk sweep stops, and a mouse that reports
/// 125 times a second, with nothing in between, still turns it smoothly.
const FADE: f32 = 0.040;

/// Slower than this, in counts a second, the mouse is taken as still: a hand
/// resting on it shouldn't walk the character.
const STILL: f32 = 40.0;

/// The least the stick moves once the mouse does. Games ignore a stick
/// barely off centre; RPCS3 starts its mouse at 60 of 255 for the same
/// reason (`Mouse Deadzone`, pad_config.h lines 146 to 155, build
/// 0.0.43-20344). A quarter is that, and Cemu's profile for this pad has no
/// dead zone of its own (`DSU_SETTINGS` in backends/cemu/controllers.rs).
const LEAST: f32 = 0.25;

/// How fast the mouse moves, in counts a second, for the stick to be pushed
/// all the way at the default speed: a brisk sweep of about five
/// centimetres a second on an 800 counts-per-inch mouse.
const FULL: f32 = 1500.0;

/// How fast the GamePad turns for each count a second of the mouse, in
/// degrees a second: the same brisk sweep turns it 120 degrees a second.
const TILT: f32 = 120.0 / FULL;

/// A mouse direction held as a button counts while the stick it would push
/// is more than half way.
const PRESS_AT: f32 = 0.5;

/// A wheel notch presses for this long, then lets go for the gap, so two
/// notches are two presses even to a game that looks once a frame.
const WHEEL_PRESS: f32 = 0.050;
const WHEEL_GAP: f32 = 0.035;
/// Notches beyond this are dropped rather than pressed for seconds after a
/// fast spin.
const WHEEL_QUEUE: u32 = 3;

#[derive(Default)]
struct Notches {
    waiting: u32,
    into: f32,
}

impl Notches {
    fn add(&mut self, turned: u32) {
        self.waiting = (self.waiting + turned).min(WHEEL_QUEUE);
    }

    fn pressed(&mut self, dt: f32) -> bool {
        if self.waiting == 0 {
            return false;
        }
        self.into += dt;
        if self.into < WHEEL_PRESS {
            return true;
        }
        if self.into >= WHEEL_PRESS + WHEEL_GAP {
            self.waiting -= 1;
            self.into = 0.0;
        }
        false
    }
}

/// The pad, kept from one snapshot to the next.
pub struct Pad {
    presses: Vec<(Output, Input)>,
    moves: Vec<(Dir, Turn)>,
    /// The player's mouse speed, 1.0 at the default.
    speed: f32,
    /// The mouse's movement, added up and fading (`FADE`), across and down.
    moved: [f32; 2],
    wheel: [Notches; 2],
    wheel_seen: (u32, u32),
    /// Keys and buttons held when the game last got the front back, which
    /// count only once let go and pressed again.
    blocked_keys: HashSet<u16>,
    blocked_buttons: u8,
    open: bool,
}

impl Pad {
    /// The pad for `player`, with the mouse moving what `aim` says.
    pub fn new(player: &Player, aim: Aim) -> Pad {
        let mut presses = Vec::new();
        let mut moves = Vec::new();
        for place in INPUTS {
            let Some(out) = output(place) else {
                continue;
            };
            for input in player.keys(place).filter_map(parse) {
                match input {
                    Input::Move(dir) => moves.push((dir, turn(out, aim))),
                    other => presses.push((out, other)),
                }
            }
        }
        Pad {
            presses,
            moves,
            speed: player.mouse as f32 / keys::DEFAULT_MOUSE as f32,
            moved: [0.0; 2],
            wheel: Default::default(),
            wheel_seen: (0, 0),
            blocked_keys: HashSet::new(),
            blocked_buttons: 0,
            open: false,
        }
    }

    /// Whether the mouse's left or right button is a button in the game,
    /// the two Cemu also takes as a touch on the GamePad's screen.
    pub fn clicks(&self) -> bool {
        self.presses.iter().any(|(_, input)| matches!(input, Input::Mouse(0 | 1)))
    }

    /// The pad after `dt` seconds more of `input`. Closed, while the game
    /// isn't the window in front, it is the pad at rest, and whatever is
    /// held as it opens again waits to be let go.
    pub fn step(&mut self, input: &Snapshot, dt: f32, open: bool) -> PadState {
        let fresh = (
            input.wheel_up.wrapping_sub(self.wheel_seen.0),
            input.wheel_down.wrapping_sub(self.wheel_seen.1),
        );
        self.wheel_seen = (input.wheel_up, input.wheel_down);
        if !open {
            self.open = false;
            self.moved = [0.0; 2];
            self.wheel = Default::default();
            return PadState::rest();
        }
        if !self.open {
            self.open = true;
            self.blocked_keys = input.keys.clone();
            self.blocked_buttons = input.buttons;
        }
        self.blocked_keys.retain(|key| input.keys.contains(key));
        self.blocked_buttons &= input.buttons;
        self.wheel[0].add(fresh.0);
        self.wheel[1].add(fresh.1);
        let wheel = [self.wheel[0].pressed(dt), self.wheel[1].pressed(dt)];

        let held = |each: &Input| match *each {
            Input::Key(scan) => input.keys.contains(&scan) && !self.blocked_keys.contains(&scan),
            Input::Mouse(bit) => (input.buttons & !self.blocked_buttons) & (1 << bit) != 0,
            Input::Wheel { up } => wheel[usize::from(!up)],
            Input::Move(_) => false,
        };

        let mut state = PadState::rest();
        // Each stick direction as far as anything pushes it, positive then
        // negative, so a key and the mouse on one direction never add up
        // past the edge.
        let mut push = [[0.0f32; 2]; 4];
        for (out, each) in &self.presses {
            if !held(each) {
                continue;
            }
            match *out {
                Output::Button(bit) => state.buttons |= 1 << bit,
                Output::Stick { axis, up } => push[axis][usize::from(!up)] = 1.0,
            }
        }

        let velocity = self.velocity(input.dx, input.dy, dt);
        let deflection = deflection(velocity, self.speed);
        for &(dir, turn) in &self.moves {
            let (amount, rate) = match dir {
                Dir::Right => (deflection[0].max(0.0), velocity[0].max(0.0)),
                Dir::Left => ((-deflection[0]).max(0.0), (-velocity[0]).max(0.0)),
                Dir::Up => (deflection[1].max(0.0), velocity[1].max(0.0)),
                Dir::Down => ((-deflection[1]).max(0.0), (-velocity[1]).max(0.0)),
            };
            match turn {
                Turn::Press(bit) if amount > PRESS_AT => state.buttons |= 1 << bit,
                Turn::Press(_) => {}
                Turn::Stick { axis, up } => {
                    let side = &mut push[axis][usize::from(!up)];
                    *side = side.max(amount);
                }
                Turn::Tilt { yaw, positive } if amount > 0.0 => {
                    let turned = rate * self.speed * TILT * if positive { 1.0 } else { -1.0 };
                    // The protocol's gyro is pitch, yaw, roll.
                    state.gyro[usize::from(yaw)] += turned;
                }
                Turn::Tilt { .. } => {}
            }
        }
        for (axis, [ahead, back]) in push.iter().enumerate() {
            state.sticks[axis] = stick_byte(ahead - back);
        }
        state
    }

    /// The mouse's speed now, in counts a second, right and up positive.
    fn velocity(&mut self, dx: i32, dy: i32, dt: f32) -> [f32; 2] {
        let keep = (-dt / FADE).exp();
        self.moved[0] = self.moved[0] * keep + dx as f32;
        self.moved[1] = self.moved[1] * keep - dy as f32;
        // Added up and fading, a steady speed v settles at v times `FADE`.
        [self.moved[0] / FADE, self.moved[1] / FADE]
    }
}

/// How far the mouse at `velocity` pushes a stick, each way, from -1 to 1:
/// nothing while it is still, then from `LEAST` up to all the way at `FULL`
/// times the player's `speed`, in the direction it moves.
fn deflection(velocity: [f32; 2], speed: f32) -> [f32; 2] {
    let fast = velocity[0].hypot(velocity[1]);
    if fast < STILL {
        return [0.0; 2];
    }
    let amount = (LEAST + (1.0 - LEAST) * fast * speed / FULL).min(1.0);
    [velocity[0] / fast * amount, velocity[1] / fast * amount]
}

/// A stick position from -1 to 1 as the protocol sends it: 128 at rest, 255
/// all the way right or up, 0 all the way left or down (cemuhook-protocol,
/// "Controller data", read 20 October 2026).
fn stick_byte(at: f32) -> u8 {
    let at = at.clamp(-1.0, 1.0);
    if at >= 0.0 {
        (128.0 + at * 127.0).round() as u8
    } else {
        (128.0 + at * 128.0).round() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pad_layout::Pad as Device;
    use std::collections::BTreeMap;

    const TICK: f32 = 0.004;

    fn keyboard() -> Player {
        Player::on(Device::keyboard())
    }

    fn holding(keys: &[&str]) -> Snapshot {
        Snapshot {
            keys: keys.iter().map(|key| keys::scan_code(key).unwrap()).collect(),
            ..Snapshot::default()
        }
    }

    /// The pad open with nothing held first, as when the game takes the front.
    fn opened(player: &Player, aim: Aim) -> Pad {
        let mut pad = Pad::new(player, aim);
        pad.step(&Snapshot::default(), TICK, true);
        pad
    }

    #[test]
    fn every_place_but_guide_lands_somewhere_and_nowhere_twice() {
        let outs: Vec<Output> = INPUTS.iter().filter_map(|place| output(place)).collect();
        assert_eq!(outs.len(), INPUTS.len() - 1);
        assert_eq!(output("Guide"), None);
        let numbers: HashSet<u64> = outs.iter().map(|out| out.cemu_number()).collect();
        assert_eq!(numbers.len(), outs.len());
    }

    #[test]
    fn cemu_numbers_follow_its_dsu_buttons_and_stick_entries() {
        assert_eq!(output("South").unwrap().cemu_number(), 14, "Cross");
        assert_eq!(output("East").unwrap().cemu_number(), 13, "Circle");
        assert_eq!(output("LT").unwrap().cemu_number(), 8, "L2, which Cemu calls ZL");
        assert_eq!(output("Back").unwrap().cemu_number(), 0, "Share");
        assert_eq!(output("LS Y+").unwrap().cemu_number(), 39, "kAxisYP");
        assert_eq!(output("LS X-").unwrap().cemu_number(), 44, "kAxisXN");
        assert_eq!(output("RS Y-").unwrap().cemu_number(), 47, "kRotationYN");
    }

    #[test]
    fn the_default_keys_press_their_places() {
        let mut pad = opened(&keyboard(), Aim::Layout);
        let state = pad.step(&holding(&["Space", "KeyW", "KeyD", "Digit1"]), TICK, true);
        assert_eq!(state.buttons, 1 << 14 | 1 << 4, "Space is South, 1 the d-pad up");
        assert_eq!(state.sticks, [255, 255, 128, 128], "W and D push the left stick up and right");
        let state = pad.step(&holding(&["KeyA", "KeyS"]), TICK, true);
        assert_eq!(state.sticks[..2], [0, 0]);
        let state = pad.step(&holding(&["KeyA", "KeyD"]), TICK, true);
        assert_eq!(state.sticks[0], 128, "both ways at once is neither");
    }

    #[test]
    fn a_click_plays_unless_the_mouse_buttons_are_taken_off() {
        assert!(Pad::new(&keyboard(), Aim::Layout).clicks());
        let keys_only = BTreeMap::from([
            ("West".to_string(), "KeyJ".to_string()),
            ("North".to_string(), "KeyK".to_string()),
        ]);
        let player = Player::with_buttons(Device::keyboard(), keys_only);
        assert!(!Pad::new(&player, Aim::Layout).clicks(), "the middle button alone is no touch in Cemu");
    }

    #[test]
    fn mouse_buttons_and_the_wheel_press_their_places() {
        let buttons = BTreeMap::from([("LB".to_string(), "WheelUp".to_string())]);
        let mut pad = opened(&Player::with_buttons(Device::keyboard(), buttons), Aim::Layout);
        let input = Snapshot { buttons: 0b11, wheel_up: 2, ..Snapshot::default() };
        let state = pad.step(&input, TICK, true);
        assert_eq!(state.buttons, 1 << 15 | 1 << 12 | 1 << 10, "left is West, right North, the wheel LB");
        // Two notches are two presses with a gap between.
        let mut presses = 0;
        let mut was = true;
        for _ in 0..100 {
            let now = pad.step(&input, TICK, true).buttons & 1 << 10 != 0;
            if now && !was {
                presses += 1;
            }
            was = now;
        }
        assert_eq!(presses, 1, "the second after the first");
        assert_eq!(pad.step(&input, TICK, true).buttons & 1 << 10, 0, "then no more");
    }

    #[test]
    fn closed_it_rests_and_what_was_held_waits_to_be_let_go() {
        let mut pad = opened(&keyboard(), Aim::Layout);
        let space = holding(&["Space"]);
        assert_ne!(pad.step(&space, TICK, true).buttons, 0);
        assert_eq!(pad.step(&space, TICK, false), PadState::rest(), "the game isn't in front");
        assert_eq!(pad.step(&space, TICK, true).buttons, 0, "held since before it came back");
        assert_eq!(pad.step(&Snapshot::default(), TICK, true).buttons, 0);
        assert_ne!(pad.step(&space, TICK, true).buttons, 0, "pressed again, it counts");
    }

    fn moving(dx: i32, dy: i32) -> Snapshot {
        Snapshot { dx, dy, ..Snapshot::default() }
    }

    /// The pad after the mouse moved steadily at `per_second` counts a second
    /// for a quarter of a second, in whole counts as a mouse reports them.
    fn swept(pad: &mut Pad, per_second: (f32, f32)) -> PadState {
        let at = |tick: i32| {
            let seconds = TICK * tick as f32;
            ((per_second.0 * seconds) as i32, (per_second.1 * seconds) as i32)
        };
        (1..=62)
            .map(|tick| {
                let (now, before) = (at(tick), at(tick - 1));
                pad.step(&moving(now.0 - before.0, now.1 - before.1), TICK, true)
            })
            .last()
            .unwrap()
    }

    #[test]
    fn the_mouse_turns_the_right_stick_by_how_fast_it_moves() {
        let mut pad = opened(&keyboard(), Aim::Layout);
        let brisk = swept(&mut pad, (1500.0, 0.0));
        assert!(brisk.sticks[2] >= 250, "{:?}", brisk.sticks);
        assert_eq!(brisk.sticks[..2], [128, 128], "the left stick stays");
        let mut pad = opened(&keyboard(), Aim::Layout);
        let slow = swept(&mut pad, (-250.0, 0.0));
        assert!((60..=90).contains(&slow.sticks[2]), "a slow sweep left is part way: {:?}", slow.sticks);
        let mut pad = opened(&keyboard(), Aim::Layout);
        let up = swept(&mut pad, (0.0, -1500.0));
        assert!(up.sticks[3] >= 250, "the mouse moving up, negative on screen, is the stick up: {:?}", up.sticks);
    }

    #[test]
    fn the_stick_comes_back_soon_after_the_mouse_stops_and_ignores_a_resting_hand() {
        let mut pad = opened(&keyboard(), Aim::Layout);
        swept(&mut pad, (1500.0, 0.0));
        let after: Vec<u8> = (0..40).map(|_| pad.step(&Snapshot::default(), TICK, true).sticks[2]).collect();
        assert!(after[10] > 128, "still turning a moment after: {after:?}");
        assert_eq!(after[39], 128, "back within 160 ms: {after:?}");
        let mut pad = opened(&keyboard(), Aim::Layout);
        assert_eq!(swept(&mut pad, (25.0, 0.0)).sticks[2], 128);
    }

    #[test]
    fn a_faster_mouse_setting_pushes_further() {
        let mut slow = opened(&keyboard().with_mouse(50), Aim::Layout);
        let mut fast = opened(&keyboard().with_mouse(200), Aim::Layout);
        let (slow, fast) = (swept(&mut slow, (500.0, 0.0)), swept(&mut fast, (500.0, 0.0)));
        assert!(fast.sticks[2] > slow.sticks[2] + 40, "{} {}", fast.sticks[2], slow.sticks[2]);
    }

    #[test]
    fn a_game_can_have_the_mouse_on_the_left_stick_or_the_gamepads_tilt() {
        let mut pad = opened(&keyboard(), Aim::LeftStick);
        let state = swept(&mut pad, (1500.0, 0.0));
        assert!(state.sticks[0] >= 250 && state.sticks[2] == 128, "{:?}", state.sticks);
        let mut pad = opened(&keyboard(), Aim::Tilt);
        let state = swept(&mut pad, (1500.0, 0.0));
        assert_eq!(state.sticks, [128; 4]);
        assert!(state.gyro[1] > 100.0 && state.gyro[0] == 0.0, "turned about the upright axis: {:?}", state.gyro);
        let still = (0..40).map(|_| pad.step(&Snapshot::default(), TICK, true)).last().unwrap();
        assert_eq!(still.gyro, [0.0; 3], "and stops turning when the mouse stops");
    }

    #[test]
    fn skylanders_aims_with_the_left_stick_unless_chosen_otherwise() {
        assert_eq!(Aim::for_game(None, "Skylanders Trap Team"), Aim::LeftStick);
        assert_eq!(Aim::for_game(None, "Skylanders SWAP Force"), Aim::LeftStick);
        assert_eq!(Aim::for_game(Some("Right stick"), "Skylanders Trap Team"), Aim::RightStick);
        assert_eq!(Aim::for_game(None, "Mario Kart 8"), Aim::Layout);
        assert_eq!(Aim::for_game(Some("GamePad tilt"), "Mario Kart 8"), Aim::Tilt);
        for choice in AIM_CHOICES {
            assert_ne!(Aim::for_game(Some(choice), "Mario Kart 8"), Aim::Layout, "{choice}");
        }
    }

    #[test]
    fn a_mouse_direction_on_a_button_presses_it_while_moving_briskly() {
        let buttons = BTreeMap::from([("RB".to_string(), "MouseMoveRight".to_string())]);
        let player = Player::with_buttons(Device::keyboard(), buttons);
        let mut pad = opened(&player, Aim::Layout);
        let state = swept(&mut pad, (1500.0, 0.0));
        assert_ne!(state.buttons & 1 << 11, 0, "RB");
        assert!(state.sticks[2] >= 250, "and the stick it was on still turns");
        let mut pad = opened(&player, Aim::Layout);
        assert_eq!(swept(&mut pad, (200.0, 0.0)).buttons & 1 << 11, 0, "not while it creeps");
    }
}
