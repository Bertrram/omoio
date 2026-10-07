//! SuperChargers' twenty vehicles and the SuperChargers who drive them, by
//! the ids Cemu 2.6's figure list gives them (`Skylander.cpp`, read 7
//! October 2026). Each vehicle goes on land, on the sea or in the sky, and
//! has one SuperCharger of its own: with both on the portal, the game calls
//! the vehicle SuperCharged, which is Activision's word too ("SuperCharged
//! combinations", its SuperChargers FAQ, question 14, read 7 October 2026).
//! A pair goes by the character, whatever the variant: Dark Hot Streak is
//! Spitfire's vehicle too.
//!
//! Terrains and pairs are from the Skylanders wiki's "Vehicles" list and each
//! vehicle's own page, read 7 October 2026. Activision's own skylanders.com
//! names its banner pictures by the same pairs (`Spitfire_HotStreak.jpg`,
//! archived 18 September 2018), all twenty of them.

use crate::core::console::Console;
use serde::Serialize;

/// Where a vehicle goes. The Sky, Land and Sea trophies have one each too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Terrain {
    Land,
    Sea,
    Sky,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vehicle {
    pub id: u16,
    pub name: &'static str,
    pub terrain: Terrain,
    /// The id of the vehicle's own SuperCharger.
    pub driver: u16,
}

const fn vehicle(id: u16, name: &'static str, terrain: Terrain, driver: u16) -> Vehicle {
    Vehicle { id, name, terrain, driver }
}

use Terrain::*;

pub const VEHICLES: [Vehicle; 20] = [
    vehicle(3220, "Jet Stream", Sky, 3413), // Hurricane Jet Vac
    vehicle(3221, "Tomb Buggy", Land, 3417), // Bone Bash Roller Brawl
    vehicle(3222, "Reef Ripper", Sea, 3422), // Deep Dive Gill Grunt
    vehicle(3223, "Burn Cycle", Land, 3421), // Lava Lance Eruptor
    vehicle(3224, "Hot Streak", Land, 3412), // Spitfire
    vehicle(3225, "Shark Tank", Land, 3416), // Shark Shooter Terrafin
    vehicle(3226, "Thump Truck", Land, 3411), // Smash Hit
    vehicle(3227, "Crypt Crusher", Land, 3400), // Fiesta
    vehicle(3228, "Stealth Stinger", Sky, 3415), // Super Shot Stealth Elf
    vehicle(3231, "Dive Bomber", Sea, 3425), // Dive-Clops
    vehicle(3232, "Sky Slicer", Sky, 3406), // Stormblade
    vehicle(3233, "Clown Cruiser", Sky, 3424), // Hammer Slam Bowser
    vehicle(3234, "Gold Rusher", Land, 3414), // Double Dare Trigger Happy
    vehicle(3235, "Shield Striker", Land, 3401), // High Volt
    vehicle(3236, "Sun Runner", Sky, 3426), // Astroblast
    vehicle(3237, "Sea Shadow", Sea, 3427), // Nightfall
    vehicle(3238, "Splatter Splasher", Sea, 3402), // Splat
    vehicle(3239, "Soda Skimmer", Sea, 3420), // Big Bubble Pop Fizz
    vehicle(3240, "Barrel Blaster", Land, 3423), // Turbo Charge Donkey Kong
    vehicle(3241, "Buzz Wing", Sky, 3428), // Thrillipede
];

/// Clown Cruiser, Barrel Blaster, Turbo Charge Donkey Kong and Hammer Slam
/// Bowser, which work on Nintendo's consoles only: in the Wii U version, and
/// never on a PlayStation (Activision's "Bowser and Donkey Kong FAQ" and its
/// toy compatibility table, read through the Wayback Machine 7 October
/// 2026). Cemu's list ends their names with "(Nintendo Only)"; RPCS3's has
/// them, so named, commented out (`skylander_dialog.cpp`, c05832b7).
pub const NINTENDO_ONLY: [u16; 4] = [3233, 3240, 3423, 3424];

pub fn nintendo_only(id: u16) -> bool {
    NINTENDO_ONLY.contains(&id)
}

/// Whether a figure works in a game on `console`. Every one does but
/// Nintendo's four, which work on the Wii U only.
pub fn plays_on(console: Console, id: u16) -> bool {
    console == Console::WiiU || !nintendo_only(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Trophy {
    pub id: u16,
    pub terrain: Option<Terrain>,
    /// The villains it lets the player race as.
    pub villains: &'static [&'static str],
    /// The race tracks it opens.
    pub tracks: &'static [&'static str],
}

/// The racing trophies, by the ids Cemu gives them, with what each unlocks
/// (the Skylanders wiki's page for each trophy, read 7 October 2026). A
/// Land, Sea or Sky trophy opens two tracks and four villains, caught in its
/// Boss Pursuit and kept on the trophy, the way a trap keeps its villain
/// (Activision's "Racing in Skylanders SuperChargers FAQ", question 8, read
/// the same day). It also opens the Mirror Cup, the SuperVillain Cup and
/// Boss Pursuit for its terrain.
/// The Kaos Trophy has no terrain, and lets Kaos race in Sky races at once.
pub const TROPHIES: [Trophy; 4] = [
    Trophy {
        id: 3500,
        terrain: Some(Sky),
        villains: &["Wolfgang", "Chef Pepper Jack", "Cluck", "Lord Stratosfear"],
        tracks: &["Cluck's Cuckoo Nest", "The Clock Rock"],
    },
    Trophy {
        id: 3501,
        terrain: Some(Land),
        villains: &["Count Moneybone", "Chompy Mage", "Glumshanks", "Dragon Hunter"],
        tracks: &["Temple of Arkus", "The After Party"],
    },
    Trophy {
        id: 3502,
        terrain: Some(Sea),
        villains: &["Golden Queen", "Captain Frightbeard", "Mesmeralda", "Spellslamzer"],
        tracks: &["Tropic Plunder", "The Golden Temple"],
    },
    Trophy { id: 3503, terrain: None, villains: &["Kaos"], tracks: &[] },
];

pub fn trophy(id: u16) -> Option<&'static Trophy> {
    TROPHIES.iter().find(|trophy| trophy.id == id)
}

/// A vehicle's terrain, or a trophy's. A SuperCharger has none of its own;
/// the menu finds it through its partner.
pub fn terrain(id: u16) -> Option<Terrain> {
    VEHICLES
        .iter()
        .find(|vehicle| vehicle.id == id)
        .map(|vehicle| vehicle.terrain)
        .or_else(|| trophy(id).and_then(|trophy| trophy.terrain))
}

/// A vehicle's own SuperCharger, or a SuperCharger's own vehicle.
pub fn partner(id: u16) -> Option<u16> {
    VEHICLES.iter().find_map(|vehicle| {
        if vehicle.id == id {
            Some(vehicle.driver)
        } else if vehicle.driver == id {
            Some(vehicle.id)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::figures::{self, Element};

    /// The vehicles and SuperChargers in Cemu 2.6's figure list, each id
    /// once, whatever its variants.
    const CEMU_VEHICLES: [u16; 20] = [
        3220, 3221, 3222, 3223, 3224, 3225, 3226, 3227, 3228, 3231, 3232, 3233, 3234, 3235, 3236, 3237, 3238, 3239,
        3240, 3241,
    ];
    const CEMU_SUPERCHARGERS: [u16; 20] = [
        3400, 3401, 3402, 3406, 3411, 3412, 3413, 3414, 3415, 3416, 3417, 3420, 3421, 3422, 3423, 3424, 3425, 3426,
        3427, 3428,
    ];

    #[test]
    fn every_vehicle_cemu_lists_has_a_terrain_and_one_driver() {
        assert_eq!(VEHICLES.map(|vehicle| vehicle.id), CEMU_VEHICLES, "every one, in id order");
        for id in CEMU_VEHICLES {
            assert!(terrain(id).is_some(), "{id}");
            let drivers: Vec<u16> = VEHICLES.iter().filter(|v| v.id == id).map(|v| v.driver).collect();
            assert_eq!(drivers.len(), 1, "{id}");
            assert!(CEMU_SUPERCHARGERS.contains(&drivers[0]), "{id}");
        }
    }

    #[test]
    fn every_supercharger_cemu_lists_drives_one_vehicle() {
        for id in CEMU_SUPERCHARGERS {
            assert_eq!(VEHICLES.iter().filter(|v| v.driver == id).count(), 1, "{id}");
            assert_eq!(terrain(id), None, "{id} goes by its vehicle's");
        }
    }

    #[test]
    fn a_vehicle_shares_its_drivers_element_except_nintendos() {
        for vehicle in VEHICLES {
            let (own, driver) = (figures::element(vehicle.id), figures::element(vehicle.driver));
            assert!(own.is_some() && driver.is_some(), "{}", vehicle.name);
            assert_eq!(own == driver, !nintendo_only(vehicle.id), "{}", vehicle.name);
            assert_eq!(nintendo_only(vehicle.id), nintendo_only(vehicle.driver), "{}", vehicle.name);
        }
        assert_eq!(figures::element(3233), Some(Element::Air)); // Clown Cruiser
        assert_eq!(figures::element(3424), Some(Element::Fire)); // Hammer Slam Bowser
        assert_eq!(figures::element(3240), Some(Element::Tech)); // Barrel Blaster
        assert_eq!(figures::element(3423), Some(Element::Life)); // Turbo Charge Donkey Kong
    }

    #[test]
    fn nine_vehicles_go_on_land_five_on_the_sea_and_six_in_the_sky() {
        let count = |terrain| VEHICLES.iter().filter(|v| v.terrain == terrain).count();
        assert_eq!([count(Land), count(Sea), count(Sky)], [9, 5, 6]);
        assert_eq!(terrain(3224), Some(Land)); // Hot Streak
        assert_eq!(terrain(3237), Some(Sea)); // Sea Shadow
        assert_eq!(terrain(3233), Some(Sky)); // Clown Cruiser
    }

    #[test]
    fn partners_find_each_other_by_id() {
        // Hot Streak's id is Dark, E3 and Golden Hot Streak's too, and
        // Spitfire's is Dark Spitfire's.
        assert_eq!(partner(3224), Some(3412));
        assert_eq!(partner(3412), Some(3224));
        assert_eq!(partner(3240), Some(3423)); // Barrel Blaster, Turbo Charge Donkey Kong
        assert_eq!(partner(3423), Some(3240));
        for vehicle in VEHICLES {
            assert_eq!(partner(vehicle.driver), Some(vehicle.id), "{}", vehicle.name);
            assert_eq!(partner(vehicle.id).and_then(partner), Some(vehicle.id), "{}", vehicle.name);
        }
        assert_eq!(partner(3403), None); // between Splat and Stormblade, no figure
        assert_eq!(partner(3500), None); // Sky Trophy
        assert_eq!(partner(16), None); // Spyro
    }

    #[test]
    fn the_sky_land_and_sea_trophies_have_their_terrain() {
        assert_eq!(TROPHIES.map(|trophy| trophy.id), [3500, 3501, 3502, 3503]);
        assert_eq!(terrain(3500), Some(Sky));
        assert_eq!(terrain(3501), Some(Land));
        assert_eq!(terrain(3502), Some(Sea));
        assert_eq!(terrain(3503), None); // Kaos Trophy
        assert_eq!(terrain(3504), None);
    }

    #[test]
    fn each_terrain_trophy_opens_four_villains_and_two_tracks() {
        // Twelve villains in all, four to a pack (Activision's racing FAQ),
        // none in two packs, and Kaos alone on his own trophy.
        let terrains: Vec<&Trophy> = TROPHIES.iter().filter(|trophy| trophy.terrain.is_some()).collect();
        assert_eq!(terrains.len(), 3);
        for each in &terrains {
            assert_eq!((each.villains.len(), each.tracks.len()), (4, 2), "{}", each.id);
        }
        let mut villains: Vec<&str> = terrains.iter().flat_map(|trophy| trophy.villains.iter().copied()).collect();
        villains.sort_unstable();
        villains.dedup();
        assert_eq!(villains.len(), 12);
        assert_eq!(trophy(3503).map(|kaos| kaos.villains), Some(&["Kaos"][..]));
        assert_eq!(trophy(3501).map(|land| land.villains[2]), Some("Glumshanks"));
        assert!(trophy(3224).is_none()); // Hot Streak
    }

    #[test]
    fn nintendos_figures_play_on_the_wii_u_only() {
        for id in NINTENDO_ONLY {
            assert!(plays_on(Console::WiiU, id), "{id}");
            assert!(!plays_on(Console::Ps3, id), "{id}");
            assert!(partner(id).is_some_and(nintendo_only), "{id} and its partner");
        }
        assert!(plays_on(Console::Ps3, 3224)); // Hot Streak
        assert!(plays_on(Console::Ps3, 3412)); // Spitfire
        assert!(plays_on(Console::Ps3, 16)); // Spyro
        assert!(plays_on(Console::WiiU, 3412));
    }

    #[test]
    fn terrains_are_written_the_way_the_menu_reads_them() {
        assert_eq!(serde_json::to_string(&Land).unwrap(), "\"land\"");
        assert_eq!(serde_json::to_string(&Sea).unwrap(), "\"sea\"");
        assert_eq!(serde_json::to_string(&Sky).unwrap(), "\"sky\"");
    }
}
