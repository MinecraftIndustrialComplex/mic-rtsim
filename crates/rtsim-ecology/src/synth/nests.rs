// SPDX-License-Identifier: AGPL-3.0-or-later
//! The oil nests' harness (OILGATE.md §4.6, EQUILIBRIUM.md §2.4.8, S10), lifted out of
//! `tests/nests.rs` so that the kill-all table (`rtsim-cli sim killall`, slice S11 EQC427) can call
//! it: a dragon engine with nests on, a fake Ice and Fire plugin answering them, and test wells
//! that drill while [`Pumps::drilling`]. `tests/nests.rs` runs its tests on the same code.
//!
//! **Scaffolding**, as the rest of [`crate::synth`].

use rtsim_industry::ids::{DistrictId, Good, GoodVec};
use rtsim_industry::threat::Threat;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_world::World;

use crate::boundary::{self, Cmd, Obs, OilChunkRec, OilChunks};
use crate::engine::Dragons;
use crate::infra::{DistrictSite, Infrastructure, RaidOutcome, Well};
use crate::synth::FakeIaf;
use crate::tunables::DragonTunables;

/// The player the fake plugin's kills name.
pub const PLAYER: (u64, u64) = (5, 6);
const DAY: u64 = TICKS_PER_DAY;

/// Wells and districts as a test infrastructure: the wells drill while `drilling`; a raid is
/// recorded and breaks one block.
#[derive(Clone, Default)]
pub struct Pumps {
    pub wells: Vec<Well>,
    pub drilling: bool,
    pub sites: Vec<DistrictSite>,
    pub raids: Vec<Threat>,
}

impl Pumps {
    /// A well of `district` in chunk (`cx`, `cz`) with `remaining` left; drilling from now.
    pub fn well(&mut self, district: u64, cx: i64, cz: i64, remaining: i64) {
        self.wells.push(Well {
            district: DistrictId(district),
            frame: 0,
            x: cx * 16 + 8,
            z: cz * 16 + 8,
            remaining,
            infinite: false,
        });
        self.drilling = true;
    }
}

impl Infrastructure for Pumps {
    fn districts_near(&mut self, frame: u16, x: i64, z: i64, r: i64) -> Vec<DistrictSite> {
        self.sites
            .iter()
            .copied()
            .filter(|s| {
                let (dx, dz) = (i64::from(s.pos.x) - x, i64::from(s.pos.z) - z);
                s.pos.frame == frame && dx * dx + dz * dz <= r * r
            })
            .collect()
    }

    fn raid(&mut self, t: &Threat) -> Option<RaidOutcome> {
        self.raids.push(*t);
        Some(RaidOutcome {
            survived: 1.0,
            floor: None,
            untested: true,
            stolen: GoodVec::new(),
            destroyed_blocks: 1,
            machine_lost: false,
            skipped: None,
        })
    }

    fn raid_in_sight(&mut self, t: &Threat, _: &[String], _: i64) -> Option<RaidOutcome> {
        self.raid(t)
    }

    fn good_name(&self, _: Good) -> Option<String> {
        None
    }

    fn wells(&mut self) -> Vec<Well> {
        if self.drilling {
            self.wells.clone()
        } else {
            Vec::new()
        }
    }
}

/// A dragon engine with nests, its world, the fake plugin and the wells, at time `t`; `cmds` is
/// every command the engine sent.
pub struct Run {
    pub w: World,
    pub d: Dragons,
    pub iaf: FakeIaf,
    pub pumps: Pumps,
    pub t: Tick,
    pub cmds: Vec<Cmd>,
}

/// The defaults with no raids: the renewal tests count worms, not fights.
pub fn tunables() -> DragonTunables {
    let mut t = DragonTunables::default();
    t.oil.nest.raid_chance_milli = 0;
    t
}

/// A world with nests on and the oil chunks `chunks` reported at the start.
pub fn start(seed: u64, tun: DragonTunables, chunks: &[(i32, i32)]) -> Run {
    let (w, _) = super::world(seed);
    let d = Dragons::new(seed, tun, Tick::ZERO);
    let mut r = Run {
        w,
        d,
        iaf: FakeIaf::new(PLAYER),
        pumps: Pumps::default(),
        t: Tick::ZERO,
        cmds: Vec::new(),
    };
    if !chunks.is_empty() {
        r.ingest(oil(chunks));
    }
    r
}

/// OIL_CHUNKS for `chunks` (Create Diesel Generators' source, dimension 0).
pub fn oil(chunks: &[(i32, i32)]) -> Obs {
    Obs::OilChunks(OilChunks {
        dimension: 0,
        source: boundary::OIL_SOURCE_CDG,
        chunks: chunks
            .iter()
            .map(|&(cx, cz)| OilChunkRec {
                cx,
                cz,
                amount: 1,
                flags: 0,
            })
            .collect(),
    })
}

impl Run {
    pub fn ingest(&mut self, o: Obs) {
        let at = self.t.max(self.d.now());
        self.d.ingest(&mut self.w, &mut self.pumps, &o, at);
        self.pump();
    }

    /// The engine's commands to the plugin and the plugin's answers due now, until both are quiet.
    pub fn pump(&mut self) {
        loop {
            let cmds = self.d.take_commands();
            let quiet = cmds.is_empty();
            self.iaf.apply(cmds.clone(), self.t);
            self.cmds.extend(cmds);
            let due = self.iaf.due(self.t);
            if quiet && due.is_empty() {
                break;
            }
            for (at, o) in due {
                let at = at.max(self.d.now());
                self.d.ingest(&mut self.w, &mut self.pumps, &o, at);
            }
        }
    }

    /// Runs `days` days in steps of a quarter day.
    pub fn days(&mut self, days: u64) {
        let end = self.t + days * DAY;
        while self.t < end {
            self.t += DAY / 4;
            self.d.run_until(&mut self.w, &mut self.pumps, self.t);
            self.pump();
        }
    }

    /// Runs until `pred` holds, a day at a time, for at most `days`; the days it took.
    pub fn until(&mut self, days: u64, mut pred: impl FnMut(&Run) -> bool) -> Option<u64> {
        for k in 0..=days {
            if pred(self) {
                return Some(k);
            }
            self.days(1);
        }
        None
    }

    pub fn worms(&self, id: u64) -> u32 {
        self.d.nests.get(id).expect("a nest").worms
    }

    /// Every NEST_RENEW the engine sent.
    pub fn renews(&self) -> Vec<boundary::NestRenew> {
        self.cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::NestRenew(n) => Some(*n),
                _ => None,
            })
            .collect()
    }

    /// The player kills `count` of nest `id`'s worms now, through the plugin.
    pub fn kill(&mut self, id: u64, count: u32) {
        let at = self.t;
        self.iaf.kill_worms(id, count, at);
        self.pump();
    }
}

/// What S10's `kill_all_nests` measures on one seed (EQC427): every worm of every nest killed
/// twice, drilled and then not.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NestRefill {
    pub seed: u64,
    /// Nests founded, and their K.
    pub nests: usize,
    pub k: u32,
    /// Every nest full two days after its well came.
    pub hatched: bool,
    /// Worms killed in the first kill-all, and whether every nest read 0 after it.
    pub killed: u32,
    pub cleared: bool,
    /// Drilled: days from the kill until half the worms and then all of them were back.
    pub half: Option<u64>,
    pub whole: Option<u64>,
    /// Every renewal after the first hatching came from a wait, one worm at a time.
    pub one_at_a_time: bool,
    /// Per nest after the first refill: worms booked lost to a player, and worms renewed.
    pub lost_player: Vec<u32>,
    pub renewed: Vec<u32>,
    /// Not drilled, 400 days after the second kill-all: renewals sent, and the worms left.
    pub undrilled_renewals: usize,
    pub undrilled_worms: u32,
    /// Drilled again: days until every nest was full (at most 150 looked at), and whether the
    /// plugin's counts then agree.
    pub redrilled: Option<u64>,
    pub plugin_agrees: bool,
}

/// S10's kill-all on seed `seed` with nests under `cells` (chunk coordinates, one nest each),
/// a well beside each: hatched, every worm killed; back within 100 days looked at; killed again
/// and the wells stopped for 400 days; then drilled again for up to 150.
pub fn kill_all(seed: u64, cells: &[(i32, i32)]) -> NestRefill {
    let mut r = start(seed, tunables(), cells);
    r.days(1);
    let ids: Vec<u64> = r.d.nests.ids();
    let mut out = NestRefill {
        seed,
        nests: ids.len(),
        ..NestRefill::default()
    };
    for (i, &(cx, cz)) in cells.iter().enumerate() {
        r.pumps
            .well(10 + i as u64, i64::from(cx) + 1, i64::from(cz), 50_000);
    }
    r.days(2);
    let k = u32::from(r.d.tun.oil.nest.k);
    out.k = k;
    out.hatched = ids.iter().all(|&id| r.worms(id) == k);
    let kill_all = |r: &mut Run| {
        let mut n = 0;
        for &id in &ids {
            let w = r.iaf.nests[&id];
            r.kill(id, w);
            n += w;
        }
        (n, ids.iter().all(|&id| r.worms(id) == 0))
    };
    // Drilled: how long until half and then all are back.
    let (killed, cleared) = kill_all(&mut r);
    out.killed = killed;
    out.cleared = cleared;
    let total = k * ids.len() as u32;
    // `until` looks once a day, from day 0: its k-th look is day k.
    let (mut half, mut day) = (None, 0);
    out.whole = r.until(100, |r| {
        let back: u32 = ids.iter().map(|&id| r.worms(id)).sum();
        if half.is_none() && 2 * back >= total {
            half = Some(day);
        }
        day += 1;
        back == total
    });
    out.half = half;
    out.one_at_a_time = r.renews().iter().skip(ids.len()).all(|n| n.count == 1);
    for &id in &ids {
        let n = r.d.nests.get(id).expect("a nest");
        out.lost_player.push(n.lost_player);
        out.renewed.push(n.renewed);
    }
    // Not drilled: nothing comes back.
    kill_all(&mut r);
    r.pumps.drilling = false;
    let renewed = r.renews().len();
    r.days(400);
    out.undrilled_renewals = r.renews().len() - renewed;
    out.undrilled_worms = ids.iter().map(|&id| r.worms(id)).sum();
    // Drilled again: they regrow, and the plugin's counts agree.
    r.pumps.drilling = true;
    out.redrilled = r.until(150, |r| ids.iter().all(|&id| r.worms(id) == k));
    out.plugin_agrees = ids.iter().all(|&id| r.iaf.nests[&id] == k);
    out
}
