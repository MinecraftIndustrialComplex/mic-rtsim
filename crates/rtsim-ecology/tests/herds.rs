// SPDX-License-Identifier: AGPL-3.0-or-later
//! Migrating herds (PLAN.md §10.2): conservation, identity after deaths, flat catch-up,
//! composition, one materialization per approach, restore, the fence, and dragons as predators.

use proptest::prelude::*;
use rtsim_archive::{Row, Severity, kinds};
use rtsim_ecology::herds::boundary::{self, Cmd, Obs};
use rtsim_ecology::herds::cohort::Work;
use rtsim_ecology::herds::forage::cell_of;
use rtsim_ecology::herds::herd::agent_of;
use rtsim_ecology::herds::synth::{self, FakeHerds, Valley};
use rtsim_ecology::herds::{HerdId, HerdTunables, Herds, NewHerd};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::tunables::PreyKind;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::codec::{Decode, Encode};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};

/// The meta page's key: rtsim-jni's herds page (table 0x4800, id 0).
const META: PageKey = PageKey::new(0x4800, 0);
use rtsim_settle::Pos;
use rtsim_travel::legs::LegIndex;
use std::sync::{Arc, RwLock};

const PLAYER: (u64, u64) = (7, 11);

fn legs() -> LegIndex {
    LegIndex::new(1_000)
}

struct Rig {
    legs: LegIndex,
    h: Herds,
    fake: FakeHerds,
}

impl Rig {
    fn valley(seed: u64) -> Rig {
        Rig {
            legs: legs(),
            h: synth::valley(seed, HerdTunables::default(), Tick::ZERO),
            fake: FakeHerds::default(),
        }
    }

    /// Runs to `to` in steps of `step`, the player standing at `at` (if any), with the plugin
    /// answering every command; checks the invariants after every step.
    fn run(&mut self, to: Tick, step: u64, at: Option<Pos>) {
        let mut t = self.h.now();
        while t < to {
            let next = (t + step).min(to);
            if let Some(p) = at {
                self.h.observe_player(&mut self.legs, PLAYER, p, t);
            }
            self.h.run_until(&mut self.legs, next);
            self.pump(next);
            self.h.check(next).expect("invariants");
            t = next;
        }
    }

    fn pump(&mut self, at: Tick) {
        for _ in 0..2 {
            let cmds = self.h.take_commands();
            self.fake.apply(cmds, at);
            self.fake.walk(&self.h, at);
            for o in self.fake.take_obs() {
                self.h.ingest(&mut self.legs, &o, at);
            }
        }
    }

    fn near(&self, h: HerdId, t: Tick) -> Pos {
        let p = self.h.position(h, t);
        Pos::new(0, p.0 as i32 + 30, p.1 as i32)
    }

    /// Groups of herd `h` ever materialized.
    fn materializes(&self, h: HerdId) -> usize {
        let a = agent_of(h);
        self.fake.history.iter().filter(|x| x.0 == a).count()
    }
}

fn flock(r: &Rig) -> HerdId {
    r.h.by_name("Northdown flock").expect("flock")
}

#[test]
fn founded_herds_hold_exactly_their_count_by_cohort() {
    let r = Rig::valley(1);
    for (id, x) in r.h.herds.iter() {
        let c = r.h.cohorts(id, Tick::ZERO);
        assert_eq!(c.iter().sum::<u64>(), x.ledger.founded, "{}", x.name);
        assert!(c[0] > 0 && c[1] > 0, "{}: {c:?}", x.name);
        assert_eq!(x.next_index, x.ledger.founded);
    }
    r.h.check(Tick::ZERO).unwrap();
}

#[test]
fn herds_go_north_for_summer_and_south_for_winter() {
    let mut r = Rig::valley(3);
    let f = flock(&r);
    let subs = r.h.subs();
    let mut z_summer = Vec::new();
    let mut z_winter = Vec::new();
    for n in 0..24i64 {
        let t = subs.start(n + 1);
        r.run(t, 24_000, None);
        let z = r.h.position(f, t).1;
        match rtsim_ecology::herds::climate::Subs::in_year(n) {
            4..=6 => z_summer.push(z),
            10 | 11 | 0 => z_winter.push(z),
            _ => {}
        }
    }
    let mean = |v: &[i64]| v.iter().sum::<i64>() as f64 / v.len() as f64;
    assert!(
        mean(&z_summer) + 1_500.0 < mean(&z_winter),
        "summer z {z_summer:?}, winter z {z_winter:?}"
    );
}

/// Kills, shearing, a lead, walking away and back.
#[test]
fn the_same_animals_come_back_after_kills_and_a_season_of_deaths() {
    let mut r = Rig::valley(5);
    let f = flock(&r);
    let t1 = Tick::from_days(20);
    r.run(t1, 2_000, None);
    let p = r.near(f, t1);
    r.run(t1 + 1_500, 100, Some(p));
    let agent = agent_of(f);
    let first = r
        .fake
        .group_of(agent)
        .cloned()
        .expect("a group materialized");
    assert_eq!(r.materializes(f), 1);
    let before = r.h.count(f, r.h.now());
    let killed = r.fake.kill(agent, 5, PLAYER);
    let led = r.fake.lead(agent, PLAYER).expect("a body");
    let now = r.h.now();
    r.pump(now);
    assert_eq!(
        r.h.count(f, now),
        before - 6,
        "the count drops by exactly six"
    );
    // Walk away: captured. A season of deaths passes unseen.
    r.h.player_left(&mut r.legs, PLAYER, now);
    r.pump(now);
    assert!(r.fake.group_of(agent).is_none());
    let t2 = now + 24 * TICKS_PER_DAY;
    r.run(t2, 4_000, None);
    let p2 = r.near(f, t2);
    r.run(t2 + 1_500, 100, Some(p2));
    let second = r.fake.group_of(agent).cloned().expect("a group again");
    let shown: Vec<u64> = second.bodies.iter().map(|b| b.spawn.member).collect();
    for k in killed.iter().chain([&led]) {
        assert!(
            !shown.contains(k),
            "member {k} was killed or led away and came back"
        );
    }
    // Every survivor of the first group is back, looking the same.
    for b in &first.bodies {
        let k = b.spawn.member;
        if killed.contains(&k) || k == led {
            continue;
        }
        let again = second
            .bodies
            .iter()
            .find(|x| x.spawn.member == k)
            .unwrap_or_else(|| panic!("member {k} did not come back"));
        assert_eq!(again.spawn.variant, b.spawn.variant);
        assert_eq!(again.spawn.seed, b.spawn.seed);
        assert!(again.spawn.age_days > b.spawn.age_days);
    }
    let x = r.h.herd(f).unwrap();
    assert!(killed.iter().all(|k| x.dead.contains(k)) && x.dead.contains(&led));
    assert_eq!(x.ledger.killed, 5);
    assert_eq!(x.ledger.taken, 1);
}

/// O5: a read after ten thousand days costs what a read after one does (bounded tree walks), and
/// a read changes nothing.
#[test]
fn catch_up_is_flat_in_elapsed_time() {
    let r = Rig::valley(9);
    let f = flock(&r);
    let x = r.h.herd(f).unwrap();
    let sp = r.h.sp(f).clone();
    let yd = r.h.year_days();
    let hash = r.h.state_hash();
    let mut works = Vec::new();
    for days in [1i64, 10, 100, 1_000, 10_000] {
        let mut w = Work::default();
        for c in &x.classes {
            let (_, k) = c.deaths_until(&sp, yd, x.seed, days);
            w.segments += k.segments;
            w.levels += k.levels;
        }
        let _ = r.h.count(f, Tick::from_days(days as u64));
        works.push(w.levels);
    }
    let max = *works.iter().max().unwrap();
    let classes = x.classes.len() as u32;
    assert!(max <= classes * 3 * 12, "levels walked {works:?}");
    assert_eq!(r.h.state_hash(), hash, "reads committed something");
}

/// One materialization per approach, however often the player is reported.
#[test]
fn one_materialization_per_approach() {
    let mut r = Rig::valley(13);
    let f = flock(&r);
    let t = Tick::from_days(10);
    r.run(t, 2_000, None);
    let p = r.near(f, t);
    r.run(t + 6_000, 20, Some(p));
    assert_eq!(r.materializes(f), 1, "{:?}", r.fake.history);
    let caps = r.h.stats.captures;
    // Walk away and back: one capture, one more materialization.
    let away = Pos::new(0, p.x + 3_000, p.z);
    let t2 = r.h.now();
    r.run(t2 + 3_000, 20, Some(away));
    assert!(r.h.stats.captures > caps);
    assert!(r.fake.group_of(agent_of(f)).is_none());
    let t3 = r.h.now();
    let p3 = r.near(f, t3);
    r.run(t3 + 6_000, 20, Some(p3));
    assert_eq!(r.materializes(f), 2);
}

fn run_pieces(seed: u64, cuts: &[u64], end: u64) -> u64 {
    let mut legs = legs();
    let mut h = synth::bench_herds(seed, 4, 3_000, Tick::ZERO);
    let mut cuts: Vec<u64> = cuts.iter().map(|c| c % end).collect();
    cuts.sort_unstable();
    for c in cuts.into_iter().chain([end]) {
        h.run_until(&mut legs, Tick(c));
        // Reads in between change nothing.
        for (id, _) in h.herds.iter() {
            let _ = h.count(id, Tick(c));
            let _ = h.cohorts(id, Tick(c + 50_000));
            let _ = h.prey_near(0, 0, 0, 5_000, Tick(c), 1);
        }
    }
    h.state_hash()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

    /// catch_up(t0 → t2) equals catch_up(t1 → t2) after catch_up(t0 → t1) (SCALE.md §4), with
    /// pure reads in between.
    #[test]
    fn running_in_pieces_equals_running_at_once(seed in 0u64..1_000, cuts in prop::collection::vec(0u64..5_000_000, 0..6)) {
        let end = 4_000_000;
        prop_assert_eq!(run_pieces(seed, &cuts, end), run_pieces(seed, &[], end));
    }
}

#[derive(Clone, Debug)]
enum Op {
    Advance(u64),
    Approach(usize),
    Kill(usize),
    Shear,
    Lead,
    Leave,
    Predate(usize, i32),
    KillAgain,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1u64..200_000).prop_map(Op::Advance),
        (0usize..6).prop_map(Op::Approach),
        (1usize..8).prop_map(Op::Kill),
        Just(Op::Shear),
        Just(Op::Lead),
        Just(Op::Leave),
        ((0usize..6), (1i32..200)).prop_map(|(h, f)| Op::Predate(h, f)),
        Just(Op::KillAgain),
    ]
}

fn apply(r: &mut Rig, o: &Op, last: &mut Vec<(u64, u64, u64)>) {
    let ids: Vec<HerdId> = r.h.herds.iter().map(|(h, _)| h).collect();
    let now = r.h.now();
    match *o {
        Op::Advance(d) => r.run(now + d, 6_000, None),
        Op::Approach(i) => {
            let h = ids[i % ids.len()];
            let p = r.near(h, now);
            r.run(now + 2_000, 100, Some(p));
        }
        Op::Kill(n) => {
            for &agent in r.fake.groups.clone().keys() {
                for k in r.fake.kill(agent, n, PLAYER) {
                    last.push((agent, 0, k));
                }
            }
            r.pump(now);
        }
        Op::Shear => {
            for &agent in r.fake.groups.clone().keys() {
                r.fake.shear(agent, 2, PLAYER);
            }
            r.pump(now);
        }
        Op::Lead => {
            for &agent in r.fake.groups.clone().keys() {
                r.fake.lead(agent, PLAYER);
            }
            r.pump(now);
        }
        Op::Leave => {
            r.h.player_left(&mut r.legs, PLAYER, now);
            r.pump(now);
        }
        Op::Predate(i, food) => {
            let h = ids[i % ids.len()];
            let p = r.h.position(h, now);
            r.h.predate(h, p.0, p.1, now, food, 99);
        }
        Op::KillAgain => {
            // A duplicate kill record (the journal replays it): applied once.
            if let Some(&(agent, _, k)) = last.last()
                && let Some(g) = r.fake.groups.get(&agent)
            {
                let o = Obs::Killed(boundary::Killed {
                    agent,
                    epoch: g.epoch,
                    member: k,
                    by: boundary::KillerKind::Player,
                    killer: PLAYER,
                    op: 1 << 63 | 1,
                });
                let n =
                    r.h.herd(rtsim_ecology::herds::herd::herd_of(agent).unwrap())
                        .unwrap()
                        .ledger
                        .killed;
                r.h.ingest(&mut r.legs, &o, now);
                r.h.ingest(&mut r.legs, &o, now);
                let m =
                    r.h.herd(rtsim_ecology::herds::herd::herd_of(agent).unwrap())
                        .unwrap()
                        .ledger
                        .killed;
                assert!(m <= n + 1);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, ..ProptestConfig::default() })]

    /// Conservation: whatever happens, every herd's classes hold exactly what its ledger says
    /// came in and went out, as integers, and identity invariants hold (checked after every
    /// step inside `Rig::run` and here after every op).
    #[test]
    fn conservation_holds_under_any_script(seed in 0u64..100, ops in prop::collection::vec(op(), 1..14)) {
        let mut r = Rig::valley(seed);
        let mut last = Vec::new();
        for o in &ops {
            apply(&mut r, o, &mut last);
            let now = r.h.now();
            r.h.check(now).map_err(TestCaseError::fail)?;
            for (_, x) in r.h.herds.iter() {
                let l = x.ledger;
                let alive: i128 = x.classes.iter().map(|c| i128::from(c.alive)).sum();
                prop_assert_eq!(alive, l.expected_at_anchors());
            }
        }
    }

    /// Restore: a herds engine saved at any point of a script, with its leg index, continues
    /// exactly as one that never stopped (state hash after every op, the same commands).
    #[test]
    fn a_restored_engine_continues_exactly(seed in 0u64..100, ops in prop::collection::vec(op(), 1..10), cut in 0usize..10) {
        let mut a = Rig::valley(seed);
        let mut last = Vec::new();
        let cut = cut % ops.len();
        for o in &ops[..cut] {
            apply(&mut a, o, &mut last);
        }
        let bytes = a.h.save();
        let mut b = Rig {
            legs: LegIndex::from_bytes(&a.legs.to_bytes()).expect("legs load"),
            h: Herds::load(HerdTunables::default(), Box::new(Valley::default()), &bytes).expect("herds load"),
            fake: a.fake.clone(),
        };
        prop_assert_eq!(b.h.state_hash(), a.h.state_hash());
        prop_assert_eq!(b.h.save(), bytes);
        let mut last_b = last.clone();
        for o in &ops[cut..] {
            apply(&mut a, o, &mut last);
            apply(&mut b, o, &mut last_b);
            prop_assert_eq!(b.h.state_hash(), a.h.state_hash(), "after {:?}", o);
        }
        prop_assert_eq!(&b.fake.history, &a.fake.history);
    }
}

/// A 3 x 3 grid of herd ranges around spawn (`synth::ranges`), with the frontier at 4 a
/// range-year so a short script sees an empty range founded again.
fn ranges_rig(seed: u64) -> Rig {
    let mut h = synth::ranges(seed, ranges_tunables(), Tick::ZERO, -1..=1, -1..=1);
    h.log_enabled = true;
    Rig {
        legs: legs(),
        h,
        fake: FakeHerds::default(),
    }
}

fn ranges_tunables() -> HerdTunables {
    HerdTunables {
        frontier_per_year: 4.0,
        ..HerdTunables::default()
    }
}

/// The ranges' scripts: the herd ops, plus a range wiped (every latent animal killed) and whole
/// years passing, so ranges empty, are founded again and take dispersers.
#[derive(Clone, Debug)]
enum RangeOp {
    Op(Op),
    Cull(usize),
    Years(u64),
}

fn range_op() -> impl Strategy<Value = RangeOp> {
    prop_oneof![
        2 => op().prop_map(RangeOp::Op),
        2 => (0usize..9).prop_map(RangeOp::Cull),
        2 => (1u64..3).prop_map(RangeOp::Years),
    ]
}

fn apply_range(r: &mut Rig, o: &RangeOp, last: &mut Vec<(u64, u64, u64)>) {
    let ids: Vec<HerdId> = r.h.herds.iter().map(|(h, _)| h).collect();
    let now = r.h.now();
    match o {
        RangeOp::Op(o) => apply(r, o, last),
        RangeOp::Cull(i) => {
            r.h.cull(ids[i % ids.len()], now);
        }
        RangeOp::Years(n) => {
            let year = r.h.subs().year_ticks();
            r.run(now + n * year, 24_000, None);
        }
    }
}

/// Animals each row was founded with: its founding count, plus `max(frontier_min,
/// frontier_share x K)` for each frontier founding in the log.
fn founded_by_row(r: &Rig, start: &[(HerdId, String, u64)]) -> Vec<(HerdId, u64)> {
    let t = &r.h.tun;
    start
        .iter()
        .map(|(h, name, count)| {
            let k = r.h.herd(*h).expect("a row").k;
            let n = ((t.frontier_share * k as f64).round() as u64).max(t.frontier_min);
            let frontiers =
                r.h.log
                    .iter()
                    .filter(|l| l.kind == "frontier" && l.herd.as_deref() == Some(name.as_str()))
                    .count() as u64;
            (*h, count + frontiers * n)
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

    /// Founding is a booked source (EQUILIBRIUM.md §2.4.2): on a grid of ranges whose herds are
    /// wiped and founded again, every animal is a founding's, a birth's or an arrival's, each
    /// row's founded is its founding count plus its frontier foundings' groups, the world's
    /// dispersers leave exactly as many as arrive, and conservation holds after every op.
    #[test]
    fn founding_and_dispersal_are_booked_sources(seed in 0u64..100, ops in prop::collection::vec(range_op(), 3..10)) {
        let mut r = ranges_rig(seed);
        let start: Vec<(HerdId, String, u64)> =
            r.h.herds.iter().map(|(h, x)| (h, x.name.clone(), x.ledger.founded)).collect();
        let mut last = Vec::new();
        for o in &ops {
            apply_range(&mut r, o, &mut last);
            let now = r.h.now();
            r.h.check(now).map_err(TestCaseError::fail)?;
            let (mut arrived, mut left) = (0, 0);
            for (_, x) in r.h.herds.iter() {
                let l = x.ledger;
                let alive: i128 = x.classes.iter().map(|c| i128::from(c.alive)).sum();
                prop_assert_eq!(alive, l.expected_at_anchors());
                prop_assert_eq!(l.founded + l.born + l.arrived, x.next_index);
                prop_assert!(x.vacant.is_none() || r.h.count(r.h.by_name(&x.name).unwrap(), now) == 0);
                arrived += l.arrived;
                left += l.left;
            }
            prop_assert_eq!(arrived, left);
            for (h, want) in founded_by_row(&r, &start) {
                prop_assert_eq!(r.h.herd(h).unwrap().ledger.founded, want);
            }
        }
    }

    /// Restore with ranges emptying and refilling: the vacancy, the booked frontier founding
    /// and the dispersers' classes survive a save exactly.
    #[test]
    fn a_restored_ranges_engine_continues_exactly(seed in 0u64..100, ops in prop::collection::vec(range_op(), 1..8), cut in 0usize..8) {
        let mut a = ranges_rig(seed);
        let mut last = Vec::new();
        let cut = cut % ops.len();
        for o in &ops[..cut] {
            apply_range(&mut a, o, &mut last);
        }
        let bytes = a.h.save();
        let mut b = Rig {
            legs: LegIndex::from_bytes(&a.legs.to_bytes()).expect("legs load"),
            h: Herds::load(ranges_tunables(), Box::new(Valley::default()), &bytes).expect("herds load"),
            fake: a.fake.clone(),
        };
        prop_assert_eq!(b.h.state_hash(), a.h.state_hash());
        prop_assert_eq!(b.h.save(), bytes);
        let mut last_b = last.clone();
        for o in &ops[cut..] {
            apply_range(&mut a, o, &mut last);
            apply_range(&mut b, o, &mut last_b);
            prop_assert_eq!(b.h.state_hash(), a.h.state_hash(), "after {:?}", o);
        }
        prop_assert_eq!(&b.fake.history, &a.fake.history);
    }

    /// The refuge (EQUILIBRIUM.md §3.4): however hungry the predators and however often they
    /// come, predation never takes a herd below `max(refuge_min, refuge_share x K)` (deaths of
    /// age and hunger still can), and a herd at or below it is offered to no predator.
    #[test]
    fn predation_never_takes_a_herd_below_refuge(seed in 0u64..100, hunts in prop::collection::vec((0u64..400_000, 0usize..6, 1i32..1_000_000), 1..40)) {
        let mut r = Rig::valley(seed);
        let ids: Vec<HerdId> = r.h.herds.iter().map(|(h, _)| h).collect();
        for (wait, i, food) in hunts {
            let now = r.h.now();
            r.run(now + wait, 50_000, None);
            let now = r.h.now();
            let h = ids[i % ids.len()];
            let refuge = r.h.refuge(r.h.herd(h).unwrap().k);
            // Many predators at once, as a lair's whole brood.
            for p in 0..8u64 {
                let before = r.h.count(h, now);
                let pos = r.h.position(h, now);
                r.h.predate(h, pos.0, pos.1, now, food, 100 + p);
                let after = r.h.count(h, now);
                prop_assert!(after >= before.min(refuge), "{} to {} below refuge {}", before, after, refuge);
            }
            let pos = r.h.position(h, now);
            if r.h.count(h, now) <= refuge {
                let offered = r.h.prey_near(0, pos.0, pos.1, 10_000, now, 100);
                prop_assert!(offered.iter().all(|o| o.herd != h), "a herd at its refuge offered");
            }
            r.h.check(now).map_err(TestCaseError::fail)?;
        }
    }
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("rtsim-herds-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

type Store = Arc<RwLock<PageStore>>;

fn store(tag: &str) -> (Store, std::path::PathBuf) {
    let dir = tmpdir(tag);
    let s = PageStore::open(&dir, Durability::NoSync).expect("store");
    (Arc::new(RwLock::new(s)), dir)
}

/// A paged save: the meta and the changed row pages in one commit, then the budget.
fn save_paged(h: &mut Herds, store: &Store) {
    let mut b = SaveBatch::new();
    let t = h.save_paged(&mut b, META);
    store.write().unwrap().commit(b, 0).expect("commit");
    h.saved(t);
    h.enforce_budget();
}

/// Row bytes a paged rig keeps resident: less than one page of herd rows.
const BUDGET: usize = 6_000;

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, ..ProptestConfig::default() })]

    /// Paged: an engine whose rows page through a store under a budget smaller than one page of
    /// herds (saved after every op, then evicted down to the budget) runs exactly as a resident
    /// one, and an engine opened from those pages at any point continues exactly.
    #[test]
    fn a_paged_engine_runs_and_restores_exactly(seed in 0u64..100, ops in prop::collection::vec(op(), 1..10), cut in 0usize..10) {
        let (st, dir) = store("prop");
        let mut a = Rig::valley(seed);
        let mut b = Rig::valley(seed);
        b.h.attach_store(st.clone(), BUDGET);
        let cut = cut % ops.len();
        let (mut la, mut lb) = (Vec::new(), Vec::new());
        for o in &ops[..cut] {
            apply(&mut a, o, &mut la);
            apply(&mut b, o, &mut lb);
            save_paged(&mut b.h, &st);
            prop_assert_eq!(b.h.state_hash(), a.h.state_hash(), "after {:?}", o);
        }
        save_paged(&mut b.h, &st);
        let mut c = Rig {
            legs: LegIndex::from_bytes(&b.legs.to_bytes()).expect("legs load"),
            h: Herds::open_paged(st.clone(), BUDGET, META, HerdTunables::default(), Box::new(Valley::default()))
                .expect("herds open"),
            fake: b.fake.clone(),
        };
        drop(b);
        prop_assert_eq!(c.h.state_hash(), a.h.state_hash());
        for o in &ops[cut..] {
            apply(&mut a, o, &mut la);
            apply(&mut c, o, &mut lb);
            save_paged(&mut c.h, &st);
            prop_assert_eq!(c.h.state_hash(), a.h.state_hash(), "after {:?}", o);
        }
        prop_assert_eq!(&c.fake.history, &a.fake.history);
        drop(c);
        drop(st);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The paged rig really pages: after a save, the rows held are within the budget, and the next
/// year's decisions load the herds back.
#[test]
fn a_paged_engine_keeps_to_its_budget() {
    let (st, dir) = store("budget");
    let mut a = Rig::valley(5);
    let mut b = Rig::valley(5);
    b.h.attach_store(st.clone(), BUDGET);
    let year = u64::from(a.h.year_days()) * TICKS_PER_DAY;
    let mut t = Tick::ZERO;
    for _ in 0..8 {
        t += year / 8;
        a.run(t, 6_000, None);
        b.run(t, 6_000, None);
        save_paged(&mut b.h, &st);
        assert!(
            b.h.rows_resident_bytes() <= BUDGET,
            "{} resident",
            b.h.rows_resident_bytes()
        );
    }
    let s = b.h.row_stats();
    assert!(s.loads > 8 && s.evictions > 8, "{s:?}");
    assert_eq!(b.h.state_hash(), a.h.state_hash());
    drop(b);
    drop(st);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_truncated_or_foreign_page_is_an_error() {
    let r = Rig::valley(2);
    let b = r.h.save();
    for n in [0, 4, b.len() / 2, b.len() - 1] {
        assert!(
            Herds::load(
                HerdTunables::default(),
                Box::new(Valley::default()),
                &b[..n]
            )
            .is_err()
        );
    }
    let mut bad = b.clone();
    bad[0] ^= 0xFF;
    assert!(Herds::load(HerdTunables::default(), Box::new(Valley::default()), &bad).is_err());
}

/// An obstacle report raises the cost of the cell the group could not enter, and the herd
/// replans around it at once.
#[test]
fn an_obstacle_raises_the_cost_and_the_herd_replans_around_it() {
    let mut r = Rig::valley(17);
    let f = flock(&r);
    // A moment the flock is walking, not resting at its target.
    let mut t = Tick::from_days(4);
    r.run(t, 2_000, None);
    while r.h.herd(f).unwrap().route.segment_at(t + 2_500).is_none() {
        t += 6_000;
        r.run(t, 6_000, None);
    }
    let p = r.near(f, t);
    r.run(t + 2_000, 100, Some(p));
    let agent = agent_of(f);
    let g = r.fake.group_of(agent).cloned().expect("embodied");
    let now = r.h.now();
    let pos = r.h.position(f, now);
    let route = r.h.herd(f).unwrap().route.clone();
    let target = route.heading(now);
    let blocked = {
        // The cell a cell ahead toward the target.
        let d = (((i64::from(target.0) - pos.0) as f64).powi(2)
            + ((i64::from(target.1) - pos.1) as f64).powi(2))
        .sqrt()
        .max(1.0);
        cell_of(
            pos.0 + ((i64::from(target.0) - pos.0) as f64 * 128.0 / d) as i64,
            pos.1 + ((i64::from(target.1) - pos.1) as f64 * 128.0 / d) as i64,
        )
    };
    let o = Obs::Obstacle(boundary::Obstacle {
        agent,
        epoch: g.epoch,
        failures: g.bodies.len() as u32,
        present: g.bodies.len() as u32,
        stuck: (pos.0 as i32, pos.1 as i32),
        target,
    });
    r.h.ingest(&mut r.legs, &o, now);
    assert_eq!(r.h.stats.obstacles, 1);
    assert_eq!(r.h.stats.replans, 1);
    let cost = r.h.obstacles.extra(
        (0, blocked.0, blocked.1),
        now,
        r.h.tun.obstacle_half_life_ticks,
    );
    assert!(cost >= 50.0, "cost {cost}");
    let after = r.h.herd(f).unwrap().route.clone();
    assert_eq!(after.start(), (pos.0 as i32, pos.1 as i32));
    // The new route does not walk into the blocked cell: no waypoint in it, and no step of
    // the route between cell centres crosses it.
    for (i, w) in after.points.windows(2).enumerate() {
        assert_ne!(
            cell_of(i64::from(w[1].0), i64::from(w[1].1)),
            blocked,
            "the replan walks into the blocked cell {blocked:?}: {:?}",
            after.points
        );
        if i == 0 {
            continue;
        }
        let mid = (
            (i64::from(w[0].0) + i64::from(w[1].0)) / 2,
            (i64::from(w[0].1) + i64::from(w[1].1)) / 2,
        );
        assert_ne!(cell_of(mid.0, mid.1), blocked, "{:?}", after.points);
    }
    // A steer to the new heading went to the plugin.
    let cmds = r.h.take_commands();
    assert!(cmds.iter().any(|c| matches!(c, Cmd::Steer(_))), "{cmds:?}");
}

/// Dragons prey on herds through the prey interface: what they take leaves the herds' ledgers as
/// eaten, exactly, and a dragon that ate from a herd follows it.
#[test]
fn dragons_eat_from_herds_and_follow_them() {
    let seed = 21;
    let (mut w, _) = rtsim_ecology::synth::world(seed);
    let mut d =
        rtsim_ecology::Dragons::new(seed, rtsim_ecology::DragonTunables::default(), Tick::ZERO);
    let l = d.add_lair(
        &mut w,
        301,
        "Test Roost",
        rtsim_ecology::Species::Fire,
        rtsim_ecology::LairKind::Roost,
        Pos::new(0, 200, 1_500),
        110,
        Tick::ZERO,
    );
    d.add_dragon(
        &mut w,
        rtsim_ecology::NewDragon {
            name: "Tester".into(),
            species: rtsim_ecology::Species::Fire,
            gender: rtsim_ecology::Gender::Female,
            variant: 0,
            age_ticks: 110 * TICKS_PER_DAY,
            lair: Some(l),
            fullness: 30,
            health: None,
            seed: None,
            at: None,
        },
        Tick::ZERO,
    );
    d.log_enabled = true;
    d.tape.set_on(true);
    let mut h = Herds::new(
        seed,
        HerdTunables::default(),
        Box::new(Valley::default()),
        Tick::ZERO,
    );
    h.tape.set_on(true);
    let herd = h.add_herd(
        NewHerd {
            name: "Bait flock".into(),
            species: rtsim_ecology::herds::HerdSpecies::Sheep,
            frame: 0,
            pos: (300, 1_700),
            count: 5_000,
            seed: Some(5),
        },
        Tick::ZERO,
    );
    d.run_until_with_herds(&mut w, &mut NoInfrastructure, &mut h, Tick::from_days(30));
    let hunts: Vec<_> = d
        .hunts
        .iter()
        .filter(|x| x.kind == PreyKind::Herd && x.food > 0)
        .collect();
    assert!(
        !hunts.is_empty(),
        "no herd hunts: {:?}",
        d.hunts.iter().map(|x| x.kind).collect::<Vec<_>>()
    );
    let taken: i64 = hunts.iter().flat_map(|x| x.taken.iter().map(|t| t.1)).sum();
    let x = h.herd(herd).unwrap();
    assert_eq!(x.ledger.eaten as i64, taken);
    assert!(d.stats.herd_hunts > 0);
    assert!(x.followers.iter().any(|&(p, _)| p != 0));
    h.check(h.now()).unwrap();
    // The herds engine came back to the caller with its time advanced.
    assert_eq!(h.now(), Tick::from_days(30));
    // The event archive: the herds' tape moved with them through the dragons' run, so every
    // predation is on the herds' tape (the herd's, taken by the dragon) and the dragons' tape
    // holds only dragons' kinds, among them the hunts of this herd.
    let tester = d.by_name("Tester").unwrap();
    let preds = events_of(&h, kinds::HERDS_PREDATION);
    assert_eq!(preds.len() as u64, h.stats.predations);
    assert!(!preds.is_empty());
    for e in &preds {
        assert_eq!(
            (e.subject.value, e.object.value),
            (herd.to_bits(), tester.to_bits())
        );
        assert_eq!(e.severity, Severity::Minor);
        assert!(
            e.magnitude > 0 && e.x.abs() < 5_000 && e.z.abs() < 5_000,
            "{e:?}"
        );
    }
    let eaten: i64 = preds.iter().map(|e| i64::from(e.magnitude)).sum();
    assert_eq!(eaten, x.ledger.eaten as i64);
    for k in rtsim_archive::record::kinds_on(&h.tape) {
        assert_eq!(k.group(), "herds", "{}", k.name());
    }
    for k in rtsim_archive::record::kinds_on(&d.tape) {
        assert_eq!(k.group(), "dragons", "{}", k.name());
    }
    let dragon_hunts: Vec<_> = rtsim_archive::record::rows_on(&d.tape)
        .into_iter()
        .filter_map(|(k, _, r)| match r {
            Row::Event(e) if k == kinds::DRAGONS_HUNT && e.object.value == herd.to_bits() => {
                Some(e)
            }
            _ => None,
        })
        .collect();
    assert!(dragon_hunts.len() >= hunts.len(), "{}", dragon_hunts.len());
    assert!(
        dragon_hunts
            .iter()
            .all(|e| e.subject.value == tester.to_bits())
    );
}

/// The tape's events of `kind`.
fn events_of(h: &Herds, kind: rtsim_archive::Kind) -> Vec<rtsim_archive::Event> {
    rtsim_archive::record::rows_on(&h.tape)
        .into_iter()
        .filter_map(|(k, _, r)| match r {
            Row::Event(e) if k == kind => Some(e),
            _ => None,
        })
        .collect()
}

/// The herds' archive rows (VIEWER.md §10.1): the valley's six founded herds, two years of
/// routes as one leg per straight piece, and the starvation of an overgrazed winter; then a herd
/// of ten that a predator eats down to its refuge (EQUILIBRIUM.md §3.4) and a wipe ends.
#[test]
fn the_archive_records_the_herds() {
    use rtsim_archive::record::{kinds_on, rows_on};
    let mut h = Herds::new(
        3,
        HerdTunables::default(),
        Box::new(Valley::default()),
        Tick::ZERO,
    );
    h.tape.set_on(true);
    for n in synth::scenario_herds() {
        h.add_herd(n, Tick::ZERO);
    }
    let mut index = legs();
    h.run_until(&mut index, Tick::from_days(2 * 365));
    for k in kinds_on(&h.tape) {
        assert_eq!(k.group(), "herds", "{}", k.name());
        assert_eq!(kinds::by_name(k.name()), Some(k));
    }
    // Founded: one row per herd at its start, with its animals; minor.
    let founded = events_of(&h, kinds::HERDS_FOUNDED);
    assert_eq!(founded.len(), synth::scenario_herds().len());
    for n in synth::scenario_herds() {
        let id = h.by_name(&n.name).unwrap();
        let e = founded
            .iter()
            .find(|e| e.subject.value == id.to_bits())
            .unwrap();
        assert_eq!((e.tick, e.x, e.z), (0, n.pos.0, n.pos.1));
        assert_eq!((e.magnitude, e.severity), (n.count as i32, Severity::Minor));
        assert_eq!(e.subject.value >> 60, 0, "no owner namespace bit");
    }
    // Migration: legs of the herds, each decision's route in pieces. The last route of the
    // flock is the last legs on its tape.
    let flock = h.by_name("Northdown flock").unwrap();
    let mine: Vec<rtsim_archive::Leg> = rows_on(&h.tape)
        .into_iter()
        .filter_map(|(k, dim, r)| match r {
            Row::Leg(l) if k == kinds::HERDS_MIGRATION && l.subject.value == flock.to_bits() => {
                assert_eq!(dim, 0);
                Some(l)
            }
            _ => None,
        })
        .collect();
    assert!(mine.len() > 20, "{} legs", mine.len());
    assert!(mine.iter().all(|l| l.severity == Severity::Minor));
    let route = &h.herd(flock).unwrap().route;
    let n = route.points.len() - 1;
    assert!(n >= 1, "the flock's last route has a piece");
    for (l, i) in mine[mine.len() - n..].iter().zip(0..) {
        let (a, b) = (route.points[i], route.points[i + 1]);
        assert_eq!((l.x0, l.z0, l.x1, l.z1), (a.0, a.1, b.0, b.1));
        assert_eq!((l.t0, l.end()), (route.times[i].0, route.times[i + 1].0));
    }
    // Starvation: a herd's, with the count that starved, and the ledgers agree.
    let starved = events_of(&h, kinds::HERDS_STARVATION);
    assert!(!starved.is_empty());
    let total: i64 = starved.iter().map(|e| i64::from(e.magnitude)).sum();
    let ledger: u64 = h.herds.iter().map(|(_, x)| x.ledger.starved).sum();
    assert_eq!(total, ledger as i64);
    assert!(
        starved
            .iter()
            .all(|e| e.magnitude > 0 && e.subject.value != 0)
    );

    // A herd of ten, eaten down to its refuge by a predator the engine only knows by its number,
    // then wiped out.
    let mut h = Herds::new(
        4,
        HerdTunables::default(),
        Box::new(Valley::default()),
        Tick::ZERO,
    );
    h.tape.set_on(true);
    let few = h.add_herd(
        NewHerd {
            name: "Few".into(),
            species: rtsim_ecology::herds::HerdSpecies::Goat,
            frame: 0,
            pos: (300, 1_700),
            count: 10,
            seed: Some(6),
        },
        Tick::ZERO,
    );
    let mut index = legs();
    let at = Tick(5_000);
    h.run_until(&mut index, at);
    const WOLF: u64 = 0x1_0000_0007;
    let (px, pz) = h.position(few, at);
    let refuge = h.refuge(10);
    assert_eq!(refuge, 8);
    let start = h.count(few, at);
    assert!(start > refuge, "{start}");
    for _ in refuge..start {
        let p = h.predate(few, px, pz, at, 100, WOLF);
        assert_eq!(p.animals, 1);
    }
    assert_eq!(h.count(few, at), refuge);
    assert_eq!(h.predate(few, px, pz, at, 100, WOLF).animals, 0);
    assert!(h.prey_near(0, px, pz, 500, at, WOLF).is_empty());
    assert_eq!(h.cull(few, at), refuge);
    h.run_until(&mut index, Tick::from_days(10));
    assert_eq!(h.count(few, h.now()), 0);
    let preds = events_of(&h, kinds::HERDS_PREDATION);
    assert_eq!(preds.len() as u64, start - refuge);
    for e in &preds {
        assert_eq!((e.subject.value, e.object.value), (few.to_bits(), WOLF));
        assert_eq!((e.tick, e.magnitude), (at.0, 1));
        assert_eq!((e.x, e.z), (px as i32, pz as i32));
    }
    let extinct = events_of(&h, kinds::HERDS_EXTINCT);
    assert_eq!(extinct.len(), 1);
    let e = extinct[0];
    assert_eq!(e.subject.value, few.to_bits());
    assert_eq!((e.severity, e.payload), (Severity::Notable, true));
    assert!(e.tick >= at.0);
    // Where its route ended.
    let end = h.herd(few).unwrap().route.end();
    assert_eq!((e.x, e.z), end);
}

/// No group outlives a session: at a session start every group is abstract again (no capture
/// came), the players' legs go, and the next approach brings the same animals back.
#[test]
fn a_new_session_makes_groups_abstract_and_the_same_animals_return() {
    let mut r = Rig::valley(23);
    let f = flock(&r);
    let t = Tick::from_days(12);
    r.run(t, 2_000, None);
    let p = r.near(f, t);
    r.run(t + 1_500, 100, Some(p));
    let first = r.fake.group_of(agent_of(f)).cloned().expect("embodied");
    let now = r.h.now();
    let legs_before = r.legs.len();
    r.fake = FakeHerds::default();
    // A new session: the replay begins and, with nothing to replay, ends (docs/replay-gate.md §2.1).
    r.h.begin_replay(true);
    assert!(
        r.h.herd(f).unwrap().embodied.is_some(),
        "the replay's start changes no group (RP-5)"
    );
    r.h.replay_done(&mut r.legs, now);
    assert!(r.h.herd(f).unwrap().embodied.is_none());
    assert!(
        r.legs.len() < legs_before,
        "the session's legs were cancelled"
    );
    r.h.check(now).unwrap();
    let p2 = r.near(f, now + 100);
    r.run(now + 2_000, 100, Some(p2));
    let second = r
        .fake
        .group_of(agent_of(f))
        .cloned()
        .expect("embodied again");
    assert!(second.epoch > first.epoch);
    let a: std::collections::BTreeSet<u64> = first.bodies.iter().map(|b| b.spawn.member).collect();
    let b: std::collections::BTreeSet<u64> = second.bodies.iter().map(|b| b.spawn.member).collect();
    assert!(a.is_subset(&b) || b.is_subset(&a), "{a:?} vs {b:?}");
}

/// The herds as a viewer of the phase 1 world's one interest area per player (the integration's
/// `World::observe_area` and `route_area`): no area of their own, one group per approach, and the
/// group goes home when the player leaves the area.
#[test]
fn the_shared_interest_area_brings_one_group_and_takes_it_back() {
    use rtsim_world::interest::Viewer;
    let seed = 31;
    let (mut w, _) = rtsim_ecology::synth::world(seed);
    let mut h = synth::valley(seed, HerdTunables::default(), Tick::ZERO);
    let mut fake = FakeHerds::default();
    let t = Tick::from_days(10);
    h.run_until(&mut w.legs, t);
    let f = h.by_name("Northdown flock").unwrap();
    let p = {
        let c = h.position(f, t);
        Pos::new(0, c.0 as i32 + 30, c.1 as i32)
    };
    let mut actor = None;
    for k in 0..300u64 {
        let at = t + k * 20;
        let (a, stamp) = w.observe_area(PLAYER, p, at, 160);
        actor = Some(a);
        if let Some(s) = stamp {
            let v: &mut dyn Viewer = &mut h;
            w.route_area(a, s, &mut [v]);
        }
        h.run_until(&mut w.legs, at + 20);
        fake.apply(h.take_commands(), at + 20);
        for o in fake.take_obs() {
            h.ingest(&mut w.legs, &o, at + 20);
        }
    }
    let agent = agent_of(f);
    assert_eq!(fake.history.iter().filter(|x| x.0 == agent).count(), 1);
    assert!(fake.group_of(agent).is_some());
    // The herds booked no player area of their own: theirs is the world's.
    assert_eq!(h.player_area(PLAYER), w.interest_area(actor.unwrap()));
    assert!(h.player_area(PLAYER).is_some());
    // The player leaves the shared area: the group is captured.
    let now = h.now();
    {
        let v: &mut dyn Viewer = &mut h;
        w.leave_area(actor.unwrap(), &mut [v]);
    }
    fake.apply(h.take_commands(), now);
    for o in fake.take_obs() {
        h.ingest(&mut w.legs, &o, now);
    }
    assert!(fake.group_of(agent).is_none());
    assert!(h.herd(f).unwrap().embodied.is_none());
    h.check(now).unwrap();
}

/// Bytes of a paged save made right after another, nothing changed between them, for `n` herds
/// in a valley grown with them, four days in.
fn quiet_save_bytes(n: usize) -> usize {
    let (st, dir) = store(&format!("quiet-{n}"));
    let mut index = legs();
    let mut h = synth::bench_herds_in(3, n, 5_000, Tick::ZERO, synth::wide_valley(n));
    h.run_until(&mut index, Tick::from_days(4));
    h.attach_store(st.clone(), 64 << 20);
    let mut bytes = 0;
    for _ in 0..2 {
        let mut b = SaveBatch::new();
        let t = h.save_paged(&mut b, META);
        bytes = b.raw_bytes();
        st.write().unwrap().commit(b, 0).expect("commit");
        h.saved(t);
    }
    drop(h);
    drop(st);
    let _ = std::fs::remove_dir_all(dir);
    bytes
}

/// A save writes what changed since the last one: with nothing changed, as many bytes at four
/// times the herds (docs/choices-paging-followups.md, the paged wheel). It failed while the meta
/// page held the whole wheel and the claims.
#[test]
fn a_save_with_nothing_changed_is_flat_in_the_herds() {
    let small = quiet_save_bytes(64);
    let big = quiet_save_bytes(256);
    eprintln!("a quiet save: {small} bytes, then {big} at four times the size");
    assert!(
        big <= small + small / 2 + 1024,
        "a save with nothing changed wrote {small} bytes at 64 herds and {big} at 256"
    );
}

// ---- the replay gate (docs/replay-gate.md §9.4, RP-80 to RP-89) -----------------------------------

/// A rig whose flock has a group in view at day 12: the herds as the pages of a save would hold
/// them.
fn with_group(seed: u64) -> (Rig, HerdId, Tick) {
    let mut r = Rig::valley(seed);
    let f = flock(&r);
    let t = Tick::from_days(12);
    r.run(t, 2_000, None);
    let p = r.near(f, t);
    r.run(t + 1_500, 100, Some(p));
    assert!(r.fake.group_of(agent_of(f)).is_some(), "a group in view");
    let now = r.h.now();
    (r, f, now)
}

/// One record of each kind naming `epoch` of `agent`, by name (docs/replay-gate.md §5.3, RP-57).
fn records_naming(agent: u64, epoch: u32) -> Vec<(&'static str, Obs)> {
    use boundary::{
        Dematerialized, GroupState, Killed, KillerKind, Materialized, Sheared, SpawnRejected,
        Taken, TakenHow,
    };
    vec![
        (
            "HERD_GROUP_STATE",
            Obs::GroupState(GroupState {
                agent,
                epoch,
                flags: 0,
                dimension: 0,
                present: 3,
                pos: (0.0, 64.0, 0.0),
            }),
        ),
        (
            "HERD_MATERIALIZED",
            Obs::Materialized(Materialized {
                agent,
                epoch,
                dimension: 0,
                spawned: 3,
            }),
        ),
        (
            "HERD_SPAWN_REJECTED",
            Obs::SpawnRejected(SpawnRejected {
                agent,
                epoch,
                reason: 1,
                cmd_seq: 0,
            }),
        ),
        (
            "HERD_DEMATERIALIZED",
            Obs::Dematerialized(Dematerialized {
                agent,
                epoch,
                reason: 1,
                members: Vec::new(),
            }),
        ),
        (
            "HERD_KILLED",
            Obs::Killed(Killed {
                agent,
                epoch,
                member: 0,
                by: KillerKind::Player,
                killer: PLAYER,
                op: 1 << 62 | 1,
            }),
        ),
        (
            "HERD_SHEARED",
            Obs::Sheared(Sheared {
                agent,
                epoch,
                member: 1,
                player: PLAYER,
            }),
        ),
        (
            "HERD_TAKEN",
            Obs::Taken(Taken {
                agent,
                epoch,
                member: 2,
                how: TakenHow::Lead,
                player: PLAYER,
                op: 1 << 62 | 2,
            }),
        ),
        (
            "HERD_OBSTACLE",
            Obs::Obstacle(boundary::Obstacle {
                agent,
                epoch,
                failures: 1,
                present: 3,
                stuck: (0, 0),
                target: (64, 64),
            }),
        ),
    ]
}

/// The epoch the flock's next group takes, from a rig that has applied a record: the player
/// walks up to the flock.
fn next_epoch(mut r: Rig, f: HerdId, from: Tick) -> u32 {
    let p = r.near(f, from);
    r.run(from + 1_500, 100, Some(p));
    r.fake
        .group_of(agent_of(f))
        .map(|g| g.epoch)
        .expect("a group came")
}

/// No epoch is used twice (RP-7, RP-30, RP-57, RP-81): a replayed record or a copy that names a
/// group's epoch raises the herd's floor, whatever its kind and whatever the record does after,
/// on an engine from its pages, on a fresh one and for a copy. The next group takes an epoch
/// above it, 5 after a record of 4 (the floor sits under the wrap rule's `+ 1`). Before, the
/// engine knew none of them and took 1.
#[test]
fn every_record_naming_an_epoch_raises_its_floor() {
    let mut cases = 0;
    for path in ["pages", "fresh", "copy"] {
        for i in 0..records_naming(0, 4).len() {
            let mut r = Rig::valley(5);
            let f = flock(&r);
            let now = Tick::from_days(12);
            r.run(now, 2_000, None);
            let (name, o) = records_naming(agent_of(f), 4).swap_remove(i);
            match path {
                "pages" | "fresh" => {
                    r.h.begin_replay(path == "pages");
                    r.h.ingest(&mut r.legs, &o, now);
                    r.h.replay_done(&mut r.legs, now);
                }
                _ => r.h.ingest_copy(&mut r.legs, &o, now),
            }
            let _ = r.h.take_commands();
            r.h.check(now).unwrap();
            assert_eq!(next_epoch(r, f, now), 5, "{name} on the {path} path");
            cases += 1;
        }
    }
    assert_eq!(cases, 24);
}

/// A record of an epoch the herd holds raises nothing (its next group is 2 after a group 1 and a
/// record of 1); a record of the highest epoch a group can have gives the highest, not a wrapped
/// one (the wrap rule only turns the one after it).
#[test]
fn the_floor_is_above_the_herds_own_epoch_and_below_the_wrap() {
    let (mut r, f, now) = with_group(23);
    let agent = agent_of(f);
    let first = r.fake.group_of(agent).map(|g| g.epoch).unwrap();
    assert_eq!(first, 1);
    r.h.begin_replay(true);
    for (_, o) in records_naming(agent, 1) {
        if matches!(o, Obs::Dematerialized(_) | Obs::Materialized(_)) {
            r.h.ingest(&mut r.legs, &o, now);
        }
    }
    r.h.replay_done(&mut r.legs, now);
    assert_eq!(r.h.herd(f).unwrap().epoch, 1, "the herd holds epoch 1");
    r.fake = FakeHerds::default();
    assert_eq!(next_epoch(r, f, now), 2);

    let mut r = Rig::valley(5);
    let f = flock(&r);
    let now = Tick::from_days(12);
    r.run(now, 2_000, None);
    let (_, o) = records_naming(agent_of(f), u32::MAX - 1).swap_remove(2);
    r.h.begin_replay(true);
    r.h.ingest(&mut r.legs, &o, now);
    r.h.replay_done(&mut r.legs, now);
    assert_eq!(next_epoch(r, f, now), u32::MAX);
}

/// A floor of an agent with no herd yet stays past the replay end (a herd founded after it, on a
/// handle an old life used, RP-31): its first group takes an epoch above it.
#[test]
fn a_floor_of_an_agent_with_no_herd_stays_past_the_replay_end() {
    let new = || NewHerd {
        name: "Late flock".into(),
        species: rtsim_ecology::herds::HerdSpecies::Sheep,
        frame: 0,
        pos: (300, 1_700),
        count: 60,
        seed: Some(9),
    };
    let now = Tick::from_days(12);
    // The agent the next herd takes, from an engine that is the same up to it.
    let mut probe = Rig::valley(5);
    probe.run(now, 2_000, None);
    let ghost = agent_of(probe.h.add_herd(new(), now));

    let mut r = Rig::valley(5);
    r.run(now, 2_000, None);
    let (_, o) = records_naming(ghost, 7).swap_remove(2);
    r.h.begin_replay(true);
    r.h.ingest(&mut r.legs, &o, now);
    r.h.replay_done(&mut r.legs, now);
    let late = r.h.add_herd(new(), now);
    assert_eq!(agent_of(late), ghost);
    r.run(now + 2_000, 100, None);
    let p = r.near(late, r.h.now());
    r.run(now + 4_000, 100, Some(p));
    let g = r.fake.group_of(ghost).expect("a group of the late flock");
    assert_eq!(g.epoch, 8, "above the floor of the old life's record");
}

/// A command decided while replaying is dropped and counted, not queued (RP-3, RP-80): a group
/// the pages hold in view is steered by an obstacle's replan, and the Steer goes nowhere. Control:
/// the same obstacle with no replay queues it.
#[test]
fn a_steer_decided_while_replaying_is_dropped_and_counted() {
    for replaying in [true, false] {
        let mut r = Rig::valley(17);
        let f = flock(&r);
        let mut t = Tick::from_days(4);
        r.run(t, 2_000, None);
        while r.h.herd(f).unwrap().route.segment_at(t + 2_500).is_none() {
            t += 6_000;
            r.run(t, 6_000, None);
        }
        let p = r.near(f, t);
        r.run(t + 2_000, 100, Some(p));
        let agent = agent_of(f);
        let g = r.fake.group_of(agent).cloned().expect("embodied");
        let now = r.h.now();
        let pos = r.h.position(f, now);
        let target = r.h.herd(f).unwrap().route.heading(now);
        let o = Obs::Obstacle(boundary::Obstacle {
            agent,
            epoch: g.epoch,
            failures: g.bodies.len() as u32,
            present: g.bodies.len() as u32,
            stuck: (pos.0 as i32, pos.1 as i32),
            target,
        });
        if replaying {
            r.h.begin_replay(true);
        }
        r.h.ingest(&mut r.legs, &o, now);
        let cmds = r.h.take_commands();
        let counts = r.h.replay_gate_counts();
        if replaying {
            assert!(cmds.is_empty(), "{cmds:?}");
            assert_eq!(counts.dropped, 1, "the steer was dropped");
            assert_eq!((counts.never, counts.deferred), (0, 0));
            // The replay end makes the group abstract; what is decided after it is sent.
            r.h.replay_done(&mut r.legs, now);
            assert!(r.h.herd(f).unwrap().embodied.is_none());
        } else {
            assert!(cmds.iter().any(|c| matches!(c, Cmd::Steer(_))), "{cmds:?}");
            assert_eq!(counts.dropped, 0);
        }
    }
}

/// Nothing embodies while replaying (RP-3, RP-83): a meeting booked before the session (the pages'
/// wheel holds the sighting) falls due with the area still live, and the herd stays abstract and
/// consistent. The sighting the replay end routes books its own meeting. Control: with no replay
/// the same meeting embodies a group.
#[test]
fn a_meeting_from_the_pages_embodies_nothing_while_replaying() {
    for replaying in [true, false] {
        let mut r = Rig::valley(23);
        let f = flock(&r);
        let t = Tick::from_days(12);
        r.run(t, 2_000, None);
        let p = r.near(f, t);
        // The player's area and its meeting with the flock are booked; the sighting is not due.
        r.h.observe_player(&mut r.legs, PLAYER, p, t);
        if replaying {
            r.h.begin_replay(true);
        }
        r.h.run_until(&mut r.legs, t + 1_500);
        r.pump(t + 1_500);
        r.h.check(t + 1_500).unwrap();
        let counts = r.h.replay_gate_counts();
        if replaying {
            assert!(r.fake.history.is_empty(), "nothing materialized");
            assert!(r.h.herd(f).unwrap().embodied.is_none());
            assert_eq!(r.h.stats.seen, 0, "the sighting did nothing");
            assert!(counts.deferred >= 1, "{counts:?}");
            assert_eq!(counts.never, 0);
            r.h.replay_done(&mut r.legs, t + 1_500);
            // The player is routed again at the replay end: a new meeting, a group.
            let p2 = r.near(f, t + 1_500);
            r.run(t + 3_500, 100, Some(p2));
            assert_eq!(r.fake.history.len(), 1, "one group after the replay end");
        } else {
            assert_eq!(r.fake.history.len(), 1, "the control embodies");
            assert_eq!(counts.deferred, 0);
        }
    }
}

/// The players' puppet counts end at the replay end (RP-26, RP-86), not at its start: the start
/// changes no group (RP-5). Control: a group in view at the start still has its puppets then.
#[test]
fn the_puppet_counts_and_the_players_areas_end_at_the_replay_end() {
    let (mut r, f, now) = with_group(23);
    let legs_before = r.legs.len();
    let puppets = r.h.puppets(PLAYER);
    assert!(puppets > 0);
    assert!(r.h.player_area(PLAYER).is_some());
    r.h.begin_replay(true);
    assert_eq!(r.h.puppets(PLAYER), puppets, "the start changes no count");
    assert!(r.h.herd(f).unwrap().embodied.is_some());
    assert!(r.h.player_area(PLAYER).is_some());
    assert_eq!(r.legs.len(), legs_before);
    r.h.replay_done(&mut r.legs, now);
    assert_eq!(r.h.puppets(PLAYER), 0);
    assert!(r.h.player_area(PLAYER).is_none());
    assert!(r.h.herd(f).unwrap().embodied.is_none());
    assert!(r.legs.len() < legs_before);
    r.h.check(now).unwrap();
}

/// A replayed HERD_MATERIALIZED is honoured only on an engine from its pages and only while
/// replaying (RP-31, RP-82): the herd is embodied at the record's epoch with its picked members,
/// no player, and nothing is sent or counted as a puppet. The replayed capture that follows lands
/// on those members (the herd's seen map). Control: the other three paths honour nothing, and the
/// same capture is late.
#[test]
fn a_replayed_materialization_is_honoured_only_on_an_engine_from_its_pages() {
    use rtsim_ecology::herds::herd::NO_PLAYER;
    // (path, from the pages, replaying, a copy, honoured)
    for (path, from_pages, replaying, copy, honoured) in [
        ("pages", true, true, false, true),
        ("fresh", false, true, false, false),
        ("live", true, false, false, false),
        ("copy", true, true, true, false),
    ] {
        let mut r = Rig::valley(5);
        let f = flock(&r);
        let now = Tick::from_days(12);
        r.run(now, 2_000, None);
        if replaying {
            r.h.begin_replay(from_pages);
        }
        let m = Obs::Materialized(boundary::Materialized {
            agent: agent_of(f),
            epoch: 1,
            dimension: 0,
            spawned: 3,
        });
        if copy {
            r.h.ingest_copy(&mut r.legs, &m, now);
        } else {
            r.h.ingest(&mut r.legs, &m, now);
        }
        assert!(r.h.take_commands().is_empty(), "{path}: nothing is sent");
        assert_eq!(r.h.puppets(PLAYER), 0, "{path}");
        r.h.check(now).unwrap();
        let x = r.h.herd(f).unwrap();
        assert_eq!(x.embodied.is_some(), honoured, "{path}");
        if !honoured {
            assert_eq!(r.h.replay_honoured(), 0, "{path}");
            continue;
        }
        let e = x.embodied.clone().unwrap();
        assert_eq!((e.epoch, e.members.len(), e.player), (1, 3, NO_PLAYER));
        assert_eq!(r.h.replay_honoured(), 1);
        // Its capture, replayed: the members come back seen at the record's time.
        let later = now + 500;
        let caps: Vec<boundary::Capture> = e
            .members
            .iter()
            .map(|&member| boundary::Capture {
                member,
                flags: boundary::MEMBER_SHEARED,
                health: 8.0,
                x: 10,
                z: 20,
            })
            .collect();
        let d = Obs::Dematerialized(boundary::Dematerialized {
            agent: agent_of(f),
            epoch: 1,
            reason: 1,
            members: caps,
        });
        r.h.ingest(&mut r.legs, &d, later);
        assert_eq!(r.h.stats.captures, 1);
        let x = r.h.herd(f).unwrap();
        assert!(x.embodied.is_none());
        for k in &e.members {
            let s = x.seen.get(k).expect("seen");
            assert_eq!((s.last, s.sheared_at), (later, Some(later)));
        }
        r.h.check(later).unwrap();
    }
}

/// Honouring edges (RP-82): a record of an epoch at or below the group the pages hold changes
/// nothing; a newer one replaces it (the older group's end was not journaled), its members stay
/// seen; and a group still honoured at the replay end becomes abstract with its members seen.
#[test]
fn a_replayed_materialization_of_an_older_epoch_changes_nothing_and_a_newer_one_replaces() {
    let (mut r, f, now) = with_group(23);
    let agent = agent_of(f);
    let held = r.h.herd(f).unwrap().embodied.clone().unwrap();
    assert_eq!(held.epoch, 1);
    r.h.begin_replay(true);
    let m = |epoch, spawned| {
        Obs::Materialized(boundary::Materialized {
            agent,
            epoch,
            dimension: 0,
            spawned,
        })
    };
    r.h.ingest(&mut r.legs, &m(1, 5), now);
    assert_eq!(r.h.herd(f).unwrap().embodied.as_ref(), Some(&held));
    assert_eq!(r.h.replay_honoured(), 0);
    r.h.ingest(&mut r.legs, &m(2, 4), now + 10);
    let e = r.h.herd(f).unwrap().embodied.clone().unwrap();
    assert_eq!((e.epoch, e.members.len()), (2, 4));
    assert_eq!(r.h.replay_honoured(), 1);
    r.h.check(now + 10).unwrap();
    let later = now + 600;
    r.h.replay_done(&mut r.legs, later);
    let x = r.h.herd(f).unwrap();
    assert!(x.embodied.is_none());
    for k in &e.members {
        assert_eq!(x.seen.get(k).map(|s| s.last), Some(later));
    }
    assert_eq!(x.epoch, 2);
    r.h.check(later).unwrap();
}

/// RP-82: the honour is above the herd's own epoch, not only above the group the pages hold. A
/// pages state need not be a prefix of the journal: a herd abstract at epoch 7 has used 5 already,
/// and a replayed record of 5 is history. Control: a record of 8 on the same herd is honoured.
#[test]
fn a_replayed_materialization_at_or_below_the_herds_own_epoch_is_not_honoured() {
    for (epoch, honoured) in [(5, false), (7, false), (8, true)] {
        let mut r = Rig::valley(5);
        let f = flock(&r);
        let agent = agent_of(f);
        let now = Tick::from_days(12);
        r.run(now, 2_000, None);
        // The pages' herd: abstract, its epoch 7 (an earlier replay's floor, folded at its end).
        let (_, o) = records_naming(agent, 7).swap_remove(2);
        r.h.begin_replay(false);
        r.h.ingest(&mut r.legs, &o, now);
        r.h.replay_done(&mut r.legs, now);
        let x = r.h.herd(f).unwrap();
        assert_eq!((x.epoch, x.embodied.is_some()), (7, false));
        // Then a replay from those pages meets a record of `epoch`.
        r.h.begin_replay(true);
        let m = Obs::Materialized(boundary::Materialized {
            agent,
            epoch,
            dimension: 0,
            spawned: 3,
        });
        r.h.ingest(&mut r.legs, &m, now + 10);
        let x = r.h.herd(f).unwrap();
        assert_eq!(x.embodied.is_some(), honoured, "a record of {epoch}");
        assert_eq!(
            r.h.replay_honoured(),
            u64::from(honoured),
            "a record of {epoch}"
        );
        if honoured {
            assert_eq!(x.embodied.as_ref().map(|e| e.epoch), Some(8));
        }
        r.h.check(now + 10).unwrap();
        r.h.replay_done(&mut r.legs, now + 20);
        assert!(r.h.herd(f).unwrap().embodied.is_none());
        assert_eq!(r.h.herd(f).unwrap().epoch, 7.max(epoch));
        r.h.check(now + 20).unwrap();
    }
}

/// An ending epoch is never reused (RP-7, RP-81): a group that ended in the replayed window (its
/// materialization and its capture both replayed, on an engine whose pages did not have it) is
/// not decided again after the replay end. Control: a group the pages held, ended in the window,
/// takes the next epoch too.
#[test]
fn an_epoch_that_ended_in_the_replayed_window_is_not_taken_again() {
    // The pages hold nothing of the group.
    let mut r = Rig::valley(5);
    let f = flock(&r);
    let now = Tick::from_days(12);
    r.run(now, 2_000, None);
    let agent = agent_of(f);
    r.h.begin_replay(true);
    let m = Obs::Materialized(boundary::Materialized {
        agent,
        epoch: 1,
        dimension: 0,
        spawned: 3,
    });
    r.h.ingest(&mut r.legs, &m, now + 10);
    let d = Obs::Dematerialized(boundary::Dematerialized {
        agent,
        epoch: 1,
        reason: 1,
        members: Vec::new(),
    });
    r.h.ingest(&mut r.legs, &d, now + 20);
    assert!(r.h.herd(f).unwrap().embodied.is_none(), "the group ended");
    r.h.replay_done(&mut r.legs, now + 30);
    assert_eq!(next_epoch(r, f, now + 30), 2);

    // The pages held it.
    let (mut r, f, now) = with_group(23);
    let agent = agent_of(f);
    r.h.begin_replay(true);
    let d = Obs::Dematerialized(boundary::Dematerialized {
        agent,
        epoch: 1,
        reason: 1,
        members: Vec::new(),
    });
    r.h.ingest(&mut r.legs, &d, now + 20);
    r.h.replay_done(&mut r.legs, now + 30);
    r.fake = FakeHerds::default();
    assert_eq!(next_epoch(r, f, now + 30), 2);
}
