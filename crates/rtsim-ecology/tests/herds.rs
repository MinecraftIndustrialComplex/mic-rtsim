// SPDX-License-Identifier: AGPL-3.0-or-later
//! Migrating herds (PLAN.md §10.2): conservation, identity after deaths, flat catch-up,
//! composition, one materialization per approach, restore, the fence, and dragons as predators.

use proptest::prelude::*;
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
    let mut h = Herds::new(
        seed,
        HerdTunables::default(),
        Box::new(Valley::default()),
        Tick::ZERO,
    );
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
    r.h.session_start(&mut r.legs, now);
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
    let mut legs = legs();
    let mut h = synth::bench_herds_in(3, n, 5_000, Tick::ZERO, synth::wide_valley(n));
    h.run_until(&mut legs, Tick::from_days(4));
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
