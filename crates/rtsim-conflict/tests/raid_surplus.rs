// SPDX-License-Identifier: AGPL-3.0-or-later
//! A raid on a village or a rival's camp takes from the surplus only (CONFLICT-LIFE.md CL-D58, as
//! the ports' PC72 and PC78 in docs/choices-ports.md): at most the culture's `loot_share_milli` of
//! each line's surplus over its target, coins included, and no fight when the band could not carry
//! off one whole item. A line taken below its target needs imports or a mint to refill, and the
//! quiet economy sends neither, so whole-stock raids drained the villages for good.

use std::collections::BTreeMap;

use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::engine::{Conflict, Party, Phase, Target};
use rtsim_conflict::{Step, synth};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_settle::NodeKind;
use rtsim_trade::{Amount, GoodId, MILLI};
use rtsim_world::World;

/// The goblin camp of the conflict's synthetic world (the census's `conflict` scenario). With
/// `quiet` its plunder drive is off, so a test's own orders are the only raids.
fn camp(seed: u64, quiet: bool) -> (World, Conflict) {
    let (mut w, _) = synth::world(seed);
    let mut c = synth::conflict(seed, &w);
    if quiet {
        for k in &mut c.cultures.list {
            k.def.plunder_mean_days = None;
        }
    }
    c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
        .expect("the goblin culture");
    (w, c)
}

/// The villages the goblins hate, nearest the camp first.
fn prey(w: &World, c: &Conflict) -> Vec<usize> {
    let goblin = c
        .cultures
        .get(c.cultures.index_of("goblin").expect("goblins"));
    let d2 = |i: usize| {
        let p = w.nodes[i].pos;
        let (dx, dz) = (
            i64::from(p.x - synth::CAMP_POS.x),
            i64::from(p.z - synth::CAMP_POS.z),
        );
        dx * dx + dz * dz
    };
    let mut v: Vec<usize> = (0..w.nodes.len())
        .filter(|&i| matches!(w.nodes[i].kind, NodeKind::Village(cu) if goblin.hates(cu.name())))
        .collect();
    v.sort_by_key(|&i| (d2(i), i));
    v
}

/// Runs the conflict event by event until party `p`'s next event is its arrival at its target,
/// and the world to that tick, so a market read now is the one the raid reads. Returns the tick.
fn to_arrival(w: &mut World, c: &mut Conflict, p: Handle<Party>) -> Tick {
    loop {
        let x = &c.parties[p];
        if x.phase == Phase::Out && x.seg as usize + 2 >= x.route.len() {
            let at = x.times[x.seg as usize + 1];
            w.run_until(at);
            return at;
        }
        let next = c.next_due().expect("the party's next event");
        c.run_until(w, &mut NoInfrastructure, next);
    }
}

/// Runs until party `p` is home and gone.
fn to_home(w: &mut World, c: &mut Conflict, p: Handle<Party>) {
    while c.parties.get(p).is_some() {
        let next = c.next_due().expect("the party's next event");
        c.run_until(w, &mut NoInfrastructure, next);
    }
    w.run_until(c.now());
}

fn taken(c: &Conflict, g: GoodId) -> i128 {
    c.ledger.from_villages.get(&g).copied().unwrap_or(0)
}

/// CL-D58: an ordered plunder at a village some of whose lines stand above their targets (made so
/// by the test) and some at or below. The raid is fought, takes nothing from a line at or below its
/// target, and from a line above it at most the goblins' share of the surplus, so every line keeps
/// at least its target. Coins over the treasury's target are taken like any good.
#[test]
fn a_raid_takes_only_the_surplus() {
    let (mut raided, mut goods_taken, mut coins_taken) = (0, 0, 0);
    for seed in 1..=8u64 {
        let (mut w, mut c) = camp(seed, true);
        let share = c.cultures.get(0).def.loot_share_milli;
        let goods: Vec<GoodId> = w.goods.ids().collect();
        let i = prey(&w, &c)[seed as usize % 2];
        let p = c
            .order_party(
                &mut w,
                synth::CAMP,
                PartyKind::Plunder,
                Target::Node(i),
                None,
                Tick::from_days(1),
            )
            .expect("a plunder band");
        let at = to_arrival(&mut w, &mut c, p);
        // Every other good, by the seed, stands up to 40 items over its target; the rest as they
        // are.
        let m = w.market(i);
        let rich: BTreeMap<GoodId, Amount> = goods
            .iter()
            .enumerate()
            .filter(|(k, _)| (*k as u64 + seed).is_multiple_of(2))
            .map(|(k, &g)| {
                let want = m.line(g).target + (k as i64 + 1) * 40 * MILLI / goods.len() as i64;
                (g, want - m.stock(g))
            })
            .filter(|e| e.1 > 0)
            .collect();
        w.stock_from_players(i, &rich, at);
        let m = w.market(i);
        let before: Vec<(Amount, Amount, i128)> = goods
            .iter()
            .map(|&g| (m.stock(g), m.line(g).target, taken(&c, g)))
            .collect();
        let raids = c.stats.raids_village;
        c.run_until(&mut w, &mut NoInfrastructure, at);
        raided += usize::from(c.stats.raids_village > raids);
        for (k, &g) in goods.iter().enumerate() {
            let (stock, target, was) = before[k];
            let got = taken(&c, g) - was;
            let most = i128::from((stock - target).max(0) * share / 1000);
            assert!(
                (0..=most).contains(&got),
                "seed {seed} good {k}: took {got} of {stock} (target {target}, at most {most})"
            );
            if got > 0 {
                if w.goods.is_currency(g) {
                    coins_taken += 1;
                } else {
                    goods_taken += 1;
                }
            }
        }
        to_home(&mut w, &mut c, p);
        assert!(c.check(&w).is_ok(), "seed {seed}: {:?}", c.check(&w));
    }
    assert!(raided >= 7, "{raided} raids of 8");
    assert!(goods_taken > 0, "goods over their targets are taken");
    assert!(
        coins_taken > 0,
        "coins over the treasury's target are taken"
    );
}

/// CL-D58: a band at a village with nothing over its targets, or less than one whole item's
/// share, does not fight: no raid counted, nobody falls, nothing taken, no robbery remembered;
/// it goes home whole.
#[test]
fn nothing_to_take_is_no_fight() {
    for seed in 1..=4u64 {
        for spare_coin in [false, true] {
            let (mut w, mut c) = camp(seed, true);
            let i = prey(&w, &c)[0];
            let p = c
                .order_party(
                    &mut w,
                    synth::CAMP,
                    PartyKind::Plunder,
                    Target::Node(i),
                    None,
                    Tick::from_days(1),
                )
                .expect("a plunder band");
            let at = to_arrival(&mut w, &mut c, p);
            // Every line down to its target; with `spare_coin`, the treasury one coin over it,
            // whose share (the goblins' 150 per mille) is under one item.
            let m = w.market(i);
            for g in w.goods.ids().collect::<Vec<_>>() {
                let over = m.stock(g) - m.line(g).target;
                if over > 0 {
                    w.take_stock(i, g, over, at);
                }
            }
            if spare_coin {
                let coin = w.nodes[i].currency.expect("a village keeps coins");
                let m = w.market(i);
                let up = m.line(coin).target + MILLI - m.stock(coin);
                w.stock_from_players(i, &BTreeMap::from([(coin, up)]), at);
            }
            let (va, fa) = (w.nodes[i].actor, c.factions[0].actor);
            let felt = w.knowledge.sentiment(va, fa, at);
            let (raids, dead) = (c.stats.raids_village, c.stats.casualties);
            let alive = c.parties[p].alive();
            let took = c.ledger.from_villages.clone();
            c.run_until(&mut w, &mut NoInfrastructure, at);
            let tag = format!("seed {seed} spare coin {spare_coin}");
            assert_eq!(c.stats.raids_village, raids, "{tag}: no raid");
            assert_eq!(c.stats.casualties, dead, "{tag}: nobody falls");
            assert_eq!(c.parties[p].alive(), alive, "{tag}");
            assert_eq!(c.ledger.from_villages, took, "{tag}: nothing taken");
            assert_eq!(c.parties[p].phase, Phase::Home, "{tag}: the band goes home");
            assert_eq!(w.knowledge.sentiment(va, fa, at), felt, "{tag}: no robbery");
        }
    }
}

// ---- the long run -------------------------------------------------------------------------------

const YEAR: u64 = 96;

/// A village's stock against its targets now: `fill`, the mean over its lines with a target of
/// the stock's share of it (at most 1), and `coins`, its treasury's share of its target.
#[derive(Clone, Copy, Debug, Default)]
struct Stock {
    fill: f64,
    coins: f64,
}

impl Stock {
    fn of(w: &World, i: usize) -> Stock {
        let m = w.market(i);
        let lines: Vec<f64> = w
            .goods
            .ids()
            .filter(|&g| m.line(g).target > 0)
            .map(|g| (m.stock(g) as f64 / m.line(g).target as f64).min(1.0))
            .collect();
        let coins = w.nodes[i]
            .currency
            .map_or(1.0, |c| m.stock(c) as f64 / m.line(c).target.max(1) as f64);
        Stock {
            fill: lines.iter().sum::<f64>() / lines.len().max(1) as f64,
            coins,
        }
    }

    fn min(self, o: Stock) -> Stock {
        Stock {
            fill: self.fill.min(o.fill),
            coins: self.coins.min(o.coins),
        }
    }
}

/// One world's run: its villages' stock floors, the raids and the feuds.
#[derive(Clone, Debug, Default)]
struct Life {
    world: &'static str,
    seed: u64,
    looting: bool,
    years: u64,
    /// Each village's lowest [`Stock`] over the run, sampled every 24 days (the fill and the coins
    /// each at its own lowest).
    floor: Vec<Stock>,
    /// Each village's [`Stock`] at the end.
    end: Vec<Stock>,
    /// Villages whose treasury is under one coin at the end.
    broke: usize,
    plunders: u64,
    raids: u64,
    items: u64,
    casualties: u64,
    feuds_end: u64,
    feuds_peak: u64,
    raids_rung: u64,
    sieges: u64,
    chiefs_killed: u64,
}

/// `years` of `world` (`camp`: the census's goblin camp and its nine villages; `many`: 36 villages
/// and a goblin, a bandit and a raider camp, each with two feuds) for `seed`, nobody watching. With
/// `looting` false every culture's `loot_share_milli` is 0, so raids take nothing: the control.
fn life(world: &'static str, seed: u64, looting: bool, years: u64) -> Life {
    let (mut w, mut c) = match world {
        "camp" => camp(seed, false),
        _ => synth::many(36, 3, seed),
    };
    if !looting {
        for k in &mut c.cultures.list {
            k.def.loot_share_milli = 0;
        }
    }
    let villages: Vec<usize> = (0..w.nodes.len())
        .filter(|&i| matches!(w.nodes[i].kind, NodeKind::Village(_)))
        .collect();
    let mut floor: Vec<Stock> = villages.iter().map(|&i| Stock::of(&w, i)).collect();
    let mut feuds_peak = 0;
    let end = Tick::from_days(years * YEAR);
    let mut t = Tick::ZERO;
    while t < end {
        t = (t + TICKS_PER_DAY * 24).min(end);
        c.run_until(&mut w, &mut NoInfrastructure, t);
        w.run_until(t);
        let _ = c.take_commands();
        for (k, &i) in villages.iter().enumerate() {
            floor[k] = floor[k].min(Stock::of(&w, i));
        }
        feuds_peak = feuds_peak.max(c.feud_count());
    }
    assert!(
        c.check(&w).is_ok(),
        "{world} seed {seed}: {:?}",
        c.check(&w)
    );
    let s = c.stats;
    Life {
        world,
        seed,
        looting,
        years,
        end: villages.iter().map(|&i| Stock::of(&w, i)).collect(),
        floor,
        broke: villages
            .iter()
            .filter(|&&i| w.market(i).treasury() < MILLI)
            .count(),
        plunders: s.parties[PartyKind::Plunder.index()],
        raids: s.raids_village,
        items: s.items_from_villages,
        casualties: s.casualties,
        feuds_end: c.feud_count(),
        feuds_peak,
        raids_rung: s.escalations[Step::Raid as usize],
        sieges: s.escalations[Step::Siege as usize],
        chiefs_killed: s.chiefs_killed,
    }
}

/// `jobs` over up to `threads` threads, in job order.
fn per_job<T: Send>(threads: usize, jobs: &[u64], f: impl Fn(u64) -> T + Sync) -> Vec<T> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut out: Vec<(usize, T)> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads.min(jobs.len()))
            .map(|_| {
                s.spawn(|| {
                    let mut got = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&j) = jobs.get(k) else { break };
                        got.push((k, f(j)));
                    }
                    got
                })
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().expect("a job"))
            .collect()
    });
    out.sort_by_key(|e| e.0);
    out.into_iter().map(|e| e.1).collect()
}

/// A looting run's village against its seed's control: the fill floor, the fill at the end, the
/// coins floor and the coins at the end (each the looting run's minus the control's), and the seed.
type Gap = (f64, f64, f64, f64, u64);

fn lowest(v: impl Iterator<Item = f64>) -> f64 {
    v.fold(f64::INFINITY, f64::min)
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// Prints each run (`LIFE`), each world and arm over the seeds (`SUMMARY`), and the looting runs
/// against their own seed's control, village by village (`PAIRED`: the looting run's minus the
/// control's).
fn report(v: &[Life]) {
    for world in ["camp", "many"] {
        for looting in [true, false] {
            let r: Vec<&Life> = v
                .iter()
                .filter(|l| l.world == world && l.looting == looting)
                .collect();
            if r.is_empty() {
                continue;
            }
            for l in &r {
                eprintln!(
                    "LIFE {world} seed {:>2} loot {looting}: fill floor {:.3} (lowest village) \
                     end {:.3}; coins floor {:.3} end {:.3}; broke {}; plunders {:.2}/y raids \
                     {:.2}/y items {:.1}/raid lost {:.2}/raid; feuds end {} peak {}; raid rungs {} \
                     sieges {} chiefs killed {}",
                    l.seed,
                    lowest(l.floor.iter().map(|s| s.fill)),
                    lowest(l.end.iter().map(|s| s.fill)),
                    lowest(l.floor.iter().map(|s| s.coins)),
                    lowest(l.end.iter().map(|s| s.coins)),
                    l.broke,
                    l.plunders as f64 / l.years as f64,
                    l.raids as f64 / l.years as f64,
                    l.items as f64 / l.raids.max(1) as f64,
                    l.casualties as f64 / l.raids.max(1) as f64,
                    l.feuds_end,
                    l.feuds_peak,
                    l.raids_rung,
                    l.sieges,
                    l.chiefs_killed,
                );
            }
            let n = r.len() as f64;
            let all = |f: &dyn Fn(&Life) -> Vec<f64>| -> Vec<f64> {
                r.iter().flat_map(|l| f(l)).collect()
            };
            let fill_floor = all(&|l| l.floor.iter().map(|s| s.fill).collect());
            let fill_end = all(&|l| l.end.iter().map(|s| s.fill).collect());
            let coin_floor = all(&|l| l.floor.iter().map(|s| s.coins).collect());
            let coin_end = all(&|l| l.end.iter().map(|s| s.coins).collect());
            let per_year = |f: &dyn Fn(&Life) -> u64| -> f64 {
                r.iter().map(|l| f(l) as f64 / l.years as f64).sum::<f64>() / n
            };
            eprintln!(
                "SUMMARY {world} loot {looting} ({} seeds, {} years, {} villages): fill floor \
                 lowest {:.3} mean {:.3}, end lowest {:.3} mean {:.3}; coins floor lowest {:.3} \
                 mean {:.3}, end lowest {:.3} mean {:.3}; broke at the end {}; plunders {:.2}/y \
                 raids {:.2}/y items {:.1}/raid lost {:.2}/raid; feuds end {:.2} peak {:.2}; raid \
                 rungs {:.3}/y sieges {:.3}/y chiefs killed {:.3}/y",
                r.len(),
                r[0].years,
                fill_end.len(),
                lowest(fill_floor.iter().copied()),
                mean(&fill_floor),
                lowest(fill_end.iter().copied()),
                mean(&fill_end),
                lowest(coin_floor.iter().copied()),
                mean(&coin_floor),
                lowest(coin_end.iter().copied()),
                mean(&coin_end),
                r.iter().map(|l| l.broke).sum::<usize>(),
                per_year(&|l| l.plunders),
                per_year(&|l| l.raids),
                r.iter().map(|l| l.items).sum::<u64>() as f64
                    / r.iter().map(|l| l.raids).sum::<u64>().max(1) as f64,
                r.iter().map(|l| l.casualties).sum::<u64>() as f64
                    / r.iter().map(|l| l.raids).sum::<u64>().max(1) as f64,
                r.iter().map(|l| l.feuds_end as f64).sum::<f64>() / n,
                r.iter().map(|l| l.feuds_peak as f64).sum::<f64>() / n,
                per_year(&|l| l.raids_rung),
                per_year(&|l| l.sieges),
                per_year(&|l| l.chiefs_killed),
            );
        }
        // Village by village against the same seed's control.
        let mut d: Vec<Gap> = Vec::new();
        for l in v.iter().filter(|l| l.world == world && l.looting) {
            let Some(k) = v
                .iter()
                .find(|x| x.world == world && !x.looting && x.seed == l.seed)
            else {
                continue;
            };
            for j in 0..l.end.len() {
                d.push((
                    l.floor[j].fill - k.floor[j].fill,
                    l.end[j].fill - k.end[j].fill,
                    l.floor[j].coins - k.floor[j].coins,
                    l.end[j].coins - k.end[j].coins,
                    l.seed,
                ));
            }
        }
        if d.is_empty() {
            continue;
        }
        let worst = |f: &dyn Fn(&Gap) -> f64| {
            d.iter()
                .min_by(|a, b| f(a).total_cmp(&f(b)))
                .map_or((0.0, 0), |e| (f(e), e.4))
        };
        let col = |f: &dyn Fn(&Gap) -> f64| -> Vec<f64> { d.iter().map(f).collect() };
        let (ff, ffs) = worst(&|e| e.0);
        let (fe, fes) = worst(&|e| e.1);
        let (cf, cfs) = worst(&|e| e.2);
        let (ce, ces) = worst(&|e| e.3);
        eprintln!(
            "PAIRED {world} (looting minus its control, {} villages): fill floor mean {:+.4} worst \
             {:+.3} (seed {ffs}); fill end mean {:+.4} worst {:+.3} (seed {fes}); coins floor mean \
             {:+.4} worst {:+.3} (seed {cfs}); coins end mean {:+.4} worst {:+.3} (seed {ces}); \
             villages whose end fill is 0.05 or more under the control's {}",
            d.len(),
            mean(&col(&|e| e.0)),
            ff,
            mean(&col(&|e| e.1)),
            fe,
            mean(&col(&|e| e.2)),
            cf,
            mean(&col(&|e| e.3)),
            ce,
            d.iter().filter(|e| e.1 <= -0.05).count(),
        );
    }
}

fn env_or(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// `years` of seeds 1 to `seeds` on both worlds, looting and not, as [`report`] prints them, on
/// `RAID_THREADS` threads (default 8).
fn measure(seeds: u64, years: u64) -> Vec<Life> {
    let threads = env_or("RAID_THREADS", 8) as usize;
    let jobs: Vec<u64> = (0..4 * seeds).collect();
    let v = per_job(threads, &jobs, |j| {
        let world = if j % 2 == 0 { "camp" } else { "many" };
        life(world, 1 + j / 4, (j / 2) % 2 == 0, years)
    });
    report(&v);
    v
}

/// CL-D58 measured: 1,000 years (96,000 game days) of seeds 1 to 12 on the census's goblin camp
/// and on three camps among 36 villages, raids taking against taking nothing. Reports only.
/// Release only (`cargo test --release -p rtsim-conflict --test raid_surplus -- --ignored`);
/// `RAID_SEEDS` and `RAID_YEARS` shorten it.
#[test]
#[ignore = "long: run in release with --ignored"]
fn raids_measured_1000_years() {
    measure(env_or("RAID_SEEDS", 12), env_or("RAID_YEARS", 1_000));
}
