//! Toy figures, as the emulators' own figure makers describe them, and the
//! facts Omoio keeps about each id to sort them: the element, what kind of
//! figure it is, and the game it came out with. The emulators list names and
//! numbers only. These facts were checked against Dolphin's figure list,
//! which has them for every figure; the table here is Omoio's own.

use crate::core::console::{Console, Features};
use crate::core::imaginators::{self, BattleClass, Casing, Crystal};
use crate::core::vehicles::{self, Terrain};
use serde::{Deserialize, Serialize};

/// The element a figure belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Element {
    Air,
    Earth,
    Fire,
    Water,
    Life,
    Undead,
    Magic,
    Tech,
    Light,
    Dark,
    /// Kaos's own, which Imaginators gives the Kaos Sensei alone.
    Kaos,
}

/// What a figure is, which decides where the menu lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Character,
    Item,
    Trap,
    Adventure,
    Vehicle,
    Trophy,
    /// An Imaginators Creation Crystal, which an Imaginator is made in.
    Crystal,
}

/// The games in the order they came out. A game reads the figures of its own
/// year and of every earlier one, never those of a later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    Spyro,
    Giants,
    SwapForce,
    TrapTeam,
    SuperChargers,
    Imaginators,
}

impl Game {
    /// The game's name as its box gives it, for a sentence about it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Spyro => "Skylanders Spyro's Adventure",
            Self::Giants => "Skylanders Giants",
            Self::SwapForce => "Skylanders SWAP Force",
            Self::TrapTeam => "Skylanders Trap Team",
            Self::SuperChargers => "Skylanders SuperChargers",
            Self::Imaginators => "Skylanders Imaginators",
        }
    }
}

/// Skylanders games are the ones with a portal to fill.
pub fn is_skylanders(title: &str) -> bool {
    title.to_lowercase().contains("skylanders")
}

/// Which game a title is, from its name. `None` for one Omoio can't tell,
/// which then shows every figure.
pub fn game_from_title(title: &str) -> Option<Game> {
    let title = title.to_lowercase();
    [
        ("imaginators", Game::Imaginators),
        ("superchargers", Game::SuperChargers),
        ("trap team", Game::TrapTeam),
        ("swap force", Game::SwapForce),
        ("giants", Game::Giants),
        ("spyro", Game::Spyro),
    ]
    .into_iter()
    .find(|(name, _)| title.contains(name))
    .map(|(_, game)| game)
}

/// Whether the portal menu works in this game on this console: the games it
/// has been played through with on each, which the README names, and
/// SuperChargers and Imaginators on the Wii U, where each comes first. Their
/// PS3 versions follow once they have been played there; Imaginators' once
/// it is known whether the PS3 game checks its figures' signature too, since
/// RPCS3 has no patch that takes that check away. Spyro's Adventure on the
/// PS3 has it too, though in one test on 7 October 2026 a figure made new
/// there, Voodood, left the game on the portal screen. That was never
/// settled, and the menu is offered so players can say how it goes. On the
/// Wii, Spyro's Adventure comes first, through Dolphin; the Wii's other
/// Skylanders games follow once each has been played there.
pub fn has_portal_menu(console: Console, title: &str) -> bool {
    is_skylanders(title)
        && matches!(
            (console, game_from_title(title)),
            (Console::Ps3, Some(Game::Spyro | Game::Giants | Game::SwapForce | Game::TrapTeam))
                | (Console::WiiU, Some(Game::SwapForce | Game::TrapTeam | Game::SuperChargers | Game::Imaginators))
                | (Console::Wii, Some(Game::Spyro))
        )
}

/// Whether a game in the library shows the portal menu: its emulator fills
/// the portal, and the menu works in the game. The game's page and Big
/// Picture ask this, so neither guesses from the title.
pub fn offers_portal_menu(features: Features, console: Console, title: &str) -> bool {
    features.portal && has_portal_menu(console, title)
}

/// For a Skylanders game the portal menu doesn't work in on this console,
/// the consoles whose version of it has the menu, which may be none. `None`
/// for a game the menu works in, and for one that isn't Skylanders, which
/// has no portal. Asked of `has_portal_menu` console by console, so that
/// stays the one place that says where the menu works.
pub fn portal_menu_elsewhere(console: Console, title: &str) -> Option<Vec<Console>> {
    if !is_skylanders(title) || has_portal_menu(console, title) {
        return None;
    }
    Some(Console::ALL.into_iter().filter(|&other| has_portal_menu(other, title)).collect())
}

/// "the Wii U version", "the PS3 and Wii U versions".
pub fn versions(consoles: &[Console]) -> String {
    let names: Vec<&str> = consoles.iter().map(|console| console.short()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => format!("the {one} version"),
        [rest @ .., last] => format!("the {} and {last} versions", rest.join(", ")),
    }
}

/// The line a Skylanders game's page shows when the portal menu doesn't
/// work in it: "The portal menu doesn't work in this version yet. It works
/// in the Wii U version." `None` when the menu works, or there is no portal.
pub fn portal_menu_note(console: Console, title: &str) -> Option<String> {
    let elsewhere = portal_menu_elsewhere(console, title)?;
    Some(if elsewhere.is_empty() {
        "The portal menu doesn't work in this game yet.".to_string()
    } else {
        format!("The portal menu doesn't work in this version yet. It works in {}.", versions(&elsewhere))
    })
}

/// The game a figure id came out with. All eight sidekicks came out with
/// Giants, whose Collection screen shows them; Spyro's Adventure has no
/// pictures of them and doesn't take them. Four share ids with the minis of
/// Trap Team, so they count from Giants.
fn id_game(id: u16) -> Game {
    match id {
        505 | 514 | 519 | 526 | 540..=543 => Game::Giants,
        0..=99 | 200..=207 | 300..=304 | 400..=449 => Game::Spyro,
        100..=199 | 208..=209 => Game::Giants,
        // Trap Team stops at 599: 600 to 699 are Imaginators' own, its
        // Senseis from 601 (King Pen) and its Creation Crystals from 680.
        210..=299 | 305..=399 | 450..=599 => Game::TrapTeam,
        1000..=3219 | 3300..=3399 => Game::SwapForce,
        3220..=3299 | 3400..=3599 => Game::SuperChargers,
        _ => Game::Imaginators,
    }
}

/// The game a variant came out with, kept in the variant's top four bits:
/// 0x0000 for the first game, 0x1801 for a Giants re-release, 0x2805 for a
/// Swap Force one, and so on.
fn variant_game(variant: u16) -> Game {
    match variant >> 12 {
        0 => Game::Spyro,
        1 => Game::Giants,
        2 => Game::SwapForce,
        3 => Game::TrapTeam,
        4 => Game::SuperChargers,
        _ => Game::Imaginators,
    }
}

/// Whether `game` reads a figure with this id and variant.
pub fn reads(game: Game, id: u16, variant: u16) -> bool {
    id_game(id) <= game && variant_game(variant) <= game
}

/// The series of a figure whose name doesn't say it. SWAP Force brought
/// older Skylanders back in new poses under new names ("Blizzard Chill"),
/// the third series, where the second series' names begin "Series 2". The
/// variant says so: the SWAP Force year in its top four bits, and the bit
/// for a new pose.
pub fn series(variant: u16) -> Option<u8> {
    (variant_game(variant) == Game::SwapForce && variant & 0x0800 != 0).then_some(3)
}

pub fn kind(id: u16) -> Kind {
    match id {
        210..=229 => Kind::Trap,
        200..=299 | 3200..=3219 => Kind::Item,
        300..=399 | 3300..=3399 => Kind::Adventure,
        3220..=3299 => Kind::Vehicle,
        3500..=3599 => Kind::Trophy,
        _ if imaginators::is_crystal(id) => Kind::Crystal,
        _ => Kind::Character,
    }
}

pub fn element(id: u16) -> Option<Element> {
    use Element::*;
    // Swap Force numbers its bottom halves from 1000, its top halves from
    // 2000 and its whole figures from 3000, two to an element in one order.
    if (1..=3).contains(&(id / 1000)) && id % 1000 < 16 {
        return Some([Air, Earth, Fire, Life, Magic, Tech, Undead, Water][usize::from(id % 1000 / 2)]);
    }
    Some(match id {
        // Spyro's Adventure
        0..=3 => Air,
        4..=7 => Earth,
        8..=11 => Fire,
        12..=15 => Water,
        16..=18 | 23 | 28 => Magic,
        19..=22 => Tech,
        24..=27 => Life,
        29..=32 => Undead,
        // Giants, a giant and a core figure to each element
        100..=101 => Air,
        102..=103 => Earth,
        104..=105 => Fire,
        106..=107 => Water,
        108..=109 => Magic,
        110..=111 => Tech,
        112..=113 => Life,
        114..=115 => Undead,
        // Trap Team's traps, one id to an element
        210 => Magic,
        211 => Water,
        212 => Air,
        213 => Undead,
        214 => Tech,
        215 => Fire,
        216 => Earth,
        217 => Life,
        218 => Dark,
        219 => Light,
        // Legendary figures of the first game
        404 => Earth,
        416 => Magic,
        419 => Tech,
        430 => Undead,
        // Trap Team, four to an element and two each for Light and Dark
        450..=453 => Air,
        454..=457 => Earth,
        458..=461 => Fire,
        462..=465 => Water,
        466..=469 => Magic,
        470..=473 => Tech,
        474..=477 => Life,
        478..=481 => Undead,
        482..=483 => Light,
        484..=485 => Dark,
        // Minis and sidekicks
        502 | 505 => Earth,
        503 | 542 => Magic,
        504 | 543 => Undead,
        506 | 508 => Air,
        507 | 509 => Fire,
        510 | 519 => Tech,
        514 | 541 => Water,
        526 | 540 => Life,
        // SuperChargers vehicles
        3220 | 3232 | 3233 => Air,
        3221 | 3227 => Undead,
        3222 | 3231 => Water,
        3223 | 3224 => Fire,
        3225 | 3226 => Earth,
        3228 | 3241 => Life,
        3234 | 3235 | 3240 => Tech,
        3236 => Light,
        3237 => Dark,
        3238 | 3239 => Magic,
        // SuperChargers characters
        3400 | 3417 => Undead,
        3401 | 3414 => Tech,
        3402 | 3420 => Magic,
        3406 | 3413 => Air,
        3411 | 3416 => Earth,
        3412 | 3421 | 3424 => Fire,
        3415 | 3423 | 3428 => Life,
        3422 | 3425 => Water,
        3426 => Light,
        3427 => Dark,
        // Imaginators' Senseis and Creation Crystals
        _ => return imaginators::element(id),
    })
}

/// The characters Windows-1252 has for the bytes 0x80 to 0x9F. The rest of
/// its top half is the same as Unicode's.
const WINDOWS_1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘',
    '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// A name as Cemu meant it. Cemu's figure list has a few names whose UTF-8
/// was read as Windows-1252, such as "Dragonâ€™s Peak": turned back into
/// those bytes, they read as the name. Any other name is left as it is.
pub fn repaired(name: &str) -> String {
    let bytes: Option<Vec<u8>> = name
        .chars()
        .map(|c| match u32::from(c) {
            0..=0x7f | 0xa0..=0xff => Some(c as u8),
            _ => WINDOWS_1252.iter().position(|&known| known == c).map(|at| 0x80 + at as u8),
        })
        .collect();
    bytes.and_then(|bytes| String::from_utf8(bytes).ok()).unwrap_or_else(|| name.to_string())
}

/// The name the menu and the portal show for a name in an emulator's list:
/// `repaired`, and without the " (Nintendo Only)" Cemu puts after Nintendo's
/// SuperChargers figures. Which figures those are is kept by id in
/// `vehicles::NINTENDO_ONLY`, not read from the name.
pub fn named(name: &str) -> String {
    let name = repaired(name);
    name.strip_suffix(" (Nintendo Only)").unwrap_or(&name).to_string()
}

/// A character an emulator can make a figure of: the name it shows, and the
/// id and variant the figure carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub id: u16,
    pub variant: u16,
}

impl Character {
    /// From the number Cemu's figure maker keeps with each name in its list:
    /// the id in the high half and the variant in the low one. `None` for
    /// its "---Select---" line, which carries 0xFFFFFFFF.
    pub fn from_item(name: &str, data: u64) -> Option<Self> {
        if data >= 0xFFFF_FFFF || name.trim().is_empty() {
            return None;
        }
        Some(Self {
            name: name.trim().to_string(),
            id: (data >> 16) as u16,
            variant: (data & 0xFFFF) as u16,
        })
    }
}

/// Which half of a Swap Force swapper a figure is. A swapper is two figures,
/// a top and a bottom, each with its own id: bottoms from 1000, tops from
/// 2000, in the same order. Both go on the portal together, and a top of one
/// character works with the bottom of another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Half {
    Top,
    Bottom,
}

pub fn half(id: u16) -> Option<Half> {
    match id {
        1000..=1999 => Some(Half::Bottom),
        2000..=2999 => Some(Half::Top),
        _ => None,
    }
}

/// How a swapper gets about, which its bottom half decides. Each of Swap
/// Force's Swap Zones lets in one of the eight, and two swappers have each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Movement {
    Bounce,
    Climb,
    Dig,
    Rocket,
    Sneak,
    Speed,
    Spin,
    Teleport,
}

/// The movement a bottom half gives its swapper; `None` for every other
/// figure. Checked against Activision's fact sheet for the game and the
/// Skylanders wiki.
pub fn movement(id: u16) -> Option<Movement> {
    use Movement::*;
    // The bottoms from 1000 in order: Boom Jet, Free Ranger, Rubble Rouser,
    // Doom Stone, Blast Zone, Fire Kraken, Stink Bomb, Grilla Drilla, Hoot
    // Loop, Trap Shadow, Magna Charge, Spy Rise, Night Shift, Rattle Shake,
    // Freeze Blade, Wash Buckler.
    const BOTTOMS: [Movement; 16] = [
        Rocket, Spin, Dig, Spin, Rocket, Bounce, Sneak, Dig, Teleport, Sneak, Speed, Climb, Teleport, Bounce, Speed, Climb,
    ];
    BOTTOMS.get(usize::from(id.checked_sub(1000)?)).copied()
}

/// The kinds of Skylander the games' own checklists mark apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// The Giants of Skylanders Giants, twice the size of the rest.
    Giant,
    /// Trap Team's Trap Masters, who carry a Traptanium weapon.
    TrapMaster,
    /// The small ones: Trap Team's Minis, and the Sidekicks of Giants they
    /// came back as.
    Mini,
    /// SuperChargers' own Skylanders, each with a vehicle of its own.
    #[serde(rename = "supercharger")]
    SuperCharger,
    /// Imaginators' Senseis, each with a battle class (`imaginators`).
    Sensei,
    /// The Senseis who are villains, one to an element, and Kaos.
    VillainSensei,
}

/// Which marked kind a figure is, from its id, variants included (Cemu
/// 2.6's list). Giants' new figures alternate between a core figure and a
/// Giant from 100; Trap Masters are the first two of each element's four
/// from 450, then Knight Light and Knight Mare. SuperChargers are the
/// vehicles' drivers, whose ids from 3400 have gaps. Senseis are 601 to 631,
/// marked villain or not in `imaginators::SENSEIS`.
pub fn class(id: u16) -> Option<Class> {
    match id {
        101 | 102 | 104 | 107 | 109 | 110 | 112 | 114 => Some(Class::Giant),
        450..=481 if (id - 450) % 4 < 2 => Some(Class::TrapMaster),
        482 | 484 => Some(Class::TrapMaster),
        502..=510 | 514 | 519 | 526 | 540..=543 => Some(Class::Mini),
        _ if vehicles::VEHICLES.iter().any(|vehicle| vehicle.driver == id) => Some(Class::SuperCharger),
        _ => imaginators::sensei(id).map(|sensei| if sensei.villain { Class::VillainSensei } else { Class::Sensei }),
    }
}

/// Figures Cemu's figure maker lists with a variant the games don't use:
/// name, id, Cemu's variant, the game's. RPCS3's list has the same mistakes.
///
/// Seven traps, which Trap Team reads as another trap or not at all (Cemu
/// issue #1816). The game's own Collection pictures, named by id and variant,
/// and a list made from real figures agree on the game's, and so does
/// Dolphin's list.
///
/// Three Trap Team variants, listed as 0x3805. Trap Team's Collection
/// pictures, SuperChargers' toy data and Dolphin's list all give 0x3809 for
/// Tidal Wave Gill Grunt and 0x3801 for the other two, and no game has a
/// picture for 0x3805 (read from Bertram's copies, 7 October 2026).
const WRONG_VARIANTS: [(&str, u16, u16, u16); 10] = [
    ("Rune Rocket", 210, 0x3014, 0x3015),
    ("Tempest Timer", 212, 0x300D, 0x300E),
    ("Tech Totem", 214, 0x3000, 0x3001),
    ("Banded Boulder", 216, 0x3000, 0x3004),
    ("Spinning Sandstorm", 216, 0x3013, 0x3012),
    ("Dark Dagger", 218, 0x3000, 0x3018),
    ("Shining Ship", 219, 0x3000, 0x3015),
    ("Tidal Wave Gill Grunt", 14, 0x3805, 0x3809),
    ("Sure Shot Shroomboom", 113, 0x3805, 0x3801),
    ("Hog Wild Fryno", 3004, 0x3805, 0x3801),
];

/// The variant the game itself gives a figure. Only the figures above
/// change, and only while Cemu still lists them wrongly; and a Sensei, which
/// the emulators list with 0x0000 where a real one carries
/// `imaginators::SENSEI_VARIANT`. Changed here, beside the wrong variants,
/// rather than in the offers, so a Sensei is made the same way whichever
/// game asks for it.
pub fn game_variant(name: &str, id: u16, variant: u16) -> u16 {
    if variant == 0x0000 && imaginators::sensei(id).is_some() {
        return imaginators::SENSEI_VARIANT;
    }
    WRONG_VARIANTS
        .iter()
        .find(|&&(figure, figure_id, listed, _)| figure == name && figure_id == id && listed == variant)
        .map_or(variant, |&(.., right)| right)
}

/// Which of those figures a figure is, from its id and the game's own
/// variant, which an emulator's list can't name. A Sensei made with the
/// variant real ones carry is one too.
pub fn fixed_name(id: u16, variant: u16) -> Option<&'static str> {
    WRONG_VARIANTS
        .iter()
        .find(|&&(_, figure_id, _, right)| figure_id == id && right == variant)
        .map(|&(name, ..)| name)
        .or_else(|| {
            imaginators::sensei(id)
                .filter(|_| variant == imaginators::SENSEI_VARIANT)
                .map(|sensei| sensei.name)
        })
}

/// The name of a figure Omoio had made that the emulator's list can't name:
/// one of those above, or a Creation Crystal, which no list has.
pub fn unlisted_name(id: u16, variant: u16) -> Option<String> {
    fixed_name(id, variant)
        .map(str::to_string)
        .or_else(|| imaginators::crystal(id, variant).map(Crystal::name))
}

/// A character as the menu lists it, with where it belongs.
#[derive(Debug, Clone, Serialize)]
pub struct Offer {
    #[serde(flatten)]
    pub character: Character,
    pub element: Option<Element>,
    pub kind: Kind,
    pub half: Option<Half>,
    pub series: Option<u8>,
    pub movement: Option<Movement>,
    pub class: Option<Class>,
    /// A vehicle's terrain, or a trophy's.
    pub terrain: Option<Terrain>,
    /// A vehicle's own SuperCharger, or a SuperCharger's own vehicle.
    pub partner: Option<u16>,
    /// A Sensei's battle class.
    pub battle_class: Option<BattleClass>,
    /// A Creation Crystal's casing.
    pub casing: Option<Casing>,
    /// What a SuperChargers trophy unlocks, left out for every other figure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlocks: Option<&'static vehicles::Trophy>,
}

fn offer(character: Character) -> Offer {
    let (id, variant) = (character.id, character.variant);
    Offer {
        element: element(id),
        kind: kind(id),
        half: half(id),
        series: series(variant),
        movement: movement(id),
        class: class(id),
        terrain: vehicles::terrain(id),
        partner: vehicles::partner(id),
        battle_class: imaginators::battle_class(id),
        casing: imaginators::crystal(id, variant).map(|crystal| crystal.casing),
        unlocks: vehicles::trophy(id),
        character,
    }
}

/// The characters `game` reads on `console`, each with its element and
/// kind. Every one when the game isn't known, but for Nintendo's figures
/// away from the Wii U. A figure made from an offer carries the game's own
/// variant, so a trap Cemu lists wrongly is made right.
///
/// Imaginators also gets every Creation Crystal, which no emulator's list
/// has (Cemu 2.6). One a later list has is offered once, under the list's
/// name, which is the one its slot will show.
pub fn offers(characters: Vec<Character>, game: Option<Game>, console: Console) -> Vec<Offer> {
    let mut offered: Vec<Offer> = characters
        .into_iter()
        .filter(|c| game.is_none_or(|game| reads(game, c.id, c.variant)))
        .filter(|c| vehicles::plays_on(console, c.id))
        .map(|character| {
            let name = named(&character.name);
            let variant = game_variant(&name, character.id, character.variant);
            offer(Character { name, id: character.id, variant })
        })
        .collect();
    if game == Some(Game::Imaginators) {
        let crystals: Vec<Offer> = imaginators::CRYSTALS
            .iter()
            .filter(|crystal| {
                !offered.iter().any(|o| (o.character.id, o.character.variant) == (crystal.id, crystal.variant))
            })
            .map(|crystal| offer(Character { name: crystal.name(), id: crystal.id, variant: crystal.variant }))
            .collect();
        offered.extend(crystals);
    }
    offered
}

/// A file name for a new figure that is not taken yet: the character's
/// name, then " 2", " 3" and so on. Characters Windows does not allow in a
/// file name are left out.
pub fn free_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let clean: String = name
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    let base = if clean.is_empty() { "Figure".to_string() } else { clean };
    let first = format!("{base}.sky");
    if !taken(&first) {
        return first;
    }
    (2..)
        .map(|n| format!("{base} {n}.sky"))
        .find(|file| !taken(file))
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_gives_its_id_and_variant() {
        let whirlwind = Character::from_item("Whirlwind", 0).unwrap();
        assert_eq!((whirlwind.id, whirlwind.variant), (0, 0));
        let dark = Character::from_item("Dark Spyro", (404 << 16) | 0x1206).unwrap();
        assert_eq!((dark.id, dark.variant), (404, 0x1206));
        assert!(Character::from_item("---Select---", 0xFFFF_FFFF).is_none());
    }

    #[test]
    fn a_new_figure_never_takes_a_name_already_used() {
        let taken = ["Spyro.sky", "Spyro 2.sky"];
        assert_eq!(free_name("Spyro", |f| taken.contains(&f)), "Spyro 3.sky");
        assert_eq!(free_name("Whirlwind", |_| false), "Whirlwind.sky");
        assert_eq!(free_name("Who?: Me*", |_| false), "Who Me.sky");
        assert_eq!(free_name("???", |_| false), "Figure.sky");
    }

    #[test]
    fn figures_are_sorted_by_their_element_and_kind() {
        assert_eq!(element(0), Some(Element::Air)); // Whirlwind
        assert_eq!(element(23), Some(Element::Magic)); // Wrecking Ball
        assert_eq!(element(28), Some(Element::Magic)); // Dark Spyro
        assert_eq!(element(101), Some(Element::Air)); // Swarm
        assert_eq!(element(1007), Some(Element::Life)); // Grilla Drilla, bottom half
        assert_eq!(element(2015), Some(Element::Water)); // Wash Buckler, top half
        assert_eq!(element(3000), Some(Element::Air)); // Scratch
        assert_eq!(element(3012), Some(Element::Undead)); // Roller Brawl
        assert_eq!(element(482), Some(Element::Light)); // Knight Light
        assert_eq!(element(485), Some(Element::Dark)); // Blackout
        assert_eq!(element(201), None); // Hidden Treasure
        assert_eq!(kind(201), Kind::Item);
        assert_eq!(kind(203), Kind::Item); // Ghost Pirate Swords
        assert_eq!(kind(3200), Kind::Item); // Battle Hammer
        assert_eq!(kind(220), Kind::Trap); // Kaos
        assert_eq!(kind(300), Kind::Adventure); // Dragon's Peak
        assert_eq!(kind(3300), Kind::Adventure); // Sheep Wreck Island
        assert_eq!(kind(3220), Kind::Vehicle); // Jet Stream
        assert_eq!(kind(3500), Kind::Trophy); // Sky Trophy
        assert_eq!(kind(16), Kind::Character); // Spyro
    }

    #[test]
    fn a_game_reads_its_own_figures_and_older_ones_only() {
        let swap = Game::SwapForce;
        assert!(reads(swap, 0, 0x0000)); // Whirlwind
        assert!(reads(swap, 0, 0x2805)); // Horn Blast Whirlwind
        assert!(!reads(swap, 0, 0x3810)); // Eon's Elite Whirlwind, from Trap Team
        assert!(reads(swap, 3000, 0x2000)); // Scratch
        assert!(reads(swap, 505, 0x0000)); // Terrabite, a sidekick of Giants
        assert!(!reads(swap, 505, 0x3000)); // Terrabite, the Trap Team mini
        assert!(!reads(Game::Spyro, 505, 0x0000)); // Spyro's Adventure takes no sidekicks
        assert!(reads(Game::Giants, 505, 0x0000));
        assert!(!reads(swap, 450, 0x3000)); // Gusto
        assert!(!reads(swap, 230, 0x3000)); // Hand of Fate
        assert!(!reads(Game::Giants, 3000, 0x2000));
        assert!(reads(Game::SuperChargers, 3400, 0x4100)); // Fiesta
        assert!(reads(Game::TrapTeam, 450, 0x0000)); // Gusto
        assert!(!reads(Game::TrapTeam, 601, 0x0000)); // King Pen, an Imaginators Sensei
        assert!(!reads(Game::SuperChargers, 630, 0x0000)); // Crash Bandicoot, also Imaginators
    }

    #[test]
    fn a_name_cemu_garbled_is_put_back() {
        assert_eq!(repaired("Dragonâ€™s Peak"), "Dragon’s Peak");
        assert_eq!(repaired("Spyro"), "Spyro");
        assert_eq!(repaired("Pokémon"), "Pokémon");
        assert_eq!(repaired("Eon's Elite Spyro"), "Eon's Elite Spyro");
    }

    #[test]
    fn nintendos_figures_are_named_without_cemus_note() {
        assert_eq!(named("Hammer Slam Bowser (Nintendo Only)"), "Hammer Slam Bowser");
        assert_eq!(named("Dark Turbo Charge Donkey Kong (Nintendo Only)"), "Dark Turbo Charge Donkey Kong");
        assert_eq!(named("Clown Cruiser (Nintendo Only)"), "Clown Cruiser");
        assert_eq!(named("Dragonâ€™s Peak"), "Dragon’s Peak");
        assert_eq!(named("Spitfire"), "Spitfire");
        assert_eq!(named("(Nintendo Only) Spitfire"), "(Nintendo Only) Spitfire");
    }

    #[test]
    fn swap_force_reposes_are_the_third_series() {
        assert_eq!(series(0x2805), Some(3)); // Blizzard Chill
        assert_eq!(series(0x2c02), Some(3)); // Dark Mega Ram Spyro
        assert_eq!(series(0x1801), None); // Series 2 Spyro says it already
        assert_eq!(series(0x0000), None); // Spyro
        assert_eq!(series(0x2402), None); // Dark Blast Zone, new in SWAP Force
        assert_eq!(series(0x2206), None); // LightCore Smolderdash
    }

    #[test]
    fn the_game_is_told_by_its_title() {
        assert_eq!(game_from_title("Skylanders - Swap Force"), Some(Game::SwapForce));
        assert_eq!(game_from_title("Skylanders SWAP Force"), Some(Game::SwapForce));
        assert_eq!(game_from_title("Skylanders Spyro's Adventure"), Some(Game::Spyro));
        assert_eq!(game_from_title("Skylanders Giants"), Some(Game::Giants));
        assert_eq!(game_from_title("Skylanders: Trap Team"), Some(Game::TrapTeam));
        assert_eq!(game_from_title("Skylanders SuperChargers"), Some(Game::SuperChargers));
        assert_eq!(game_from_title("Skylanders: Imaginators"), Some(Game::Imaginators));
        assert_eq!(game_from_title("LittleBigPlanet 3"), None);
    }

    #[test]
    fn games_are_written_the_way_the_menu_reads_them() {
        let written = [
            Game::Spyro,
            Game::Giants,
            Game::SwapForce,
            Game::TrapTeam,
            Game::SuperChargers,
            Game::Imaginators,
        ]
        .map(|game| serde_json::to_value(game).unwrap());
        assert_eq!(written, ["spyro", "giants", "swapforce", "trapteam", "superchargers", "imaginators"]);
    }

    #[test]
    fn giants_trap_masters_minis_and_superchargers_are_told_by_their_id() {
        let giant = Some(Class::Giant);
        let master = Some(Class::TrapMaster);
        let mini = Some(Class::Mini);
        let supercharger = Some(Class::SuperCharger);
        assert_eq!(class(112), giant); // Tree Rex, and Gnarly Tree Rex
        assert_eq!(class(101), giant); // Swarm
        assert_eq!(class(114), giant); // Eye Brawl
        assert_eq!(class(100), None); // Jet Vac
        assert_eq!(class(113), None); // Shroomboom
        assert_eq!(class(450), master); // Gusto
        assert_eq!(class(451), master); // Thunderbolt
        assert_eq!(class(452), None); // Fling Kong
        assert_eq!(class(470), master); // Jawbreaker, and Legendary Jawbreaker
        assert_eq!(class(481), None); // Funny Bone
        assert_eq!(class(482), master); // Knight Light
        assert_eq!(class(483), None); // Spotlight
        assert_eq!(class(484), master); // Knight Mare
        assert_eq!(class(485), None); // Blackout
        assert_eq!(class(503), mini); // Spry
        assert_eq!(class(542), mini); // Mini Jini
        assert_eq!(class(108), None); // Pop Fizz
        assert_eq!(class(3400), supercharger); // Fiesta, and Frightful Fiesta
        assert_eq!(class(3406), supercharger); // Stormblade
        assert_eq!(class(3424), supercharger); // Hammer Slam Bowser
        assert_eq!(class(3428), supercharger); // Thrillipede
        assert_eq!(class(3403), None); // no figure
        assert_eq!(class(3220), None); // Jet Stream, a vehicle
        assert_eq!(class(3500), None); // Sky Trophy
        assert_eq!((3400..3500).filter(|&id| class(id) == supercharger).count(), 20);
        assert_eq!(serde_json::to_value(Class::SuperCharger).unwrap(), "supercharger");
        assert_eq!(serde_json::to_value(Class::TrapMaster).unwrap(), "trap_master");
    }

    #[test]
    fn swap_force_halves_are_told_apart() {
        assert_eq!(half(1000), Some(Half::Bottom)); // Boom Jet (Bottom)
        assert_eq!(half(2015), Some(Half::Top)); // Wash Buckler (Top)
        assert_eq!(half(3000), None); // Scratch, a whole figure
        assert_eq!(half(16), None); // Spyro
    }

    #[test]
    fn a_swapper_moves_as_its_bottom_half_does() {
        assert_eq!(movement(1000), Some(Movement::Rocket)); // Boom Jet (Bottom)
        assert_eq!(movement(1003), Some(Movement::Spin)); // Doom Stone (Bottom)
        assert_eq!(movement(1005), Some(Movement::Bounce)); // Fire Kraken (Bottom)
        assert_eq!(movement(1010), Some(Movement::Speed)); // Magna Charge (Bottom)
        assert_eq!(movement(1015), Some(Movement::Climb)); // Wash Buckler (Bottom)
        assert_eq!(movement(2000), None); // Boom Jet (Top)
        assert_eq!(movement(1016), None);
        assert_eq!(movement(16), None); // Spyro
        use Movement::*;
        for each in [Bounce, Climb, Dig, Rocket, Sneak, Speed, Spin, Teleport] {
            assert_eq!((1000..1016).filter(|&id| movement(id) == Some(each)).count(), 2, "{each:?}");
        }
    }

    #[test]
    fn traps_cemu_lists_wrongly_are_made_with_the_games_variant() {
        assert_eq!(game_variant("Tempest Timer", 212, 0x300D), 0x300E);
        assert_eq!(game_variant("Rune Rocket", 210, 0x3014), 0x3015);
        assert_eq!(game_variant("Tech Totem", 214, 0x3000), 0x3001);
        assert_eq!(game_variant("Banded Boulder", 216, 0x3000), 0x3004);
        assert_eq!(game_variant("Spinning Sandstorm", 216, 0x3013), 0x3012);
        assert_eq!(game_variant("Dark Dagger", 218, 0x3000), 0x3018);
        assert_eq!(game_variant("Shining Ship", 219, 0x3000), 0x3015);
        // The traps Cemu has right, and the seven once Cemu fixes its list.
        assert_eq!(game_variant("Breezy Bird", 212, 0x3003), 0x3003);
        assert_eq!(game_variant("Rock Hawk", 216, 0x3003), 0x3003);
        assert_eq!(game_variant("Tempest Timer", 212, 0x300E), 0x300E);
        // Another figure that happens to share an id and variant.
        assert_eq!(game_variant("Whirlwind", 216, 0x3000), 0x3000);
        let made = offers(
            vec![Character { name: "Tempest Timer".into(), id: 212, variant: 0x300D }],
            Some(Game::TrapTeam),
            Console::WiiU,
        );
        assert_eq!((made[0].character.id, made[0].character.variant), (212, 0x300E));
    }

    #[test]
    fn three_trap_team_variants_cemu_lists_as_3805_are_made_with_the_games() {
        assert_eq!(game_variant("Tidal Wave Gill Grunt", 14, 0x3805), 0x3809);
        assert_eq!(game_variant("Sure Shot Shroomboom", 113, 0x3805), 0x3801);
        assert_eq!(game_variant("Hog Wild Fryno", 3004, 0x3805), 0x3801);
        // Their other variants, which the lists have right.
        assert_eq!(game_variant("Gill Grunt", 14, 0x0000), 0x0000);
        assert_eq!(game_variant("Anchors Away Gill Grunt", 14, 0x2805), 0x2805);
        assert_eq!(fixed_name(14, 0x3809), Some("Tidal Wave Gill Grunt"));
        assert_eq!(fixed_name(3004, 0x3801), Some("Hog Wild Fryno"));
        assert_eq!(fixed_name(14, 0x3805), None);
    }

    #[test]
    fn a_trap_made_with_the_games_variant_is_named() {
        assert_eq!(fixed_name(212, 0x300E), Some("Tempest Timer"));
        assert_eq!(fixed_name(210, 0x3015), Some("Rune Rocket"));
        assert_eq!(fixed_name(219, 0x3015), Some("Shining Ship"));
        // The variant the emulators list, and a trap they have right.
        assert_eq!(fixed_name(212, 0x300D), None);
        assert_eq!(fixed_name(212, 0x3003), None);
    }

    #[test]
    fn only_skylanders_games_have_a_portal() {
        assert!(is_skylanders("Skylanders SWAP Force"));
        assert!(is_skylanders("Skylanders Giants"));
        assert!(!is_skylanders("LittleBigPlanet 3"));
    }

    #[test]
    fn the_portal_menu_works_in_the_games_played_with_on_each_console() {
        assert!(has_portal_menu(Console::Ps3, "Skylanders: Spyro's Adventure"));
        assert!(has_portal_menu(Console::Ps3, "Skylanders Giants™"));
        assert!(has_portal_menu(Console::Ps3, "Skylanders SWAP Force"));
        assert!(has_portal_menu(Console::Ps3, "Skylanders Trap Team"));
        assert!(has_portal_menu(Console::WiiU, "Skylanders: Swap Force"));
        assert!(has_portal_menu(Console::WiiU, "Skylanders - Trap Team"));
        assert!(has_portal_menu(Console::WiiU, "Skylanders SuperChargers"));
        assert!(has_portal_menu(Console::WiiU, "Skylanders: SuperChargers"));
        assert!(has_portal_menu(Console::WiiU, "Skylanders Imaginators"));
        assert!(has_portal_menu(Console::WiiU, "Skylanders: Imaginators"));
        assert!(has_portal_menu(Console::Wii, "Skylanders Spyro's Adventure"));

        assert!(!has_portal_menu(Console::WiiU, "Skylanders: Giants"));
        assert!(!has_portal_menu(Console::GameCube, "Skylanders Spyro's Adventure"), "no such release");
        assert!(!has_portal_menu(Console::Ps3, "Giants: Citizen Kabuto"), "not a Skylanders game");
    }

    #[test]
    fn the_other_skylanders_games_have_no_portal_menu_yet() {
        // Spyro's Adventure's Wii U release was sold in Japan only.
        // SuperChargers and Imaginators have it on the Wii U only so far, and
        // the Wii's later games wait for a play there.
        assert!(!has_portal_menu(Console::WiiU, "Skylanders Spyro's Adventure"));
        assert!(!has_portal_menu(Console::Wii, "Skylanders Giants"));
        assert!(!has_portal_menu(Console::Ps3, "Skylanders SuperChargers"));
        assert!(!has_portal_menu(Console::Ps3, "Skylanders: SuperChargers"));
        assert!(!has_portal_menu(Console::Ps3, "Skylanders Imaginators"));
        assert!(!has_portal_menu(Console::Ps3, "Skylanders: Imaginators"));
    }

    #[test]
    fn titles_are_read_whatever_their_case() {
        assert!(has_portal_menu(Console::WiiU, "SKYLANDERS SWAP FORCE"));
        assert!(has_portal_menu(Console::WiiU, "Skylanders Trap Team™"));
        assert!(has_portal_menu(Console::Ps3, "skylanders giants"));
    }

    #[test]
    fn the_portal_menu_is_offered_only_where_the_emulator_fills_the_portal() {
        let portal = Features { portal: true, ..Features::default() };
        let none = Features::default();

        assert!(offers_portal_menu(portal, Console::Ps3, "Skylanders Giants"));
        assert!(offers_portal_menu(portal, Console::WiiU, "Skylanders SWAP Force"));
        assert!(!offers_portal_menu(none, Console::Ps3, "Skylanders Giants"));
        assert!(!offers_portal_menu(portal, Console::Ps3, "Skylanders SuperChargers"));
        assert!(!offers_portal_menu(portal, Console::WiiU, "Mario Kart 8"));
    }

    #[test]
    fn a_game_without_the_menu_names_every_version_that_has_it() {
        use Console::*;
        let elsewhere = |console, title| portal_menu_elsewhere(console, title);
        assert_eq!(elsewhere(Ps3, "Skylanders SuperChargers™"), Some(vec![WiiU]));
        assert_eq!(elsewhere(Ps3, "Skylanders Imaginators"), Some(vec![WiiU]));
        assert_eq!(elsewhere(WiiU, "Skylanders: Spyro's Adventure"), Some(vec![Ps3, Wii]));
        assert_eq!(elsewhere(Wii, "Skylanders Giants"), Some(vec![Ps3]));
        assert_eq!(elsewhere(Wii, "Skylanders SWAP Force"), Some(vec![Ps3, WiiU]));
        assert_eq!(elsewhere(Wii, "Skylanders Trap Team"), Some(vec![Ps3, WiiU]));
        assert_eq!(elsewhere(Wii, "Skylanders SuperChargers Racing"), Some(vec![WiiU]));
        assert_eq!(elsewhere(Ps3, "Skylanders Battlecast"), Some(vec![]), "a game Omoio can't tell");

        // Where the menu works, or there is no portal, there is nothing to say.
        assert_eq!(elsewhere(Ps3, "Skylanders Trap Team"), None);
        assert_eq!(elsewhere(Ps3, "Skylanders Spyro's Adventure"), None);
        assert_eq!(elsewhere(WiiU, "Skylanders Imaginators"), None);
        assert_eq!(elsewhere(Wii, "Skylanders Spyro's Adventure"), None);
        assert_eq!(elsewhere(Ps3, "LittleBigPlanet 3"), None);
    }

    #[test]
    fn the_game_page_says_where_the_menu_works() {
        assert_eq!(
            portal_menu_note(Console::Ps3, "Skylanders SuperChargers").as_deref(),
            Some("The portal menu doesn't work in this version yet. It works in the Wii U version.")
        );
        assert_eq!(
            portal_menu_note(Console::Wii, "Skylanders SWAP Force").as_deref(),
            Some("The portal menu doesn't work in this version yet. It works in the PS3 and Wii U versions.")
        );
        assert_eq!(
            portal_menu_note(Console::Ps3, "Skylanders Battlecast").as_deref(),
            Some("The portal menu doesn't work in this game yet.")
        );
        assert_eq!(portal_menu_note(Console::WiiU, "Skylanders SuperChargers"), None);
        assert_eq!(portal_menu_note(Console::WiiU, "Mario Kart 8"), None);
        assert_eq!(
            versions(&[Console::Ps3, Console::WiiU, Console::Wii]),
            "the PS3, Wii U and Wii versions"
        );
    }

    #[test]
    fn the_menu_gets_what_the_game_reads_with_its_element() {
        let list = vec![
            Character { name: "Whirlwind".into(), id: 0, variant: 0 },
            Character { name: "Gusto".into(), id: 450, variant: 0x3000 },
        ];
        let offered = offers(list.clone(), Some(Game::SwapForce), Console::Ps3);
        assert_eq!(offered.len(), 1);
        let json = serde_json::to_value(&offered[0]).unwrap();
        assert_eq!(json["name"], "Whirlwind");
        assert_eq!(json["element"], "air");
        assert_eq!(json["kind"], "character");
        assert_eq!(offers(list, None, Console::Ps3).len(), 2);
    }

    #[test]
    fn nintendos_figures_are_offered_on_the_wii_u_only() {
        let list = vec![
            Character { name: "Hammer Slam Bowser (Nintendo Only)".into(), id: 3424, variant: 0 },
            Character { name: "Dark Turbo Charge Donkey Kong (Nintendo Only)".into(), id: 3423, variant: 0x4502 },
            Character { name: "Clown Cruiser (Nintendo Only)".into(), id: 3233, variant: 0 },
            Character { name: "Dark Barrel Blaster (Nintendo Only)".into(), id: 3240, variant: 0x4402 },
            Character { name: "Spitfire".into(), id: 3412, variant: 0 },
        ];
        let names = |console| -> Vec<String> {
            offers(list.clone(), Some(Game::SuperChargers), console)
                .into_iter()
                .map(|offer| offer.character.name)
                .collect()
        };
        assert_eq!(names(Console::Ps3), ["Spitfire"]);
        assert_eq!(
            names(Console::WiiU),
            [
                "Hammer Slam Bowser",
                "Dark Turbo Charge Donkey Kong",
                "Clown Cruiser",
                "Dark Barrel Blaster",
                "Spitfire"
            ]
        );
        assert_eq!(offers(list, None, Console::Ps3).len(), 1, "whatever the game");
    }

    #[test]
    fn a_vehicle_and_its_supercharger_are_offered_as_partners() {
        let list = vec![
            Character { name: "Dark Hot Streak".into(), id: 3224, variant: 0x4402 },
            Character { name: "Dark Spitfire".into(), id: 3412, variant: 0x4502 },
            Character { name: "Sea Trophy".into(), id: 3502, variant: 0 },
            Character { name: "Whirlwind".into(), id: 0, variant: 0 },
        ];
        let json = serde_json::to_value(offers(list, Some(Game::SuperChargers), Console::Ps3)).unwrap();
        assert_eq!(json[0]["kind"], "vehicle");
        assert_eq!(json[0]["terrain"], "land");
        assert_eq!(json[0]["partner"], 3412);
        assert_eq!(json[1]["class"], "supercharger");
        assert!(json[1]["terrain"].is_null(), "a SuperCharger goes by its vehicle's");
        assert_eq!(json[1]["partner"], 3224);
        assert_eq!(json[2]["terrain"], "sea");
        assert!(json[2]["partner"].is_null());
        assert_eq!(json[2]["unlocks"]["villains"][0], "Golden Queen");
        assert_eq!(json[2]["unlocks"]["tracks"][1], "The Golden Temple");
        assert!(json[3]["terrain"].is_null() && json[3]["partner"].is_null());
        assert!(json[0].get("unlocks").is_none() && json[3].get("unlocks").is_none(), "only a trophy has it");
    }

    #[test]
    fn senseis_and_crystals_are_sorted_by_their_element_and_kind() {
        assert_eq!(element(601), Some(Element::Water)); // King Pen
        assert_eq!(element(627), Some(Element::Kaos)); // Kaos
        assert_eq!(element(631), Some(Element::Tech)); // Dr. Neo Cortex
        assert_eq!(element(680), Some(Element::Magic)); // a Magic Creation Crystal
        assert_eq!(element(689), Some(Element::Light));
        assert_eq!(element(600), None);
        assert_eq!(kind(601), Kind::Character);
        assert_eq!(kind(685), Kind::Crystal);
        assert_eq!(class(601), Some(Class::Sensei));
        assert_eq!(class(610), Some(Class::VillainSensei)); // Dr. Krankcase
        assert_eq!(class(680), None);
        assert!(reads(Game::Imaginators, 680, 0x5208));
        assert!(reads(Game::Imaginators, 3400, 0x4100), "and every older figure");
        assert!(!reads(Game::SuperChargers, 680, 0x5208));
        assert!(!reads(Game::SuperChargers, 601, 0x5000));
    }

    #[test]
    fn a_sensei_is_made_with_the_variant_real_ones_carry() {
        assert_eq!(game_variant("King Pen", 601, 0x0000), 0x5000);
        assert_eq!(game_variant("King Pen", 601, 0x5000), 0x5000);
        assert_eq!(game_variant("Whirlwind", 0, 0x0000), 0x0000);
        assert_eq!(fixed_name(601, 0x5000), Some("King Pen"));
        assert_eq!(fixed_name(631, 0x5000), Some("Dr. Neo Cortex"));
        assert_eq!(fixed_name(601, 0x0000), None, "Cemu names that one itself");
    }

    #[test]
    fn a_figure_no_list_names_is_named() {
        assert_eq!(unlisted_name(680, 0x5208).as_deref(), Some("Magic Lantern Crystal"));
        assert_eq!(unlisted_name(689, 0x5616).as_deref(), Some("Legendary Light Fanged Crystal"));
        assert_eq!(unlisted_name(601, 0x5000).as_deref(), Some("King Pen"));
        assert_eq!(unlisted_name(212, 0x300E).as_deref(), Some("Tempest Timer"));
        assert_eq!(unlisted_name(682, 0x5212), None, "Air Acorn was never sold");
        assert_eq!(unlisted_name(16, 0x0000), None);
    }

    #[test]
    fn older_games_are_offered_what_they_were_before_imaginators() {
        let list = vec![
            Character { name: "Whirlwind".into(), id: 0, variant: 0 },
            Character { name: "Tempest Timer".into(), id: 212, variant: 0x300D },
            Character { name: "Gusto".into(), id: 450, variant: 0x3000 },
            Character { name: "Dark Hot Streak".into(), id: 3224, variant: 0x4402 },
            Character { name: "Hammer Slam Bowser (Nintendo Only)".into(), id: 3424, variant: 0 },
            Character { name: "King Pen".into(), id: 601, variant: 0 },
            Character { name: "Crash Bandicoot".into(), id: 630, variant: 0 },
        ];
        let made = |game, console| -> Vec<String> {
            offers(list.clone(), Some(game), console)
                .into_iter()
                .map(|offer| format!("{} {} {:#06X}", offer.character.name, offer.character.id, offer.character.variant))
                .collect()
        };
        let trap_team = ["Whirlwind 0 0x0000", "Tempest Timer 212 0x300E", "Gusto 450 0x3000"];
        assert_eq!(made(Game::TrapTeam, Console::WiiU), trap_team);
        assert_eq!(made(Game::TrapTeam, Console::Ps3), trap_team);
        let superchargers = [&trap_team[..], &["Dark Hot Streak 3224 0x4402"][..]].concat();
        assert_eq!(made(Game::SuperChargers, Console::Ps3), superchargers);
        let wii_u = [&superchargers[..], &["Hammer Slam Bowser 3424 0x0000"][..]].concat();
        assert_eq!(made(Game::SuperChargers, Console::WiiU), wii_u);

        for game in [Game::TrapTeam, Game::SuperChargers] {
            let json = serde_json::to_value(offers(list.clone(), Some(game), Console::WiiU)).unwrap();
            for offer in json.as_array().unwrap() {
                assert!(offer["battle_class"].is_null() && offer["casing"].is_null(), "{offer}");
            }
        }
    }
}
