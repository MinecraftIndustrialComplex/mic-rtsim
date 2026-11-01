// SPDX-License-Identifier: AGPL-3.0-or-later
//! Settlements other engines run, served to predators (EQUILIBRIUM.md §2.4.11;
//! docs/overnight-choices-equilibrium.md EQC556, EQC560, Ben's PP-4 (a)).
//!
//! A prey engine serves its settlements through [`Prey`]: a snapshot of what a predator may take
//! (members above the refuge) before the predators run, and the losses back after. The host does
//! both each round, so neither engine holds the other.

use rtsim_industry::ids::DistrictId;
use rtsim_kernel::clock::Tick;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_world::World;
use serde::{Deserialize, Serialize};

/// A camp whose members a predator may take (EQC556): its members above the refuge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CampPrey {
    /// The camp's key (the faction's).
    pub key: u64,
    /// The camp as a knowledge actor: the news of a raid names it.
    pub actor: ActorId,
    pub pos: Pos,
    /// Members a predator may take now: the pool less the refuge.
    pub takeable: u32,
}

/// A member a predator took from a camp, for the camp's engine to book: a loss no player made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CampLoss {
    pub key: u64,
    pub predator: ActorId,
    pub at: Tick,
}

/// A player district with food in store (EQC556, PP-1 (a)): what a hungry dragon weighs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Larder {
    pub id: DistrictId,
    pub pos: Pos,
    pub watched: bool,
    /// The fullness points its edible goods are worth.
    pub food: i64,
}

/// What a prey engine serves predators (PP-4 (a)).
pub trait Prey {
    /// Its camps of `cultures` (the cultures whose members are the predator's prey) with members
    /// above `refuge`, at `at`.
    fn camp_prey(&self, cultures: &[String], refuge: u32, at: Tick) -> Vec<CampPrey>;
    /// Books `loss`: the camp loses one member to a predator, no player's loss (no pressure).
    fn lose_to_predator(&mut self, w: &mut World, loss: &CampLoss);
}

/// A plugin's trophic rows (EQC550 to EQC562, Ben's PP-4 (a)), as cultures are data
/// (`data/trophic/<plugin>.json`, built in): the predator's response and one row per prey kind.
/// Ice and Fire's rows set the dragons' tunables ([`crate::DragonTunables::default`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrophicRows {
    pub plugin: String,
    pub predator: String,
    /// The weight at which the predator catches half its need (its handling, EQC551).
    pub half_saturation: f64,
    /// Maintenance: under this share of need it starves out; at or under it nothing breeds.
    pub maintenance: f32,
    /// A newcomer or a mate comes only where it would catch this share (the shared gate).
    pub arrive: f32,
    pub rows: Vec<TrophicRow>,
}

/// One trophic link: what a prey kind gives the predator (EQC551, the table after EQC569).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrophicRow {
    /// The prey kind: `herd`, `wild`, `village`, `conflict.camp`, `district`, `oil.field`.
    pub prey: String,
    pub shape: Shape,
    /// The weight's scale (EQC551).
    pub attack: f64,
    /// The share of what it eats from this row that can become young (EQC552; oil 0).
    pub conversion: f32,
    /// For camps: the cultures whose members are its prey.
    #[serde(default)]
    pub cultures: Vec<String>,
    /// Members a raid never takes (a camp's last ones).
    #[serde(default)]
    pub refuge: u32,
    /// Fullness points one member is worth.
    #[serde(default)]
    pub food_each: i32,
    /// Days a settlement rests after a raid (EQC557).
    #[serde(default)]
    pub rest_days: u64,
    /// Which of the predator's species eat from it: `uses_oil` for the species that drink oil;
    /// every species when absent.
    #[serde(default)]
    pub species: Option<String>,
}

/// A row's weight shape (EQC551).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// `(N - R)^2 / K`, with the herds' refuge R.
    Type3Refuge,
    /// The attack itself, shared by mouths.
    Constant,
    /// `attack x min(1, F / 400)` on the edible stock F.
    Stock,
    /// Hunted, never weighed by the gate (a player's district: a lair near a player's base is not
    /// held for his stores).
    HuntOnly,
}

/// Ice and Fire's rows.
pub const ICEANDFIRE_JSON: &str = include_str!("../data/trophic/iceandfire.json");

impl TrophicRows {
    pub fn from_json(doc: &str) -> Result<TrophicRows, String> {
        serde_json::from_str(doc).map_err(|e| e.to_string())
    }

    /// Ice and Fire's rows (`data/trophic/iceandfire.json`).
    pub fn builtin() -> TrophicRows {
        TrophicRows::from_json(ICEANDFIRE_JSON).expect("built-in trophic rows")
    }

    pub fn row(&self, prey: &str) -> Option<&TrophicRow> {
        self.rows.iter().find(|r| r.prey == prey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in rows parse, and they are what the dragons' tunables hold.
    #[test]
    fn the_rows_set_the_dragons_tunables() {
        let rows = TrophicRows::builtin();
        let t = crate::DragonTunables::default();
        assert_eq!(rows.predator, "dragon");
        assert_eq!(t.gate_half, rows.half_saturation);
        assert_eq!(t.starve_leave, rows.maintenance);
        assert_eq!(t.arrive_food, rows.arrive);
        assert_eq!(t.gate_wild, rows.row("wild").unwrap().attack);
        assert_eq!(t.gate_village, rows.row("village").unwrap().attack);
        assert_eq!(t.oil_weight, rows.row("oil.field").unwrap().attack);
        assert_eq!(rows.row("oil.field").unwrap().conversion, 0.0);
        let camp = rows.row("conflict.camp").unwrap();
        assert_eq!(
            (
                t.camp_attack,
                &t.camp_cultures,
                t.camp_refuge,
                t.camp_member_food
            ),
            (camp.attack, &camp.cultures, camp.refuge, camp.food_each)
        );
        assert_eq!(t.raid_rest_days, rows.row("village").unwrap().rest_days);
    }
}
