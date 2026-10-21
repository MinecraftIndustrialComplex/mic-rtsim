// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dragon tribute (SINK.md §4.5, §4.6, §6 walkthrough B): a lair dragon's raid makes a works
//! village its lair's tributary, which pays at once and then every season in whole coins, until
//! its purse runs dry or the lair stands empty. The coin goes to the lair's chest in stacks; a
//! paying village is left alone by its lair's dragons; golems frighten dragons; every coin is in
//! the books; and the tribute survives a restart and paging exactly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rtsim_ecology::boundary::{LairChest, Obs};
use rtsim_ecology::dragon::{LairId, Target};
use rtsim_ecology::engine::{Dragons, TRIBUTE_STACK};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::prey::Item;
use rtsim_ecology::synth::{self, LairSpec};
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rng::fnv1a64;
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::works::synthetic_catalog;
use rtsim_world::{World, WorldConfig};

/// Ashpeak Roost: two fire dragons, at (1000, 1100).
const ASHPEAK: u64 = 101;
/// Old Scorch Roost: nobody lives there.
const OLD_SCORCH: u64 = 105;
/// Ashby, 500 blocks from Ashpeak.
const ASHBY: u64 = 4;

fn spec(key: u64) -> LairSpec {
    synth::lairs()
        .into_iter()
        .find(|l| l.key == key)
        .expect("a scenario lair")
}

/// The scenario world with the works catalogue, and the given lairs.
fn start(seed: u64, lairs: &[LairSpec]) -> (World, Dragons) {
    let (mut w, _) = synth::world(seed);
    let cat = synthetic_catalog(&w.goods);
    w.set_works(cat, Tick::ZERO);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    synth::populate(&mut w, &mut d, lairs, Tick::ZERO);
    (w, d)
}

fn node(w: &World, key: u64) -> usize {
    w.node_of_key(key).expect("a scenario village")
}

fn lair(d: &Dragons, key: u64) -> LairId {
    d.lair_of_key(key).expect("a scenario lair")
}

fn emerald(w: &World) -> GoodId {
    w.goods.find("emerald").expect("the human coin")
}

/// Village `village` is raided by lair `lair`'s dragon at `at` with `coins` in its purse: the
/// raid keeps the purse for the lair, the grant fills it, and the tribute starts.
fn tributary(
    w: &mut World,
    d: &mut Dragons,
    village: u64,
    lair: u64,
    coins: i64,
    at: Tick,
) -> bool {
    d.run_until(w, &mut NoInfrastructure, at);
    let i = node(w, village);
    w.village_raided(i, true, at);
    if coins > 0 {
        assert!(w.grant_purse(i, coins * MILLI, at));
    }
    d.tribute_start(w, &mut NoInfrastructure, village, lair, at)
}

fn tribute_paid(w: &World, i: usize, g: GoodId) -> i64 {
    w.works_of(i).map_or(0, |s| s.tribute(g))
}

fn purse(w: &World, i: usize, g: GoodId) -> i64 {
    w.works_purse(i, w.now())
        .into_iter()
        .find(|e| e.0 == g)
        .map_or(0, |e| e.1)
}

/// A season's payment out of a purse that held `before`: a quarter in whole coins, at least one.
fn share(before: i64) -> i64 {
    (before * 250 / 1000 / MILLI).max(1) * MILLI
}

#[test]
fn a_raid_pays_at_once_and_then_every_season_in_whole_coins() {
    let (mut w, mut d) = start(7, &[spec(ASHPEAK)]);
    let (i, l, g) = (node(&w, ASHBY), lair(&d, ASHPEAK), emerald(&w));
    let t0 = Tick(TICKS_PER_DAY + 123);
    let hoard0 = d
        .lairs
        .get(l)
        .unwrap()
        .hoard
        .get(&Item::World(g))
        .copied()
        .unwrap_or(0);
    assert!(tributary(&mut w, &mut d, ASHBY, ASHPEAK, 40, t0));
    let row = d.tribute_row(i).expect("Ashby pays Ashpeak");
    assert_eq!((row.lair, row.paid_at), (l, Some(t0)));
    assert_eq!(
        tribute_paid(&w, i, g),
        10 * MILLI,
        "a quarter of 40 coins at the raid"
    );
    let x = d.lairs.get(l).unwrap();
    assert_eq!(x.hoard[&Item::World(g)] - hoard0, 10 * MILLI);
    assert_eq!(
        x.unplaced.get(&Item::World(g)),
        Some(&10),
        "whole coins, no chest yet"
    );
    assert!(d.placing().next().is_none());
    d.check(&w, None).unwrap();

    let mut at = t0;
    let mut paid = tribute_paid(&w, i, g);
    for season in 1..=4 {
        let next = d.tun.calendar.next_change(at);
        d.run_until(&mut w, &mut NoInfrastructure, Tick(next.0 - 1));
        assert_eq!(
            tribute_paid(&w, i, g),
            paid,
            "nothing before the season changes"
        );
        d.run_until(&mut w, &mut NoInfrastructure, next);
        let row = d.tribute_row(i).expect("still paying");
        assert_eq!(row.paid_at, Some(next), "season {season}");
        let now = tribute_paid(&w, i, g);
        let a = now - paid;
        assert_eq!(a % MILLI, 0, "whole coins");
        assert_eq!(
            a,
            share(a + purse(&w, i, g)),
            "a quarter of the purse, season {season}"
        );
        // Ashpeak is the only lair: all the tribute in the books is its, and none is placed.
        let x = d.lairs.get(l).unwrap();
        let booked = d.ledger.tribute[&Item::World(g)];
        assert_eq!(
            i128::from(x.unplaced[&Item::World(g)]) * i128::from(MILLI),
            booked
        );
        assert!(booked >= i128::from(now));
        d.check(&w, None).unwrap();
        (at, paid) = (next, now);
    }
    assert!(d.stats.tributes >= 5);
}

#[test]
fn a_dry_purse_lapses_at_the_raid_and_at_the_season() {
    let (mut w, mut d) = start(7, &[spec(ASHPEAK)]);
    let (i, g) = (node(&w, ASHBY), emerald(&w));
    // Both raids in one instant, so no hunt of Ashpeak's own comes between them.
    let t0 = Tick::from_days(1);
    // Nothing in the purse: the tribute lapses at once, and the village buys again, golems first.
    let lapses = d.stats.tribute_lapses;
    assert!(!tributary(&mut w, &mut d, ASHBY, ASHPEAK, 0, t0));
    assert!(d.tribute_row(i).is_none());
    assert_eq!(d.stats.tribute_lapses, lapses + 1);
    let s = w.works_of(i).unwrap();
    assert!(!s.tributary);
    assert_eq!(s.threat_at, Some(t0));
    // One coin: paid at the raid; the next season finds the purse dry.
    let t1 = t0;
    assert!(tributary(&mut w, &mut d, ASHBY, ASHPEAK, 1, t1));
    assert_eq!(tribute_paid(&w, i, g), MILLI);
    assert_eq!(purse(&w, i, g), 0);
    let next = d.tun.calendar.next_change(t1);
    d.run_until(&mut w, &mut NoInfrastructure, next);
    assert!(d.tribute_row(i).is_none(), "lapsed");
    assert!(
        d.stats.tribute_lapses >= lapses + 2,
        "Ashpeak's dragons raid dry villages too"
    );
    let s = w.works_of(i).unwrap();
    assert!(!s.tributary);
    assert_eq!(s.threat_at, Some(next));
    assert_eq!(tribute_paid(&w, i, g), MILLI, "nothing more paid");
    d.check(&w, None).unwrap();
}

#[test]
fn a_vacant_lair_loses_its_tribute() {
    // No dragons at all, so no raid takes the village from Old Scorch.
    let (mut w, mut d) = start(7, &[]);
    // Old Scorch joins on day 10: nobody comes to it before its respawn on day 30.
    let joined = Tick::from_days(10);
    d.run_until(&mut w, &mut NoInfrastructure, joined);
    synth::populate(&mut w, &mut d, &[spec(OLD_SCORCH)], joined);
    let (i, l, g) = (node(&w, ASHBY), lair(&d, OLD_SCORCH), emerald(&w));
    let t0 = Tick::from_days(11);
    assert!(tributary(&mut w, &mut d, ASHBY, OLD_SCORCH, 20, t0));
    assert_eq!(
        tribute_paid(&w, i, g),
        5 * MILLI,
        "the first payment at the raid"
    );
    let next = d.tun.calendar.next_change(t0);
    d.run_until(&mut w, &mut NoInfrastructure, Tick(next.0 - 1));
    let x = d.lairs.get(l).unwrap();
    assert!(
        x.occupants.is_empty() && x.vacant_since.is_some(),
        "still empty"
    );
    d.run_until(&mut w, &mut NoInfrastructure, next);
    assert!(d.tribute_row(i).is_none(), "lapsed");
    assert!(!w.works_of(i).unwrap().tributary);
    assert_eq!(
        tribute_paid(&w, i, g),
        5 * MILLI,
        "nothing paid to an empty lair"
    );
    assert_eq!(
        d.lairs.get(l).unwrap().unplaced[&Item::World(g)],
        5,
        "the coin it had stays"
    );
    d.check(&w, None).unwrap();
}

type DragonId = rtsim_ecology::dragon::DragonId;

/// The utility `dragon` sees in village `node` at `at`, if it weighs it at all.
fn village_utility(d: &Dragons, w: &World, dragon: DragonId, node: usize, at: Tick) -> Option<f64> {
    d.weigh(w, dragon, at)
        .into_iter()
        .find(|(t, _, _)| *t == Target::Village { node: node as u32 })
        .map(|e| e.2)
}

/// The villages `dragon` weighs at `at`.
fn villages_weighed(d: &Dragons, w: &World, dragon: DragonId, at: Tick) -> Vec<usize> {
    d.weigh(w, dragon, at)
        .into_iter()
        .filter_map(|(t, _, _)| match t {
            Target::Village { node } => Some(node as usize),
            _ => None,
        })
        .collect()
}

#[test]
fn a_paying_village_gives_its_lairs_dragons_nothing_to_hunt() {
    let (mut w, mut d) = start(7, &[spec(ASHPEAK), spec(102)]);
    let t0 = Tick(TICKS_PER_DAY / 4);
    d.run_until(&mut w, &mut NoInfrastructure, t0);
    let cinderjaw = d.by_name("Cinderjaw").unwrap();
    let frostmaw = d.by_name("Frostmaw").unwrap();
    let villages = villages_weighed(&d, &w, cinderjaw, t0);
    assert!(
        villages.len() >= 2,
        "Cinderjaw weighs villages: {villages:?}"
    );
    let theirs = villages_weighed(&d, &w, frostmaw, t0);
    let i = villages[0];
    let key = w.nodes[i].key;
    assert!(tributary(&mut w, &mut d, key, ASHPEAK, 40, t0));
    assert_eq!(
        village_utility(&d, &w, cinderjaw, i, t0),
        None,
        "the tributary is left alone"
    );
    for &v in &villages[1..] {
        assert!(
            village_utility(&d, &w, cinderjaw, v, t0).is_some(),
            "the others are not"
        );
    }
    // Another lair's dragon weighs what it weighed: the tribute buys only its own lair's peace.
    assert_eq!(villages_weighed(&d, &w, frostmaw, t0), theirs);
    // At the season's change it pays again, and is left alone for another season.
    let next = d.tun.calendar.next_change(t0);
    d.run_until(&mut w, &mut NoInfrastructure, next);
    assert_eq!(d.tribute_row(i).and_then(|r| r.paid_at), Some(next));
    assert_eq!(village_utility(&d, &w, cinderjaw, i, next), None);
}

#[test]
fn golems_raise_a_villages_risk() {
    let (mut w, mut d) = start(7, &[spec(ASHPEAK)]);
    let t0 = Tick(TICKS_PER_DAY / 4);
    d.run_until(&mut w, &mut NoInfrastructure, t0);
    let cinderjaw = d.by_name("Cinderjaw").unwrap();
    let (i, _) = d
        .weigh(&w, cinderjaw, t0)
        .into_iter()
        .find_map(|(t, _, u)| match t {
            Target::Village { node } => Some((node as usize, u)),
            _ => None,
        })
        .expect("Cinderjaw weighs a village");
    let tun = d.tun.clone();
    assert_eq!(d.village_risk(&w, i), i64::from(tun.village_risk));
    let before = village_utility(&d, &w, cinderjaw, i, t0).unwrap();
    w.set_golems(i, 3);
    assert_eq!(
        d.village_risk(&w, i),
        i64::from(tun.village_risk) + 3 * i64::from(tun.golem_risk)
    );
    let bold = f64::from(d.dragon(cinderjaw).unwrap().temperament().bold);
    let bend = (2.0 - bold / 128.0).max(0.0);
    let after = village_utility(&d, &w, cinderjaw, i, t0).unwrap();
    let fall = before - after;
    assert!(
        (fall - 3.0 * f64::from(tun.golem_risk) * bend).abs() < 1e-9,
        "fall {fall}, bend {bend}"
    );
    // A village without works (the player's base is none) has the base risk.
    let base = node(&w, synth::PLAYER_BASE_KEY);
    assert_eq!(d.village_risk(&w, base), i64::from(tun.village_risk));
}

fn chest_of(key: u64) -> Obs {
    let s = spec(key);
    Obs::LairChest(LairChest {
        lair: key,
        dimension: 0,
        pos: (s.x + 2, s.y, s.z - 1),
    })
}

#[test]
fn tribute_goes_to_the_lairs_chest_in_stacks() {
    let run = || {
        let (mut w, mut d) = start(7, &[spec(ASHPEAK)]);
        let (l, g) = (lair(&d, ASHPEAK), emerald(&w));
        let t0 = Tick::from_days(1);
        assert!(tributary(&mut w, &mut d, ASHBY, ASHPEAK, 1_000, t0));
        assert_eq!(d.lairs.get(l).unwrap().unplaced[&Item::World(g)], 250);
        d.ingest(&mut w, &mut NoInfrastructure, &chest_of(ASHPEAK), t0);
        assert_eq!(d.chest(l).map(|c| c.pos), Some([1002, 110, 1099]));
        assert!(d.lairs.get(l).unwrap().unplaced.is_empty(), "all sent");
        let mut stacks: Vec<(u64, i64)> = d.placing().map(|(op, p)| (op, p.count)).collect();
        assert_eq!(stacks.len(), 4);
        assert!(
            stacks
                .iter()
                .all(|&(op, n)| op != 0 && op < 1 << 55 && n <= TRIBUTE_STACK)
        );
        assert_eq!(stacks.iter().map(|s| s.1).sum::<i64>(), 250);
        d.check(&w, None).unwrap();
        stacks.sort_by_key(|s| s.0);
        let (a, b) = (stacks[0], stacks[1]);
        assert!(d.tribute_placed(a.0));
        assert!(!d.tribute_placed(a.0), "answered once");
        assert_eq!(d.lairs.get(l).unwrap().placed, a.1);
        assert_eq!(d.stats.tribute_stacks, 1);
        assert!(d.tribute_failed(b.0));
        assert_eq!(d.chest(l), None, "a failure forgets the chest");
        assert_eq!(d.lairs.get(l).unwrap().unplaced[&Item::World(g)], b.1);
        d.check(&w, None).unwrap();
        // Reported again: the failed coin goes as a new stack.
        let t1 = Tick(t0.0 + 50);
        d.ingest(&mut w, &mut NoInfrastructure, &chest_of(ASHPEAK), t1);
        let again: Vec<(u64, i64)> = d.placing().map(|(op, p)| (op, p.count)).collect();
        assert_eq!(again.len(), 3);
        assert!(again.iter().all(|s| s.0 != a.0 && s.0 != b.0));
        assert!(again.iter().any(|s| s.1 == b.1));
        for (op, _) in &again {
            assert!(d.tribute_placed(*op));
        }
        assert_eq!(d.lairs.get(l).unwrap().placed, 250);
        // The chest is this session's only.
        d.session_start(&mut w, t1);
        assert_eq!(d.chest(l), None);
        d.check(&w, None).unwrap();
        (stacks, again, d.state_hash())
    };
    assert_eq!(run(), run(), "the same op ids every run");
}

// ---- a long run: conservation, restarts, paging ---------------------------------------------

const META: PageKey = PageKey::new(0x4400, 0);
/// A row or two: lair and dragon pages evict and load back all the time.
const BUDGET: usize = 2 << 10;
const DAYS: u64 = 120;

struct Run {
    w: World,
    d: Dragons,
    store: Option<Arc<RwLock<PageStore>>>,
}

fn scenario(seed: u64) -> Run {
    let (w, d) = start(seed, &synth::lairs());
    Run { w, d, store: None }
}

/// One day: the dragons hunt (their raids make villages tributaries), every tenth day each
/// village is granted coin, the plugin reports every lair's chest and answers the stacks (every
/// third fails, so its chest is forgotten until the next day's report).
fn day(r: &mut Run, day: u64) {
    let t = Tick::from_days(day);
    r.d.run_until(&mut r.w, &mut NoInfrastructure, t);
    if day % 10 == 1 {
        for i in 0..r.w.nodes.len() {
            r.w.grant_purse(i, 30 * MILLI, t);
        }
    }
    for s in synth::lairs() {
        r.d.ingest(&mut r.w, &mut NoInfrastructure, &chest_of(s.key), t);
    }
    let ops: Vec<u64> = r.d.placing().map(|(op, _)| op).collect();
    for op in ops {
        if op % 3 == 0 {
            assert!(r.d.tribute_failed(op));
        } else {
            assert!(r.d.tribute_placed(op));
        }
    }
    let _ = r.d.take_commands();
    let _ = r.w.take_commands();
    if let Some(store) = &r.store {
        let mut b = SaveBatch::new();
        let tickets = r.d.save_paged(&mut b, META);
        store
            .write()
            .unwrap()
            .commit(b, 0)
            .expect("the save commits");
        r.d.saved(tickets);
        r.d.enforce_budget();
    }
}

fn restart(r: &Run, seed: u64) -> Run {
    let (goods, ids) = synth::goods();
    let w = World::load(
        WorldConfig::new(seed),
        goods,
        synth::recipes(&ids),
        &r.w.save(),
    )
    .expect("the world loads");
    let d = Dragons::load(DragonTunables::default(), &r.d.save()).expect("the dragons load");
    Run { w, d, store: None }
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rtsim-tribute-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn page(r: &mut Run, dir: &Path) {
    let store = Arc::new(RwLock::new(
        PageStore::open(dir, Durability::NoSync).expect("the store opens"),
    ));
    r.d.attach_store(store.clone(), BUDGET);
    r.store = Some(store);
}

#[test]
fn every_coin_of_tribute_is_in_the_books() {
    let mut r = scenario(11);
    for k in 1..=DAYS {
        day(&mut r, k);
        r.d.check(&r.w, None)
            .unwrap_or_else(|e| panic!("day {k}: {e}"));
    }
    let s = r.d.stats;
    assert!(
        s.tributes >= 5 && s.tribute_lapses > 0 && s.tribute_stacks > 0,
        "{s:?}"
    );
    let paid: i128 = r.w.works_totals().1.iter().map(|e| e.1).sum();
    assert!(paid > 0);
    let placed: i64 = r.d.lairs.iter().map(|l| l.placed).sum();
    assert!(placed > 0, "coin reached the chests");
}

#[test]
fn tribute_continues_exactly_after_a_restart_and_when_paged() {
    let seed = 11;
    let (dir_b, dir_c) = (tmpdir("b"), tmpdir("c"));
    let mut a = scenario(seed);
    let mut b = scenario(seed);
    page(&mut b, &dir_b);
    let cut = DAYS / 2;
    let mut c: Option<Run> = None;
    let mut e: Option<Run> = None;
    for k in 1..=DAYS {
        if k == cut {
            // A restart is a session start: the plugin's chests are reported again.
            let mut re = restart(&a, seed);
            assert_eq!(re.d.state_hash(), a.d.state_hash());
            let t = Tick::from_days(k - 1);
            re.d.session_start(&mut re.w, t);
            a.d.session_start(&mut a.w, t);
            b.d.session_start(&mut b.w, t);
            e = Some(re);
            // A second engine from (a copy of) the paged store, with nothing resident.
            std::fs::create_dir_all(&dir_c).unwrap();
            for f in std::fs::read_dir(&dir_b).unwrap() {
                let f = f.unwrap();
                if f.file_type().unwrap().is_file() {
                    std::fs::copy(f.path(), dir_c.join(f.file_name())).unwrap();
                }
            }
            let store = Arc::new(RwLock::new(
                PageStore::open(&dir_c, Durability::NoSync).expect("the copy opens"),
            ));
            let mut d = Dragons::open_paged(store.clone(), BUDGET, META, DragonTunables::default())
                .expect("the dragons open from their pages");
            let (goods, ids) = synth::goods();
            let mut w = World::load(
                WorldConfig::new(seed),
                goods,
                synth::recipes(&ids),
                &b.w.save(),
            )
            .expect("the world loads");
            d.session_start(&mut w, t);
            assert_eq!(d.state_hash(), a.d.state_hash(), "opened from its pages");
            c = Some(Run {
                w,
                d,
                store: Some(store),
            });
        }
        day(&mut a, k);
        day(&mut b, k);
        let h = a.d.state_hash();
        assert_eq!(h, b.d.state_hash(), "paged, day {k}");
        assert_eq!(a.w.state_hash(), b.w.state_hash(), "paged world, day {k}");
        for (what, r) in [("restarted", e.as_mut()), ("opened", c.as_mut())] {
            if let Some(r) = r {
                day(r, k);
                assert_eq!(r.d.state_hash(), h, "{what}, day {k}");
                assert_eq!(r.w.state_hash(), a.w.state_hash(), "{what} world, day {k}");
            }
        }
    }
    assert!(a.d.stats.tributes > 0 && a.d.stats.tribute_stacks > 0);
    let p = b.d.paging_stats();
    assert!(p.evictions() > 0 && p.loads() > 0, "{p:?}");
    let save = fnv1a64(&a.d.save());
    assert_eq!(save, fnv1a64(&b.d.save()));
    assert_eq!(save, fnv1a64(&e.as_ref().unwrap().d.save()));
    assert_eq!(save, fnv1a64(&c.as_ref().unwrap().d.save()));
    assert_eq!(a.d.stats, b.d.stats);
    drop((b, c));
    let _ = std::fs::remove_dir_all(&dir_b);
    let _ = std::fs::remove_dir_all(&dir_c);
}

#[test]
fn tribute_is_deterministic() {
    let go = |seed| {
        let mut r = scenario(seed);
        for k in 1..=60 {
            day(&mut r, k);
        }
        (
            r.d.state_hash(),
            r.w.state_hash(),
            fnv1a64(&r.d.save()),
            r.d.stats,
        )
    };
    assert_eq!(go(5), go(5));
}
