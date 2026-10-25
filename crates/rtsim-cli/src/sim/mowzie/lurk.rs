// SPDX-License-Identifier: AGPL-3.0-or-later
//! `sim mowzie`'s lurk runs (M8, MZB-371, MZB-372): a Frostmaw den's raid lying in wait by a line
//! while trains pass it, against the engine, an industry engine and the Java side's lurk table.
//!
//! The rig is [`rail`]'s synthetic lines, and behind them what the real pack has:
//! - **The industry.** Every leg a line's trains run is booked in a real `rtsim_industry::Engine`
//!   (a departure with a cargo), and the conflict engine's robberies go through `IndustryLink`, so
//!   `raid_train` finds the leg, its cargo and the 24,000-tick cooldown of I25.
//! - **The Java lurk table.** [`LurkTable`] keeps each AMBUSH_SPOT until the one that removes it
//!   and hears the engine's commands [`LAG`] ticks after they are said (one command drain). A
//!   train that passes a lurk's track point within the spot's reach is sent as TRAIN_PASSED at the
//!   tick its timetable puts it there ([`synth::pass_tick`]), stamped with the epoch of the spot.
//!   A pass of the few ticks between a lurk's end and Java hearing of it is stale, which is the one
//!   stale pass the engine may count.
//!
//! - **Run 1's ambushes** ([`ambushes`]): the 12- and 24-a-day lines, each with two trains.
//! - **Run 8's lurk points** ([`trips`]): the engine saved and loaded, blob and paged, or
//!   restarted, in the middle of a lurk, in a cowed time and in a meter's decay, against the run
//!   that kept going.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use rtsim_conflict::boundary::{self, AmbushSpot, Cmd, Obs, TrainPassed, encode_cmd};
use rtsim_conflict::culture::{SignalKind, Step};
use rtsim_conflict::engine::{Conflict, ConflictStats, Party, Signal};
use rtsim_conflict::scenario::META_KEY;
use rtsim_conflict::synth::{
    self, LurkTable, MIDDLING, PLAYER, RailDen, RailEvent, RailLeg, RailLine, pass_tick,
};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, StationId, TrainId};
use rtsim_industry::obs::{Carriage, Obs as IndObs, StationDecl, Timed, TrainDeparture};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_trade::MILLI;

use super::rail::{DAY, JITTER, SEEDS, TRUNK_X, den_at, line_at, steady};
use super::{Check, HOUR, check};

/// Ticks between the engine saying a command and Java's lurk table hearing it (PLACEHOLDER: one
/// command drain, five seconds).
const LAG: u64 = 100;
/// The industry's good the trains carry.
const CARGO_GOOD: Good = Good(1);
/// What each of a line's two trains carries a leg: 1280 items, whose tenth (128) is over the
/// Frostmaw's carry of 64, so the carry limits the theft, and 320, whose tenth (32) is under it, so
/// the cap does. (At 640 the two limits coincide and a wrong carry or a missing cap goes unseen.)
const CARGO: [i64; 2] = [1280, 320];
/// The most a train loses in a day, a Frostmaw's carry (MOWZIE.md §4.4).
const DAY_MAX: i64 = 64;
/// The most two trains lose over a middling 2-day wait (§4.4).
const WAIT_MAX: i64 = 256;
/// The share of the passes the cooldown lets through that rob, at `ambush_milli` 500 (MZB-371).
const SHARE: (f64, f64) = (0.35, 0.65);

/// What a train carries each leg: its id picks one of [`CARGO`] (the train number, from the low
/// byte of its id).
fn cargo_of(train: u64) -> i64 {
    CARGO[(train & 0xff).saturating_sub(1) as usize % CARGO.len()]
}

/// A scene: lines, dens with the tick each is found, and the days to run.
#[derive(Clone)]
struct Scene {
    lines: Vec<RailLine>,
    dens: Vec<(u64, Pos, Tick)>,
    days: u64,
}

/// One line of two trains at `per_day` legs a day, and one den.
fn scene(per_day: f64, days: u64) -> Scene {
    let mut l = line_at(1, TRUNK_X, PLAYER, steady(per_day));
    l.trains = 2;
    Scene {
        lines: vec![l],
        dens: vec![den_at(0, 900, 1000, Tick::ZERO)],
        days,
    }
}

/// A lurk as the rig saw it: when the engine began it and ended it, and what its passes took.
#[derive(Clone, Copy, Debug)]
struct LurkLog {
    agent: u64,
    /// The den's faction key.
    faction: u64,
    start: Tick,
    end: Option<Tick>,
    passes: u32,
    items: i64,
}

/// One TRAIN_PASSED the rig sent, and what the engine made of it.
#[derive(Clone, Copy, Debug)]
struct PassLog {
    at: Tick,
    agent: u64,
    train: u64,
    /// The engine took it as a pass of a lurk (`stats.passes` grew).
    counted: bool,
    /// The engine counted it stale.
    stale: bool,
    /// The roll hit and the industry's cooldown said no.
    skipped: bool,
    /// Items robbed, as the engine counted them.
    items: i64,
    /// Items the industry booked as stolen by this pass, from its own books.
    stolen: i64,
    /// The industry's cooldown on this train was over when the pass came (a robbery of it by any
    /// band in the day before sets it).
    open: bool,
    /// The day of the wait it fell on, from 0.
    day: u64,
}

/// How the engine goes through a point of a run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Trip {
    Never,
    Blob,
    Paged,
    /// A session restart: the lurks go to Java again.
    Restart,
    /// The engine opens from its pages, then the session restarts (a load, then a session start).
    PagedRestart,
}

/// What a run leaves, to compare with the run that kept going.
#[derive(Clone, PartialEq, Debug)]
struct Tape {
    /// Every command the engine said, encoded, in order.
    wire: Vec<u8>,
    /// The lurk stream alone: spots (not the session's resends) and TRAIN_ROBBED, encoded, in order.
    lurk_wire: Vec<u8>,
    stats: ConflictStats,
    /// Every feud's grievance, hourly, by bits.
    grievance: Vec<u64>,
    hash: u64,
}

enum Ext {
    Found(usize),
    Depart(usize),
    Rail(usize),
    Hour,
}

impl Ext {
    fn rank(&self) -> (u8, usize) {
        match self {
            Ext::Found(i) => (0, *i),
            Ext::Depart(i) => (1, *i),
            Ext::Rail(i) => (2, *i),
            Ext::Hour => (3, 0),
        }
    }
}

/// The world: the engine, the industry behind it and Java's lurk table, driven tick by tick.
struct Rig {
    scene: Scene,
    w: rtsim_world::World,
    c: Conflict,
    actors: Vec<ActorId>,
    engine: Engine,
    index: DistrictIndex,
    legs: Vec<RailLeg>,
    events: Vec<(Tick, RailEvent)>,
    ext: Vec<(Tick, Ext)>,
    next: usize,
    fid: Vec<Option<u32>>,
    table: LurkTable,
    /// Commands on their way to Java's table: the tick it hears them.
    lagged: Vec<(Tick, Cmd)>,
    /// The passes Java will send, by tick.
    due: BTreeMap<(Tick, u64), TrainPassed>,
    due_seq: u64,
    seq: u64,
    lurk_seq: u64,
    wire: Vec<u8>,
    lurk_wire: Vec<u8>,
    grievance: Vec<u64>,
    lurks: Vec<LurkLog>,
    passes: Vec<PassLog>,
    /// TRAIN_ROBBED, by the tick it was said and the train.
    robbed: Vec<(Tick, u64)>,
    /// AMBUSH_SPOTs sent again by session starts.
    resent: u64,
    /// The agents the hourly look found lurking.
    lurking_seen: BTreeSet<u64>,
    store: Option<Arc<RwLock<PageStore>>>,
    dir: PathBuf,
    /// A load changed the bytes it was given.
    load_drift: bool,
    end: Tick,
}

impl Rig {
    fn new(seed: u64, scene: &Scene, paged: bool) -> Rig {
        let (mut w, _) = synth::world(7);
        let mut c = synth::conflict(7, &w);
        let mut owners: Vec<boundary::Uuid> = Vec::new();
        for l in &scene.lines {
            if !owners.contains(&l.owner) {
                owners.push(l.owner);
            }
        }
        let actors: Vec<ActorId> = owners.iter().map(|o| c.player(&mut w, *o)).collect();
        let dens: Vec<RailDen> = scene
            .dens
            .iter()
            .map(|&(key, p, since)| RailDen {
                key,
                x: p.x,
                z: p.z,
                radius: 200,
                since,
            })
            .collect();
        let end = Tick::from_days(scene.days);
        let events = synth::rail_events(&scene.lines, &dens, seed, JITTER, end);
        let mut legs = synth::rail_legs(&scene.lines, seed, JITTER, end);
        legs.sort_by_key(|l| (l.depart, l.line, l.train));
        // The industry: the world's first good, and each line's two stations.
        let name = w.goods.iter().next().expect("a good").1.name.clone();
        let mut engine = Engine::new(1, Tunables::default(), Tick(0));
        engine.define_good(CARGO_GOOD, GoodInfo::item(&name, 64));
        let mut ends = BTreeSet::new();
        for l in &scene.lines {
            for e in [l.a, l.b] {
                if ends.insert(e.id) {
                    engine.ingest(&Timed {
                        at: Tick(0),
                        obs: IndObs::Station(StationDecl {
                            station: StationId(e.id),
                            district: e.district.map(DistrictId),
                            dim: 0,
                            pos: BlockPos::new(e.x, 64, e.z),
                            name: 0,
                        }),
                    });
                }
            }
        }
        let mut ext: Vec<(Tick, Ext)> = Vec::new();
        for (i, d) in scene.dens.iter().enumerate() {
            ext.push((d.2, Ext::Found(i)));
        }
        for (i, l) in legs.iter().enumerate() {
            ext.push((l.depart, Ext::Depart(i)));
        }
        for (i, e) in events.iter().enumerate() {
            ext.push((e.0, Ext::Rail(i)));
        }
        let mut t = HOUR;
        while t <= end.0 {
            ext.push((Tick(t), Ext::Hour));
            t += HOUR;
        }
        ext.sort_by_key(|e| (e.0, e.1.rank()));
        let dir = std::env::temp_dir().join(format!(
            "rtsim-sim-lurk-{seed}-{}-{}",
            std::process::id(),
            rand_tag()
        ));
        let mut store = None;
        if paged {
            let _ = std::fs::remove_dir_all(&dir);
            let s = Arc::new(RwLock::new(
                PageStore::open(&dir, Durability::NoSync).expect("a page store"),
            ));
            c.attach_store(s.clone(), 8 << 10);
            store = Some(s);
        }
        Rig {
            scene: scene.clone(),
            w,
            c,
            actors,
            engine,
            index: DistrictIndex::default(),
            legs,
            events,
            ext,
            next: 0,
            fid: vec![None; scene.dens.len()],
            table: LurkTable::default(),
            lagged: Vec::new(),
            due: BTreeMap::new(),
            due_seq: 0,
            seq: 0,
            lurk_seq: 0,
            wire: Vec::new(),
            lurk_wire: Vec::new(),
            grievance: Vec::new(),
            lurks: Vec::new(),
            passes: Vec::new(),
            robbed: Vec::new(),
            resent: 0,
            lurking_seen: BTreeSet::new(),
            store,
            dir,
            load_drift: false,
            end,
        }
    }

    fn now(&self) -> Tick {
        self.c.now()
    }

    /// Takes what the engine said: encoded, logged, and on its way to Java's table. `resend` is
    /// a session start's, which the table hears at once.
    fn say(&mut self, resend: bool) {
        let now = self.c.now();
        for cmd in self.c.take_commands() {
            encode_cmd(self.seq, &cmd, &mut self.wire);
            self.seq += 1;
            match &cmd {
                Cmd::AmbushSpot(s) => {
                    if resend {
                        self.resent += 1;
                    } else {
                        encode_cmd(self.lurk_seq, &cmd, &mut self.lurk_wire);
                        self.lurk_seq += 1;
                        if s.flags & boundary::AMBUSH_REMOVED != 0 {
                            if let Some(l) = self
                                .lurks
                                .iter_mut()
                                .rev()
                                .find(|l| l.agent == s.agent && l.end.is_none())
                            {
                                l.end = Some(now);
                            }
                        } else {
                            self.lurks.push(LurkLog {
                                agent: s.agent,
                                faction: s.faction,
                                start: now,
                                end: None,
                                passes: 0,
                                items: 0,
                            });
                        }
                    }
                    let at = if resend { now } else { Tick(now.0 + LAG) };
                    self.lagged.push((at, cmd));
                }
                Cmd::TrainRobbed(r) => {
                    encode_cmd(self.lurk_seq, &cmd, &mut self.lurk_wire);
                    self.lurk_seq += 1;
                    self.robbed.push((now, r.train));
                }
                _ => {}
            }
        }
    }

    /// Java's table hears a command at `at`; a lurk added schedules the passes from `from` on.
    fn hear(&mut self, cmd: &Cmd, from: Tick) {
        if let Cmd::AmbushSpot(s) = cmd
            && s.flags & boundary::AMBUSH_REMOVED != 0
        {
            self.due.retain(|_, tp| tp.agent != s.agent);
        }
        if let Some(s) = self.table.hear(cmd) {
            self.due.retain(|_, tp| tp.agent != s.agent);
            self.schedule(&s, from);
        }
    }

    /// The passes of every leg that meets the spot at `from` or later.
    fn schedule(&mut self, s: &AmbushSpot, from: Tick) {
        for leg in &self.legs {
            if leg.depart.0 + leg.duration < from.0 {
                continue;
            }
            if let Some(at) = pass_tick(&self.scene.lines, leg, s)
                && at >= from
            {
                self.due_seq += 1;
                self.due.insert(
                    (at, self.due_seq),
                    TrainPassed {
                        agent: s.agent,
                        epoch: s.epoch,
                        flags: 0,
                        carriage: 0,
                        train: leg.train,
                    },
                );
            }
        }
    }

    /// Runs the next tick that has anything to do, if it is no later than `limit`.
    fn step(&mut self, limit: Tick) -> Option<Tick> {
        let t = [
            self.c.next_due(),
            self.ext.get(self.next).map(|e| e.0),
            self.due.keys().next().map(|k| k.0),
            self.lagged.iter().map(|l| l.0).min(),
        ]
        .into_iter()
        .flatten()
        .min()?
        .max(self.c.now());
        if t > limit {
            return None;
        }
        {
            let mut link = IndustryLink {
                engine: &mut self.engine,
                index: &mut self.index,
            };
            self.c.run_until(&mut self.w, &mut link, t);
        }
        self.say(false);
        // Java hears what is due, in the order it was said.
        let mut heard = Vec::new();
        let mut keep = Vec::new();
        for l in std::mem::take(&mut self.lagged) {
            if l.0 <= t {
                heard.push(l);
            } else {
                keep.push(l);
            }
        }
        self.lagged = keep;
        for (_, cmd) in heard {
            self.hear(&cmd, t);
        }
        while self.next < self.ext.len() && self.ext[self.next].0 <= t {
            let i = self.next;
            self.next += 1;
            self.external(i, t);
        }
        while let Some(((at, _), _)) = self.due.first_key_value() {
            if *at > t {
                break;
            }
            let ((at, _), tp) = self.due.pop_first().expect("a pass");
            self.deliver(tp, at.max(t));
        }
        Some(t)
    }

    fn external(&mut self, i: usize, t: Tick) {
        match self.ext[i].1 {
            Ext::Found(d) => {
                let (key, pos, _) = self.scene.dens[d];
                let f = self
                    .c
                    .add_faction(&mut self.w, key, "frostmaw", pos, t)
                    .expect("the Frostmaw");
                self.c.factions[f as usize].chief_seed = MIDDLING;
                self.fid[d] = Some(f);
            }
            Ext::Depart(k) => {
                let leg = self.legs[k];
                let (from, to) = leg.ends(&self.scene.lines);
                self.engine.ingest(&Timed {
                    at: leg.depart,
                    obs: IndObs::TrainDeparted(TrainDeparture {
                        train: TrainId(leg.train),
                        station: StationId(from.id),
                        district: DistrictId(from.district.unwrap_or(0)),
                        destination: Some(StationId(to.id)),
                        eta_ticks: leg.duration,
                        carriages: vec![Carriage {
                            index: 0,
                            item_slots: 36,
                            fluid_capacity: 0,
                            goods: GoodVec::from_pairs([(CARGO_GOOD, cargo_of(leg.train))]),
                        }],
                    }),
                });
            }
            Ext::Rail(k) => {
                let mut link = IndustryLink {
                    engine: &mut self.engine,
                    index: &mut self.index,
                };
                match self.events[k].1 {
                    RailEvent::Pass(p) => {
                        self.c.ingest(&mut self.w, &mut link, &Obs::RailPass(p), t)
                    }
                    RailEvent::Arrive(call) => self.c.rail_call(&mut self.w, &call, t),
                }
                self.say(false);
            }
            Ext::Hour => {
                for (d, f) in self.fid.iter().enumerate() {
                    if f.is_none() {
                        continue;
                    }
                    for a in &self.actors {
                        let g = self
                            .c
                            .feud_view(self.scene.dens[d].0, *a)
                            .map_or(0.0, |v| v.grievance);
                        self.grievance.push(g.to_bits());
                    }
                }
                let lurking: Vec<u64> = self
                    .c
                    .parties
                    .iter()
                    .filter(|(h, _)| self.c.lurking(*h))
                    .map(|(h, _)| Party::agent(h))
                    .collect();
                self.lurking_seen.extend(lurking);
            }
        }
    }

    /// Java sends a TRAIN_PASSED at `at`.
    fn deliver(&mut self, tp: TrainPassed, at: Tick) {
        let (passes, stale, items, skipped) = (
            self.c.stats.passes,
            self.c.stats.stale,
            self.c.stats.items_from_districts,
            self.engine.stats.attacks_skipped,
        );
        let open = at >= self.engine.train_cooldown_until(TrainId(tp.train));
        let stolen_before = self.engine.books.stolen.iter().map(|(_, n)| n).sum::<i64>();
        {
            let mut link = IndustryLink {
                engine: &mut self.engine,
                index: &mut self.index,
            };
            self.c
                .ingest(&mut self.w, &mut link, &Obs::TrainPassed(tp), at);
        }
        let got = (self.c.stats.items_from_districts - items) as i64;
        let day = self
            .lurks
            .iter()
            .rev()
            .find(|l| l.agent == tp.agent)
            .map_or(0, |l| (at.0.saturating_sub(l.start.0)) / TICKS_PER_DAY);
        let log = PassLog {
            at,
            agent: tp.agent,
            train: tp.train,
            counted: self.c.stats.passes > passes,
            stale: self.c.stats.stale > stale,
            skipped: self.engine.stats.attacks_skipped > skipped,
            items: got,
            stolen: self.engine.books.stolen.iter().map(|(_, n)| n).sum::<i64>() - stolen_before,
            open,
            day,
        };
        if let Some(l) = self.lurks.iter_mut().rev().find(|l| l.agent == tp.agent) {
            l.passes += u32::from(log.counted);
            l.items += got;
        }
        self.passes.push(log);
        self.say(false);
    }

    /// Runs to `to`.
    /// A player's kills inside the first den's territory, twice the Raid rung's threshold: the
    /// feud climbs again, as a cowed faction's does when it is provoked.
    fn provoke(&mut self) {
        let at = self.now();
        let (key, pos, _) = self.scene.dens[0];
        let f = self.fid[0].expect("the den's faction");
        let amount = 2.0 * self.c.rung_threshold(f, Step::Raid);
        let who = self.actors[0];
        let s = Signal {
            faction: Some(key),
            kind: SignalKind::Killing,
            amount,
            actor: who,
            pos,
            district: None,
        };
        self.c.signal(&mut self.w, s, at);
        self.say(false);
    }

    fn advance(&mut self, to: Tick) {
        while self.step(to).is_some() {}
        {
            let mut link = IndustryLink {
                engine: &mut self.engine,
                index: &mut self.index,
            };
            self.c.run_until(&mut self.w, &mut link, to);
        }
        self.say(false);
    }

    /// Runs on until `f` holds (checked before each tick's work), at most to `limit`.
    fn until(&mut self, limit: Tick, f: impl Fn(&Rig) -> bool) -> bool {
        loop {
            if f(self) {
                return true;
            }
            if self.step(limit).is_none() {
                return false;
            }
        }
    }

    fn lurking(&self) -> usize {
        self.c
            .parties
            .iter()
            .filter(|(h, _)| self.c.lurking(*h))
            .count()
    }

    /// The engine goes through `how`, between two ticks.
    fn trip(&mut self, how: Trip) {
        // Between ticks the replay is over: `restart` ends each one it begins (the host takes no
        // page while replaying, RP-42).
        assert!(!self.c.replaying(), "a trip while replaying");
        let log = self.c.log_enabled;
        match how {
            Trip::Never => {}
            Trip::Blob => {
                let bytes = self.c.save();
                let back = Conflict::load(self.c.tun.clone(), self.c.cultures.clone(), &bytes)
                    .expect("loads");
                self.load_drift |= back.save() != bytes;
                self.c = back;
            }
            Trip::Paged | Trip::PagedRestart => {
                let store = self.store.clone().expect("a paged rig");
                let mut batch = SaveBatch::new();
                let t = self.c.save_paged(&mut batch, META_KEY);
                store.write().unwrap().commit(batch, 0).unwrap();
                self.c.saved(t);
                self.c.enforce_budget();
                let (tun, cultures, hash) = (
                    self.c.tun.clone(),
                    self.c.cultures.clone(),
                    self.c.state_hash(),
                );
                let old = std::mem::replace(
                    &mut self.c,
                    Conflict::new(1, tun.clone(), cultures.clone(), Tick::ZERO),
                );
                drop(old);
                self.c = Conflict::open_paged(store, 8 << 10, META_KEY, tun, cultures)
                    .expect("opens")
                    .expect("a meta page");
                self.load_drift |= self.c.state_hash() != hash;
                if how == Trip::PagedRestart {
                    self.restart();
                }
            }
            Trip::Restart => self.restart(),
        }
        self.c.log_enabled = log;
    }

    /// The session ends: Java's table is gone with it, the engine starts another, and once its
    /// replay is over says every lurk again, which the new table hears at once.
    fn restart(&mut self) {
        self.c.begin_replay(true);
        self.c.resend_notices();
        self.table.clear();
        self.lagged.clear();
        self.due.clear();
        let from = Tick(self.c.now().0 + 1);
        self.say(true);
        for (_, cmd) in std::mem::take(&mut self.lagged) {
            self.hear(&cmd, from);
        }
    }

    fn tape(&self) -> Tape {
        Tape {
            wire: self.wire.clone(),
            lurk_wire: self.lurk_wire.clone(),
            stats: self.c.stats,
            grievance: self.grievance.clone(),
            hash: self.c.state_hash(),
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A tag for a rig's page directory, so rigs of one process do not share one.
fn rand_tag() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

// ---- run 1: the ambushes -------------------------------------------------------------------------

/// What one seed's run leaves of a line's ambushes.
struct Ambushes {
    lurks: Vec<LurkLog>,
    passes: Vec<PassLog>,
    stats: ConflictStats,
    /// TRAIN_ROBBED, by the tick said and the train.
    robbed: Vec<(Tick, u64)>,
    /// Legs the line's trains ran.
    legs: usize,
    open: usize,
    seen: usize,
    unseen: usize,
    destroyed: i128,
    carried: i128,
    home: i128,
    industry: i64,
}

fn one_line(seed: u64, per_day: f64, days: u64) -> Ambushes {
    let mut r = Rig::new(seed, &scene(per_day, days), false);
    let end = r.end;
    r.advance(end);
    let started: BTreeSet<u64> = r.lurks.iter().map(|l| l.agent).collect();
    let stolen = r.engine.books.stolen.iter().map(|(_, n)| n).sum::<i64>();
    Ambushes {
        open: r.lurking(),
        seen: r.lurking_seen.len(),
        unseen: r.lurking_seen.difference(&started).count(),
        robbed: r.robbed.clone(),
        legs: r.legs.len(),
        destroyed: r.c.ledger.destroyed_from_players.values().sum(),
        carried: r.c.ledger.from_players.values().sum(),
        home: r
            .c
            .parties
            .iter()
            .map(|(_, p)| i128::from(p.from_players.values().sum::<i64>()))
            .sum(),
        industry: stolen,
        lurks: r.lurks.clone(),
        passes: r.passes.clone(),
        stats: r.c.stats,
    }
}

fn day_of(t: Tick) -> u64 {
    t.0 / TICKS_PER_DAY
}

/// Run 1's ambush half (MZB-371): the 12- and 24-a-day lines, each with two trains, 8 seeds.
pub(super) fn ambushes(out: &mut Vec<Check>) {
    let mut shares: Vec<(&str, u64, u64, f64)> = Vec::new();
    for (per_day, name) in [(12.0, "12 legs a day"), (24.0, "24 legs a day")] {
        let runs: Vec<Ambushes> = (0..SEEDS).map(|s| one_line(s, per_day, 45)).collect();
        let lurks: usize = runs.iter().map(|a| a.lurks.len()).sum();
        let passes: usize = runs.iter().map(|a| a.passes.len()).sum();

        // Every Raid on a track sends its spot at arrival and the removal when it ends.
        let started: u64 = runs.iter().map(|a| a.stats.lurks).sum();
        let ended: u64 = runs.iter().map(|a| a.stats.lurks_ended).sum();
        let open: usize = runs.iter().map(|a| a.open).sum();
        let heard_ended: usize = runs
            .iter()
            .map(|a| a.lurks.iter().filter(|l| l.end.is_some()).count())
            .sum();
        let seen: usize = runs.iter().map(|a| a.seen).sum();
        let unseen: usize = runs.iter().map(|a| a.unseen).sum();
        check(
            out,
            "every Raid on a track sends its AMBUSH_SPOT when it arrives and the removal when it ends",
            lurks as u64 == started
                && heard_ended as u64 == ended
                && started == ended + open as u64
                && unseen == 0
                && lurks > 0
                && seen > 0,
            format!(
                "{name}: {lurks} lurks over {SEEDS} seeds, {started} started and {ended} ended in \
                 the engine, {heard_ended} removals heard, {open} still waiting at the end; {seen} \
                 lurking bands seen by the hour, {unseen} of them with no spot; the two trains \
                 made {:.1} legs a day",
                runs.iter().map(|a| a.legs).sum::<usize>() as f64 / (SEEDS * 45) as f64
            ),
        );

        // At most 64 items per train per day; at most 256 per wait.
        let mut day: BTreeMap<(u64, u64, u64), i64> = BTreeMap::new();
        let mut robberies: BTreeMap<(u64, u64, u64), u32> = BTreeMap::new();
        for (s, a) in runs.iter().enumerate() {
            for p in a.passes.iter().filter(|p| p.items > 0) {
                *day.entry((s as u64, p.train, day_of(p.at))).or_default() += p.items;
                *robberies
                    .entry((s as u64, p.train, day_of(p.at)))
                    .or_default() += 1;
            }
        }
        let most_day = day.values().copied().max().unwrap_or(0);
        let most_rob = robberies.values().copied().max().unwrap_or(0);
        let robbed_days = day.len();
        check(
            out,
            "a train loses at most 64 items a day, in one robbery",
            robbed_days > 0 && most_day <= DAY_MAX && most_rob <= 1,
            format!(
                "{name}: {robbed_days} train-days robbed, most items {most_day} (limit {DAY_MAX}), \
                 most robberies in one day {most_rob}"
            ),
        );
        // Each robbery takes the Frostmaw's carry or a tenth of the train's cargo, whichever is
        // less: the 1280-item train loses 64 (the carry), the 320-item train 32 (the cap).
        let robberies_all: Vec<&PassLog> = runs
            .iter()
            .flat_map(|a| a.passes.iter().filter(|p| p.items > 0))
            .collect();
        let want = |p: &PassLog| DAY_MAX.min(cargo_of(p.train) / 10);
        let wrong = robberies_all.iter().filter(|p| p.items != want(p)).count();
        let by_cargo = |c: i64| {
            robberies_all
                .iter()
                .filter(|p| cargo_of(p.train) == c)
                .map(|p| p.items)
                .collect::<BTreeSet<i64>>()
        };
        check(
            out,
            "each robbery takes the carry or a tenth of its train's cargo, whichever is less",
            wrong == 0 && by_cargo(CARGO[0]).len() == 1 && by_cargo(CARGO[1]).len() == 1,
            format!(
                "{name}: {} robberies, {wrong} off min({DAY_MAX}, cargo / 10); a {}-item train \
                 loses {:?}, a {}-item train {:?}",
                robberies_all.len(),
                CARGO[0],
                by_cargo(CARGO[0]),
                CARGO[1],
                by_cargo(CARGO[1])
            ),
        );
        let wait = runs
            .iter()
            .flat_map(|a| a.lurks.iter().map(|l| l.items))
            .max()
            .unwrap_or(0);
        let wait_len = runs
            .iter()
            .flat_map(|a| {
                a.lurks
                    .iter()
                    .filter_map(|l| l.end.map(|e| e.0 - l.start.0))
            })
            .max()
            .unwrap_or(0);
        check(
            out,
            "two trains lose at most 256 items over a middling 2-day wait",
            wait > 0 && wait <= WAIT_MAX && wait_len <= 2 * TICKS_PER_DAY,
            format!(
                "{name}: the most one wait took {wait} items (limit {WAIT_MAX}); the longest wait \
                 {:.2} days",
                wait_len as f64 / DAY
            ),
        );

        // The first open pass of each train on each day of a wait (the cooldown is clear): the
        // roll alone decides it, so about half rob. The roll's rate over every pass is the same.
        let mut first: BTreeMap<(usize, u64, u64, u64), bool> = BTreeMap::new();
        let (mut hits, mut counted_all) = (0u64, 0u64);
        for (s, a) in runs.iter().enumerate() {
            for p in a.passes.iter().filter(|p| p.counted) {
                counted_all += 1;
                hits += u64::from(p.items > 0 || p.skipped);
                if p.open && !p.skipped {
                    first
                        .entry((s, p.agent, p.train, p.day))
                        .or_insert(p.items > 0);
                }
            }
        }
        let (n, hit) = (
            first.len() as u64,
            first.values().filter(|r| **r).count() as u64,
        );
        let roll = hits as f64 / counted_all.max(1) as f64;
        shares.push((name, n, hit, roll));

        // The roll is drawn afresh at each pass: the rate on the pass that follows a miss of the
        // same lurk and train, and on the one that follows a hit, are both about half. (A roll
        // keyed by the train alone repeats its answer: nothing after a miss, all after a hit.)
        let mut next = [(0u64, 0u64); 2];
        for a in &runs {
            let mut last: BTreeMap<(u64, u64), bool> = BTreeMap::new();
            for p in a.passes.iter().filter(|p| p.counted) {
                let hit = p.items > 0 || p.skipped;
                if let Some(before) = last.insert((p.agent, p.train), hit) {
                    next[usize::from(before)].0 += 1;
                    next[usize::from(before)].1 += u64::from(hit);
                }
            }
        }
        let rate = |i: usize| next[i].1 as f64 / next[i].0.max(1) as f64;
        check(
            out,
            "the roll is independent from pass to pass: the rate after a miss and after a hit are both about half",
            next.iter().all(|n| n.0 >= 100)
                && (0..2).all(|i| (SHARE.0..=SHARE.1).contains(&rate(i))),
            format!(
                "{name}: {:.0}% of {} passes that follow a miss hit, {:.0}% of {} that follow a hit \
                 (limits {:.0}% to {:.0}%)",
                rate(0) * 100.0,
                next[0].0,
                rate(1) * 100.0,
                next[1].0,
                SHARE.0 * 100.0,
                SHARE.1 * 100.0
            ),
        );

        // The goods are destroyed, not carried home.
        let destroyed: i128 = runs.iter().map(|a| a.destroyed).sum();
        let carried: i128 = runs.iter().map(|a| a.carried).sum();
        let home: i128 = runs.iter().map(|a| a.home).sum();
        let industry: i64 = runs.iter().map(|a| a.industry).sum();
        let items: u64 = runs.iter().map(|a| a.stats.items_from_districts).sum();
        check(
            out,
            "the robbed goods are in the ledger's destroyed share and none is carried home",
            destroyed > 0
                && destroyed == carried
                && home == 0
                && destroyed == i128::from(items) * i128::from(MILLI)
                && i64::try_from(items).is_ok_and(|i| i == industry),
            format!(
                "{name}: {} items destroyed of {} taken from players, {} carried by bands, the \
                 industry booked {industry} stolen, the engine counted {items}",
                destroyed / i128::from(MILLI),
                carried / i128::from(MILLI),
                home / i128::from(MILLI)
            ),
        );

        // The robberies are counted three ways that share no source: the engine's counter, the
        // TRAIN_ROBBED commands the rig heard, and the industry's own books for each pass.
        let told: usize = runs.iter().map(|a| a.robbed.len()).sum();
        let robberies: u64 = runs.iter().map(|a| a.stats.lurk_robberies).sum();
        let ambushes: u64 = runs.iter().map(|a| a.stats.ambushes).sum();
        let booked: usize = runs
            .iter()
            .map(|a| a.passes.iter().filter(|p| p.stolen > 0).count())
            .sum();
        let alike = runs.iter().all(|a| {
            let mut from_passes: Vec<(Tick, u64)> = a
                .passes
                .iter()
                .filter(|p| p.items > 0)
                .map(|p| (p.at, p.train))
                .collect();
            from_passes.sort();
            let mut said = a.robbed.clone();
            said.sort();
            from_passes == said && a.passes.iter().all(|p| p.stolen == p.items)
        });
        check(
            out,
            "the robberies are the same by the engine's counter, the TRAIN_ROBBED sent and the industry's books",
            told as u64 == robberies && booked as u64 == robberies && alike && robberies > 0,
            format!(
                "{name}: {robberies} robberies counted by the engine, {told} TRAIN_ROBBED sent, \
                 {booked} passes with a theft in the industry's books; the same trains at the same \
                 ticks, and the same items for each: {alike} ({ambushes} ambushes in all, the rest \
                 on a march)"
            ),
        );
        let counted: u64 = runs.iter().map(|a| a.stats.passes).sum();
        let sent = passes as u64;
        let stale_ok = runs.iter().all(|a| {
            a.passes.iter().filter(|p| p.stale).all(|p| {
                a.lurks
                    .iter()
                    .rev()
                    .find(|l| l.agent == p.agent)
                    .is_some_and(|l| l.end.is_some_and(|e| p.at >= e && p.at.0 <= e.0 + LAG))
            })
        });
        let stale = runs
            .iter()
            .map(|a| a.passes.iter().filter(|p| p.stale).count())
            .sum::<usize>();
        check(
            out,
            "no TRAIN_PASSED is stale except just after a lurk ended",
            stale_ok && counted + stale as u64 == sent && counted > 0,
            format!(
                "{name}: {sent} passes sent, {counted} counted, {stale} stale, every one within \
                 {LAG} ticks after its lurk's end: {stale_ok}"
            ),
        );
    }
    let (n, hit) = shares.iter().fold((0, 0), |a, s| (a.0 + s.1, a.1 + s.2));
    let share = hit as f64 / n.max(1) as f64;
    let each: Vec<String> = shares
        .iter()
        .map(|(name, n, h, roll)| {
            format!(
                "{name}: {h} of {n} ({:.0}%), the roll hit on {:.0}% of its passes",
                100.0 * *h as f64 / (*n).max(1) as f64,
                roll * 100.0
            )
        })
        .collect();
    check(
        out,
        "the share of each train's first open pass of a day of the wait that robs is about half",
        n >= 100
            && (SHARE.0..=SHARE.1).contains(&share)
            && shares.iter().all(|s| (SHARE.0..=SHARE.1).contains(&s.3)),
        format!(
            "{hit} of {n} robbed ({:.0}%, limits {:.0}% to {:.0}%); {}",
            share * 100.0,
            SHARE.0 * 100.0,
            SHARE.1 * 100.0,
            each.join("; ")
        ),
    );
}

// ---- run 8: save and restore in a lurk, a cowed time and a meter's decay --------------------------

/// The day the Decay point's line is cut from 24 to 4 legs a day.
const DECAY_CUT: u64 = 7;

/// The three points run 8 saves and restores at (MZB-372); a reconcile in flight is
/// `reconcile_in_flight`'s.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Point {
    /// Two dens' bands lie in wait at one track point, each with passes behind it.
    Lurk,
    /// A day after a lurking chief fell, and right after the faction is provoked again: it is cowed,
    /// and the other den's bands go on lurking.
    Cowed,
    /// After a line's service was cut from 24 to 4 legs a day, with a band lurking, a train in
    /// flight (its passes held) and the meter's wake booked.
    Decay,
}

impl Point {
    fn scene(self) -> Scene {
        match self {
            Point::Lurk => {
                let mut s = scene(12.0, 45);
                s.dens.push(den_at(1, 1200, 1000, Tick::ZERO));
                s
            }
            Point::Cowed => {
                // The first den's chief falls. A Frostmaw den has no recruits, and its next body
                // comes from the renewal wait the camp plugin's ledger runs (run 6), which this
                // rig has no plugin for: so the den stays empty, and the second den's lurks are
                // what goes on after the fall.
                let mut s = scene(12.0, 60);
                s.dens.push(den_at(1, 1200, 1000, Tick::ZERO));
                s
            }
            Point::Decay => {
                let mut s = scene(24.0, 30);
                s.lines[0].service.push(synth::Service {
                    from: Tick::from_days(DECAY_CUT),
                    per_day: 4.0,
                    path: 0,
                });
                s
            }
        }
    }
}

/// What one run through a point leaves.
struct Went {
    tape: Tape,
    /// The tick of the trip, how many bands lurked then, and the passes held.
    at: Tick,
    lurking: usize,
    held: usize,
    cowed: bool,
    reached: bool,
    /// Lurks begun (spots said, not the session's resends), and resends.
    starts: usize,
    resent: u64,
    drift: bool,
}

fn go(point: Point, seed: u64, trip: Trip) -> Went {
    let scene = point.scene();
    let mut r = Rig::new(
        seed,
        &scene,
        matches!(trip, Trip::Paged | Trip::PagedRestart),
    );
    let end = r.end;
    let mut reached;
    let mut cowed_f = None;
    match point {
        Point::Lurk => {
            reached = r.until(end, |r| {
                r.lurking() == 2 && r.lurks.iter().all(|l| l.end.is_some() || l.passes >= 1)
            });
            if !reached {
                // One band alone will do, but say so.
                let mut r2 = Rig::new(
                    seed,
                    &scene,
                    matches!(trip, Trip::Paged | Trip::PagedRestart),
                );
                reached = r2.until(end, |r| {
                    r.lurking() == 1 && r.lurks.last().is_some_and(|l| l.passes >= 2)
                });
                r = r2;
            }
        }
        Point::Cowed => {
            let key = r.scene.dens[0].0;
            reached = r.until(end, |r| {
                r.lurks
                    .iter()
                    .any(|l| l.faction == key && l.end.is_none() && l.passes >= 1)
            });
            if reached {
                let f = r.fid[0].expect("the first den's faction");
                let h =
                    r.c.parties
                        .iter()
                        .find(|(h, p)| p.faction == f && r.c.lurking(*h))
                        .map(|(h, _)| h)
                        .expect("a lurking band");
                let killed = r.now();
                let who = r.actors[0];
                r.c.chief_killed(&mut r.w, h, Some(who), killed);
                r.say(false);
                cowed_f = Some(f);
                reached = r.until(end, |r| r.now().0 >= killed.0 + TICKS_PER_DAY);
                // Cowed still, the faction is provoked: its feud climbs again and waits for the end
                // of the cowed time. The trip is right after that.
                r.provoke();
            }
        }
        Point::Decay => {
            reached = r.until(end, |r| {
                r.now() >= Tick::from_days(DECAY_CUT)
                    && r.lurking() >= 1
                    && r.c.held_passes() > 0
                    && r.c.pending_by_kind().get("rail").is_some_and(|n| *n > 0)
            });
        }
    }
    let (at, lurking, held) = (r.now(), r.lurking(), r.c.held_passes());
    let cowed = cowed_f.is_some_and(|f| r.c.factions[f as usize].cowed_until > at);
    let (before_starts, before_resent) = (r.lurks.len(), r.resent);
    r.trip(trip);
    let resent = r.resent - before_resent;
    r.advance(end);
    Went {
        tape: r.tape(),
        at,
        lurking,
        held,
        cowed,
        reached,
        starts: r.lurks.len() - before_starts,
        resent,
        drift: r.load_drift,
    }
}

/// Run 8's M8 points (MZB-372): each is saved and loaded (blob and paged) in place and restarted,
/// against the run that went through it untouched.
pub(super) fn trips(out: &mut Vec<Check>) {
    let seeds = [0u64, 1];
    for (point, saved, restarted) in [
        (
            Point::Lurk,
            "a save in the middle of a lurk, blob and paged, goes on as the run that kept going",
            "a restart in the middle of a lurk, also after a load from pages, says each lurking band's spot once more, and the lurk goes on",
        ),
        (
            Point::Cowed,
            "a save in a cowed time, with the other den lurking, blob and paged, goes on as the run that kept going",
            "a restart in a cowed time, also after a load from pages, says the lurking den's spot once more, and the lurks come as they would have",
        ),
        (
            Point::Decay,
            "a save in a meter's decay, with a band lurking, passes held and a wake booked, goes on as the run that kept going",
            "a restart in a meter's decay, with a band lurking and also after a load from pages, changes no lurk, no grievance and no count",
        ),
    ] {
        let (mut same, mut ok, mut detail) = (true, true, Vec::new());
        let (mut r_ok, mut r_detail) = (true, Vec::new());
        for &seed in &seeds {
            let control = go(point, seed, Trip::Never);
            let blob = go(point, seed, Trip::Blob);
            let paged = go(point, seed, Trip::Paged);
            let restart = go(point, seed, Trip::Restart);
            let paged_restart = go(point, seed, Trip::PagedRestart);
            let state = match point {
                Point::Lurk => control.lurking >= 1,
                Point::Cowed => control.cowed && control.starts >= 1,
                Point::Decay => control.held > 0 && control.lurking >= 1,
            };
            let eq =
                |w: &Went| w.tape == control.tape && w.at == control.at && !w.drift && w.reached;
            let bits = control.tape.grievance.len();
            same &= eq(&blob) && eq(&paged);
            ok &= control.reached && state;
            detail.push(format!(
                "seed {seed}: at tick {} ({} lurking, {} held, cowed {}), {} command bytes, {bits} \
                 grievances by bits, {} lurks begun after, blob {}, paged {}",
                control.at.0,
                control.lurking,
                control.held,
                control.cowed,
                control.tape.wire.len(),
                control.starts,
                if eq(&blob) { "same" } else { "DIFFERENT" },
                if eq(&paged) { "same" } else { "DIFFERENT" },
            ));
            // The restart, plain and after a load from pages: one spot per band lurking then, and
            // everything else as before.
            for (what, restart) in [("restart", &restart), ("paged restart", &paged_restart)] {
                let s = &restart.tape.stats;
                let c = &control.tape.stats;
                let lurk_same = restart.tape.lurk_wire == control.tape.lurk_wire
                    && restart.tape.grievance == control.tape.grievance
                    && (
                        s.lurks,
                        s.lurks_ended,
                        s.passes,
                        s.lurk_robberies,
                        s.ambushes,
                        s.stale,
                    ) == (
                        c.lurks,
                        c.lurks_ended,
                        c.passes,
                        c.lurk_robberies,
                        c.ambushes,
                        c.stale,
                    );
                let once = restart.resent == restart.lurking as u64
                    && restart.starts == control.starts
                    && !restart.drift;
                r_ok &= lurk_same && once && restart.reached;
                r_detail.push(format!(
                    "seed {seed}, {what}: {} lurking, {} spots said again, the lurk stream {} and \
                     the grievances {} the run that kept going",
                    restart.lurking,
                    restart.resent,
                    if restart.tape.lurk_wire == control.tape.lurk_wire {
                        "the same as"
                    } else {
                        "DIFFERENT from"
                    },
                    if restart.tape.grievance == control.tape.grievance {
                        "match"
                    } else {
                        "DIFFER from"
                    },
                ));
            }
        }
        check(out, saved, same && ok, detail.join("; "));
        check(out, restarted, r_ok && ok, r_detail.join("; "));
    }
}
