// SPDX-License-Identifier: AGPL-3.0-or-later
//! The kinds the engine records (VIEWER.md §10.1), each with the label and the usual severity the
//! viewer shows. A recorder names a kind by its constant here ([`CONFLICT_RAID`]), so every kind a
//! module records is registered by construction, and the archive and the page agree on what a name
//! means. A kind's own rows may still be more or less severe than its usual (a raid that takes a
//! district is Major).
//!
//! Names are `<module>.<what>`: the part before the first dot is the page's group. The archive's
//! dictionary numbers names on first use, so a constant's position here is not stored anywhere:
//! kinds may be added anywhere in the list, and a name never changes meaning.

use crate::row::Severity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub severity: Severity,
}

/// A registered kind: its place in [`KINDS`]. Recorders write these; the engine's recorder turns
/// them into the archive dictionary's numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Kind(u16);

impl Kind {
    /// Its place in [`KINDS`].
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The kind at a place in [`KINDS`].
    pub fn from_index(i: usize) -> Option<Kind> {
        (i < KINDS.len()).then_some(Kind(i as u16))
    }

    pub fn info(self) -> &'static KindInfo {
        &KINDS[self.0 as usize]
    }

    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// The kind's usual severity.
    pub fn severity(self) -> Severity {
        self.info().severity
    }

    /// The part of the name before the first dot.
    pub fn group(self) -> &'static str {
        group(self.name())
    }
}

macro_rules! kinds {
    ($($id:ident = $name:literal, $label:literal, $sev:ident;)*) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms, dead_code)]
        #[repr(u16)]
        enum Place { $($id),* }
        $(#[doc = concat!("`", $name, "`: ", $label, ".")] pub const $id: Kind = Kind(Place::$id as u16);)*
        /// Every kind a recorder writes.
        pub const KINDS: &[KindInfo] = &[
            $(KindInfo { name: $name, label: $label, severity: Severity::$sev }),*
        ];
    };
}

kinds! {
    // Conflict (rtsim-conflict). A party's legs are a named mover's when it is a war party.
    CONFLICT_FACTION = "conflict.faction", "Camp founded", Minor;
    CONFLICT_STEP = "conflict.step", "Feud escalates", Notable;
    CONFLICT_CALM = "conflict.calm", "Feud calms", Minor;
    CONFLICT_DEMAND = "conflict.demand", "Tribute demanded", Notable;
    CONFLICT_TRIBUTE = "conflict.tribute", "Tribute paid", Notable;
    CONFLICT_COMPLIED = "conflict.complied", "Demand met by stopping", Notable;
    CONFLICT_UNMET = "conflict.unmet", "Demand refused or expired", Notable;
    CONFLICT_PARTY = "conflict.party", "Party musters", Minor;
    CONFLICT_PARTY_LEG = "conflict.party_leg", "Party on the move", Minor;
    CONFLICT_RAID = "conflict.raid", "Raid", Major;
    CONFLICT_AMBUSH = "conflict.ambush", "Train ambushed", Notable;
    CONFLICT_FIGHT = "conflict.fight", "Watched fight ends", Notable;
    CONFLICT_CHIEF = "conflict.chief", "War chief falls", Major;
    CONFLICT_HEIR = "conflict.heir", "Heir takes over", Notable;
    CONFLICT_PARTY_HOME = "conflict.party_home", "Party home", Minor;
    // Dragons (rtsim-ecology). Every dragon is named: its legs are notable.
    DRAGONS_FLIGHT = "dragons.flight", "Dragon flight", Notable;
    DRAGONS_HUNT = "dragons.hunt", "Dragon hunt", Minor;
    DRAGONS_RAID = "dragons.raid", "Dragon raid on a district", Major;
    DRAGONS_CLAIM = "dragons.claim", "Lair claimed", Notable;
    DRAGONS_DEPART = "dragons.depart", "Dragon departs", Notable;
    DRAGONS_BIRTH = "dragons.birth", "Dragon hatched", Notable;
    DRAGONS_DEATH = "dragons.death", "Dragon dies", Major;
    DRAGONS_ARRIVE = "dragons.arrive", "Dragon arrives", Notable;
    DRAGONS_TRIBUTE = "dragons.tribute", "Village pays a lair tribute", Notable;
    DRAGONS_TRIBUTE_LAPSED = "dragons.tribute_lapsed", "Village stops paying tribute", Notable;
    // Herds (rtsim-ecology).
    HERDS_FOUNDED = "herds.founded", "Herd founded", Minor;
    HERDS_MIGRATION = "herds.migration", "Herd migrates", Minor;
    HERDS_PREDATION = "herds.predation", "Herd hunted", Minor;
    HERDS_STARVATION = "herds.starvation", "Herd starves", Minor;
    HERDS_EXTINCT = "herds.extinct", "Herd dies out", Notable;
    // Ships (rtsim-ships). Pirates' legs are a named mover's (a captain is a notable).
    SHIPS_PIRATE_LEG = "ships.pirate_leg", "Pirate under way", Notable;
    SHIPS_RAID = "ships.raid", "Pirate raid", Notable;
    SHIPS_LOST = "ships.lost", "Ship lost", Notable;
    SHIPS_BACK = "ships.back", "Ship back", Minor;
    SHIPS_HAVEN = "ships.haven", "Haven threat changes", Minor;
    SHIPS_PORT = "ships.port", "Port blockaded, closed or reopened", Notable;
    // Phase 1: markets (rtsim-trade, recorded by rtsim-world).
    TRADE_SHIFT = "trade.shift", "Price shift", Minor;
    // Phase 1: villages and bases (rtsim-settle, recorded by rtsim-world).
    SETTLE_JOINED = "settle.joined", "Village joins", Minor;
    SETTLE_BASE = "settle.base", "Base marked", Minor;
    SETTLE_UNMARKED = "settle.unmarked", "Base unmarked", Minor;
    SETTLE_HOUSE = "settle.house", "House placed", Minor;
    // Phase 1: movers (rtsim-travel, recorded by rtsim-world).
    TRAVEL_CARAVAN = "travel.caravan", "Caravan sets out", Routine;
    TRAVEL_CARAVAN_LEG = "travel.caravan_leg", "Caravan on the road", Routine;
    TRAVEL_ARRIVED = "travel.arrived", "Caravan arrives", Routine;
    TRAVEL_BAND_LEG = "travel.band_leg", "Band on the road", Routine;
    // Phase 1: reports (rtsim-knowledge, recorded by rtsim-world where it makes them).
    KNOWLEDGE_ASSAULT = "knowledge.assault", "Assault", Minor;
    KNOWLEDGE_KILLING = "knowledge.killing", "Killing", Notable;
    KNOWLEDGE_ROBBERY = "knowledge.robbery", "Robbery", Notable;
    KNOWLEDGE_THEFT = "knowledge.theft", "Theft", Minor;
    KNOWLEDGE_TRADE = "knowledge.trade", "Trade", Routine;
    KNOWLEDGE_RESCUE = "knowledge.rescue", "Rescue", Notable;
    KNOWLEDGE_RESIDENT_ASSAULT = "knowledge.resident_assault", "Resident assaulted", Minor;
    KNOWLEDGE_RESIDENT_KILLING = "knowledge.resident_killing", "Resident killed", Notable;
    KNOWLEDGE_ENCROACHMENT = "knowledge.encroachment", "Encroachment on a dragon's oil", Minor;
    // Industry (rtsim-industry), at the district's centre.
    INDUSTRY_DETECTED = "industry.detected", "District found", Minor;
    INDUSTRY_LEARNED = "industry.learned", "District learned", Minor;
    INDUSTRY_STALE = "industry.stale", "District changed", Minor;
    INDUSTRY_UNLEARNABLE = "industry.unlearnable", "District not learnable", Minor;
    INDUSTRY_DAMAGED = "industry.damaged", "District damaged", Notable;
    INDUSTRY_RECONCILE = "industry.reconcile", "Reconcile report", Routine;
    INDUSTRY_ATTACK = "industry.attack", "Attack on a district", Major;
    INDUSTRY_LINE = "industry.line", "Train line first seen", Minor;
}

/// A kind by name.
pub fn info(name: &str) -> Option<&'static KindInfo> {
    KINDS.iter().find(|k| k.name == name)
}

/// A registered kind by name.
pub fn by_name(name: &str) -> Option<Kind> {
    KINDS
        .iter()
        .position(|k| k.name == name)
        .and_then(Kind::from_index)
}

/// Every registered kind of a group (`"conflict"`).
pub fn of_group(g: &str) -> impl Iterator<Item = Kind> + '_ {
    (0..KINDS.len())
        .filter_map(Kind::from_index)
        .filter(move |k| k.group() == g)
}

/// A name's group: the part before the first dot (`"conflict.raid"` is `"conflict"`).
pub fn group(name: &str) -> &str {
    name.split_once('.').map_or(name, |(g, _)| g)
}

/// A label for a name [`KINDS`] lacks: its last part in words (`"herds.route_leg"` is
/// `"Route leg"`).
pub fn fallback_label(name: &str) -> String {
    let tail = name.rsplit('.').next().unwrap_or(name).replace('_', " ");
    let mut c = tail.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// The groups recorders use (VIEWER.md §10.1).
pub const GROUPS: &[&str] = &[
    "conflict",
    "dragons",
    "herds",
    "ships",
    "trade",
    "settle",
    "world",
    "travel",
    "knowledge",
    "industry",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_grouped() {
        assert!(!KINDS.is_empty());
        for (i, k) in KINDS.iter().enumerate() {
            assert!(k.name.contains('.'), "{} has no group", k.name);
            assert!(
                GROUPS.contains(&group(k.name)),
                "{} in no known group",
                k.name
            );
            assert!(!k.label.is_empty());
            assert!(
                KINDS[..i].iter().all(|o| o.name != k.name),
                "{} twice",
                k.name
            );
            let kind = Kind::from_index(i).unwrap();
            assert_eq!((kind.name(), kind.index()), (k.name, i));
            assert_eq!(by_name(k.name), Some(kind));
        }
        assert_eq!(Kind::from_index(KINDS.len()), None);
        assert_eq!(CONFLICT_RAID.name(), "conflict.raid");
        assert_eq!(CONFLICT_RAID.group(), "conflict");
        assert_eq!(CONFLICT_RAID.severity(), Severity::Major);
        assert!(of_group("herds").all(|k| k.name().starts_with("herds.")));
        // One per report kind (rtsim-knowledge's ReportKind, Encroachment the ninth).
        assert_eq!(of_group("knowledge").count(), 9);
        assert_eq!(group("conflict.raid"), "conflict");
        assert_eq!(group("loose"), "loose");
        assert_eq!(fallback_label("herds.route_leg"), "Route leg");
        assert_eq!(fallback_label(""), "");
    }
}
