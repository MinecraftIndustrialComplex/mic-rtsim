// SPDX-License-Identifier: AGPL-3.0-or-later
//! Oil nests, the oil gate's underground half (OILGATE.md §4.6, Ben's OG-9e; EQUILIBRIUM.md §2.4.8;
//! choices in docs/choices-oilgate.md from OGC70).
//!
//! An oil nest is a chamber under an oil field with a spawner heart in it, and Ice and Fire's Death
//! Worms are its body. It is the fifth threat class, `Burrowing`: worms come up under a district's
//! machines and inventories from below, past walls, turrets and guards; an armoured floor and the
//! heart are the counters.
//!
//! - **Founding** ([`Dragons::nest_seam`]): an oil chunk new to the index (its first load) founds a
//!   nest in its cell of 8 by 8 chunks if the cell has none, and asks the plugin for its chamber
//!   (PLACE_NEST). The plugin answers NEST_FOUND, placed with its heart or failed (no room); a failed
//!   cell is given up. One nest a cell, keyed by (frame, cell x, cell z) in a paged map (0x4419).
//! - **Asleep until drilled**: a nest is awake while a pumpjack's oil deposit, sampled in its
//!   district's latest sweep and not dry, lies within `reach` blocks of its heart
//!   ([`crate::infra::Infrastructure::wells`]). An asleep nest holds: nothing renews, nothing raids,
//!   its waits stand still.
//! - **Renewal** (EQUILIBRIUM.md §2.2's shared rule, [`rtsim_kernel::renewal`]): its first waking
//!   sends K worms (NEST_RENEW). A worm lost starts a wait of `min(cap, base x 2^P)` drilling days
//!   when the nest is short and no wait runs; at its end one worm is owed, and if the nest is still
//!   short the next wait starts. A player's loss adds count / K to P, which halves every half-life.
//!   Worms appear underground at the heart's next unwatched load (OnLoad); NEST_CENSUS reconciles.
//! - **The stage rises with drainage**: the wells in reach left against the most they held, in
//!   thirds (stage 1 to 3, a bottomless well 1). It is the worms' size and their strength.
//! - **Raids**: each drilling day, with a chance, the worms go at a district with a chunk within
//!   `reach` of the heart, a Burrowing threat through the industry engine (its I25 cooldown paces
//!   them). A watched district is raided the same way at once, and the loaded worms come up under it
//!   for a real fight (NEST_SORTIE, as OGC56 does for dragons).
//! - **The heart**: broken (NEST_HEART, or a census that finds it gone), nothing renews; it reforms
//!   after `min(reform cap, reform base x 2^P)` drilling days, P bumped by one, and the nest asks for
//!   its heart again.
//!
//! The nests' watch is one event for the world, every `day_ticks` (a game day; the test order
//! NEST_DAY shortens it). Its day counter is the nests' own clock for pressure and waits.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::meta_pages::PagedMap;
use rtsim_kernel::renewal::{LossCause, Pressure, RenewalRule};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_world::World;
use serde::Serialize;

use crate::boundary::{self, Cmd};
use crate::engine::{Dragons, Ev};
use crate::infra::Infrastructure;

/// The nests' page table when the dragons page.
pub const NEST_TABLE: u16 = 0x4419;

/// Chunks a side of a nest's cell: 8 (128 blocks).
pub const CELL_CHUNKS: i64 = 8;
const CELL_BLOCKS: i64 = CELL_CHUNKS * 16;

const SYS_NEST: SystemId = SystemId::named("ecology.nest");

/// A nest's key: (frame, cell x, cell z), cells of [`CELL_CHUNKS`] canonical chunks.
pub type NestKey = (u16, i64, i64);

/// The nest's wire id: the frame in the top 16 bits, then the cell's x and z in 24 bits each.
pub fn nest_id(k: NestKey) -> u64 {
    (u64::from(k.0) << 48) | ((k.1 as u64 & 0xFF_FFFF) << 24) | (k.2 as u64 & 0xFF_FFFF)
}

/// The key of a wire id (24-bit cells sign-extended).
pub fn nest_key(id: u64) -> NestKey {
    let ext = |v: u64| ((v << 40) as i64) >> 40;
    ((id >> 48) as u16, ext((id >> 24) & 0xFF_FFFF), ext(id & 0xFF_FFFF))
}

/// The nests' tunables (OG-9e). Every number is a PLACEHOLDER until OG-10's measurement but K and
/// the renewal rule's three, which are EQUILIBRIUM.md §2.4.8's.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NestTunables {
    /// Worms a nest holds when full (§2.4.8: 2).
    pub k: u16,
    /// The renewal rule (§2.4.8: 12 days, capped at 96, half-life 96).
    pub base_days: f32,
    pub cap_days: f32,
    pub half_life_days: f32,
    /// A broken heart reforms after `min(cap, base x 2^P)` drilling days.
    pub reform_base_days: f32,
    pub reform_cap_days: f32,
    /// Blocks from the heart: a well this near wakes the nest, a district this near is its prey.
    pub reach: u32,
    /// A drilling day's chance of a raid, thousandths.
    pub raid_chance_milli: u32,
    /// A worm's strength at stage 1, in the units fights are observed in; it scales with the stage.
    pub worm_strength: f64,
    /// The chamber's floor this many blocks below the surface, drawn per nest.
    pub depth_min: u32,
    pub depth_max: u32,
    /// Ticks a sortie's worms fight before they go home.
    pub sortie_ticks: u32,
    /// Ticks between the nests' watches: a game day.
    pub day_ticks: u64,
}

impl Default for NestTunables {
    fn default() -> Self {
        NestTunables {
            k: 2,
            base_days: 12.0,
            cap_days: 96.0,
            half_life_days: 96.0,
            reform_base_days: 96.0,
            reform_cap_days: 384.0,
            reach: 128,
            raid_chance_milli: 250,
            worm_strength: 1.0,
            depth_min: 20,
            depth_max: 60,
            sortie_ticks: 1_200,
            day_ticks: TICKS_PER_DAY,
        }
    }
}

impl NestTunables {
    pub fn renewal(&self) -> RenewalRule {
        RenewalRule {
            base_days: self.base_days,
            cap_days: self.cap_days,
            half_life_days: self.half_life_days,
        }
    }

    pub fn reform(&self) -> RenewalRule {
        RenewalRule {
            base_days: self.reform_base_days,
            cap_days: self.reform_cap_days,
            half_life_days: self.half_life_days,
        }
    }
}

/// Where a nest is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum NestState {
    /// PLACE_NEST sent, no answer yet (and again after a heart reforms).
    Asked,
    /// The chamber and its heart stand.
    Placed,
    /// The heart is broken: nothing renews until it reforms.
    Broken,
    /// No room for the chamber: the cell is given up.
    Failed,
}

impl NestState {
    pub fn code(self) -> u8 {
        match self {
            NestState::Asked => 0,
            NestState::Placed => 1,
            NestState::Broken => 2,
            NestState::Failed => 3,
        }
    }

    pub fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            0 => NestState::Asked,
            1 => NestState::Placed,
            2 => NestState::Broken,
            3 => NestState::Failed,
            _ => return None,
        })
    }
}

/// One oil nest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nest {
    /// The oil chunk that founded it (canonical): the chamber is under it.
    pub chunk: (i32, i32),
    pub depth: u32,
    /// The heart, as NEST_FOUND reported it (canonical x and z).
    pub heart: Option<(i32, i32, i32)>,
    pub state: NestState,
    /// Worms alive or owed.
    pub worms: u32,
    /// The first brood was sent (its first waking).
    pub hatched: bool,
    pub pressure: Pressure,
    /// A worm's wait running: thousandths of a drilling day left.
    pub wait_milli: Option<u32>,
    /// A broken heart's wait: thousandths of a drilling day left.
    pub reform_milli: Option<u32>,
    /// The wells in reach at the last drilling day: mB left, the most they held, and whether one is
    /// bottomless (the stage, module docs).
    pub left_mb: i64,
    pub peak_mb: i64,
    pub bottomless: bool,
    /// Worms the nest renewed, worms players took, raids it made.
    pub renewed: u32,
    pub lost_player: u32,
    pub raids: u32,
}

impl Nest {
    fn asked(chunk: (i32, i32), depth: u32) -> Self {
        Nest {
            chunk,
            depth,
            heart: None,
            state: NestState::Asked,
            worms: 0,
            hatched: false,
            pressure: Pressure::default(),
            wait_milli: None,
            reform_milli: None,
            left_mb: 0,
            peak_mb: 0,
            bottomless: false,
            renewed: 0,
            lost_player: 0,
            raids: 0,
        }
    }

    /// The nest's place: its heart, else its chunk's centre.
    pub fn centre(&self) -> (i64, i64) {
        match self.heart {
            Some((x, _, z)) => (i64::from(x), i64::from(z)),
            None => (
                i64::from(self.chunk.0) * 16 + 8,
                i64::from(self.chunk.1) * 16 + 8,
            ),
        }
    }

    /// The stage (module docs): the wells' oil left against the most they held, in thirds.
    pub fn stage(&self) -> u32 {
        if self.bottomless || self.peak_mb <= 0 {
            return 1;
        }
        let left = self.left_mb.max(0) as i128 * 3;
        let peak = i128::from(self.peak_mb);
        if left > 2 * peak {
            1
        } else if left > peak {
            2
        } else {
            3
        }
    }
}

/// The nests' counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NestStats {
    pub founded: u64,
    pub placed: u64,
    pub failed: u64,
    /// First broods sent.
    pub broods: u64,
    /// Worms renewed after a loss.
    pub renewed: u64,
    pub lost_player: u64,
    pub lost_other: u64,
    pub hearts_broken: u64,
    pub reforms: u64,
    pub raids: u64,
    pub sorties: u64,
    pub wakes: u64,
    pub sleeps: u64,
}

impl NestStats {
    pub const LEN: usize = 13;

    pub fn array(&self) -> [u64; Self::LEN] {
        [
            self.founded,
            self.placed,
            self.failed,
            self.broods,
            self.renewed,
            self.lost_player,
            self.lost_other,
            self.hearts_broken,
            self.reforms,
            self.raids,
            self.sorties,
            self.wakes,
            self.sleeps,
        ]
    }

    pub fn of(v: [u64; Self::LEN]) -> Self {
        NestStats {
            founded: v[0],
            placed: v[1],
            failed: v[2],
            broods: v[3],
            renewed: v[4],
            lost_player: v[5],
            lost_other: v[6],
            hearts_broken: v[7],
            reforms: v[8],
            raids: v[9],
            sorties: v[10],
            wakes: v[11],
            sleeps: v[12],
        }
    }
}

/// Every nest and the watch's state. The rows are resident and saved in pages of their own (0x4419);
/// the rest is in the meta page.
#[derive(Debug, Default)]
pub struct Nests {
    pub(crate) rows: PagedMap<NestKey, Nest>,
    /// Nests awake at the last watch.
    pub(crate) awake: BTreeSet<NestKey>,
    /// The nests' clock: watches run.
    pub(crate) day: u32,
    /// The booked watch's plan (a watch of an older plan is stale); 0, none booked.
    pub(crate) plan: u32,
    /// Ticks between watches when not the tunable's (NEST_DAY, FLAG_TEST); 0, the tunable's.
    pub(crate) day_ticks: u64,
    pub stats: NestStats,
}

impl Nests {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&Nest> {
        self.rows.get(&nest_key(id))
    }

    pub fn is_awake(&self, id: u64) -> bool {
        self.awake.contains(&nest_key(id))
    }

    /// Every nest by wire id, in key order.
    pub fn ids(&self) -> Vec<u64> {
        self.rows.keys().map(|k| nest_id(*k)).collect()
    }

    /// The nests' clock.
    pub fn day(&self) -> u32 {
        self.day
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.rows.heap_bytes() + self.awake.len() * (std::mem::size_of::<NestKey>() * 3 / 2)
    }
}

/// What the wells in reach of one nest hold on a drilling day.
#[derive(Clone, Copy, Debug, Default)]
struct Drill {
    left: i64,
    bottomless: bool,
}

/// A nest's fields for the plugin's tests (rtsim-jni's nDragonsNest): state, stage, worms, K,
/// awake, pressure in thousandths, the wait in ticks (-1 none), renewed, players' losses, raids,
/// hatched, and the number of nests.
pub const NEST_INFO: usize = 12;

impl Dragons {
    fn nest_tun(&self) -> &NestTunables {
        &self.tun.oil.nest
    }

    /// Ticks between the nests' watches.
    pub fn nest_day_ticks(&self) -> u64 {
        if self.nests.day_ticks > 0 {
            self.nests.day_ticks
        } else {
            self.nest_tun().day_ticks.max(1)
        }
    }

    /// The test order NEST_DAY: watches every `ticks` from now (0 restores the tunable's).
    pub fn set_nest_day(&mut self, ticks: u64) {
        self.nests.day_ticks = ticks;
        if self.nests.plan != 0 {
            self.book_nest_watch(self.now);
        }
    }

    fn book_nest_watch(&mut self, at: Tick) {
        self.nests.plan = self.nests.plan.wrapping_add(1).max(1);
        let plan = self.nests.plan;
        let next = at + self.nest_day_ticks();
        self.schedule(next, Ev::NestWatch { plan });
    }

    /// An oil chunk new to the index (canonical chunk (cx, cz)): its cell's nest is founded if the
    /// cell has none and nests are on (`boundary::OIL_PLACE_NESTS`), and its chamber asked for.
    pub(crate) fn nest_seam(&mut self, frame: u16, cx: i64, cz: i64, at: Tick) {
        if self.oil_flags & boundary::OIL_PLACE_NESTS == 0 {
            return;
        }
        if self.found_nest(frame, cx, cz, at) {
            let k = (frame, cx.div_euclid(CELL_CHUNKS), cz.div_euclid(CELL_CHUNKS));
            self.ask_nest(k);
        }
    }

    /// Founds the nest of chunk (cx, cz)'s cell if it has none; whether it did.
    fn found_nest(&mut self, frame: u16, cx: i64, cz: i64, at: Tick) -> bool {
        let k = (frame, cx.div_euclid(CELL_CHUNKS), cz.div_euclid(CELL_CHUNKS));
        if self.nests.rows.contains_key(&k) {
            return false;
        }
        let (lo, hi) = {
            let t = self.nest_tun();
            (t.depth_min, t.depth_max.max(t.depth_min))
        };
        let mut rng = StreamKey::new(self.seed, SYS_NEST, nest_id(k)).slot(0);
        let depth = lo + rng.below(u64::from(hi - lo) + 1) as u32;
        let chunk = (cx as i32, cz as i32);
        self.nests.rows.insert(k, Nest::asked(chunk, depth));
        self.nests.stats.founded += 1;
        if self.nests.plan == 0 {
            self.book_nest_watch(at);
        }
        self.note(
            at,
            "nest",
            None,
            format!(
                "nest {:#x} founded under oil chunk ({cx}, {cz}), {depth} blocks down",
                nest_id(k)
            ),
        );
        true
    }

    fn ask_nest(&mut self, k: NestKey) {
        let Some(n) = self.nests.rows.get(&k) else {
            return;
        };
        self.outbox.push(Cmd::PlaceNest(boundary::PlaceNest {
            nest: nest_id(k),
            dimension: u32::from(k.0),
            cx: n.chunk.0,
            cz: n.chunk.1,
            depth: n.depth,
        }));
    }

    /// OIL_CONFIG: with nests on, a nest is founded under every known oil chunk's cell that has none
    /// (the switch was off when they came), and every nest not answered yet is asked for again.
    pub(crate) fn nests_config(&mut self, was: u32, at: Tick) {
        if self.oil_flags & boundary::OIL_PLACE_NESTS == 0 {
            return;
        }
        if was & boundary::OIL_PLACE_NESTS == 0 {
            let chunks: Vec<(u16, i64, i64)> = self
                .oil
                .regions
                .iter()
                .flat_map(|(&(frame, rx, rz), v)| {
                    v.iter().map(move |c| {
                        let (lx, lz) = (
                            i64::from(c.idx) / crate::oil::REGION_CHUNKS,
                            i64::from(c.idx) % crate::oil::REGION_CHUNKS,
                        );
                        (
                            frame,
                            rx * crate::oil::REGION_CHUNKS + lx,
                            rz * crate::oil::REGION_CHUNKS + lz,
                        )
                    })
                })
                .collect();
            for (frame, cx, cz) in chunks {
                self.found_nest(frame, cx, cz, at);
            }
        }
        let asked: Vec<NestKey> = self
            .nests
            .rows
            .iter()
            .filter(|(_, n)| n.state == NestState::Asked)
            .map(|(k, _)| *k)
            .collect();
        for k in asked {
            self.ask_nest(k);
        }
    }

    /// The nests' watch (module docs): which nests the wells wake, and a drilling day for each.
    pub(crate) fn nest_watch(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        plan: u32,
        at: Tick,
    ) {
        if plan != self.nests.plan {
            self.stats.stale += 1;
            return;
        }
        let next = at + self.nest_day_ticks();
        self.schedule(next, Ev::NestWatch { plan });
        self.nests.day = self.nests.day.saturating_add(1);
        if self.nests.rows.is_empty() {
            return;
        }
        let drilled = self.drilled(infra);
        let gone: Vec<NestKey> = self
            .nests
            .awake
            .iter()
            .filter(|k| !drilled.contains_key(k))
            .copied()
            .collect();
        for k in gone {
            self.nests.awake.remove(&k);
            self.nests.stats.sleeps += 1;
            self.note(
                at,
                "nest",
                None,
                format!("nest {:#x} sleeps: no well drills in reach", nest_id(k)),
            );
        }
        for (k, d) in drilled {
            if self.nests.awake.insert(k) {
                self.nests.stats.wakes += 1;
                self.note(
                    at,
                    "nest",
                    None,
                    format!("nest {:#x} wakes: a well drills in reach", nest_id(k)),
                );
            }
            self.drilling_day(w, infra, k, d, at);
        }
    }

    /// The nests the wells wake, with what the wells in reach hold.
    fn drilled(&self, infra: &mut dyn Infrastructure) -> BTreeMap<NestKey, Drill> {
        let reach = i64::from(self.nest_tun().reach);
        let mut out: BTreeMap<NestKey, Drill> = BTreeMap::new();
        for well in infra.wells() {
            let t = self.topo(well.frame);
            let mut cells: Vec<(i64, i64)> = Vec::new();
            for part in t.rects_near(well.x, well.z, reach) {
                let (x0, z0, x1, z1) = part.grid_cells(CELL_BLOCKS);
                for cx in x0..=x1 {
                    for cz in z0..=z1 {
                        cells.push((cx, cz));
                    }
                }
            }
            cells.sort_unstable();
            cells.dedup();
            for (cx, cz) in cells {
                let k = (well.frame, cx, cz);
                let Some(n) = self.nests.rows.get(&k) else {
                    continue;
                };
                if !matches!(n.state, NestState::Placed | NestState::Broken) {
                    continue;
                }
                if t.dist_sq(n.centre(), (well.x, well.z)) > (reach * reach) as u64 {
                    continue;
                }
                let e = out.entry(k).or_default();
                e.left = e.left.saturating_add(well.remaining.max(0));
                e.bottomless |= well.infinite;
            }
        }
        out
    }

    /// The wait for a worm, started now: thousandths of a drilling day.
    fn worm_wait(&self, n: &Nest) -> u32 {
        let days = self.nest_tun().renewal().delay_days(n.pressure, self.nests.day);
        ((days * 1_000.0).ceil() as u32).max(1)
    }

    fn renew(&mut self, k: NestKey, count: u32, stage: u32) {
        self.outbox.push(Cmd::NestRenew(boundary::NestRenew {
            nest: nest_id(k),
            dimension: u32::from(k.0),
            count,
            stage,
        }));
    }

    /// One drilling day of an awake nest: its first brood, its waits, a raid.
    fn drilling_day(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        k: NestKey,
        d: Drill,
        at: Tick,
    ) {
        let kk = u32::from(self.nest_tun().k.max(1));
        let Some(mut n) = self.nests.rows.get(&k).copied() else {
            return;
        };
        n.left_mb = d.left;
        n.peak_mb = n.peak_mb.max(d.left);
        n.bottomless = d.bottomless;
        let stage = n.stage();
        let id = nest_id(k);
        let mut ask = false;
        match n.state {
            NestState::Asked | NestState::Failed => {}
            NestState::Broken => {
                let left = n.reform_milli.unwrap_or(0).saturating_sub(1_000);
                if left == 0 {
                    n.reform_milli = None;
                    n.state = NestState::Asked;
                    ask = true;
                    self.nests.stats.reforms += 1;
                    self.note(at, "nest", None, format!("nest {id:#x}'s heart reforms"));
                } else {
                    n.reform_milli = Some(left);
                }
            }
            NestState::Placed if !n.hatched => {
                let owed = kk.saturating_sub(n.worms);
                n.hatched = true;
                n.worms = kk;
                n.wait_milli = None;
                self.nests.stats.broods += 1;
                if owed > 0 {
                    self.renew(k, owed, stage);
                }
                self.note(
                    at,
                    "nest",
                    None,
                    format!("nest {id:#x} hatches {owed} worms (stage {stage})"),
                );
            }
            NestState::Placed if n.worms < kk => {
                let left = n
                    .wait_milli
                    .unwrap_or_else(|| self.worm_wait(&n))
                    .saturating_sub(1_000);
                if left == 0 {
                    n.worms += 1;
                    n.renewed += 1;
                    self.nests.stats.renewed += 1;
                    self.renew(k, 1, stage);
                    n.wait_milli = (n.worms < kk).then(|| self.worm_wait(&n));
                    self.note(
                        at,
                        "nest",
                        None,
                        format!("nest {id:#x} renews a worm: {} of {kk}", n.worms),
                    );
                } else {
                    n.wait_milli = Some(left);
                }
            }
            NestState::Placed => {}
        }
        self.nests.rows.insert(k, n);
        if ask {
            self.ask_nest(k);
        }
        let chance = self.nest_tun().raid_chance_milli;
        let raids = matches!(n.state, NestState::Placed | NestState::Broken) && n.worms > 0;
        let mut rng = StreamKey::new(self.seed, SYS_NEST, id).slot(u64::from(self.nests.day));
        if raids && rng.below(1_000) < u64::from(chance) {
            self.nest_raid(w, infra, k, &mut rng, at);
        }
    }

    /// The worms go at a district in reach of the heart (module docs).
    fn nest_raid(
        &mut self,
        _w: &mut World,
        infra: &mut dyn Infrastructure,
        k: NestKey,
        rng: &mut rtsim_kernel::rng::SlotRng,
        at: Tick,
    ) {
        let Some(n) = self.nests.rows.get(&k).copied() else {
            return;
        };
        let (reach, strength, sortie) = {
            let t = self.nest_tun();
            (i64::from(t.reach), t.worm_strength, t.sortie_ticks)
        };
        let (x, z) = n.centre();
        let sites = infra.districts_reaching(k.0, x, z, reach);
        if sites.is_empty() {
            return;
        }
        let site = sites[rng.below(sites.len() as u64) as usize];
        let id = nest_id(k);
        let stage = n.stage();
        let threat = Threat {
            id: fnv1a64(
                &[
                    id.to_le_bytes(),
                    at.0.to_le_bytes(),
                    site.id.0.to_le_bytes(),
                ]
                .concat(),
            ),
            source: ThreatSource::Nest,
            class: ThreatClass::Burrowing,
            count: n.worms,
            strength: f64::from(n.worms) * f64::from(stage) * strength,
            target: ThreatTarget::District(site.id),
            loads: None,
            at,
        };
        let out = if site.watched {
            infra.raid_in_sight(&threat, &[], 0)
        } else {
            infra.raid(&threat)
        };
        let Some(out) = out else {
            return;
        };
        if let Some(why) = out.skipped {
            self.note(
                at,
                "nest",
                None,
                format!("nest {id:#x}'s raid on district {} skipped: {why}", site.id.0),
            );
            return;
        }
        self.nests.stats.raids += 1;
        if let Some(r) = self.nests.rows.get_mut(&k) {
            r.raids += 1;
        }
        self.note(
            at,
            "nest",
            None,
            format!(
                "nest {id:#x}'s {} worms (stage {stage}) raid district {}: {} blocks broken",
                n.worms, site.id.0, out.destroyed_blocks
            ),
        );
        if site.watched {
            self.nests.stats.sorties += 1;
            self.outbox.push(Cmd::NestSortie(boundary::NestSortie {
                nest: id,
                dimension: u32::from(k.0),
                count: n.worms,
                x: site.pos.x,
                z: site.pos.z,
                timeout_ticks: sortie,
            }));
        }
    }

    /// NEST_FOUND: the chamber stands with its heart, or the cell is given up.
    pub(crate) fn nest_found(&mut self, f: &boundary::NestFound, at: Tick) {
        let k = nest_key(f.nest);
        let kk = u32::from(self.nest_tun().k.max(1));
        let (hx, hz) = self.topo(k.0).canonical(i64::from(f.heart.0), i64::from(f.heart.2));
        let wait = self.nests.rows.get(&k).map(|n| self.worm_wait(n));
        let Some(n) = self.nests.rows.get_mut(&k) else {
            self.stats.stale += 1;
            return;
        };
        if f.placed {
            n.heart = Some((hx as i32, f.heart.1, hz as i32));
            if n.state != NestState::Placed {
                self.nests.stats.placed += 1;
            }
            n.state = NestState::Placed;
            if n.hatched && n.worms < kk && n.wait_milli.is_none() {
                n.wait_milli = wait;
            }
            self.note(
                at,
                "nest",
                None,
                format!(
                    "nest {:#x} placed: heart at ({hx}, {}, {hz})",
                    f.nest, f.heart.1
                ),
            );
        } else {
            if n.state != NestState::Placed {
                n.state = NestState::Failed;
            }
            self.nests.stats.failed += 1;
            self.nests.awake.remove(&k);
            self.note(
                at,
                "nest",
                None,
                format!("nest {:#x} has no room: its cell is given up", f.nest),
            );
        }
    }

    /// `count` worms of nest `k` lost (`cause`); a player's loss presses the nest.
    fn nest_lost(&mut self, k: NestKey, count: u32, cause: LossCause, at: Tick) {
        let tun = self.nest_tun().clone();
        let kk = u32::from(tun.k.max(1));
        let day = self.nests.day;
        let Some(mut n) = self.nests.rows.get(&k).copied() else {
            self.stats.stale += 1;
            return;
        };
        let lost = count.min(n.worms);
        n.worms -= lost;
        if cause == LossCause::Player {
            n.pressure
                .bump(day, count as f32 / kk as f32, tun.half_life_days);
            n.lost_player = n.lost_player.saturating_add(count);
            self.nests.stats.lost_player += u64::from(count);
        } else {
            self.nests.stats.lost_other += u64::from(count);
        }
        if n.hatched && n.worms < kk && n.wait_milli.is_none() && n.state == NestState::Placed {
            n.wait_milli = Some(self.worm_wait(&n));
        }
        self.nests.rows.insert(k, n);
        self.note(
            at,
            "nest",
            None,
            format!(
                "nest {:#x} loses {count} worms ({cause:?}): {} left, pressure {:.2}",
                nest_id(k),
                n.worms,
                n.pressure.now(day, tun.half_life_days)
            ),
        );
    }

    /// NEST_LOSS.
    pub(crate) fn nest_loss(&mut self, l: &boundary::NestLoss, at: Tick) {
        self.nest_lost(nest_key(l.nest), l.count, LossCause::from_code(l.cause), at);
    }

    /// NEST_CENSUS: the plugin's count (alive or owed) is the nest's; a shortfall is a loss of
    /// cause Other. A heart found gone is broken.
    pub(crate) fn nest_census(&mut self, c: &boundary::NestCensus, at: Tick) {
        let k = nest_key(c.nest);
        let kk = u32::from(self.nest_tun().k.max(1));
        let Some(n) = self.nests.rows.get(&k).copied() else {
            self.stats.stale += 1;
            return;
        };
        let seen = c.worms.min(kk);
        if seen < n.worms {
            self.nest_lost(k, n.worms - seen, LossCause::Other, at);
        } else if seen > n.worms
            && let Some(r) = self.nests.rows.get_mut(&k)
        {
            r.worms = seen;
            r.hatched = true;
            if seen >= kk {
                r.wait_milli = None;
            }
        }
        if c.heart_missing && n.state == NestState::Placed {
            self.heart_broken(k, at);
        }
    }

    /// NEST_HEART.
    pub(crate) fn nest_heart(&mut self, h: &boundary::NestHeart, at: Tick) {
        let k = nest_key(h.nest);
        if self
            .nests
            .rows
            .get(&k)
            .is_some_and(|n| n.state == NestState::Placed)
        {
            self.heart_broken(k, at);
        }
    }

    fn heart_broken(&mut self, k: NestKey, at: Tick) {
        let tun = self.nest_tun().clone();
        let day = self.nests.day;
        let Some(n) = self.nests.rows.get_mut(&k) else {
            return;
        };
        n.pressure.bump(day, 1.0, tun.half_life_days);
        let days = tun.reform().delay_days(n.pressure, day);
        n.state = NestState::Broken;
        n.wait_milli = None;
        n.reform_milli = Some(((days * 1_000.0).ceil() as u32).max(1));
        self.nests.stats.hearts_broken += 1;
        self.note(
            at,
            "nest",
            None,
            format!(
                "nest {:#x}'s heart is broken: it reforms after {days:.0} drilling days",
                nest_id(k)
            ),
        );
    }

    /// The nest's fields for the plugin's tests ([`NEST_INFO`]).
    pub fn nest_info(&self, id: u64) -> Option<[i64; NEST_INFO]> {
        let k = nest_key(id);
        let n = self.nests.rows.get(&k)?;
        let tun = self.nest_tun();
        let p = n.pressure.now(self.nests.day, tun.half_life_days);
        let ticks = self.nest_day_ticks() as i64;
        let wait = n
            .wait_milli
            .map_or(-1, |m| i64::from(m).saturating_mul(ticks) / 1_000);
        Some([
            i64::from(n.state.code()),
            i64::from(n.stage()),
            i64::from(n.worms),
            i64::from(tun.k),
            i64::from(self.nests.awake.contains(&k)),
            (f64::from(p) * 1_000.0).round() as i64,
            wait,
            i64::from(n.renewed),
            i64::from(n.lost_player),
            i64::from(n.raids),
            i64::from(n.hatched),
            self.nests.rows.len() as i64,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_with_negative_cells() {
        for k in [
            (0u16, 0i64, 0i64),
            (3, -1, 5),
            (65_535, -8_388_608, 8_388_607),
            (1, 234_375, -234_375),
        ] {
            assert_eq!(nest_key(nest_id(k)), k, "{k:?}");
        }
    }

    #[test]
    fn stage_rises_in_thirds() {
        let mut n = Nest::asked((0, 0), 30);
        assert_eq!(n.stage(), 1);
        n.peak_mb = 900;
        for (left, stage) in [(900, 1), (601, 1), (600, 2), (301, 2), (300, 3), (0, 3)] {
            n.left_mb = left;
            assert_eq!(n.stage(), stage, "left {left}");
        }
        n.bottomless = true;
        assert_eq!(n.stage(), 1);
    }
}
