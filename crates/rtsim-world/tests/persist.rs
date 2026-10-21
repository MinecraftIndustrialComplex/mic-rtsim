// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 world's snapshot (`World::save`, `World::load`): a world saved at any point of any
//! script and read back continues exactly as one that never stopped, state hash and every byte of
//! its next snapshot alike.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use proptest::prelude::*;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::build::Genesis;
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::synth::{self, SynthGoods};
use rtsim_world::world::NodeData;
use rtsim_world::{Side, World, WorldConfig};

fn cfg(seed: u64) -> WorldConfig {
    let mut cfg = WorldConfig::new(seed);
    cfg.genesis = Genesis::Share(500);
    cfg
}

fn world(seed: u64) -> (World, SynthGoods) {
    synth::build(cfg(seed), true)
}

/// A world read back from its own snapshot, built from the same configuration and catalogue.
fn reload(w: &World, seed: u64) -> World {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    World::load(cfg(seed), goods, book, &w.save()).expect("the snapshot reads back")
}

fn busiest(w: &World, from: usize) -> Option<rtsim_travel::CaravanKey> {
    let mut best: Option<(i64, rtsim_travel::CaravanKey)> = None;
    for d in &w.nodes[from].out {
        for t in w.travellers(d.edge, d.dir) {
            let load: i64 = t.cargo.values().sum();
            if load > 0 && best.is_none_or(|b| load > b.0) {
                best = Some((load, t.key));
            }
        }
    }
    best.map(|b| b.1)
}

#[derive(Clone, Debug)]
enum Op {
    Trade {
        node: usize,
        good: usize,
        items: i64,
        buy: bool,
    },
    Rob {
        node: usize,
        share: i64,
    },
    Rescue {
        node: usize,
    },
    TakeLlama {
        node: usize,
    },
    KillLeader {
        node: usize,
    },
    Join {
        x: i32,
        z: i32,
        ribbit: bool,
    },
    Walk {
        player: u64,
        x: i32,
        z: i32,
    },
    Base {
        player: u64,
        x: i32,
        z: i32,
    },
    Report {
        node: usize,
        kind: u8,
    },
    Commands,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0usize..10, 0usize..7, 1i64..80, any::<bool>()).prop_map(|(node, good, items, buy)| {
            Op::Trade {
                node,
                good,
                items,
                buy,
            }
        }),
        (0usize..10, 0i64..=1000).prop_map(|(node, share)| Op::Rob { node, share }),
        (0usize..10).prop_map(|node| Op::Rescue { node }),
        (0usize..10).prop_map(|node| Op::TakeLlama { node }),
        (0usize..10).prop_map(|node| Op::KillLeader { node }),
        (-2000i32..2000, -2000i32..2000, any::<bool>()).prop_map(|(x, z, ribbit)| Op::Join {
            x,
            z,
            ribbit
        }),
        (1u64..4, -1500i32..1500, -1500i32..1500).prop_map(|(player, x, z)| Op::Walk {
            player,
            x,
            z
        }),
        (1u64..4, -1500i32..1500, -1500i32..1500).prop_map(|(player, x, z)| Op::Base {
            player,
            x,
            z
        }),
        (0usize..10, 1u8..9).prop_map(|(node, kind)| Op::Report { node, kind }),
        Just(Op::Commands),
    ]
}

/// Applies one op at `t` (the world is run there first).
fn apply(w: &mut World, ids: &SynthGoods, o: &Op, t: Tick, joined: &mut u64) {
    w.run_until(t);
    let player = w.player(7);
    let goods = [
        ids.wheat,
        ids.bread,
        ids.leather,
        ids.iron,
        ids.wool,
        ids.slime,
        ids.clay,
    ];
    let n = w.nodes.len();
    match *o {
        Op::Trade {
            node,
            good,
            items,
            buy,
        } => {
            let _ = w.trade(
                node % n,
                player,
                goods[good],
                items,
                if buy { Side::Buy } else { Side::Sell },
                t,
            );
        }
        Op::Rob { node, share } => {
            if let Some(k) = busiest(w, node % n)
                && let Some(c) = w.promote(k)
            {
                w.rob(c, player, share);
            }
        }
        Op::Rescue { node } => {
            if let Some(k) = busiest(w, node % n)
                && let Some(c) = w.promote(k)
            {
                w.rescue(c, player);
            }
        }
        Op::TakeLlama { node } => {
            if let Some(k) = busiest(w, node % n)
                && let Some(c) = w.promote(k)
            {
                let m = w.caravans.get(c).unwrap().members.len() - 1;
                w.take_member(c, m, player);
            }
        }
        Op::KillLeader { node } => {
            if let Some(k) = busiest(w, node % n)
                && let Some(c) = w.promote(k)
            {
                w.kill_member(c, 0, player);
            }
        }
        Op::Join { x, z, ribbit } => {
            *joined += 1;
            let mut resources = BTreeMap::new();
            resources.insert(rtsim_trade::labour::ResourceId(0), 12 * MILLI);
            w.join(
                NodeData {
                    key: 50_000 + *joined,
                    kind: NodeKind::Village(if ribbit {
                        Culture::Ribbit
                    } else {
                        Culture::Human
                    }),
                    pos: Pos::new(0, x, z),
                    pop: 9,
                    resources,
                },
                t,
            );
        }
        Op::Walk { player: p, x, z } => {
            let a = w.player_uuid(p, p * 31);
            w.set_player_pos(a, Pos::new(0, x, z), t);
        }
        Op::Base { player: p, x, z } => {
            let a = w.player_uuid(p, p * 31);
            let key = 1 << 62 | (x as u64 & 0xffff) << 16 | (z as u64 & 0xffff);
            w.mark_base(key, Pos::new(0, x, z), a, t);
        }
        Op::Report { node, kind } => {
            if let Some(k) = rtsim_knowledge::ReportKind::from_u16(u16::from(kind)) {
                w.village_report(node % n, player, k, t);
            }
        }
        Op::Commands => {
            let _ = w.take_commands();
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Saved and read back at any point of any script, the world continues exactly as the one
    /// that never stopped: the same state hash after every later op, and the same bytes in its
    /// snapshot at the end.
    #[test]
    fn a_world_saved_anywhere_continues_exactly(
        seed in 0u64..1000,
        ops in prop::collection::vec((1u64..60, op()), 1..16),
        cut in 0usize..12,
    ) {
        let (mut a, ids) = world(seed);
        let (mut b, _) = world(seed);
        let (mut ja, mut jb) = (0, 0);
        let mut t = Tick::from_days(2);
        for (i, (gap, o)) in ops.iter().enumerate() {
            t += gap * 1000;
            if i == cut {
                b.run_until(Tick(t.0 - 1));
                b = reload(&b, seed);
                prop_assert_eq!(b.state_hash(), {
                    a.run_until(Tick(t.0 - 1));
                    a.state_hash()
                });
            }
            apply(&mut a, &ids, o, t, &mut ja);
            apply(&mut b, &ids, o, t, &mut jb);
            prop_assert_eq!(a.state_hash(), b.state_hash(), "after op {} ({:?})", i, o);
        }
        let end = t + 3 * TICKS_PER_DAY;
        a.run_until(end);
        b.run_until(end);
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert_eq!(a.save(), b.save());
        prop_assert_eq!(a.take_commands(), b.take_commands());
        a.check_conserved(end).unwrap();
        b.check_conserved(end).unwrap();
    }
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("rtsim-world-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

type Store = Arc<RwLock<PageStore>>;

/// A paged save: the meta page and the changed market pages in one commit, then the budget.
fn save_paged(w: &mut World, store: &Store) {
    let mut b = SaveBatch::new();
    let t = w.save_paged(&mut b);
    store.write().unwrap().commit(b, 0).expect("commit");
    w.saved(t);
    w.enforce_budget();
}

/// Market bytes a paged world keeps resident: a page or two of the synthetic world's markets.
const BUDGET: usize = 12_000;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Paged: a world whose market rows page through a store under a budget of a page or two
    /// (saved after every op, then evicted down to the budget) runs exactly as a resident one,
    /// and one opened from those pages at any point continues exactly.
    #[test]
    fn a_paged_world_runs_and_restores_exactly(
        seed in 0u64..1000,
        ops in prop::collection::vec((1u64..60, op()), 1..12),
        cut in 0usize..12,
    ) {
        let dir = tmpdir("prop");
        let store: Store = Arc::new(RwLock::new(PageStore::open(&dir, Durability::NoSync).unwrap()));
        let (mut a, ids) = world(seed);
        let (mut b, _) = world(seed);
        b.attach_store(store.clone(), BUDGET);
        save_paged(&mut b, &store);
        let (mut ja, mut jb) = (0, 0);
        let mut t = Tick::from_days(2);
        for (i, (gap, o)) in ops.iter().enumerate() {
            t += gap * 1000;
            if i == cut {
                b.run_until(Tick(t.0 - 1));
                save_paged(&mut b, &store);
                let (goods, gids) = synth::goods();
                let book = synth::recipes(&gids);
                b = World::open_paged(cfg(seed), goods, book, store.clone(), BUDGET)
                    .expect("the pages open");
                a.run_until(Tick(t.0 - 1));
                prop_assert_eq!(b.state_hash(), a.state_hash());
            }
            apply(&mut a, &ids, o, t, &mut ja);
            apply(&mut b, &ids, o, t, &mut jb);
            save_paged(&mut b, &store);
            prop_assert_eq!(a.state_hash(), b.state_hash(), "after op {} ({:?})", i, o);
        }
        let end = t + 3 * TICKS_PER_DAY;
        a.run_until(end);
        b.run_until(end);
        save_paged(&mut b, &store);
        prop_assert!(b.markets.rows().resident_bytes() <= BUDGET);
        prop_assert!(b.markets.rows().stats().loads > 0, "the markets paged");
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert_eq!(a.save(), b.save());
        prop_assert_eq!(a.take_commands(), b.take_commands());
        b.check_conserved(end).unwrap();
        drop(b);
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// A busy world's snapshot reads back to the same bytes, and the reloaded world's next 20 days
/// match the original's day by day.
#[test]
fn a_busy_world_round_trips_and_runs_on_identically() {
    let (mut a, ids) = world(42);
    let player = a.player(7);
    a.run_until(Tick::from_days(10));
    let _ = a.trade(1, player, ids.wheat, 30, Side::Sell, Tick::from_days(10));
    if let Some(k) = busiest(&a, 1)
        && let Some(c) = a.promote(k)
    {
        a.rob(c, player, 500);
    }
    if let Some(k) = busiest(&a, 2) {
        a.promote(k);
    }
    a.run_until(Tick(Tick::from_days(10).0 + 500));
    assert!(
        !a.caravans.is_empty(),
        "a caravan row is on the road at the save"
    );
    assert!(!a.packets.is_empty(), "news is on the road at the save");
    let bytes = a.save();
    let mut b = reload(&a, 42);
    assert_eq!(b.save(), bytes, "the snapshot reads back byte for byte");
    assert_eq!(b.state_hash(), a.state_hash());
    for day in 11..=32 {
        a.run_until(Tick::from_days(day));
        b.run_until(Tick::from_days(day));
        assert_eq!(a.state_hash(), b.state_hash(), "day {day}");
        assert_eq!(a.take_commands(), b.take_commands(), "day {day}");
    }
    assert_eq!(a.stats, b.stats);
    assert_eq!(a.save(), b.save());
    let _ = GoodId(0);
}

/// A snapshot from another catalogue, or cut short, is refused with an error, never a panic.
#[test]
fn a_wrong_or_truncated_snapshot_is_an_error() {
    let (mut a, _) = world(3);
    a.run_until(Tick::from_days(3));
    let bytes = a.save();
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    for cut in [0, 4, bytes.len() / 3, bytes.len() / 2, bytes.len() - 1] {
        assert!(World::load(cfg(3), goods.clone(), book.clone(), &bytes[..cut]).is_err());
    }
    let mut more = bytes.clone();
    more.push(0);
    assert!(World::load(cfg(3), goods, book, &more).is_err());
}
