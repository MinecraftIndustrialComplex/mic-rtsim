// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict scenario (PLAN.md §10.3's walkthrough), shared by the tests and `rtsim-cli sim
//! conflict`. **Scaffolding.**
//!
//! A goblin tribe at Gloomhollow claims the Thornwood. A player builds a polluting refinery at the
//! forest's edge (an industry district: its smog counts against the player, who spends the most
//! time in its chunk, P14 as changed) and logs the forest for
//! five days (a direct act: the player's). The tribe takes notice, sends an envoy with a demand,
//! and, in the run where the player ignores it, sends scouts and then a war band down the trail.
//! The player logs off before it arrives: the band meets the refinery abstractly through the
//! industry threat interface (capped, never better than observed), and what it did is applied when
//! the player comes back and the district loads (C17, C18). In the other runs the player pays the
//! envoy's tribute, or fits a catalytic converter (the smog falls under the tribe's tolerance),
//! and the feud steps back down.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use rtsim_archive::record::Meter;
use rtsim_ecology::infra::{DistrictIndex, IndustryLink};
use rtsim_industry::district::{AttackReport, Phase as DPhase};
use rtsim_industry::fake::{FakeWorld, Visit};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rows::RowStats;
use rtsim_knowledge::ActorId;
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
    /// Stop every this many ticks: a save point, where [`Outcome::trail`] records the conflict's
    /// state hash (and a paged run saves). The same stops paged or not, so runs compare.
    pub save_every: Option<u64>,
    /// Page the conflict's row tables to a page store (A16), saving at every save point; a
    /// restart reopens the conflict from the store ([`Conflict::open_paged`]).
    pub paged: Option<Paging>,
    /// Measure the event archive's rows (R, VIEWER.md §10.4): every tape the run has (the
    /// world's two, the conflict engine's, the refinery's industry engine's) is on and drained
    /// into [`Outcome::meter`] at each step, so [`Outcome::conflict`]'s own tape ends empty. Off,
    /// only the conflict's tape records and it keeps its rows for the tests to read.
    pub measure: bool,
}

impl Options {
    pub fn new(seed: u64, days: u64, variant: Variant) -> Self {
        Options {
            seed,
            days,
            variant,
            restart_day: None,
            save_every: None,
            paged: None,
            measure: false,
        }
    }
}

/// Where and under what budget a paged run keeps its pages.
#[derive(Clone, Debug)]
pub struct Paging {
    /// The store's folder (emptied first).
    pub dir: PathBuf,
    /// The row tables' budget, bytes.
    pub budget: usize,
}

/// The conflict's meta page in a paged run (rtsim-jni's conflict page key).
pub const META_KEY: PageKey = PageKey::new(0x4300, 0);

/// A paged save: the meta page and the dirty row pages in one commit, then the tickets.
fn save_paged(c: &mut Conflict, store: &RwLock<PageStore>) {
    let mut b = SaveBatch::new();
    let t = c.save_paged(&mut b, META_KEY);
    store
        .write()
        .expect("store")
        .commit(b, 0)
        .expect("conflict pages commit");
    c.saved(t);
    c.enforce_budget();
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
    /// Whom the refinery's smog counted against (P14, changed: the player who spent the most time
    /// in its chunk; the settlers while nobody had), by name as the rungs give it.
    pub smog_owner: String,
    /// State hashes at the end: the conflict engine's and the world's.
    pub hashes: (u64, u64),
    /// The conflict's state hash at each save point ([`Options::save_every`]).
    pub trail: Vec<u64>,
    /// The row tables' counters (a paged run's loads and evictions), summed over restarts.
    pub rows: RowStats,
    /// The archive rows every tape took ([`Options::measure`]); empty when not measured.
    pub meter: Meter,
}

/// Counts and empties every tape the run has.
fn drain_tapes(m: &mut Meter, w: &mut World, c: &mut Conflict, f: &mut FakeWorld) {
    m.drain(&mut w.tape);
    m.drain(&mut w.knowledge.tape);
    m.drain(&mut c.tape);
    m.drain(&mut f.engine.tape);
}

/// Turns on the world's two tapes.
fn world_tapes_on(w: &mut World) {
    w.tape.set_on(true);
    w.knowledge.tape.set_on(true);
}

/// Runs the scenario.
pub fn run(o: &Options) -> Outcome {
    let (mut w, _ids) = synth::world(o.seed);
    let mut meter = Meter::new();
    if o.measure {
        world_tapes_on(&mut w);
    }
    let mut c = synth::conflict(o.seed, &w);
    c.log_enabled = true;
    c.tape.set_on(true);
    let store = o.paged.as_ref().map(|p| {
        let _ = std::fs::remove_dir_all(&p.dir);
        let s = Arc::new(RwLock::new(
            PageStore::open(&p.dir, Durability::NoSync).expect("page store"),
        ));
        c.attach_store(s.clone(), p.budget);
        s
    });
    let mut trail = Vec::new();
    let mut rows = RowStats::default();
    let mut next_save = o.save_every.map(Tick);
    let player = c.player(&mut w, synth::PLAYER);
    let mut smog_owner: Option<ActorId> = None;
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
    fake.engine.tape.set_on(o.measure);
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
    // The world's difficulty, as a plugin reports it at a session start (Normal: no change).
    plugin.session_start(Tick::ZERO);
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
            next_save,
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
        // Whom the smog counts against (P14): the player who runs the refinery, once they have
        // spent time in its chunk.
        if let Some(s) = c.sources.get(&(synth::REFINERY.0, SignalKind::Smog)) {
            smog_owner = Some(s.actor);
        }
        // The converter goes in half a day after the demand on the smog's owner is made.
        if o.variant == Variant::Converter
            && converter_at.is_none()
            && let Some(owner) = smog_owner
        {
            let f = c.faction_of_key(synth::CAMP).expect("camp");
            if let Some(d) = c.feud(f, owner).and_then(|x| x.demand) {
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
        // The ladder, for the timeline (the one faction's feuds).
        let camp = c.faction_of_key(synth::CAMP).expect("camp");
        for (a, feud) in c.feuds_of(camp) {
            let (a, k) = (*a, a.to_bits());
            if last_steps.get(&k) != Some(&feud.step) {
                last_steps.insert(k, feud.step);
                rungs.push(Rung {
                    day: next.0 as f64 / TICKS_PER_DAY as f64,
                    actor: c.actor_name(&w, a),
                    step: feud.step,
                });
            }
        }
        if next_save == Some(next) {
            if let Some(s) = &store {
                save_paged(&mut c, s);
            }
            trail.push(c.state_hash());
            c.enforce_budget();
            next_save = o.save_every.map(|e| next + e);
        } else if let Some(s) = store.as_ref().filter(|_| c.needs_save()) {
            // The budget's save (every resident page changed; PM-C10): no save point.
            save_paged(&mut c, s);
        }
        if o.measure {
            drain_tapes(&mut meter, &mut w, &mut c, &mut fake);
        }
        if restart == Some(next) {
            restarted = true;
            let wb = w.save();
            let (goods, ids) = rtsim_world::synth::goods();
            let book = rtsim_world::synth::recipes(&ids);
            w = World::load(synth::world_config(o.seed), goods, book, &wb).expect("world loads");
            if o.measure {
                world_tapes_on(&mut w);
            }
            let log = std::mem::take(&mut c.log);
            let tun = c.tun.clone();
            let cultures = c.cultures.clone();
            let tape = std::mem::take(&mut c.tape);
            c = match (&store, &o.paged) {
                (Some(s), Some(p)) => {
                    save_paged(&mut c, s);
                    add_stats(&mut rows, c.row_stats());
                    drop(c);
                    Conflict::open_paged(s.clone(), p.budget, META_KEY, tun, cultures)
                        .expect("conflict pages read")
                        .expect("a meta page")
                }
                _ => Conflict::load(tun, cultures, &c.save()).expect("conflict loads"),
            };
            c.log = log;
            c.log_enabled = true;
            c.tape = tape;
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
    if o.measure {
        drain_tapes(&mut meter, &mut w, &mut c, &mut fake);
    }
    let attacks = fake
        .engine
        .district(synth::REFINERY)
        .map(|d| d.attacks.iter().cloned().collect())
        .unwrap_or_default();
    let blocks_broken = fake.districts[0].broken.len();
    let smog_owner = smog_owner.map_or_else(|| "nobody".to_string(), |a| c.actor_name(&w, a));
    let hashes = (c.state_hash(), w.state_hash());
    c.enforce_budget();
    add_stats(&mut rows, c.row_stats());
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
        smog_owner,
        hashes,
        trail,
        rows,
        meter,
    }
}

fn add_stats(to: &mut RowStats, s: RowStats) {
    to.loads += s.loads;
    to.load_bytes += s.load_bytes;
    to.evictions += s.evictions;
    to.over_budget += s.over_budget;
    to.peak_resident_bytes = to.peak_resident_bytes.max(s.peak_resident_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A measured run counts rows from every tape it has (the world's, the conflict's and the
    /// refinery's), by kind, with none dropped, and ends in the state the plain run reaches: the
    /// meter drains the tapes, it does not change what happens. The plain run keeps the conflict's
    /// own rows on its tape; the measured run moves them to the meter.
    #[test]
    fn a_measured_run_counts_every_tapes_rows_and_changes_nothing() {
        let plain = run(&Options::new(7, 24, Variant::Ignore));
        let o = run(&Options {
            measure: true,
            ..Options::new(7, 24, Variant::Ignore)
        });
        assert_eq!(o.hashes, plain.hashes);
        assert!(plain.meter.rows() == 0, "not measured, not counted");
        assert!(o.conflict.tape.is_empty(), "drained");
        let m = &o.meter;
        assert!(m.rows() > 0 && m.dropped == 0, "{m:?}");
        let groups = |g: &str| {
            m.by_kind
                .iter()
                .filter(|(k, _)| rtsim_archive::kinds::group(k) == g)
                .map(|(_, n)| n)
                .sum::<u64>()
        };
        for g in ["conflict", "industry"] {
            assert!(groups(g) > 0, "no {g} rows in {:?}", m.by_kind);
        }
        // The conflict's rows are the ones the plain run keeps on its tape.
        let kept = rtsim_archive::record::rows_on(&plain.conflict.tape).len() as u64;
        assert_eq!(groups("conflict"), kept);
    }
}
