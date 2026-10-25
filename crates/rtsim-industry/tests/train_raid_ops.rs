// SPDX-License-Identifier: AGPL-3.0-or-later
//! A train attack's op is the attack's identity (docs/choices-mowzie-build.md RP-180 to RP-183).
//! Industry is not held while a raider replays its window (docs/replay-gate.md §3.6), so a raider
//! whose pages are older than a raid it decided re-decides it after a crash, against an industry
//! engine that has moved on to Java's present. The re-decided CarriageDelta must carry the op
//! Java applied before, so Java answers DUPLICATE and the goods leave the train once.

use rtsim_industry::cmd::{CarriageReason, Cmd};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, StationId, TrainId};
use rtsim_industry::obs::{
    Carriage, CarriageResult, CarriageResultEntry, Obs, StationDecl, Timed, TrainDeparture,
};
use rtsim_industry::threat::{Loads, Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::OpId;
use rtsim_kernel::page::PageKey;

const IRON: Good = Good(1);
const WEST: StationId = StationId(10);
const EAST: StationId = StationId(11);
const TRAIN: TrainId = TrainId(3);

fn send(e: &mut Engine, at: u64, obs: Obs) {
    e.ingest(&Timed { at: Tick(at), obs });
}

/// A train of 640 iron leaves the west station at tick 10 for a leg of 100,000 ticks.
fn engine() -> Engine {
    let mut e = Engine::new(77, Tunables::default(), Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    for (station, district, x) in [(WEST, DistrictId(1), 0), (EAST, DistrictId(2), 8000)] {
        send(
            &mut e,
            0,
            Obs::Station(StationDecl {
                station,
                district: Some(district),
                dim: 0,
                pos: BlockPos::new(x, 64, 0),
                name: 0,
            }),
        );
    }
    let mut goods = GoodVec::new();
    goods.add(IRON, 640);
    send(
        &mut e,
        10,
        Obs::TrainDeparted(TrainDeparture {
            train: TRAIN,
            station: WEST,
            district: DistrictId(1),
            destination: Some(EAST),
            eta_ticks: 100_000,
            carriages: vec![Carriage {
                index: 0,
                item_slots: 27,
                fluid_capacity: 0,
                goods,
            }],
        }),
    );
    e
}

fn raid(id: u64, at: u64) -> Threat {
    Threat {
        id,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::Train(TRAIN),
        at: Tick(at),
        loads: None,
    }
}

/// The engine's industry pages, as a save leaves them.
fn pages(e: &Engine) -> Vec<(PageKey, Vec<u8>)> {
    e.snapshot()
}

fn restore(p: &[(PageKey, Vec<u8>)]) -> Engine {
    Engine::restore(p.iter().cloned(), Tunables::default()).expect("restore")
}

/// The loot deltas the engine decided since the last drain: (op, iron into the carriage).
fn loot(e: &mut Engine) -> Vec<(OpId, i64)> {
    e.drain_commands()
        .into_iter()
        .filter_map(|c| match c {
            Cmd::CarriageDelta(d) if d.reason == CarriageReason::Loot => {
                Some((d.op, d.entries.iter().map(|x| x.2).sum()))
            }
            _ => None,
        })
        .collect()
}

/// A raider whose pages are older than its raid re-decides it after a crash. Industry, restored
/// from pages older than the raid too, has moved on to Java's present (it is not held), so the
/// re-decision books at a later tick: its op is still the live one's, which Java applied, so Java
/// answers DUPLICATE and the exchange is dropped (RP-180, RP-182).
#[test]
fn a_re_decided_train_raid_carries_the_op_java_applied() {
    let mut live = engine();
    live.advance_to(Tick(500));
    let before = pages(&live);
    live.threat(&raid(42, 1_000));
    let applied = loot(&mut live);
    assert_eq!(applied.len(), 1, "one raid, one delta");
    let (x, took) = applied[0];
    assert!(took < 0, "the raid takes iron: {took}");
    let stolen = live.books.stolen.get(IRON);
    assert_eq!(stolen, -took);

    // The crash: industry's pages are older than the raid and its clock jumps to Java's present.
    let mut again = restore(&before);
    again.advance_to(Tick(5_000));
    again.threat(&raid(42, 1_000));
    let redone = loot(&mut again);
    assert_eq!(redone.len(), 1, "the re-decision is one delta");
    assert_eq!(
        redone[0].0, x,
        "the re-decided raid carries the op Java applied (main derives it from industry's now)"
    );
    // Java answers DUPLICATE: the exchange goes, the books keep the one theft.
    assert!(again.command_duplicate(x), "the loot exchange is dropped");
    assert!(!again.command_duplicate(x), "and only once");
    assert_eq!(again.books.stolen.get(IRON), stolen);
    again.check_books().unwrap();
}

/// The replayed IND_CARRIAGE_RESULT of the live application comes after the re-decided raid
/// (order O1, RP-182): it takes the exchange, and the DUPLICATE that follows finds none.
#[test]
fn a_replayed_result_before_the_duplicate_takes_the_exchange() {
    let mut live = engine();
    live.advance_to(Tick(500));
    let before = pages(&live);
    live.threat(&raid(42, 1_000));
    let (x, took) = loot(&mut live)[0];

    let mut again = restore(&before);
    again.advance_to(Tick(5_000));
    again.threat(&raid(42, 1_000));
    assert_eq!(loot(&mut again)[0].0, x);
    // Live, the carriage had 40 fewer than asked: the result says so.
    let short = 40;
    send(
        &mut again,
        5_000,
        Obs::CarriageResult(CarriageResult {
            op: x,
            train: TRAIN,
            entries: vec![CarriageResultEntry {
                carriage: 0,
                good: IRON,
                requested: took,
                applied: took + short,
            }],
        }),
    );
    assert_eq!(again.books.stolen.get(IRON), -took - short);
    assert!(!again.command_duplicate(x), "the result took the exchange");
    assert_eq!(again.books.stolen.get(IRON), -took - short);
}

/// Industry's pages are newer than the raider's: the re-decided raid is one the op window
/// remembers, so nothing is booked again, even past the train's cooldown (RP-183).
#[test]
fn industry_holding_the_raid_books_it_once() {
    let mut live = engine();
    live.threat(&raid(42, 1_000));
    assert_eq!(loot(&mut live).len(), 1);
    let stolen = live.books.stolen.get(IRON);
    // Past the per-class cooldown (24,000 ticks by default): only the known key stops it.
    live.advance_to(Tick(40_000));
    live.threat(&raid(42, 1_000));
    assert!(loot(&mut live).is_empty(), "nothing decided again");
    assert_eq!(live.books.stolen.get(IRON), stolen);
    // Through a save and a restore too: the window is in the pages.
    let mut back = restore(&pages(&live));
    back.threat(&raid(42, 1_000));
    assert!(loot(&mut back).is_empty());
    assert_eq!(back.books.stolen.get(IRON), stolen);
    // Another attack on the same train is its own op.
    back.threat(&raid(43, 41_000));
    let other = loot(&mut back);
    assert_eq!(other.len(), 1);
    assert!(back.books.stolen.get(IRON) > stolen);
}

/// Two raiders whose ids meet, or one threat on two trains, or two classes of one ambush, are
/// different attacks with different ops: the key holds the source, the class and the train.
#[test]
fn the_key_parts_the_attacks_it_should() {
    let op = |t: Threat| {
        let mut e = engine();
        e.threat(&t);
        let l = loot(&mut e);
        assert_eq!(l.len(), 1, "{t:?}");
        l[0].0
    };
    let base = op(raid(42, 1_000));
    assert_eq!(op(raid(42, 2_000)), base, "the tick is not in the key");
    assert_ne!(op(raid(43, 1_000)), base);
    assert_ne!(
        op(Threat {
            source: ThreatSource::Dragon,
            ..raid(42, 1_000)
        }),
        base
    );
    assert_ne!(
        op(Threat {
            // Only sieges carry by default: a melee band with its own loads.
            class: ThreatClass::GroundMelee,
            loads: Some(Loads {
                carry: 192,
                blocks: 0
            }),
            ..raid(42, 1_000)
        }),
        base
    );
    // A station exchange's DUPLICATE is not the loot's: nothing to drop under an unknown op.
    let mut e = engine();
    assert!(!e.command_duplicate(OpId(0x4900_0000_0000_0001)));
}
