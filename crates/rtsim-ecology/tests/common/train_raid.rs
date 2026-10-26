// SPDX-License-Identifier: AGPL-3.0-or-later
//! A raider's train raid across a crash, for the raiders' tests (dragons here; conflict and ships
//! include this file by path). docs/choices-mowzie-build.md RP-180 to RP-186, RP-190, RP-191.
//!
//! An industry engine with one booked leg, an infrastructure that sends a raider's train raids
//! to it, and the real train as Java keeps it ([`JavaTrain`]). The crash is modelled as the host
//! has it (docs/replay-gate.md §3.6): one save batch holds the raider's pages and industry's (both
//! taken at `s`, before the raid); the raider comes back from them and runs its window again,
//! while industry is not held: it comes back from the same pages and moves on to Java's present
//! (`c`) before the raider re-decides, so the re-decided raid books at industry's `now`.
//!
//! Java's side (RP-184, RP-190): the train's goods are saved in `create_tracks.dat`
//! (RailwaySavedData), the IND_CARRIAGE_RESULTs the journal replays in `mic_rtsim_journal.dat`.
//! TrainWatch dedups by the train's own ring of applied ops, saved in the train's NBT with its
//! goods ([`Dedup::Ring`], slice T2), or, when the ring's mixin did not apply, by the journal's
//! op index (`Industry.opApplied`, [`Dedup::Journal`], all slice T had). A crash restores each
//! file from its own last save: [`JavaTrain::crash`] takes each part from the save the case names.

#![allow(dead_code)]

use std::collections::BTreeSet;

use rtsim_ecology::infra::{DistrictSite, Infrastructure, RaidOutcome, TrainSighting};
use rtsim_industry::cmd::{CarriageReason, Cmd};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, StationId, TrainId};
use rtsim_industry::obs::{
    Carriage, CarriageResult, CarriageResultEntry, Obs, StationDecl, Timed, TrainDeparture,
};
use rtsim_industry::threat::Threat;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::PageKey;

pub const IRON: Good = Good(1);
/// What the train carries at departure, in carriage 0.
pub const LOAD: i64 = 640;

/// Industry's tunables in these tests: every class carries 32 per attacker (by default only a
/// siege carries), so a dragon's flight or a ship's crew takes something.
pub fn tunables() -> Tunables {
    let mut tun = Tunables::default();
    tun.threat.carry_per_attacker = [32; 5];
    tun
}

/// An industry engine with one booked leg: `train` leaves station 1 (x -2,000) at `depart` for
/// station 2 (x 2,000), both at z `z`, with [`LOAD`] iron in carriage 0, for `eta` ticks.
pub fn industry(seed: u64, train: TrainId, depart: Tick, eta: u64, z: i32) -> Engine {
    let mut e = Engine::new(seed, tunables(), Tick::ZERO);
    e.define_good(IRON, GoodInfo::item("minecraft:iron_ingot", 64));
    for (id, x) in [(1u64, -2_000), (2, 2_000)] {
        e.ingest(&Timed {
            at: Tick(1),
            obs: Obs::Station(StationDecl {
                station: StationId(id),
                district: None,
                dim: 0,
                pos: BlockPos::new(x, 64, z),
                name: id as u32,
            }),
        });
    }
    e.ingest(&Timed {
        at: depart,
        obs: Obs::TrainDeparted(TrainDeparture {
            train,
            station: StationId(1),
            district: DistrictId(0),
            destination: Some(StationId(2)),
            eta_ticks: eta,
            carriages: vec![Carriage {
                index: 0,
                item_slots: 27,
                fluid_capacity: 0,
                goods: GoodVec::from_pairs([(IRON, LOAD)]),
            }],
        }),
    });
    e
}

/// The engine's pages as a save leaves them, and the engine they restore.
pub fn pages(e: &Engine) -> Vec<(PageKey, Vec<u8>)> {
    e.snapshot()
}

pub fn restore(p: &[(PageKey, Vec<u8>)]) -> Engine {
    Engine::restore(p.iter().cloned(), tunables()).expect("industry restores")
}

/// What TrainWatch.carriageDelta dedups by (RP-191).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dedup {
    /// The train's ring, saved with its goods (slice T2, the ring's mixin applied).
    Ring,
    /// The journal's op index alone: slice T, and T2's fallback when the mixin did not apply.
    Journal,
}

/// The real train as Java keeps it: the carriage's goods and the ring of applied ops
/// (`create_tracks.dat`), and the op index with the journaled results (`mic_rtsim_journal.dat`).
#[derive(Clone, Debug, PartialEq)]
pub struct JavaTrain {
    pub train: TrainId,
    pub dedup: Dedup,
    /// Carriage 0's goods.
    pub goods: GoodVec,
    /// The train's ring (`mic_rtsim:applied_ops`): every carriage delta applied to it, saved with
    /// its goods. Unbounded here: the real ring keeps the last 256 (RP-192).
    pub ring: BTreeSet<rtsim_kernel::journal::OpId>,
    /// The journal's op index (`appliedCmdOps`): every carriage delta applied.
    pub ops: BTreeSet<rtsim_kernel::journal::OpId>,
    /// The journal's IND_CARRIAGE_RESULTs, in order.
    pub results: Vec<CarriageResult>,
    /// Carriage deltas applied, and answered DUPLICATE.
    pub applied: u32,
    pub duplicates: u32,
    /// DUPLICATE answers whose loot exchange the engine still held and dropped (RP-182).
    pub dropped: u32,
}

/// Which of Java's two saves landed after the raid's application when the server died (RP-184):
/// `J1` both, `J2` neither, `J3` the train's goods only, `J4` the journal only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landed {
    J1,
    J2,
    J3,
    J4,
}

impl JavaTrain {
    pub fn new(train: TrainId, dedup: Dedup) -> JavaTrain {
        JavaTrain {
            train,
            dedup,
            goods: GoodVec::from_pairs([(IRON, LOAD)]),
            ring: BTreeSet::new(),
            ops: BTreeSet::new(),
            results: Vec::new(),
            applied: 0,
            duplicates: 0,
            dropped: 0,
        }
    }

    /// Java takes the engine's commands, as TrainWatch.carriageDelta does: a carriage delta whose
    /// op the ring (or, by [`Dedup::Journal`], the index) holds is answered DUPLICATE (the engine
    /// is told, as rtsim-jni's `answered_again` tells it); any other is applied as far as the
    /// carriage goes, its op put in the ring and the index, and its IND_CARRIAGE_RESULT journaled
    /// and handed to the engine.
    pub fn take(&mut self, e: &mut Engine) {
        for c in e.drain_commands() {
            let Cmd::CarriageDelta(d) = c else {
                continue;
            };
            assert_eq!(d.train, self.train);
            assert_eq!(d.reason, CarriageReason::Loot);
            let seen = match self.dedup {
                Dedup::Ring => &self.ring,
                Dedup::Journal => &self.ops,
            };
            if seen.contains(&d.op) {
                self.duplicates += 1;
                if e.command_duplicate(d.op) {
                    self.dropped += 1;
                }
                continue;
            }
            self.applied += 1;
            self.ring.insert(d.op);
            self.ops.insert(d.op);
            let mut entries = Vec::new();
            for &(carriage, good, delta) in &d.entries {
                let applied = delta.max(-self.goods.get(good));
                self.goods.add(good, applied);
                entries.push(CarriageResultEntry {
                    carriage,
                    good,
                    requested: delta,
                    applied,
                });
            }
            let r = CarriageResult {
                op: d.op,
                train: d.train,
                entries,
            };
            let at = e.now();
            e.ingest(&Timed {
                at,
                obs: Obs::CarriageResult(r.clone()),
            });
            self.results.push(r);
        }
    }

    /// What a restarted Java has after a crash: each part from the save `landed` names, `before`
    /// being Java at the last save before the raid and `self` Java when the server died.
    pub fn crash(&self, before: &JavaTrain, landed: Landed) -> JavaTrain {
        let (tracks, journal) = match landed {
            Landed::J1 => (self, self),
            Landed::J2 => (before, before),
            Landed::J3 => (self, before),
            Landed::J4 => (before, self),
        };
        JavaTrain {
            goods: tracks.goods.clone(),
            ring: tracks.ring.clone(),
            ops: journal.ops.clone(),
            results: journal.results.clone(),
            ..self.clone()
        }
    }

    /// Iron taken off the train.
    pub fn taken(&self) -> i64 {
        LOAD - self.goods.get(IRON)
    }
}

/// Infrastructure with no districts, on which every leg a raider walks or sails crosses `train`:
/// a raid on it goes to the industry engine, and its loot is what the engine booked stolen, as
/// `IndustryLink::raid_train` reports it. Records every train raid with its loot in `raids`.
pub struct Rails<'a> {
    pub engine: &'a mut Engine,
    pub train: TrainId,
    pub raids: &'a mut Vec<(Threat, Option<GoodVec>)>,
}

impl Infrastructure for Rails<'_> {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        None
    }

    fn good_name(&self, g: Good) -> Option<String> {
        self.engine.goods().get(&g).map(|i| i.name.clone())
    }

    fn trains_crossed(
        &mut self,
        _: u16,
        _: &[(i32, i32)],
        from: Tick,
        to: Tick,
    ) -> Vec<TrainSighting> {
        vec![TrainSighting {
            train: self.train,
            from,
            to,
        }]
    }

    fn raid_train(&mut self, t: &Threat) -> Option<GoodVec> {
        let before = self.engine.books.stolen.clone();
        self.engine.threat(t);
        let loot = self.engine.books.stolen.minus(&before);
        let loot = (!loot.is_empty()).then_some(loot);
        self.raids.push((*t, loot.clone()));
        loot
    }
}

/// When the journal's IND_CARRIAGE_RESULT of the live application reaches the restarted engine
/// (RP-182): before the raider re-decides the raid (`ResultFirst`, O2), or after it and before
/// Java's DUPLICATE answer (`RaidFirst`, O1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    ResultFirst,
    RaidFirst,
}

/// A raider the tests crash: its pages, and its run on an industry engine.
pub trait Raider: Sized {
    type Saved;
    fn save(&self) -> Self::Saved;
    fn restore(saved: &Self::Saved) -> Self;
    /// Runs live to `to`, its train raids going to `e`.
    fn run(&mut self, e: &mut Engine, to: Tick);
    /// Runs its window again after a restart, to `to`: the replay as the host drives this raider.
    fn replay(&mut self, e: &mut Engine, to: Tick);
}

/// A live run and the run that crashed and came back.
pub struct Crashed<R> {
    /// The live run's raider, industry and Java at `c`, never crashed: the control.
    pub raider: R,
    pub live: Engine,
    pub java: JavaTrain,
    /// After the restart and the raider's replay to `c`.
    pub again: R,
    pub restarted: Engine,
    pub java_again: JavaTrain,
}

/// [`across_a_crash_on`] with T2's ring, as Java dedups when the ring's mixin applied. J1 and J2
/// give the same under either dedup.
pub fn across_a_crash<R: Raider>(
    r: R,
    e: Engine,
    train: TrainId,
    s: Tick,
    c: Tick,
    landed: Landed,
    order: Order,
) -> Crashed<R> {
    across_a_crash_on(Dedup::Ring, r, e, train, s, c, landed, order)
}

/// The raider and industry run live to `s` and are saved there (one host batch), then on to `c`,
/// Java taking industry's commands and deduping by `dedup`; the server dies at `c`, Java's saves
/// as `landed` says. The restart: the raider and industry from `s`'s pages, industry moved on to
/// `c` (not held), the journal's carriage results from `s` on in `order`, and the raider's replay
/// to `c`, Java taking what industry decides.
#[allow(clippy::too_many_arguments)]
pub fn across_a_crash_on<R: Raider>(
    dedup: Dedup,
    mut r: R,
    mut e: Engine,
    train: TrainId,
    s: Tick,
    c: Tick,
    landed: Landed,
    order: Order,
) -> Crashed<R> {
    let mut java = JavaTrain::new(train, dedup);
    r.run(&mut e, s);
    java.take(&mut e);
    let saved = r.save();
    let at_s = pages(&e);
    let java_s = java.clone();
    r.run(&mut e, c);
    java.take(&mut e);
    assert!(java.applied > java_s.applied, "the live run raided after s");

    let mut java_again = java.crash(&java_s, landed);
    let replayed: Vec<CarriageResult> = java_again.results[java_s.results.len()..].to_vec();
    let mut again = R::restore(&saved);
    let mut restarted = restore(&at_s);
    restarted.advance_to(c);
    let feed = |e: &mut Engine| {
        for res in &replayed {
            e.ingest(&Timed {
                at: c,
                obs: Obs::CarriageResult(res.clone()),
            });
        }
    };
    if order == Order::ResultFirst {
        feed(&mut restarted);
    }
    again.replay(&mut restarted, c);
    if order == Order::RaidFirst {
        feed(&mut restarted);
    }
    java_again.take(&mut restarted);
    Crashed {
        raider: r,
        live: e,
        java,
        again,
        restarted,
        java_again,
    }
}

/// Industry's pages newer than the raider's (RP-183): the raider comes back from `s` while the
/// live industry engine, moved on to `c2`, holds the raid. The raider's replay to `c`. Returns
/// the live Java at `c` and Java after the replay.
pub fn industry_newer<R: Raider>(
    mut r: R,
    mut e: Engine,
    train: TrainId,
    s: Tick,
    c: Tick,
    c2: Tick,
) -> (JavaTrain, JavaTrain, Engine) {
    let mut java = JavaTrain::new(train, Dedup::Ring);
    r.run(&mut e, s);
    java.take(&mut e);
    let saved = r.save();
    let before = java.applied;
    r.run(&mut e, c);
    java.take(&mut e);
    assert!(java.applied > before, "the live run raided after s");
    let live = java.clone();
    let mut again = R::restore(&saved);
    e.advance_to(c2);
    again.replay(&mut e, c);
    java.take(&mut e);
    (live, java, e)
}
