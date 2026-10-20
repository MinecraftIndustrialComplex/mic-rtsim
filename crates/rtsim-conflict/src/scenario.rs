// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict scenario (PLAN.md §10.3's walkthrough), shared by the tests and `rtsim-cli sim
//! conflict`. **Scaffolding.**
//!
//! A goblin tribe at Gloomhollow claims the Thornwood. A player builds a polluting refinery at the
//! forest's edge (an industry district: its smog is the settlers', P14) and logs the forest for
//! five days (a direct act: the player's). The tribe takes notice, sends an envoy with a demand,
//! and, in the run where the player ignores it, sends scouts and then a war band down the trail.
//! The player logs off before it arrives: the band meets the refinery abstractly through the
//! industry threat interface (capped, never better than observed), and what it did is applied when
//! the player comes back and the district loads (C17, C18). In the other runs the player pays the
//! envoy's tribute, or fits a catalytic converter (the smog falls under the tribe's tolerance),
//! and the feud steps back down.

use std::collections::BTreeMap;

use rtsim_ecology::infra::{DistrictIndex, IndustryLink};
use rtsim_industry::district::{AttackReport, Phase as DPhase};
use rtsim_industry::fake::{FakeWorld, Visit};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_world::World;
use serde::Serialize;

use crate::culture::{SignalKind, Step};
use crate::engine::{Conflict, Signal};
use crate::synth::{self, Answer, FakeConflict};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    /// The player ignores the envoy: a war band marches.
    Ignore,
    /// The player pays the envoy's tribute.
    Tribute,
    /// The player fits a catalytic converter half a day after the demand.
    Converter,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub seed: u64,
    pub days: u64,
    pub variant: Variant,
    /// Save the conflict engine and the world on this day, load them, and go on (the restore).
    pub restart_day: Option<f64>,
}

/// The refinery's smog, units a day (Destroy's, PLACEHOLDER), and with the converter.
pub const SMOG_PER_DAY: f64 = 30.0;
pub const CONVERTED_PER_DAY: f64 = 3.0;
/// Logs broken a day, days 1 to 5.
pub const LOGS_PER_DAY: f64 = 64.0;
/// The player is at the refinery until this day, offline until [`BACK_DAY`].
pub const OFFLINE_DAY: f64 = 8.0;
pub const BACK_DAY: f64 = 13.0;

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// A rung reached, for the timeline.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Rung {
    pub day: f64,
    pub actor: String,
    pub step: Step,
}

pub struct Outcome {
    pub world: World,
    pub conflict: Conflict,
    pub fake: FakeWorld,
    pub plugin: FakeConflict,
    pub rungs: Vec<Rung>,
    pub attacks: Vec<AttackReport>,
    /// Blocks the industry world had broken at the end (damage applied on load).
    pub blocks_broken: usize,
    /// The day the refinery's district loaded again after the raid, if it did.
    pub reloaded_day: Option<f64>,
    pub converter_day: Option<f64>,
    /// State hashes at the end: the conflict engine's and the world's.
    pub hashes: (u64, u64),
}

/// Runs the scenario.
pub fn run(o: &Options) -> Outcome {
    let (mut w, _ids) = synth::world(o.seed);
    let mut c = synth::conflict(o.seed, &w);
    c.log_enabled = true;
    let player = c.player(&mut w, synth::PLAYER);
    let settlers = c.settlers(&mut w);
    let visits = vec![
        Visit {
            from: Tick::ZERO,
            at: vec![0],
        },
        Visit {
            from: day(OFFLINE_DAY),
            at: Vec::new(),
        },
        Visit {
            from: day(BACK_DAY),
            at: vec![0],
        },
    ];
    let mut fake = synth::refinery(o.seed, visits);
    fake.engine.log_enabled = true;
    let mut index = DistrictIndex::default();
    let mut plugin = FakeConflict::new(synth::PLAYER);
    plugin.answer = match o.variant {
        Variant::Tribute => Answer::Pay,
        _ => Answer::Ignore,
    };
    // At home the player defends the refinery: it kills four in ten of each wave the director
    // sends, the war chief among them.
    plugin.fight = Some(synth::Fight {
        kill_share: 0.4,
        after: 200,
        kill_chief: true,
        only_at_base: true,
    });
    c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
        .expect("goblins");
    let end = Tick::from_days(o.days);
    let built = day(0.5);
    let mut smog = 0.0;
    let mut converter_day: Option<f64> = None;
    let mut converter_at: Option<Tick> = None;
    let mut logs_done = 0u64;
    let mut online = true;
    let mut reloaded_day = None;
    let mut was_damaged = false;
    let mut rungs: Vec<Rung> = Vec::new();
    let mut last_steps: BTreeMap<u64, Step> = BTreeMap::new();
    let mut next_obs = Tick::ZERO;
    let mut restarted = false;
    let mut t = Tick::ZERO;
    loop {
        let log_at = (logs_done < 5).then(|| day(1.5 + logs_done as f64));
        let presence = if online {
            day(OFFLINE_DAY)
        } else {
            day(BACK_DAY)
        };
        let restart = o.restart_day.filter(|_| !restarted).map(day);
        let mut next = [
            Some(end),
            c.next_due(),
            plugin.next_due(),
            Some(built).filter(|&b| b > t || t == Tick::ZERO),
            log_at,
            converter_at.filter(|&a| a > t),
            Some(presence).filter(|&p| p > t),
            online.then_some(next_obs),
            restart,
        ]
        .into_iter()
        .flatten()
        .filter(|&x| x >= t)
        .min()
        .expect("end");
        if next == t && t > Tick::ZERO && next < end {
            next = t + 1;
        }
        let next = next.min(end);
        fake.run_until(next);
        {
            let mut link = IndustryLink {
                engine: &mut fake.engine,
                index: &mut index,
            };
            for (at, ob) in plugin.due(next) {
                c.ingest(&mut w, &mut link, &ob, at);
            }
            c.run_until(&mut w, &mut link, next);
        }
        // The script at this tick.
        if next >= built && smog == 0.0 && converter_at.is_none_or(|a| next < a) {
            smog = SMOG_PER_DAY;
            c.set_emission(
                &mut w,
                synth::REFINERY.0,
                SignalKind::Smog,
                synth::REFINERY_POS,
                Some(synth::REFINERY),
                smog,
                next,
            );
        }
        if log_at == Some(next) {
            logs_done += 1;
            let sig = Signal {
                faction: None,
                kind: SignalKind::Logging,
                amount: LOGS_PER_DAY,
                actor: player,
                pos: synth::FOREST_POS,
                district: None,
            };
            c.signal(&mut w, sig, next);
        }
        // The converter goes in half a day after the settlers' demand is made.
        if o.variant == Variant::Converter && converter_at.is_none() {
            let f = c.faction_of_key(synth::CAMP).expect("camp");
            if let Some(d) = c.feuds.get(&(f, settlers)).and_then(|x| x.demand) {
                converter_at = Some(d.made + TICKS_PER_DAY / 2);
            }
        }
        if converter_at == Some(next) {
            converter_day = Some(next.0 as f64 / TICKS_PER_DAY as f64);
            smog = CONVERTED_PER_DAY;
            c.set_emission(
                &mut w,
                synth::REFINERY.0,
                SignalKind::Smog,
                synth::REFINERY_POS,
                Some(synth::REFINERY),
                smog,
                next,
            );
        }
        // A destroyed machine stops the refinery (I23), and its smog with it.
        let damaged = fake
            .engine
            .district(synth::REFINERY)
            .is_some_and(|d| d.phase == DPhase::Damaged);
        if damaged != was_damaged && smog > 0.0 {
            was_damaged = damaged;
            c.set_emission(
                &mut w,
                synth::REFINERY.0,
                SignalKind::Smog,
                synth::REFINERY_POS,
                Some(synth::REFINERY),
                if damaged { 0.0 } else { smog },
                next,
            );
        }
        if online && next >= day(OFFLINE_DAY) && next < day(BACK_DAY) {
            online = false;
            c.player_left(&mut w, player);
        } else if !online && next >= day(BACK_DAY) {
            online = true;
            reloaded_day = Some(next.0 as f64 / TICKS_PER_DAY as f64);
            next_obs = next;
        }
        if online && next >= next_obs {
            c.observe_player(&mut w, synth::PLAYER, synth::REFINERY_POS, next);
            next_obs = next + 1_200;
        }
        plugin.apply(c.take_commands(), next);
        // The ladder, for the timeline.
        for (&(_, a), feud) in &c.feuds {
            let k = a.to_bits();
            if last_steps.get(&k) != Some(&feud.step) {
                last_steps.insert(k, feud.step);
                rungs.push(Rung {
                    day: next.0 as f64 / TICKS_PER_DAY as f64,
                    actor: c.actor_name(&w, a),
                    step: feud.step,
                });
            }
        }
        if restart == Some(next) {
            restarted = true;
            let (cb, wb) = (c.save(), w.save());
            let (goods, ids) = rtsim_world::synth::goods();
            let book = rtsim_world::synth::recipes(&ids);
            w = World::load(synth::world_config(o.seed), goods, book, &wb).expect("world loads");
            let log = std::mem::take(&mut c.log);
            let tun = c.tun.clone();
            let cultures = c.cultures.clone();
            c = Conflict::load(tun, cultures, &cb).expect("conflict loads");
            c.log = log;
            c.log_enabled = true;
        }
        t = next;
        if t >= end {
            break;
        }
    }
    // Let the last records land, with the player at the refinery.
    let settle = end + 600;
    fake.run_until(settle);
    {
        let mut link = IndustryLink {
            engine: &mut fake.engine,
            index: &mut index,
        };
        for (at, ob) in plugin.due(settle) {
            c.ingest(&mut w, &mut link, &ob, at);
        }
        c.run_until(&mut w, &mut link, settle);
    }
    plugin.apply(c.take_commands(), settle);
    let attacks = fake
        .engine
        .district(synth::REFINERY)
        .map(|d| d.attacks.iter().cloned().collect())
        .unwrap_or_default();
    let blocks_broken = fake.districts[0].broken.len();
    let hashes = (c.state_hash(), w.state_hash());
    Outcome {
        world: w,
        conflict: c,
        fake,
        plugin,
        rungs,
        attacks,
        blocks_broken,
        reloaded_day,
        converter_day,
        hashes,
    }
}
