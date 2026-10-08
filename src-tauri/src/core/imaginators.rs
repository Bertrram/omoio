//! Skylanders Imaginators' own figures: its 31 Senseis, and the Creation
//! Crystals an Imaginator is made in. Imaginators also reads every figure of
//! the earlier games (Activision's "Skylanders Imaginators Toy FAQ", question
//! 1, read 7 October 2026), which `figures::reads` gives by year.
//!
//! The Senseis go by the ids Cemu 2.6's figure list gives them
//! (`Skylander.cpp`, lines 310 to 340, the same on main; read 7 October
//! 2026): 601 to 631, King Pen first, and no 600. RPCS3's list and
//! Texthead1's Skylander-IDs number them the same. Each has an element and a
//! battle class, and ten are villains, one to an element, with Kaos besides
//! (the Skylanders wiki's "Senseis" and "Skylanders: Imaginators", read 7
//! October 2026). The wiki calls those Villain Senseis; the game's own word
//! for them wasn't found. Three of them had another element in Trap Team
//! (`villains`): Tae Kwon Crow was Dark there, Grave Clobber Earth and Hood
//! Sickle Undead.
//!
//! Crash Bandicoot and Dr. Neo Cortex work on every console that runs the
//! game (the Toy FAQ, question 4), so no Imaginators figure is kept to some
//! consoles, as Nintendo's SuperChargers are.
//!
//! Creation Crystals are in no emulator's list, so Omoio offers them from the
//! table here, and Cemu's figure maker makes one from the id and variant
//! typed into it.

use crate::core::figures::Element;
use serde::Serialize;

/// How a Sensei or an Imaginator fights, in the order the game numbers the
/// classes on an Imaginator's crystal, 1 to 11 (NefariousTechSupport's Runes
/// docs, `SkylanderFormat.md` lines 1145 to 1160, and Modified_SkyEditGUI's
/// `frmCrystals.vb` lines 68 to 88, which agree; read 7 October 2026). No
/// door or level was found open to one class only, and the replies under
/// skylanderscharacterlist.com's article of 25 November 2016 agree. Each
/// class has a Sensei Shrine, used by Senseis only, for their Sky-Chi (the
/// Skylanders wiki's "Battle Classes" and "Sky-Chi"), and the earlier games'
/// element gates became Sensei Elemental Realms, which only a Sensei of that
/// element opens (Activision's "Skylanders Imaginators Gameplay FAQ",
/// question 1). All read 7 October 2026.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BattleClass {
    Knight,
    Bowslinger,
    Quickshot,
    Ninja,
    Brawler,
    Smasher,
    Sorcerer,
    Swashbuckler,
    Sentinel,
    Bazooker,
    /// The Kaos Sensei's own, which no other figure has.
    Kaos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sensei {
    pub id: u16,
    /// As Cemu's list names it, so a slot reads the same as the menu.
    pub name: &'static str,
    pub element: Element,
    pub class: BattleClass,
    pub villain: bool,
}

/// Crash and Cortex are guest stars, not villains, so they go with the
/// heroes.
const fn hero(id: u16, name: &'static str, element: Element, class: BattleClass) -> Sensei {
    Sensei { id, name, element, class, villain: false }
}

const fn villain(id: u16, name: &'static str, element: Element, class: BattleClass) -> Sensei {
    Sensei { id, name, element, class, villain: true }
}

use BattleClass::*;
use Casing::*;
use Element::*;

pub const SENSEIS: [Sensei; 31] = [
    hero(601, "King Pen", Water, Brawler),
    hero(602, "Tri-Tip", Earth, Smasher),
    hero(603, "Chopscotch", Undead, Smasher),
    hero(604, "Boom Bloom", Life, Ninja),
    hero(605, "Pit Boss", Undead, Sorcerer),
    hero(606, "Barbella", Earth, Sentinel),
    hero(607, "Air Strike", Air, Brawler),
    hero(608, "Ember", Fire, Sentinel),
    hero(609, "Ambush", Life, Knight),
    villain(610, "Dr. Krankcase", Tech, Quickshot),
    villain(611, "Hood Sickle", Dark, Sentinel),
    villain(612, "Tae Kwon Crow", Fire, Ninja),
    villain(613, "Golden Queen", Earth, Sorcerer),
    villain(614, "Wolfgang", Undead, Bowslinger),
    villain(615, "Pain-Yatta", Magic, Smasher),
    hero(616, "Mysticat", Magic, Sorcerer),
    hero(617, "Starcast", Dark, Ninja),
    hero(618, "Buckshot", Magic, Bowslinger),
    hero(619, "Aurora", Light, Swashbuckler),
    hero(620, "Flare Wolf", Fire, Bazooker),
    villain(621, "Chompy Mage", Life, Bazooker),
    villain(622, "Bad Juju", Air, Swashbuckler),
    villain(623, "Grave Clobber", Water, Brawler),
    villain(624, "Blaster-Tron", Light, Knight),
    hero(625, "Ro-Bow", Tech, Bowslinger),
    hero(626, "Chain Reaction", Tech, Swashbuckler),
    villain(627, "Kaos", Element::Kaos, BattleClass::Kaos),
    hero(628, "Wild Storm", Air, Knight),
    hero(629, "Tidepool", Water, Quickshot),
    hero(630, "Crash Bandicoot", Life, Brawler),
    hero(631, "Dr. Neo Cortex", Tech, Sorcerer),
];

/// The variant a real Sensei carries: the year in its top four bits, 5 for
/// Imaginators (Runes, `SkylanderFormat.md` lines 16 to 27), and nothing else
/// (Texthead1's Skylander-IDs, README lines 400 to 494, a list taken from
/// real figures, last changed 11 September 2026; read 7 October 2026).
/// Cemu's and RPCS3's lists give every Sensei 0x0000, as they give any figure
/// with no variant of its own, so Omoio has a Sensei made with this one
/// instead. Cemu then shows it in its slot as "Unknown (601 20480)"
/// (`FindSkylander`, v2.6), which `figures::fixed_name` names.
pub const SENSEI_VARIANT: u16 = 0x5000;

/// The design on a Creation Crystal's casing. Activision never named them;
/// these are the names fans gave them in Texthead1's Skylander-IDs (README
/// lines 458 to 493, read 7 October 2026), in the order of the design
/// numbers there: three to a casing, and a fourth Rune last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Casing {
    Angel,
    Pyramid,
    Lantern,
    Rune,
    Reactor,
    Acorn,
    Armor,
    Fanged,
    Claw,
    Rocket,
}

/// A Creation Crystal: one id to an element, 680 to 689, in the element
/// order of Trap Team's traps (210 to 219). The variant's low byte is the
/// crystal's design, 0x0200 is the LightCore bit, under which Runes lists
/// the crystals, and 0x0400 marks a Legendary one (Texthead1's
/// Skylander-IDs, README lines 458 to 493, read 7 October 2026).
///
/// An Imaginator's battle class is chosen the first time its crystal goes on
/// and is kept for good: "There are no options to reset a Creation Crystal or
/// change a Battle Class" (Activision's Imaginators Toy FAQ, read 7 October
/// 2026), so a crystal once used is never offered for a new Imaginator. Which
/// Imaginator a crystal holds isn't read: the only description of it, in
/// Runes, says it may be incorrect and leaves out the order of its bits and
/// how the name is written, and Omoio decrypts no figure but a trap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crystal {
    pub id: u16,
    pub variant: u16,
    pub element: Element,
    pub casing: Casing,
}

impl Crystal {
    pub fn legendary(&self) -> bool {
        self.variant & 0x0400 != 0
    }

    /// The name the menu shows, the figure's file takes and a slot reads as:
    /// "Magic Lantern Crystal", or "Legendary Magic Lantern Crystal".
    pub fn name(&self) -> String {
        let legendary = if self.legendary() { "Legendary " } else { "" };
        format!("{legendary}{} {} Crystal", element_word(self.element), casing_word(self.casing))
    }
}

fn element_word(element: Element) -> &'static str {
    match element {
        Air => "Air",
        Earth => "Earth",
        Fire => "Fire",
        Water => "Water",
        Life => "Life",
        Undead => "Undead",
        Magic => "Magic",
        Tech => "Tech",
        Light => "Light",
        Dark => "Dark",
        Element::Kaos => "Kaos",
    }
}

fn casing_word(casing: Casing) -> &'static str {
    match casing {
        Angel => "Angel",
        Pyramid => "Pyramid",
        Lantern => "Lantern",
        Rune => "Rune",
        Reactor => "Reactor",
        Acorn => "Acorn",
        Armor => "Armor",
        Fanged => "Fanged",
        Claw => "Claw",
        Rocket => "Rocket",
    }
}

const fn crystal_of(id: u16, variant: u16, element: Element, casing: Casing) -> Crystal {
    Crystal { id, variant, element, casing }
}

/// The 29 crystals that were sold. Texthead1's list has five more, which
/// were never released (skylanderswiki.com's "Creation Crystal" agrees, read
/// 7 October 2026) and are left out: Air Acorn (682, 0x5212), Tech Pyramid
/// (684, 0x5205), Fire Angel (685, 0x5201), Earth Rune (686, 0x520C) and
/// Light Angel (689, 0x5203).
pub const CRYSTALS: [Crystal; 29] = [
    crystal_of(680, 0x5204, Magic, Pyramid),
    crystal_of(680, 0x5208, Magic, Lantern),
    crystal_of(680, 0x5608, Magic, Lantern),
    crystal_of(680, 0x521B, Magic, Claw),
    crystal_of(681, 0x5214, Water, Armor),
    crystal_of(681, 0x5218, Water, Fanged),
    crystal_of(681, 0x521C, Water, Rocket),
    crystal_of(682, 0x5202, Air, Angel),
    crystal_of(682, 0x5207, Air, Lantern),
    crystal_of(683, 0x5209, Undead, Lantern),
    crystal_of(683, 0x5217, Undead, Fanged),
    crystal_of(683, 0x5219, Undead, Claw),
    crystal_of(684, 0x520D, Tech, Reactor),
    crystal_of(684, 0x5215, Tech, Armor),
    crystal_of(685, 0x520F, Fire, Reactor),
    crystal_of(685, 0x5211, Fire, Acorn),
    crystal_of(686, 0x5213, Earth, Armor),
    crystal_of(686, 0x521D, Earth, Rocket),
    crystal_of(687, 0x5210, Life, Acorn),
    crystal_of(687, 0x5610, Life, Acorn),
    crystal_of(687, 0x521A, Life, Claw),
    crystal_of(687, 0x521E, Life, Rocket),
    crystal_of(687, 0x521F, Life, Rune),
    crystal_of(688, 0x5206, Dark, Pyramid),
    crystal_of(688, 0x520A, Dark, Rune),
    crystal_of(688, 0x520E, Dark, Reactor),
    crystal_of(689, 0x520B, Light, Rune),
    crystal_of(689, 0x5216, Light, Fanged),
    crystal_of(689, 0x5616, Light, Fanged),
];

pub fn sensei(id: u16) -> Option<&'static Sensei> {
    SENSEIS.iter().find(|sensei| sensei.id == id)
}

pub fn crystal(id: u16, variant: u16) -> Option<&'static Crystal> {
    CRYSTALS.iter().find(|crystal| crystal.id == id && crystal.variant == variant)
}

/// Whether a figure is a Creation Crystal, whatever its design, from its id
/// alone.
pub fn is_crystal(id: u16) -> bool {
    (680..=689).contains(&id)
}

/// A Sensei's battle class. An Imaginator's is on its crystal, which isn't
/// read.
pub fn battle_class(id: u16) -> Option<BattleClass> {
    sensei(id).map(|sensei| sensei.class)
}

/// A Sensei's element, or a crystal's, which its id says whatever the
/// design.
pub fn element(id: u16) -> Option<Element> {
    sensei(id)
        .map(|sensei| sensei.element)
        .or_else(|| CRYSTALS.iter().find(|crystal| crystal.id == id).map(|crystal| crystal.element))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::console::Console;
    use crate::core::figures::{self, Character, Class, Game, Kind};

    /// Cemu 2.6's list as the figure maker gives it: every Sensei with
    /// 0x0000.
    fn cemu_senseis() -> Vec<Character> {
        SENSEIS.iter().map(|sensei| Character { name: sensei.name.into(), id: sensei.id, variant: 0 }).collect()
    }

    #[test]
    fn every_sensei_cemu_lists_has_an_element_and_a_class() {
        assert_eq!(SENSEIS.map(|sensei| sensei.id).to_vec(), (601..=631).collect::<Vec<u16>>(), "in id order");
        for id in 601..=631 {
            assert!(figures::element(id).is_some(), "{id}");
            assert!(battle_class(id).is_some(), "{id}");
            assert!(matches!(figures::class(id), Some(Class::Sensei | Class::VillainSensei)), "{id}");
            assert_eq!(figures::kind(id), Kind::Character, "{id}");
        }
        assert!(sensei(600).is_none(), "King Pen is 601");
        assert!(sensei(632).is_none());
        assert_eq!(sensei(601).map(|king_pen| king_pen.name), Some("King Pen"));
    }

    #[test]
    fn the_classes_and_elements_add_up() {
        let in_class = |class| SENSEIS.iter().filter(|sensei| sensei.class == class).count();
        let classes = [
            Knight,
            Bowslinger,
            Quickshot,
            Ninja,
            Brawler,
            Smasher,
            Sorcerer,
            Swashbuckler,
            Sentinel,
            Bazooker,
            BattleClass::Kaos,
        ]
        .map(in_class);
        assert_eq!(classes, [3, 3, 2, 3, 4, 3, 4, 3, 3, 2, 1]);

        // Three to each of the first eight elements and two each to Light and
        // Dark, with Crash for Life and Cortex for Tech, and Kaos his own.
        let of_element = |element| SENSEIS.iter().filter(|sensei| sensei.element == element).count();
        let elements = [Air, Earth, Fire, Water, Life, Undead, Magic, Tech, Light, Dark, Element::Kaos].map(of_element);
        assert_eq!(elements, [3, 3, 3, 3, 4, 3, 3, 4, 2, 2, 1]);
        assert_eq!(sensei(627).map(|kaos| (kaos.element, kaos.class)), Some((Element::Kaos, BattleClass::Kaos)));
    }

    #[test]
    fn ten_villains_one_to_an_element_and_kaos() {
        let villains: Vec<&Sensei> = SENSEIS.iter().filter(|sensei| sensei.villain).collect();
        assert_eq!(villains.len(), 11);
        for element in [Air, Earth, Fire, Water, Life, Undead, Magic, Tech, Light, Dark, Element::Kaos] {
            assert_eq!(villains.iter().filter(|villain| villain.element == element).count(), 1, "{element:?}");
        }
        assert_eq!(figures::class(627), Some(Class::VillainSensei)); // Kaos
        assert_eq!(figures::class(630), Some(Class::Sensei)); // Crash Bandicoot, a guest star
    }

    #[test]
    fn every_crystal_has_its_ids_element() {
        assert_eq!(CRYSTALS.len(), 29);
        for crystal in CRYSTALS {
            let name = crystal.name();
            assert!(is_crystal(crystal.id), "{name}");
            assert_eq!(figures::element(crystal.id), Some(crystal.element), "{name}");
            // The same order as Trap Team's traps, 470 ids before.
            assert_eq!(figures::element(crystal.id - 470), Some(crystal.element), "{name}");
            assert_eq!(figures::kind(crystal.id), Kind::Crystal, "{name}");
            assert_eq!(crystal.variant & 0xF200, 0x5200, "{name}: Imaginators' year and the LightCore bit");
            assert_eq!(super::crystal(crystal.id, crystal.variant), Some(&crystal), "{name}");
        }
        assert!(!is_crystal(679) && !is_crystal(690));
        assert_eq!(figures::element(690), None);
    }

    #[test]
    fn each_crystal_is_listed_once_under_its_own_name() {
        let mut pairs: Vec<(u16, u16)> = CRYSTALS.iter().map(|crystal| (crystal.id, crystal.variant)).collect();
        pairs.sort_unstable();
        pairs.dedup();
        assert_eq!(pairs.len(), CRYSTALS.len());
        let mut names: Vec<String> = CRYSTALS.iter().map(Crystal::name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CRYSTALS.len());
        assert_eq!(crystal(680, 0x5208).map(Crystal::name).as_deref(), Some("Magic Lantern Crystal"));
        assert_eq!(crystal(680, 0x5608).map(Crystal::name).as_deref(), Some("Legendary Magic Lantern Crystal"));
        assert_eq!(crystal(687, 0x521F).map(Crystal::name).as_deref(), Some("Life Rune Crystal"));
        assert_eq!(CRYSTALS.iter().filter(|crystal| crystal.legendary()).count(), 3);
    }

    #[test]
    fn the_designs_go_three_to_a_casing_and_the_unsold_ones_are_left_out() {
        for crystal in CRYSTALS {
            let design = crystal.variant & 0xFF;
            if design == 0x1F {
                assert_eq!(crystal.casing, Rune, "{}", crystal.name());
            } else {
                assert_eq!((design - 1) / 3, crystal.casing as u16, "{}", crystal.name());
            }
        }
        for (id, variant) in [(682, 0x5212), (684, 0x5205), (685, 0x5201), (686, 0x520C), (689, 0x5203)] {
            assert!(super::crystal(id, variant).is_none(), "{id} {variant:#06X}");
        }
    }

    #[test]
    fn imaginators_figures_are_never_offered_in_an_older_game() {
        let mut list = cemu_senseis();
        list.push(Character { name: "Whirlwind".into(), id: 0, variant: 0 });
        for game in [Game::Spyro, Game::Giants, Game::SwapForce, Game::TrapTeam, Game::SuperChargers] {
            for console in [Console::WiiU, Console::Ps3] {
                let offered = figures::offers(list.clone(), Some(game), console);
                assert_eq!(offered.len(), 1, "{game:?}: Whirlwind alone");
                assert!(offered.iter().all(|offer| offer.kind != Kind::Crystal), "{game:?}");
                assert!(offered.iter().all(|offer| offer.battle_class.is_none() && offer.casing.is_none()));
            }
        }
        // A game Omoio can't tell gets every Sensei, but no crystals, which
        // only Imaginators' own menu offers.
        let unknown = figures::offers(list, None, Console::WiiU);
        assert_eq!(unknown.len(), 32);
        assert!(unknown.iter().all(|offer| offer.kind != Kind::Crystal));
    }

    #[test]
    fn imaginators_gets_every_sensei_as_a_real_one_and_every_crystal() {
        let mut list = cemu_senseis();
        list.push(Character { name: "Whirlwind".into(), id: 0, variant: 0 });
        for console in [Console::WiiU, Console::Ps3] {
            let offered = figures::offers(list.clone(), Some(Game::Imaginators), console);
            assert_eq!(offered.len(), 31 + 1 + 29);
            let senseis: Vec<_> = offered.iter().filter(|offer| sensei(offer.character.id).is_some()).collect();
            assert_eq!(senseis.len(), 31);
            assert!(senseis.iter().all(|offer| offer.character.variant == SENSEI_VARIANT));
            assert!(senseis.iter().all(|offer| offer.battle_class.is_some() && offer.element.is_some()));
            let crystals: Vec<_> = offered.iter().filter(|offer| offer.kind == Kind::Crystal).collect();
            assert_eq!(crystals.len(), 29);
            assert!(crystals.iter().all(|offer| offer.casing.is_some() && offer.element.is_some()));
        }
    }

    #[test]
    fn a_crystal_cemu_lists_one_day_is_offered_once() {
        let list = vec![Character { name: "Magic Lantern".into(), id: 680, variant: 0x5208 }];
        let offered = figures::offers(list, Some(Game::Imaginators), Console::WiiU);
        assert_eq!(offered.len(), 29);
        assert_eq!(offered.iter().filter(|offer| offer.character.variant == 0x5208).count(), 1);
        assert_eq!(offered[0].character.name, "Magic Lantern", "Cemu's own name, which its slot shows");
    }

    #[test]
    fn they_are_written_the_way_the_menu_reads_them() {
        let classes = [
            Knight,
            Bowslinger,
            Quickshot,
            Ninja,
            Brawler,
            Smasher,
            Sorcerer,
            Swashbuckler,
            Sentinel,
            Bazooker,
            BattleClass::Kaos,
        ]
        .map(|class| serde_json::to_value(class).unwrap());
        assert_eq!(
            classes,
            [
                "knight",
                "bowslinger",
                "quickshot",
                "ninja",
                "brawler",
                "smasher",
                "sorcerer",
                "swashbuckler",
                "sentinel",
                "bazooker",
                "kaos"
            ]
        );
        let casings = [Angel, Pyramid, Lantern, Rune, Reactor, Acorn, Armor, Fanged, Claw, Rocket]
            .map(|casing| serde_json::to_value(casing).unwrap());
        assert_eq!(
            casings,
            ["angel", "pyramid", "lantern", "rune", "reactor", "acorn", "armor", "fanged", "claw", "rocket"]
        );
        assert_eq!(serde_json::to_value(Element::Kaos).unwrap(), "kaos");
        assert_eq!(serde_json::to_value(Kind::Crystal).unwrap(), "crystal");
        assert_eq!(serde_json::to_value(Class::Sensei).unwrap(), "sensei");
        assert_eq!(serde_json::to_value(Class::VillainSensei).unwrap(), "villain_sensei");

        let list = vec![
            Character { name: "King Pen".into(), id: 601, variant: 0 },
            Character { name: "Kaos".into(), id: 627, variant: 0 },
        ];
        let json = serde_json::to_value(figures::offers(list, Some(Game::Imaginators), Console::WiiU)).unwrap();
        assert_eq!(json[0]["name"], "King Pen");
        assert_eq!(json[0]["variant"], 20480);
        assert_eq!(json[0]["element"], "water");
        assert_eq!(json[0]["class"], "sensei");
        assert_eq!(json[0]["battle_class"], "brawler");
        assert!(json[0]["casing"].is_null());
        assert_eq!(json[1]["element"], "kaos");
        assert_eq!(json[1]["class"], "villain_sensei");
        assert_eq!(json[1]["battle_class"], "kaos");
        let lantern = &json[3];
        assert_eq!(lantern["name"], "Magic Lantern Crystal");
        assert_eq!(lantern["id"], 680);
        assert_eq!(lantern["variant"], 0x5208);
        assert_eq!(lantern["kind"], "crystal");
        assert_eq!(lantern["element"], "magic");
        assert_eq!(lantern["casing"], "lantern");
        assert!(lantern["battle_class"].is_null() && lantern["class"].is_null());
    }
}
