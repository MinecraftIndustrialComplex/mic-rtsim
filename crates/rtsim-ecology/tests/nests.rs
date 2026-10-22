// SPDX-License-Identifier: AGPL-3.0-or-later
//! Oil nests, the oil gate's underground half (OILGATE.md §4.6, OG-9e; EQUILIBRIUM.md §2.4.8, S10).
//!
//! - **Founding**: an oil chunk new to the index founds its cell's nest, once, and asks for its
//!   chamber; a nest with no room gives its cell up.
//! - **Asleep until drilled** (S10 `asleep_nest_holds`): no well in reach, nothing renews and the
//!   waits stand still.
//! - **Kill all** (S10 `kill_all_nests`): a cleared nest under a working pumpjack is back within a
//!   game year (72 days in the model); stop drilling and it stays as it is.
//! - **The heart, the census, the stage and the raids**: a broken heart stops renewal until it
//!   reforms; the census reconciles the count; the stage rises as the wells drain; raids are
//!   Burrowing threats on the districts in reach, and a watched district draws a sortie.
//! - **Saves**: a run with nests restores exactly from its resident save and from its pages.
//! - **The industry link**: a pumpjack's deposit sample of the latest sweep is a well; once a sweep
//!   comes without it, it is gone.

use std::sync::{Arc, RwLock};

use rtsim_ecology::boundary::{self, Cmd, Obs, OilChunkRec, OilChunks};
use rtsim_ecology::engine::Dragons;
use rtsim_ecology::infra::{DistrictIndex, DistrictSite, IndustryLink, Infrastructure, RaidOutcome, Well};
use rtsim_ecology::nest::{NestState, nest_id};
use rtsim_ecology::synth::{self, FakeIaf};
use rtsim_ecology::tunables::DragonTunables;
use rtsim_industry::ids::{DistrictId, Good, GoodVec};
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::renewal::LossCause;
use rtsim_settle::Pos;
use rtsim_world::World;

const PLAYER: (u64, u64) = (5, 6);
const DAY: u64 = TICKS_PER_DAY;

/// Wells and districts as a test infrastructure: the wells drill while `drilling`; a raid is
/// recorded and breaks one block.
#[derive(Clone, Default)]
struct Pumps {
    wells: Vec<Well>,
    drilling: bool,
    sites: Vec<DistrictSite>,
    raids: Vec<Threat>,
}

impl Pumps {
    fn well(&mut self, district: u64, cx: i64, cz: i64, remaining: i64) {
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
        self.raids.push(t.clone());
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

struct Run {
    w: World,
    d: Dragons,
    iaf: FakeIaf,
    pumps: Pumps,
    t: Tick,
    cmds: Vec<Cmd>,
}

fn tun() -> DragonTunables {
    let mut t = DragonTunables::default();
    // No raids unless a test asks: the renewal tests count worms, not fights.
    t.oil.nest.raid_chance_milli = 0;
    t
}

/// A world with nests on and the oil chunks `chunks` reported at the start.
fn start(seed: u64, tun: DragonTunables, chunks: &[(i32, i32)]) -> Run {
    let (w, _) = synth::world(seed);
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

fn oil(chunks: &[(i32, i32)]) -> Obs {
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
    fn ingest(&mut self, o: Obs) {
        let at = self.t.max(self.d.now());
        self.d.ingest(&mut self.w, &mut self.pumps, &o, at);
        self.pump();
    }

    /// The engine's commands to the plugin and the plugin's answers due now, until both are quiet.
    fn pump(&mut self) {
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
    fn days(&mut self, days: u64) {
        let end = self.t + days * DAY;
        while self.t < end {
            self.t += DAY / 4;
            self.d.run_until(&mut self.w, &mut self.pumps, self.t);
            self.pump();
        }
    }

    /// Runs until `pred` holds, a day at a time, for at most `days`; the days it took.
    fn until(&mut self, days: u64, mut pred: impl FnMut(&Run) -> bool) -> Option<u64> {
        for k in 0..=days {
            if pred(self) {
                return Some(k);
            }
            self.days(1);
        }
        None
    }

    fn worms(&self, id: u64) -> u32 {
        self.d.nests.get(id).expect("a nest").worms
    }

    fn renews(&self) -> Vec<boundary::NestRenew> {
        self.cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::NestRenew(n) => Some(*n),
                _ => None,
            })
            .collect()
    }

    fn kill(&mut self, id: u64, count: u32) {
        let at = self.t;
        self.iaf.kill_worms(id, count, at);
        self.pump();
    }
}

fn placed(cmds: &[Cmd]) -> Vec<boundary::PlaceNest> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::PlaceNest(p) => Some(*p),
            _ => None,
        })
        .collect()
}

/// One nest under chunk (100, 100), placed, with a well 32 blocks from its heart.
fn one_nest(seed: u64, tun: DragonTunables) -> (Run, u64) {
    let mut r = start(seed, tun, &[(100, 100), (101, 100), (100, 101)]);
    r.days(1);
    let id = nest_id((0, 12, 12));
    assert_eq!(r.d.nests.get(id).unwrap().state, NestState::Placed);
    r.pumps.well(9, 102, 100, 90_000);
    (r, id)
}

// ---- founding -----------------------------------------------------------------------------------

#[test]
fn an_oil_chunk_founds_its_cells_nest_once_and_asks_for_its_chamber() {
    let mut r = start(1, tun(), &[(100, 100), (101, 100), (103, 102)]);
    // Three chunks of one 8 by 8 cell: one nest, asked for under the first.
    let asked = placed(&r.cmds);
    assert_eq!(asked.len(), 1, "{asked:?}");
    let p = asked[0];
    assert_eq!((p.cx, p.cz, p.dimension), (100, 100, 0));
    assert_eq!(p.nest, nest_id((0, 12, 12)));
    let tn = &r.d.tun.oil.nest;
    assert!((tn.depth_min..=tn.depth_max).contains(&p.depth));
    // A chunk in the next cell founds another; a known chunk founds nothing.
    r.ingest(oil(&[(104, 100), (100, 100), (-1, -9)]));
    let asked = placed(&r.cmds);
    assert_eq!(asked.len(), 3);
    assert_eq!(asked[1].nest, nest_id((0, 13, 12)));
    assert_eq!(asked[2].nest, nest_id((0, -1, -2)));
    assert_eq!((asked[2].cx, asked[2].cz), (-1, -9));
    r.days(1);
    assert_eq!(r.d.nests.stats.placed, 3);
    let n = r.d.nests.get(asked[2].nest).unwrap();
    assert_eq!(n.heart.map(|h| (h.0, h.2)), Some((-8, -136)));
    // Asleep: nothing hatched.
    assert!(r.renews().is_empty());
}

#[test]
fn nests_off_found_nothing_until_the_switch_comes_on() {
    let mut r = start(2, tun(), &[]);
    let off = boundary::OIL_FIELD_FLIGHTS | boundary::OIL_PLACE_LAIRS;
    r.ingest(Obs::OilConfig(boundary::OilConfig { flags: off }));
    r.ingest(oil(&[(100, 100), (120, 100)]));
    assert!(placed(&r.cmds).is_empty());
    assert!(r.d.nests.is_empty());
    // On: a nest under every known oil chunk's cell, and each asked for.
    r.ingest(Obs::OilConfig(boundary::OilConfig {
        flags: off | boundary::OIL_PLACE_NESTS,
    }));
    assert_eq!(r.d.nests.len(), 2);
    assert_eq!(placed(&r.cmds).len(), 2);
    // Another OIL_CONFIG asks again only for nests not answered yet.
    r.days(1);
    let before = placed(&r.cmds).len();
    r.ingest(Obs::OilConfig(boundary::OilConfig {
        flags: off | boundary::OIL_PLACE_NESTS,
    }));
    assert_eq!(placed(&r.cmds).len(), before);
}

#[test]
fn a_nest_with_no_room_gives_its_cell_up() {
    let mut r = start(3, tun(), &[]);
    r.iaf.nest_no_room = true;
    r.ingest(oil(&[(100, 100)]));
    r.days(1);
    let id = nest_id((0, 12, 12));
    assert_eq!(r.d.nests.get(id).unwrap().state, NestState::Failed);
    r.pumps.well(9, 100, 100, 1_000);
    r.iaf.nest_no_room = false;
    r.ingest(oil(&[(101, 101)]));
    r.ingest(Obs::OilConfig(boundary::OilConfig {
        flags: r.d.tun.oil.flags,
    }));
    r.days(20);
    assert_eq!(placed(&r.cmds).len(), 1, "never asked again");
    assert!(!r.d.nests.is_awake(id));
    assert!(r.renews().is_empty());
}

// ---- EQUILIBRIUM.md S10 ---------------------------------------------------------------------------

/// S10 `asleep_nest_holds`: no well in reach, a placed nest never hatches; a hatched one that
/// lost a worm keeps its count and its wait while it sleeps, and the wait runs on when drilling
/// comes back.
#[test]
fn asleep_nest_holds() {
    let (mut r, id) = one_nest(4, tun());
    r.pumps.drilling = false;
    r.days(200);
    assert!(r.renews().is_empty(), "never drilled, never hatched");
    assert!(!r.d.nests.is_awake(id));
    assert_eq!(r.worms(id), 0);
    // Drilled: it wakes and hatches K at once.
    r.pumps.drilling = true;
    r.days(2);
    let k = u32::from(r.d.tun.oil.nest.k);
    assert!(r.d.nests.is_awake(id));
    assert_eq!(r.renews().iter().map(|n| n.count).sum::<u32>(), k);
    assert_eq!(r.worms(id), k);
    assert_eq!(r.iaf.nests[&id], k);
    // A worm killed, then 10 drilling days of its wait, then 300 days asleep.
    r.kill(id, 1);
    let full = r.d.nests.get(id).unwrap().wait_milli.expect("a wait runs");
    r.days(10);
    let left = r.d.nests.get(id).unwrap().wait_milli.unwrap();
    assert!(full - left >= 9_000 && full - left <= 11_000, "{full} {left}");
    r.pumps.drilling = false;
    r.days(2);
    let asleep = r.d.nests.get(id).unwrap().wait_milli.unwrap();
    let renewed = r.renews().len();
    r.days(300);
    assert!(!r.d.nests.is_awake(id));
    assert_eq!(r.renews().len(), renewed, "an asleep nest renews nothing");
    assert_eq!(r.worms(id), k - 1, "an asleep nest holds");
    assert_eq!(r.d.nests.get(id).unwrap().wait_milli, Some(asleep), "its wait stands still");
    // Drilling again: the worm comes back when the rest of its wait has run.
    r.pumps.drilling = true;
    let took = r.until(200, |r| r.worms(id) == k).expect("it comes back");
    let rest = u64::from(asleep).div_ceil(1_000);
    assert!(took.abs_diff(rest) <= 1, "back after {took} days, {rest} left");
    assert_eq!(r.iaf.nests[&id], k);
}

/// S10 `kill_all_nests`: every worm of every nest killed. Drilled, each nest is full again within
/// 72 days (§2.4.8: "back within a game year"); not drilled, nothing comes back in 400 days;
/// drilled again, they regrow.
#[test]
fn kill_all_nests() {
    let cells = [(100, 100), (120, 100), (100, 120), (140, 140)];
    let mut r = start(5, tun(), &cells);
    r.days(1);
    let ids: Vec<u64> = r.d.nests.ids();
    assert_eq!(ids.len(), 4);
    for (i, &(cx, cz)) in cells.iter().enumerate() {
        r.pumps.well(10 + i as u64, i64::from(cx) + 1, i64::from(cz), 50_000);
    }
    r.days(2);
    let k = u32::from(r.d.tun.oil.nest.k);
    assert!(ids.iter().all(|&id| r.worms(id) == k), "all hatched");
    let kill_all = |r: &mut Run| {
        for &id in &ids {
            let n = r.iaf.nests[&id];
            r.kill(id, n);
        }
        assert!(ids.iter().all(|&id| r.worms(id) == 0));
    };
    // Drilled: back within 72 days.
    kill_all(&mut r);
    let took = r
        .until(100, |r| ids.iter().all(|&id| r.worms(id) == k))
        .expect("they come back");
    assert!(took <= 72, "back after {took} days");
    // Every renewal came from a wait, one worm at a time.
    assert!(r.renews().iter().skip(ids.len()).all(|n| n.count == 1));
    for &id in &ids {
        let n = r.d.nests.get(id).unwrap();
        assert_eq!(n.lost_player, k);
        assert_eq!(n.renewed, k);
    }
    // Not drilled: nothing comes back.
    kill_all(&mut r);
    r.pumps.drilling = false;
    let renewed = r.renews().len();
    r.days(400);
    assert_eq!(r.renews().len(), renewed);
    assert!(ids.iter().all(|&id| r.worms(id) == 0));
    // Drilled again: they regrow, and the plugin's counts agree.
    r.pumps.drilling = true;
    r.until(150, |r| ids.iter().all(|&id| r.worms(id) == k))
        .expect("they regrow");
    assert!(ids.iter().all(|&id| r.iaf.nests[&id] == k));
}

/// Farmed: each worm killed two days after it appears. A nest that is short when a worm arrives
/// starts the next wait then (§2.2), so under farming its waits chain from arrivals, each read at
/// the pressure left from the last kill: `gap = min(cap, base x 2^(P* x 2^(-(gap - c)/H)))` with
/// `P* = (1/K) / (1 - 2^(-gap/H))`. §3.3's closed form adds c to the wait instead (a K 1 site, whose
/// wait starts at the kill), so it reads the gap longer than a K 2 nest's. Within 5% of the chained
/// form, and under §3.3's.
#[test]
fn a_farmed_nest_renews_at_the_chained_rate() {
    let (mut r, id) = one_nest(6, tun());
    r.days(2);
    let tn = r.d.tun.oil.nest.clone();
    let k = u32::from(tn.k);
    let mut arrivals: Vec<u64> = Vec::new();
    let mut seen = r.renews().len();
    r.kill(id, k);
    let mut day = 0u64;
    while day < 30 * 96 {
        r.days(1);
        day += 1;
        let now = r.renews().len();
        if now > seen {
            arrivals.push(day);
            seen = now;
        }
        if let Some(&a) = arrivals.last()
            && day == a + 2
            && r.worms(id) > 0
        {
            let n = r.worms(id);
            r.kill(id, n);
        }
    }
    let settled: Vec<u64> = arrivals.into_iter().filter(|&a| a >= 10 * 96).collect();
    let gap = (settled.last().unwrap() - settled[0]) as f64 / (settled.len() - 1) as f64;
    let (base, cap, h, c) = (
        f64::from(tn.base_days),
        f64::from(tn.cap_days),
        f64::from(tn.half_life_days),
        2.0,
    );
    let per = 1.0 / f64::from(k);
    let wait = |g: f64| {
        let p = per / (1.0 - (-g / h).exp2());
        (base * (p * (-(g - c) / h).exp2()).exp2()).min(cap)
    };
    let (mut lo, mut hi) = (1.0f64, cap);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if wait(mid) > mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let want = 0.5 * (lo + hi);
    assert!(
        (gap - want).abs() / want < 0.05,
        "{gap} days an arrival, chained form {want}"
    );
    let (_, closed) = tn.renewal().farmed(tn.k, 2.0);
    assert!(gap < f64::from(closed), "{gap} under §3.3's {closed}");
}

// ---- the heart, the census, the stage, the raids ------------------------------------------------

#[test]
fn a_broken_heart_stops_renewal_until_it_reforms() {
    let (mut r, id) = one_nest(7, tun());
    r.days(2);
    let k = u32::from(r.d.tun.oil.nest.k);
    r.kill(id, k);
    r.ingest(Obs::NestHeart(boundary::NestHeart {
        nest: id,
        player: PLAYER,
    }));
    let n = *r.d.nests.get(id).unwrap();
    assert_eq!(n.state, NestState::Broken);
    assert_eq!(n.wait_milli, None);
    let reform = u64::from(n.reform_milli.unwrap()).div_ceil(1_000);
    // P is 1 (two worms of K 2) + 1 (the heart): 96 x 2^2 = 384, the cap.
    assert_eq!(reform, 384);
    let asked = placed(&r.cmds).len();
    let renewed = r.renews().len();
    r.days(reform - 2);
    assert_eq!(r.renews().len(), renewed, "no renewal while the heart is broken");
    let took = r
        .until(5, |r| placed(&r.cmds).len() > asked)
        .expect("it asks for its heart again");
    assert!(took <= 3);
    // Placed again, it renews.
    r.until(100, |r| r.worms(id) > 0).expect("it renews");
    assert_eq!(r.d.nests.get(id).unwrap().state, NestState::Placed);
    assert_eq!(r.d.nests.stats.reforms, 1);
}

#[test]
fn the_census_reconciles_without_pressing_and_finds_a_missing_heart() {
    let (mut r, id) = one_nest(8, tun());
    r.days(2);
    let k = u32::from(r.d.tun.oil.nest.k);
    assert_eq!(r.worms(id), k);
    // One worm gone unseen: a loss of cause Other, which presses nothing.
    r.ingest(Obs::NestCensus(boundary::NestCensus {
        nest: id,
        worms: k - 1,
        heart_missing: false,
    }));
    let n = *r.d.nests.get(id).unwrap();
    assert_eq!(n.worms, k - 1);
    assert_eq!(n.pressure.now(r.d.nests.day(), 96.0), 0.0);
    assert_eq!(r.d.nests.stats.lost_other, 1);
    let wait = n.wait_milli.expect("a wait runs");
    assert_eq!(wait, 12_000, "the base wait: no pressure");
    // A player's loss presses: 1/K.
    r.ingest(Obs::NestLoss(boundary::NestLoss {
        nest: id,
        count: 1,
        cause: LossCause::Player.code(),
        player: PLAYER,
    }));
    let p = r.d.nests.get(id).unwrap().pressure.now(r.d.nests.day(), 96.0);
    assert!((p - 1.0 / k as f32).abs() < 1e-6, "{p}");
    // A census with more than K counts K; a missing heart is a broken one.
    r.ingest(Obs::NestCensus(boundary::NestCensus {
        nest: id,
        worms: 9,
        heart_missing: true,
    }));
    let n = *r.d.nests.get(id).unwrap();
    assert_eq!(n.worms, k);
    assert_eq!(n.state, NestState::Broken);
    assert_eq!(r.d.nests.stats.hearts_broken, 1);
}

#[test]
fn the_stage_rises_as_the_wells_drain() {
    let (mut r, id) = one_nest(9, tun());
    r.days(2);
    assert_eq!(r.renews()[0].stage, 1, "a full well: stage 1");
    let k = u32::from(r.d.tun.oil.nest.k);
    // Drained to a half: stage 2; to a tenth: stage 3.
    for (left, stage) in [(45_000, 2), (9_000, 3)] {
        r.pumps.wells[0].remaining = left;
        r.kill(id, k);
        let seen = r.renews().len();
        r.until(80, |r| r.renews().len() > seen).expect("a renewal");
        assert_eq!(r.renews().last().unwrap().stage, stage, "{left} mB left");
        r.until(80, |r| r.worms(id) == k).expect("full");
    }
    // A dry well no longer drills (the link drops it): the nest sleeps.
    r.pumps.wells.clear();
    r.days(2);
    assert!(!r.d.nests.is_awake(id));
}

#[test]
fn raids_are_burrowing_threats_on_the_districts_in_reach() {
    let mut t = tun();
    t.oil.nest.raid_chance_milli = 1_000;
    let (mut r, id) = one_nest(10, t);
    // A district 60 blocks from the heart, one 400 away, and a watched one 100 away.
    let heart = r.d.nests.get(id).unwrap().heart.unwrap();
    let site = |id: u64, dx: i32, watched: bool| DistrictSite {
        id: DistrictId(id),
        pos: Pos::new(0, heart.0 + dx, heart.2),
        watched,
        loaded: watched,
    };
    r.pumps.sites = vec![site(1, 60, false), site(2, 400, false)];
    r.days(3);
    let k = u32::from(r.d.tun.oil.nest.k);
    assert!(!r.pumps.raids.is_empty());
    for t in &r.pumps.raids {
        assert_eq!(t.class, ThreatClass::Burrowing);
        assert_eq!(t.source, ThreatSource::Nest);
        assert_eq!(t.target, ThreatTarget::District(DistrictId(1)));
        assert_eq!(t.count, k);
        assert!((t.strength - f64::from(k)).abs() < 1e-9, "stage 1: a worm's strength each");
    }
    assert_eq!(r.iaf.sorties, 0);
    // Watched, the raid resolves and the worms come up under it.
    r.pumps.sites = vec![site(3, 100, true)];
    r.days(2);
    assert!(r.iaf.sorties > 0);
    let s = r
        .cmds
        .iter()
        .find_map(|c| match c {
            Cmd::NestSortie(s) => Some(*s),
            _ => None,
        })
        .unwrap();
    assert_eq!((s.nest, s.x, s.z, s.count), (id, heart.0 + 100, heart.2, k));
    // Asleep, no raids.
    r.pumps.drilling = false;
    r.days(1);
    let n = r.pumps.raids.len();
    r.days(20);
    assert_eq!(r.pumps.raids.len(), n);
}

#[test]
fn a_well_out_of_reach_wakes_nothing() {
    let (mut r, id) = one_nest(11, tun());
    r.pumps.wells.clear();
    // 144 blocks from the heart: past the 128 of reach.
    r.pumps.well(9, 100 + 9, 100, 90_000);
    r.days(10);
    assert!(!r.d.nests.is_awake(id));
    assert!(r.renews().is_empty());
}

#[test]
fn the_nest_day_order_shortens_the_watch() {
    let (mut r, id) = one_nest(12, tun());
    r.d.set_nest_day(1_000);
    r.days(1);
    assert!(r.d.nests.day() >= 20, "{}", r.d.nests.day());
    let k = u32::from(r.d.tun.oil.nest.k);
    r.kill(id, k);
    // 12 nest days a worm at no pressure... and two at 1: well under a game day each.
    r.until(3, |r| r.worms(id) == k).expect("back in short days");
    r.d.set_nest_day(0);
    assert_eq!(r.d.nest_day_ticks(), DAY);
}

// ---- saves ----------------------------------------------------------------------------------------

const META: PageKey = PageKey::new(0x4400, 0);

#[test]
fn nests_restore_exactly_from_the_resident_save_and_from_pages() {
    let mut t = tun();
    t.oil.nest.raid_chance_milli = 300;
    let (mut a, id) = one_nest(13, t.clone());
    a.pumps.sites = vec![DistrictSite {
        id: DistrictId(1),
        pos: Pos::new(0, 1_640, 1_610),
        watched: false,
        loaded: false,
    }];
    a.days(3);
    a.kill(id, 1);
    a.days(5);
    a.ingest(oil(&[(160, 160)]));
    let dir = std::env::temp_dir().join(format!("rtsim-nests-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Arc::new(RwLock::new(
        PageStore::open(&dir, Durability::NoSync).expect("the store opens"),
    ));
    a.d.attach_store(store.clone(), 1 << 20);
    let mut b = SaveBatch::new();
    let tickets = a.d.save_paged(&mut b, META);
    store.write().unwrap().commit(b, 0).expect("commits");
    a.d.saved(tickets);
    let resident = Dragons::load(t.clone(), &a.d.save()).expect("the resident save loads");
    let paged = Dragons::open_paged(store.clone(), 1 << 20, META, t).expect("the pages open");
    let h = a.d.state_hash();
    assert_eq!(resident.state_hash(), h);
    assert_eq!(paged.state_hash(), h);
    assert_eq!(paged.nests.stats, a.d.nests.stats);
    assert_eq!(paged.nests.get(id), a.d.nests.get(id));
    let mut runs = [resident, paged].map(|d| Run {
        w: synth::world(13).0,
        d,
        iaf: a.iaf.clone(),
        pumps: a.pumps.clone(),
        t: a.t,
        cmds: Vec::new(),
    });
    let from = a.cmds.len();
    a.days(60);
    for r in &mut runs {
        r.days(60);
        assert_eq!(r.d.state_hash(), a.d.state_hash());
        assert_eq!(r.cmds, a.cmds[from..]);
    }
    drop(runs);
    std::fs::remove_dir_all(&dir).unwrap();
}

// ---- the industry link ----------------------------------------------------------------------------

#[test]
fn a_pumpjacks_sample_of_the_latest_sweep_is_a_well() {
    use rtsim_industry::ids::ChunkPos;
    use rtsim_industry::obs::{DepositPluginId, DepositSample, DistrictDecl, Obs as IObs, SweepEnd, Timed};
    let mut ind = rtsim_industry::engine::Engine::new(
        21,
        rtsim_industry::tunables::Tunables::default(),
        Tick(0),
    );
    let pump = DistrictId(9);
    let feed = |e: &mut rtsim_industry::engine::Engine, at: Tick, obs: IObs| {
        e.ingest(&Timed { at, obs });
    };
    feed(
        &mut ind,
        Tick(0),
        IObs::District(DistrictDecl {
            district: pump,
            revision: 1,
            dim: 0,
            name: 1,
            not_learnable: None,
            inventory_count: 0,
        }),
    );
    feed(
        &mut ind,
        Tick(0),
        IObs::DistrictChunks {
            district: pump,
            revision: 1,
            part: 0,
            chunks: vec![(66, 1), (67, 1)],
        },
    );
    let sweep = |e: &mut rtsim_industry::engine::Engine, n: u32, sample: bool| {
        if sample {
            feed(
                e,
                Tick(u64::from(n) * 100),
                IObs::DepositSample(DepositSample {
                    district: pump,
                    good: Good(4),
                    plugin: DepositPluginId::DieselOil,
                    chunk: ChunkPos { dim: 0, x: 67, z: 1 },
                    remaining: 70_000,
                    infinite: false,
                    sweep: n,
                }),
            );
        }
        feed(
            e,
            Tick(u64::from(n) * 100),
            IObs::SweepEnd(SweepEnd {
                district: pump,
                sweep: n,
                records: u32::from(sample),
                inventories: 0,
                live: false,
                creative: false,
                overstressed: false,
            }),
        );
    };
    let mut index = DistrictIndex::default();
    sweep(&mut ind, 1, true);
    let wells = IndustryLink {
        engine: &mut ind,
        index: &mut index,
    }
    .wells();
    assert_eq!(
        wells,
        vec![Well {
            district: pump,
            frame: 0,
            x: 67 * 16 + 8,
            z: 24,
            remaining: 70_000,
            infinite: false,
        }]
    );
    // The next sweep finds no pumpjack: the well is gone.
    sweep(&mut ind, 2, false);
    let wells = IndustryLink {
        engine: &mut ind,
        index: &mut index,
    }
    .wells();
    assert!(wells.is_empty(), "{wells:?}");
}
