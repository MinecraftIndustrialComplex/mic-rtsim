// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slice R, step 1 (RP-260 to RP-269): are the industry engine's derived op ids the same live and
//! replayed? On bc669502 every `IndustryOps::derive` call in the reconcile paths passed
//! `self.now`, which depends on the history of calls, and that history differs between a live run
//! and a replay after a crash (docs/replay-gate.md, overnight choice W3). Step 2 (RP-270 to RP-279)
//! derives with the decision's own tick and holds industry while replaying, as the replay below
//! does: the step 1 tests it turned green lost their `#[ignore]`.
//!
//! The harness follows rtsim-jni's engine/industry.rs:
//! - **Live**: one hand-over is its journaled records, each preceded by the deferred latest-wins
//!   records stamped before it (`Link::flush(Some(at))`); then the rest of the deferred ones and the
//!   threats (`simulate`), then `advance_to(G)`, G being the hand-over's game time.
//! - **Replay** after a crash: the engine restored from the pages of a step boundary, the journaled
//!   records of the later hand-overs only, in order, with no `advance_to` between them. The
//!   engine's own session start (`begin_session`) runs after them.
//!
//! The commands both runs decide after the pages are compared by content (`Cmd::without_op`), then
//! by op id. A test that fails (`#[ignore]`d, so the workspace stays green) proves a divergence:
//! run it with `--ignored`.

use std::collections::VecDeque;

use rtsim_industry::cmd::{ChunkReconcile, Cmd};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::learn::Learned;
use rtsim_industry::obs::{
    DepositPluginId, DepositSample, DistrictDecl, InvKind, InventoryDecl, LoadState, Obs, SweepEnd,
    Timed,
};
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;

const IRON: Good = Good(1);
const OIL: Good = Good(5);
const MILL: DistrictId = DistrictId(1);
/// A second district, whose declaration is a harmless journaled record.
const OTHER: DistrictId = DistrictId(2);
const VAULT: u32 = 0;
const SEED: u64 = 7;

fn vault_pos() -> BlockPos {
    BlockPos::new(8, 64, 8)
}

fn frog_pos() -> BlockPos {
    BlockPos::new(40, 64, 8)
}

fn chunks() -> [ChunkPos; 2] {
    [vault_pos(), frog_pos()].map(|p| ChunkPos {
        dim: 0,
        x: p.x >> 4,
        z: p.z >> 4,
    })
}

fn t(at: u64, obs: Obs) -> Timed {
    Timed { at: Tick(at), obs }
}

fn load_state(at: u64, state: LoadState, ticking: u32) -> Timed {
    t(
        at,
        Obs::LoadState {
            district: MILL,
            state,
            ticking_chunks: ticking,
            total_chunks: 2,
        },
    )
}

fn chunk_loaded(at: u64, which: usize) -> Timed {
    t(
        at,
        Obs::ChunkLoaded {
            district: MILL,
            chunk: chunks()[which],
            applied_epoch: 0,
        },
    )
}

/// One hand-over of the industry link: what its records are, and the game time it reaches.
#[derive(Clone, Default)]
struct Step {
    /// The journaled records, in journal order.
    journaled: Vec<Timed>,
    /// The latest-wins records (stock samples and the like), by tick.
    deferred: Vec<Timed>,
    /// The threats `simulate` applies before `advance_to`.
    threats: Vec<Threat>,
    /// The hand-over's game time.
    g: u64,
}

impl Step {
    fn new(g: u64) -> Self {
        Step {
            g,
            ..Step::default()
        }
    }
}

/// A live hand-over, as rtsim-jni runs it. The commands it decided are appended to `out`.
fn live_step(e: &mut Engine, s: &Step, out: &mut Vec<Cmd>) {
    let mut deferred: VecDeque<Timed> = s.deferred.iter().cloned().collect();
    for rec in &s.journaled {
        // `Link::flush(Some(at))`: the deferred records stamped before this one come first.
        while deferred.front().is_some_and(|d| d.at < rec.at) {
            e.ingest(&deferred.pop_front().expect("front"));
        }
        e.ingest(rec);
    }
    for d in deferred {
        e.ingest(&d);
    }
    for th in &s.threats {
        e.threat(th);
    }
    e.advance_to(Tick(s.g));
    out.extend(e.drain_commands());
}

/// A replayed hand-over: the journaled records only, no `advance_to`.
fn replay_step(e: &mut Engine, s: &Step, out: &mut Vec<Cmd>) {
    for rec in &s.journaled {
        e.ingest(rec);
    }
    out.extend(e.drain_commands());
}

/// The mill in a fresh engine: a district of one vault and one frogport, live, with `stock` iron and
/// a learned model that makes `rate` iron a tick while it is away.
fn mill(stock: i64, rate: f64) -> Engine {
    mill_with(stock, 0, rate)
}

/// [`mill`] with `frog` iron in the frogport.
fn mill_with(stock: i64, frog: i64, rate: f64) -> Engine {
    mill_oil(stock, frog, rate, None)
}

/// [`mill_with`], and with `oil` a diesel well on the vault's chunk that the model draws `oil` a
/// tick from while away (a deposit delta in each round that settles the vault's chunk).
fn mill_oil(stock: i64, frog: i64, rate: f64, oil: Option<f64>) -> Engine {
    let mut e = Engine::new(SEED, Tunables::default(), Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    if oil.is_some() {
        e.define_good(OIL, GoodInfo::deposit("diesel:oil/00000000000000aa"));
    }
    let send = |e: &mut Engine, rec: Timed| e.ingest(&rec);
    send(
        &mut e,
        t(
            0,
            Obs::District(DistrictDecl {
                district: MILL,
                revision: 1,
                dim: 0,
                name: 0,
                not_learnable: None,
                inventory_count: 2,
            }),
        ),
    );
    for (index, pos, kind, slots) in [
        (VAULT, vault_pos(), InvKind::ITEMS, 100u32),
        (1, frog_pos(), InvKind::ITEMS, 18),
    ] {
        send(
            &mut e,
            t(
                0,
                Obs::Inventory(InventoryDecl {
                    district: MILL,
                    revision: 1,
                    index,
                    pos,
                    kind: InvKind(kind),
                    item_slots: slots,
                    fluid_capacity: 0,
                    energy_capacity: 0,
                }),
            ),
        );
    }
    send(&mut e, chunk_loaded(0, 0));
    send(&mut e, chunk_loaded(0, 1));
    send(&mut e, load_state(0, LoadState::Live, 2));
    for (inventory, n) in [(VAULT, stock), (1, frog)] {
        send(
            &mut e,
            t(
                10,
                Obs::StockSample {
                    district: MILL,
                    inventory,
                    sweep: 1,
                    goods: GoodVec::from_pairs([(IRON, n)]),
                },
            ),
        );
    }
    if oil.is_some() {
        send(
            &mut e,
            t(
                10,
                Obs::DepositSample(DepositSample {
                    district: MILL,
                    good: OIL,
                    plugin: DepositPluginId::DieselOil,
                    chunk: chunks()[0],
                    remaining: 1_000_000,
                    infinite: false,
                    sweep: 1,
                }),
            ),
        );
    }
    send(
        &mut e,
        t(
            10,
            Obs::SweepEnd(SweepEnd {
                district: MILL,
                sweep: 1,
                records: if oil.is_some() { 3 } else { 2 },
                inventories: 2,
                live: true,
                creative: false,
                overstressed: false,
            }),
        ),
    );
    e.adopt_model(
        MILL,
        Learned {
            at: Tick(20),
            window_ticks: 12_000,
            rates: [(IRON, rate)]
                .into_iter()
                .chain(oil.map(|o| (OIL, o)))
                .collect(),
            links: vec![],
            confidence: 1.0,
            points: 0,
            longest_gap: 0,
            latency_ticks: 0,
        },
        vec![],
    );
    e.advance_to(Tick(50));
    let _ = e.drain_commands();
    e
}

/// A harmless journaled record: another district's declaration.
fn harmless(at: u64) -> Timed {
    t(
        at,
        Obs::District(DistrictDecl {
            district: OTHER,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: 0,
        }),
    )
}

/// What a run decided after the save.
struct Outcome {
    /// Live: the commands decided in the steps after the save.
    live: Vec<Cmd>,
    /// Replayed: the commands decided by the records of those steps and by the session start.
    replayed: Vec<Cmd>,
    /// `now` of each engine when the steps are done (the session start is not counted).
    now_live: Tick,
    now_replayed: Tick,
}

/// Runs `steps` live on a fresh mill; saves the pages after step `saved_after` (an index; the
/// steps up to it are the live prefix); restores a second engine from them and replays the later
/// steps' journaled records, then runs its session start at `reopen`.
fn run(
    mut live: Engine,
    steps: &[Step],
    saved_after: usize,
    reopen: u64,
    replayed_threats: bool,
) -> Outcome {
    let mut sink = Vec::new();
    let mut pages = None;
    let mut decided = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        let out = if i <= saved_after {
            &mut sink
        } else {
            &mut decided
        };
        live_step(&mut live, s, out);
        if i == saved_after {
            pages = Some(live.snapshot());
        }
    }
    let mut replay =
        Engine::restore(pages.expect("saved"), Tunables::default()).expect("the pages restore");
    let mut replayed = Vec::new();
    for s in &steps[saved_after + 1..] {
        replay_step(&mut replay, s, &mut replayed);
        if replayed_threats {
            // A raid re-decided by its source at the same tick, after the hand-over's records.
            for th in &s.threats {
                replay.threat(th);
            }
            replayed.extend(replay.drain_commands());
        }
    }
    let now_replayed = replay.now();
    replay.session_start(Tick(reopen));
    replayed.extend(replay.drain_commands());
    Outcome {
        live: decided,
        replayed,
        now_live: live.now(),
        now_replayed,
    }
}

fn reconciles(cmds: &[Cmd]) -> Vec<&ChunkReconcile> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::ChunkReconcile(r) => Some(r),
            _ => None,
        })
        .collect()
}

/// For each reconcile `live` decided: the replayed one of the same content, if any, and whether
/// its op id is the same. Returns the mismatches as text; empty when every live command was
/// re-decided with the same content and the same id.
fn mismatches(o: &Outcome) -> Vec<String> {
    let mut bad = Vec::new();
    let replayed = reconciles(&o.replayed);
    for l in reconciles(&o.live) {
        let want = Cmd::ChunkReconcile(l.clone()).without_op();
        let same = replayed
            .iter()
            .find(|r| Cmd::ChunkReconcile((**r).clone()).without_op() == want);
        match same {
            None => bad.push(format!(
                "live {:#x} (epoch {}, chunk {:?}, {} deltas {:?}) has no replayed command of the same content; replayed: {:?}",
                l.op.0,
                l.epoch,
                l.chunk,
                l.deltas.len(),
                l.deltas.iter().map(|d| d.delta).collect::<Vec<_>>(),
                replayed
                    .iter()
                    .map(|r| (r.op.0, r.epoch, r.chunk, r.deltas.iter().map(|d| d.delta).collect::<Vec<_>>()))
                    .collect::<Vec<_>>()
            )),
            Some(r) if r.op != l.op => bad.push(format!(
                "same content (epoch {}, chunk {:?}, deltas {:?}), live op {:#x}, replayed op {:#x}",
                l.epoch,
                l.chunk,
                l.deltas.iter().map(|d| d.delta).collect::<Vec<_>>(),
                l.op.0,
                r.op.0
            )),
            Some(_) => {}
        }
    }
    bad
}

fn assert_same(o: &Outcome, ran: usize, what: &str) {
    let live = reconciles(&o.live);
    assert!(
        live.len() >= ran,
        "{what}: the case's round did not run live ({} reconciles: {:?}); now live {:?}, replayed {:?}",
        live.len(),
        o.live,
        o.now_live,
        o.now_replayed
    );
    assert!(
        live.iter().any(|r| !r.deltas.is_empty()),
        "{what}: the live reconcile moved nothing"
    );
    let bad = mismatches(o);
    assert!(
        bad.is_empty(),
        "{what}: live and replayed differ (now at the end: live {}, replayed {}): {bad:#?}",
        o.now_live.0,
        o.now_replayed.0
    );
}

/// The mill unloads at 100 and replays for 4,900 ticks (hand-over 1, G = 5,000): the pages are
/// saved there. Hand-over 2 holds `step2`, hand-over 3 the case.
fn base_steps() -> Vec<Step> {
    let mut away = Step::new(5_000);
    away.journaled.push(load_state(100, LoadState::Unloaded, 0));
    vec![away]
}

// ---- (e) a round triggered from a record ---------------------------------------------------

/// The control: the chunks load at a tick after the previous hand-over's time, as Java stamps
/// them. `at = max(t.at, now)` is `t.at` on both paths, whatever the history, so the replayed
/// reconcile is the live one, op id and all. Holds because `ingest_obs` raises `now` to the
/// record's own tick (`fire_until(at)`) before its handler runs, and every record of a hand-over
/// is stamped at or after the hand-over before's G.
#[test]
fn a_round_from_a_record_stamped_after_the_last_hand_over_is_rederived_with_the_same_id() {
    let mut steps = base_steps();
    let mut quiet = Step::new(9_000);
    quiet.journaled.push(harmless(5_100));
    steps.push(quiet);
    let mut load = Step::new(9_500);
    load.journaled = vec![
        chunk_loaded(9_200, 0),
        chunk_loaded(9_200, 1),
        load_state(9_200, LoadState::Live, 2),
    ];
    steps.push(load);
    let o = run(mill(1_000, 0.05), &steps, 0, 20_000, false);
    assert_same(&o, 1, "chunks loaded at 9,200 after G = 9,000");
}

/// A record the Java side stamped before the previous hand-over's time reaches the next hand-over
/// (the journal's `drainInto` copies what fits the buffer and leaves the rest for the next one; a
/// REASSERT keeps its original's GAME_TIME). Live, `now` is that hand-over's G, so
/// `at = max(8,000, 9,000) = 9,000`; replayed, no `advance_to(9,000)` ran, `now` is the last
/// record's 5,100, and `at = 8,000`. The round is decided at another tick: another content (the
/// iron the replay made until then) and another id. Fails on bc669502.
///
/// Still fails after step 2: both paths now derive at the record's own 8,000 (RP-270), but the
/// live engine had run to 9,000 before it took the record, and the content (445 against 395) is
/// part of a chunk reconcile's identity, so the ids differ. That is a loss, never a double: both
/// carry the chunk's epoch, and Java's chunk epoch guard applies one reconcile per epoch
/// (`a_stale_records_round_decided_again_meets_the_live_reconciles_epoch`, RP-272). Making the
/// content the same needs Java to say a hand-over left journal records for the next one, so that
/// the live engine does not run past them (RP-279).
#[test]
#[ignore = "slice R: a record stamped before the previous G is decided on another state replayed (RP-262); a loss by the chunk epoch, never a double (RP-272)"]
fn a_round_from_a_record_stamped_before_the_last_hand_over_is_rederived_with_the_same_id() {
    let mut steps = base_steps();
    let mut quiet = Step::new(9_000);
    quiet.journaled.push(harmless(5_100));
    steps.push(quiet);
    let mut load = Step::new(9_500);
    // Left over from the hand-over before: stamped 8,000, G was 9,000.
    load.journaled = vec![
        chunk_loaded(8_000, 0),
        chunk_loaded(8_000, 1),
        load_state(8_000, LoadState::Live, 2),
    ];
    steps.push(load);
    let o = run(mill(1_000, 0.05), &steps, 0, 20_000, false);
    assert_same(&o, 1, "chunks loaded at 8,000 after G = 9,000");
}

/// The same stale record, with a content that does not depend on the tick: the mill's model
/// makes nothing, and a raid before the pages (while it is away, no replay running) left its
/// theft owed, so the reconcile's deltas are the owed ones whatever `at` is. Live and replayed
/// decide the *same command* (the precondition below holds), and the only thing left to differ is
/// the op id, which `derive` took from `self.now`: live 9,000 (the hand-over before's G), replayed
/// 5,100. Failed on bc669502 (live op 0x492cd7d0fd2f3291, replayed 0x49be17700102776d). Both now
/// derive with the record's own tick, 8,000 (RP-270): the same op.
#[test]
fn a_stale_record_gives_the_same_command_another_op_id_in_the_replay() {
    let mut steps = Vec::new();
    let mut away = Step::new(5_000);
    away.journaled.push(load_state(100, LoadState::Unloaded, 0));
    away.threats.push(Threat {
        id: 9,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: Tick(4_000),
    });
    steps.push(away);
    let mut quiet = Step::new(9_000);
    quiet.journaled.push(harmless(5_100));
    steps.push(quiet);
    let mut load = Step::new(9_500);
    load.journaled = vec![
        chunk_loaded(8_000, 0),
        chunk_loaded(8_000, 1),
        load_state(8_000, LoadState::Live, 2),
    ];
    steps.push(load);
    let o = run(mill(1_000, 0.0), &steps, 0, 20_000, false);
    let (live, replayed) = (reconciles(&o.live), reconciles(&o.replayed));
    assert!(
        !live.is_empty() && live.len() == replayed.len(),
        "the round ran on both paths: live {:?}, replayed {:?}",
        o.live,
        o.replayed
    );
    assert!(
        live.iter().any(|r| r.deltas.iter().any(|d| d.delta < 0)),
        "the raid's theft is in the reconcile: {live:?}"
    );
    for (l, r) in live.iter().zip(&replayed) {
        assert_eq!(
            Cmd::ChunkReconcile((*l).clone()).without_op(),
            Cmd::ChunkReconcile((*r).clone()).without_op(),
            "the precondition: the same command on both paths"
        );
    }
    assert_same(&o, 1, "owed theft loaded at 8,000 after G = 9,000");
}

/// The stale record of the 445 against 395 case above, decided again on another state: the two
/// reconciles differ, but each names the same chunk and the same epoch (the chunk's applied epoch,
/// which only Java's journaled results move), so Java's chunk epoch guard applies the live one and
/// refuses the replay's. The content difference is lost, never applied twice (RP-272).
#[test]
fn a_stale_records_round_decided_again_meets_the_live_reconciles_epoch() {
    let mut steps = base_steps();
    let mut quiet = Step::new(9_000);
    quiet.journaled.push(harmless(5_100));
    steps.push(quiet);
    let mut load = Step::new(9_500);
    load.journaled = vec![
        chunk_loaded(8_000, 0),
        chunk_loaded(8_000, 1),
        load_state(8_000, LoadState::Live, 2),
    ];
    steps.push(load);
    let o = run(mill(1_000, 0.05), &steps, 0, 20_000, false);
    let (live, replayed) = (reconciles(&o.live), reconciles(&o.replayed));
    assert!(
        live.iter().any(|r| !r.deltas.is_empty()),
        "the round ran live: {live:?}"
    );
    for l in &live {
        let twin = replayed.iter().find(|r| r.chunk == l.chunk);
        assert!(
            twin.is_some_and(|r| r.epoch == l.epoch),
            "the live reconcile of {:?} at epoch {} (op {:#x}) has a replayed twin at the same \
             epoch: {:?}",
            l.chunk,
            l.epoch,
            l.op.0,
            replayed
                .iter()
                .map(|r| (r.chunk, r.epoch, r.op.0))
                .collect::<Vec<_>>()
        );
    }
    // Nothing but chunk reconciles: no command guarded by its op alone carries the difference.
    assert!(
        o.replayed
            .iter()
            .all(|c| matches!(c, Cmd::ChunkReconcile(_))),
        "{:?}",
        o.replayed
    );
}

/// Stale records of two ticks: the mill's chunks load at 8,000 and 8,100 and it is live at 8,200,
/// all after G = 9,000. Away, the mill used up the vault's 300 iron and then the frogport's 600,
/// so each chunk has a change. The replay takes the records at their own ticks and settles the
/// vault's chunk before the frogport's record: two rounds, epochs 1 and 2. Live, the clock was at
/// 9,000 for all three; settled only when the clock moved (bc669502, and step 2 before RP-271's
/// trigger), both chunks went in one round at epoch 1, and the replay's frogport reconcile
/// (epoch 2, -600) carried an epoch past the one the world applied, which Java's chunk epoch guard
/// lets through: the frogport emptied twice. Settled at each record past the last decision's tick
/// (RP-271), both runs make the same rounds, at the same epochs, under the same op ids.
#[test]
fn stale_records_of_two_ticks_settle_in_the_same_rounds_live_and_replayed() {
    // A raid while the mill is away takes iron from both its inventories, the vault's chunk and
    // the frogport's; the theft is owed until each chunk loads.
    let mut away = Step::new(5_000);
    away.journaled.push(load_state(100, LoadState::Unloaded, 0));
    away.threats.push(Threat {
        id: 9,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: Tick(4_000),
    });
    let mut steps = vec![away];
    let mut quiet = Step::new(9_000);
    quiet.journaled.push(harmless(5_100));
    steps.push(quiet);
    let mut load = Step::new(9_500);
    load.journaled = vec![
        chunk_loaded(8_000, 0),
        chunk_loaded(8_100, 1),
        load_state(8_200, LoadState::Live, 2),
    ];
    steps.push(load);
    let o = run(mill_with(300, 600, -0.2), &steps, 0, 20_000, false);
    let (live, replayed) = (reconciles(&o.live), reconciles(&o.replayed));
    let pairs = |v: &[&ChunkReconcile]| {
        v.iter()
            .map(|r| (r.chunk, r.epoch, r.op.0))
            .collect::<Vec<_>>()
    };
    assert!(
        chunks()
            .iter()
            .all(|c| live.iter().any(|r| r.chunk == *c && !r.deltas.is_empty())),
        "both chunks reconciled live: {:?}",
        pairs(&live)
    );
    for l in &live {
        assert!(
            replayed
                .iter()
                .any(|r| r.chunk == l.chunk && r.epoch == l.epoch),
            "the live reconcile of {:?} at epoch {} has a replayed twin at that epoch: live {:?}, \
             replayed {:?}",
            l.chunk,
            l.epoch,
            pairs(&live),
            pairs(&replayed)
        );
    }
    let epochs = |v: &[&ChunkReconcile]| v.iter().map(|r| r.epoch).max();
    assert_eq!(
        epochs(&live),
        epochs(&replayed),
        "as many rounds: live {:?}, replayed {:?}",
        pairs(&live),
        pairs(&replayed)
    );
}

/// The same stale record with no hand-over between it and the pages: the restored engine's `now`
/// is the saved G, as the live one's: they agree. The divergence needs two hand-overs past the
/// pages (the first one's `advance_to` is what the replay lacks).
#[test]
fn a_stale_record_in_the_first_hand_over_after_the_pages_is_rederived_with_the_same_id() {
    let mut steps = base_steps();
    let mut load = Step::new(9_500);
    load.journaled = vec![
        chunk_loaded(4_000, 0),
        chunk_loaded(4_000, 1),
        load_state(4_000, LoadState::Live, 2),
    ];
    steps.push(load);
    let o = run(mill(1_000, 0.05), &steps, 0, 20_000, false);
    assert_same(
        &o,
        1,
        "stale chunks right after the pages (now = G of the save)",
    );
}

// ---- (c) latest-wins records moving `now` --------------------------------------------------

/// A deferred stock sample between two journaled records raises `now` to its tick live; the replay
/// never sees it. The partial round at the chunk's load (a `mark`, flushed by the next record of a
/// later tick) is decided at the same tick on both: `ingest_obs` flushes at the old `now` before
/// `fire_until` moves it, and a deferred record is released only before a journaled one stamped
/// after it, whose own `at` then sets `now`. Holds.
#[test]
fn a_deferred_stock_sample_between_two_records_does_not_move_the_ids() {
    let mut steps = base_steps();
    let mut quiet = Step::new(9_000);
    quiet.journaled.push(harmless(5_100));
    steps.push(quiet);
    let mut load = Step::new(9_500);
    // The vault's chunk loads at 9,100 (the district is partial: a round flushed by the next
    // record), a sample is read at 9,200, and the frogport's chunk and the district's live state
    // come at 9,300.
    load.journaled = vec![
        load_state(9_100, LoadState::Partial, 1),
        chunk_loaded(9_100, 0),
        chunk_loaded(9_300, 1),
        load_state(9_300, LoadState::Live, 2),
    ];
    load.deferred = vec![
        t(
            9_200,
            Obs::StockSample {
                district: MILL,
                inventory: VAULT,
                sweep: 2,
                goods: GoodVec::from_pairs([(IRON, 1_400)]),
            },
        ),
        t(
            9_250,
            Obs::StockSample {
                district: MILL,
                inventory: 1,
                sweep: 2,
                goods: GoodVec::from_pairs([(IRON, 0)]),
            },
        ),
    ];
    steps.push(load);
    let o = run(mill(1_000, 0.05), &steps, 0, 20_000, false);
    // The partial round at 9,100 and the full one at 9,300.
    assert_same(&o, 1, "a sample at 9,200 between two records");
}

/// A deferred sample *after* the last journaled record of the hand-over, in the same hand-over as
/// a dirty mark: live, it flushes the mark at the old `now` before it raises it. Holds, with the
/// partial round decided at the record's tick on both.
#[test]
fn a_deferred_sample_after_the_last_record_flushes_the_marks_at_the_records_tick() {
    let mut steps = base_steps();
    let mut load = Step::new(9_500);
    load.journaled = vec![
        load_state(8_000, LoadState::Partial, 1),
        chunk_loaded(8_000, 0),
    ];
    load.deferred = vec![t(
        9_300,
        Obs::StockSample {
            district: MILL,
            inventory: VAULT,
            sweep: 2,
            goods: GoodVec::from_pairs([(IRON, 1_400)]),
        },
    )];
    steps.push(load);
    // The next hand-over's first record, the frogport's chunk, closes the district.
    let mut close = Step::new(9_900);
    close.journaled = vec![
        chunk_loaded(9_600, 1),
        load_state(9_600, LoadState::Live, 2),
    ];
    steps.push(close);
    let o = run(mill(1_000, 0.05), &steps, 0, 20_000, false);
    assert_same(&o, 1, "a sample at 9,300 after the partial load at 8,000");
}

// ---- (b) the flush tick at a hand-over's end -------------------------------------------------

/// The last records of hand-over 2 mark the district dirty; live, `advance_to(G)` flushes at the
/// last record's tick (its `now`), and the replay flushes at the next hand-over's first record
/// (`at > now`, before `fire_until`), at the same `now`: the partial round is the same command.
/// Holds.
#[test]
fn the_flush_at_a_hand_overs_end_and_at_the_next_hand_overs_first_record_agree() {
    let mut steps = base_steps();
    let mut part = Step::new(9_000);
    part.journaled = vec![
        load_state(8_000, LoadState::Partial, 1),
        chunk_loaded(8_000, 0),
    ];
    steps.push(part);
    let mut rest = Step::new(9_500);
    rest.journaled = vec![
        chunk_loaded(9_100, 1),
        load_state(9_100, LoadState::Live, 2),
    ];
    steps.push(rest);
    let o = run(mill(1_000, 0.05), &steps, 0, 20_000, false);
    assert_same(&o, 1, "a partial round flushed across the hand-over's end");
}

// ---- (d) threats -----------------------------------------------------------------------------

/// A bandit raid on the live mill, delivered by `simulate` after the hand-over's records and before
/// `advance_to`; the same raid (same `at`) re-decided after the replayed hand-over. `threat_with`
/// takes `at = t.at.max(now)` and runs `advance_to(at)`, so the round (`attack_district`'s
/// `reconcile_round(Full)`) is decided at `t.at` on both paths when `t.at` is at or past the
/// engine's `now`, which a raid's source decides at the hand-over's time. Holds.
#[test]
fn a_raid_delivered_at_the_hand_overs_time_is_rederived_with_the_same_id() {
    let mut steps = Vec::new();
    let mut first = Step::new(1_000);
    first.journaled.push(harmless(200));
    steps.push(first);
    let mut quiet = Step::new(2_000);
    quiet.journaled.push(harmless(1_100));
    steps.push(quiet);
    let mut raid = Step::new(3_000);
    raid.journaled.push(harmless(2_100));
    raid.threats.push(Threat {
        id: 9,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: Tick(3_000),
    });
    steps.push(raid);
    let o = run(mill(1_000, 0.0), &steps, 0, 20_000, true);
    assert_same(&o, 1, "a raid at G = 3,000");
}

// ---- the recent-id window (RP-275) -----------------------------------------------------------

/// Two identical commands of one tick take ordinals 0 and 1 (`IndustryOps::derive`). The first is
/// decided before the pages are saved, the second after; the crash loses the second, and the
/// replay decides it again. The window of recent ids is part of the engine's globals page
/// (`e.ops.put(w)` beside `e.out`), so the restored window holds the first, and the replay's
/// second takes ordinal 1 again: the op Java applied, answered DUPLICATE. A window that restarted
/// empty (step 1's RP-267 said so; it was wrong) would give it ordinal 0, the first command's op,
/// which Java has long forgotten (its applied ops are trimmed at the acknowledgement): applied
/// twice.
#[test]
fn a_collision_before_a_save_gives_the_same_ordinal_after_a_crash() {
    use rtsim_industry::cmd::{DepositDelta, IndustryOps};
    use rtsim_industry::persist::Persist;
    use rtsim_kernel::codec::{Reader, Writer};
    use rtsim_kernel::journal::OpId;

    let oil = |delta| {
        Cmd::DepositDelta(DepositDelta {
            op: OpId(0),
            plugin: DepositPluginId::DieselOil,
            chunk: ChunkPos { dim: 0, x: 7, z: 0 },
            good: IRON,
            delta,
        })
    };
    let at = Tick(4_000);
    let mut live = IndustryOps::default();
    let first = live.derive(SEED, at, &oil(-5));
    // Other decisions between, as a busy engine makes them.
    for n in 0..40 {
        live.derive(SEED, Tick(4_000 + n), &oil(n as i64 + 1));
    }
    // The save: the window as the engine's globals page carries it.
    let mut w = Writer::new();
    live.put(&mut w);
    let page = w.into_bytes();
    let second = live.derive(SEED, at, &oil(-5));
    assert_ne!(second, first, "the collision takes another ordinal");
    // The crash and the replay: the window from the page, the second decided again.
    let mut r = Reader::new(&page);
    let mut replay: IndustryOps = Persist::get(&mut r).expect("the window decodes");
    assert_eq!(replay, {
        let mut r = Reader::new(&page);
        Persist::get(&mut r).expect("again")
    });
    assert_eq!(
        replay.derive(SEED, at, &oil(-5)),
        second,
        "the same ordinal after the crash"
    );
    // The control: an empty window gives the first one's op.
    assert_eq!(IndustryOps::default().derive(SEED, at, &oil(-5)), first);
}

// ---- the review of 2a975cc3 (docs/choices-mowzie-build.md RP-259) ------------------------------

fn deposits(v: &[Cmd]) -> Vec<(u64, i64)> {
    v.iter()
        .filter_map(|c| match c {
            Cmd::DepositDelta(d) => Some((d.op.0, d.delta)),
            _ => None,
        })
        .collect()
}

fn chunk_rounds(v: &[Cmd]) -> Vec<(u64, u32, ChunkPos, Vec<i64>)> {
    reconciles(v)
        .iter()
        .map(|r| {
            (
                r.op.0,
                r.epoch,
                r.chunk,
                r.deltas.iter().map(|d| d.delta).collect(),
            )
        })
        .collect()
}

/// The oil mill of the review: away from 100 with a raid at 4,000 (hand-over 1, G = 5,000), a
/// harmless record (hand-over 2, G = 9,000), then its chunks load at 8,000 and 8,100 and it is live
/// at 8,200, all stale (hand-over 3, G = 9,500).
fn oil_steps() -> Vec<Step> {
    let mut away = Step::new(5_000);
    away.journaled.push(load_state(100, LoadState::Unloaded, 0));
    away.threats.push(Threat {
        id: 9,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: Tick(4_000),
    });
    let mut quiet = Step::new(9_000);
    quiet.journaled.push(harmless(5_100));
    let mut load = Step::new(9_500);
    load.journaled = vec![
        chunk_loaded(8_000, 0),
        chunk_loaded(8_100, 1),
        load_state(8_200, LoadState::Live, 2),
    ];
    vec![away, quiet, load]
}

fn oil_mill() -> Engine {
    mill_oil(300, 600, -0.2, Some(-0.5))
}

/// `Engine::decided` is saved with the pages (P1-1 of the review): pages can be taken in the middle
/// of a hand-over, right before the first record past a world save's journal (rtsim-jni
/// engine.rs, `journaled`), so after a stale record whose marks wait for the next record. Here the
/// pages come right after the vault's chunk loads (8,000, stale: G was 9,000). Live, the
/// frogport's record at 8,100 settles the vault's chunk at 8,000 first, then its own: two rounds.
/// On 2a975cc3 the restored engine started `decided` at `now` (9,000), so neither later record
/// was past it, the marks merged into one round, and the round's deposit delta was decided at
/// another tick: live 0x4962c4fe7b7be699, replayed 0x49274e3eaf93756b, both -4,450 (the well
/// drawn twice), and the frogport's chunk at epoch 1 where the world applied 2.
#[test]
fn pages_taken_after_a_stale_record_rederive_its_rounds() {
    let steps = oil_steps();
    let mut e = oil_mill();
    let mut sink = Vec::new();
    live_step(&mut e, &steps[0], &mut sink);
    live_step(&mut e, &steps[1], &mut sink);
    // Hand-over 3, live, with the pages taken after its first record.
    let load = &steps[2];
    e.ingest(&load.journaled[0]);
    let pages = e.snapshot();
    let mut live = Vec::new();
    for rec in &load.journaled[1..] {
        e.ingest(rec);
    }
    e.advance_to(Tick(load.g));
    live.extend(e.drain_commands());
    let mut replay = Engine::restore(pages, Tunables::default()).expect("the pages restore");
    for rec in &load.journaled[1..] {
        replay.ingest(rec);
    }
    replay.session_start(Tick(20_000));
    let replayed = replay.drain_commands();
    let (l, r) = (deposits(&live), deposits(&replayed));
    let what = format!(
        "live deposits {l:x?} rounds {:x?}; replayed deposits {r:x?} rounds {:x?}",
        chunk_rounds(&live),
        chunk_rounds(&replayed)
    );
    assert!(!l.is_empty(), "a deposit delta live: {what}");
    assert_eq!(l, r, "the same deposit deltas under the same ops: {what}");
    assert_eq!(
        chunk_rounds(&live),
        chunk_rounds(&replayed),
        "the same rounds: {what}"
    );
}

/// The same stale records with the pages at the step boundary before them (G = 5,000): the
/// restored engine's clock is behind the live one's (5,000 against 9,000) when it takes them, so
/// a round's immediate deposit delta derived with `now` would differ (the mutation check of
/// RP-270's site in `reconcile_round`, P2-2 of the review). Derived with the record's tick, both
/// runs send the same ops. Their amounts differ (live -4,450, replayed -3,950): the live engine
/// ran its clock to 9,000 before it took the records (RP-262, RP-279), so the op is answered
/// DUPLICATE and the difference is lost (RP-272).
#[test]
fn stale_records_rederive_their_rounds_deposit_deltas() {
    let o = run(oil_mill(), &oil_steps(), 0, 20_000, false);
    let (l, r) = (deposits(&o.live), deposits(&o.replayed));
    let what = format!(
        "live deposits {l:x?} rounds {:x?}; replayed deposits {r:x?} rounds {:x?}; now live {}, \
         replayed {}",
        chunk_rounds(&o.live),
        chunk_rounds(&o.replayed),
        o.now_live.0,
        o.now_replayed.0
    );
    assert!(!l.is_empty(), "a deposit delta live: {what}");
    let ops = |v: &[(u64, i64)]| v.iter().map(|d| d.0).collect::<Vec<_>>();
    assert_eq!(ops(&l), ops(&r), "the same deposit ops: {what}");
    assert_eq!(
        chunk_rounds(&o.live),
        chunk_rounds(&o.replayed),
        "the same rounds: {what}"
    );
}

/// A district with a vault and a frogport `x0` blocks east of the mill's, live, learned to draw
/// `oil` a tick from a diesel well on the mill's vault chunk ([`chunks`]`[0]`), which it shares.
fn oil_district(e: &mut Engine, d: DistrictId, x0: i32, oil: f64) {
    let send = |e: &mut Engine, rec: Timed| e.ingest(&rec);
    send(
        e,
        t(
            0,
            Obs::District(DistrictDecl {
                district: d,
                revision: 1,
                dim: 0,
                name: 0,
                not_learnable: None,
                inventory_count: 2,
            }),
        ),
    );
    let at = |p: BlockPos| BlockPos::new(p.x + x0, p.y, p.z);
    for (index, pos, slots) in [(VAULT, at(vault_pos()), 100u32), (1, at(frog_pos()), 18)] {
        send(
            e,
            t(
                0,
                Obs::Inventory(InventoryDecl {
                    district: d,
                    revision: 1,
                    index,
                    pos,
                    kind: InvKind(InvKind::ITEMS),
                    item_slots: slots,
                    fluid_capacity: 0,
                    energy_capacity: 0,
                }),
            ),
        );
    }
    for c in district_chunks(x0) {
        send(
            e,
            t(
                0,
                Obs::ChunkLoaded {
                    district: d,
                    chunk: c,
                    applied_epoch: 0,
                },
            ),
        );
    }
    send(e, district_load(0, d, LoadState::Live, 2));
    for inventory in [VAULT, 1] {
        send(
            e,
            t(
                10,
                Obs::StockSample {
                    district: d,
                    inventory,
                    sweep: 1,
                    goods: GoodVec::from_pairs([(IRON, 100)]),
                },
            ),
        );
    }
    send(
        e,
        t(
            10,
            Obs::DepositSample(DepositSample {
                district: d,
                good: OIL,
                plugin: DepositPluginId::DieselOil,
                chunk: chunks()[0],
                remaining: 1_000_000,
                infinite: false,
                sweep: 1,
            }),
        ),
    );
    send(
        e,
        t(
            10,
            Obs::SweepEnd(SweepEnd {
                district: d,
                sweep: 1,
                records: 3,
                inventories: 2,
                live: true,
                creative: false,
                overstressed: false,
            }),
        ),
    );
    e.adopt_model(
        d,
        Learned {
            at: Tick(20),
            window_ticks: 12_000,
            rates: vec![(OIL, oil)],
            links: vec![],
            confidence: 1.0,
            points: 0,
            longest_gap: 0,
            latency_ticks: 0,
        },
        vec![],
    );
}

fn district_chunks(x0: i32) -> Vec<ChunkPos> {
    let mut v: Vec<ChunkPos> = [vault_pos(), frog_pos()]
        .iter()
        .map(|p| ChunkPos {
            dim: 0,
            x: (p.x + x0) >> 4,
            z: p.z >> 4,
        })
        .collect();
    v.dedup();
    v
}

fn district_load(at: u64, d: DistrictId, state: LoadState, ticking: u32) -> Timed {
    t(
        at,
        Obs::LoadState {
            district: d,
            state,
            ticking_chunks: ticking,
            total_chunks: 2,
        },
    )
}

/// The review's table (RP-272): two different decisions under one key at one tick, in a live
/// run. A Mail pool's packages cannot be (RP-272 says why); deposit deltas can: two districts
/// drawing on one diesel well both load at one tick, and one `flush_dirty` writes back both
/// draws, the same plugin, chunk and good. On 2a975cc3 they had one key and the ordinal told
/// them apart; since RP-275 the district is in the key. Either way two ops: Java, which guards a
/// deposit delta by its op alone, applies both.
#[test]
fn two_districts_on_one_well_write_back_at_one_tick_and_both_apply() {
    const WELL: DistrictId = DistrictId(3);
    let mut e = Engine::new(SEED, Tunables::default(), Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    e.define_good(OIL, GoodInfo::deposit("diesel:oil/00000000000000aa"));
    oil_district(&mut e, MILL, 0, -0.5);
    oil_district(&mut e, WELL, 64, -0.3);
    e.advance_to(Tick(50));
    let _ = e.drain_commands();
    for d in [MILL, WELL] {
        e.ingest(&district_load(100, d, LoadState::Unloaded, 0));
    }
    e.advance_to(Tick(5_000));
    let _ = e.drain_commands();
    for (d, x0) in [(MILL, 0), (WELL, 64)] {
        for c in district_chunks(x0) {
            e.ingest(&t(
                5_000,
                Obs::ChunkLoaded {
                    district: d,
                    chunk: c,
                    applied_epoch: 0,
                },
            ));
        }
        e.ingest(&district_load(5_000, d, LoadState::Live, 2));
    }
    e.advance_to(Tick(5_001));
    let out = e.drain_commands();
    let wells: Vec<&rtsim_industry::cmd::DepositDelta> = out
        .iter()
        .filter_map(|c| match c {
            Cmd::DepositDelta(x) => Some(x),
            _ => None,
        })
        .collect();
    assert_eq!(
        wells.len(),
        2,
        "both districts write back their draw: {out:?}"
    );
    assert_eq!(
        (wells[0].plugin, wells[0].chunk, wells[0].good),
        (wells[1].plugin, wells[1].chunk, wells[1].good),
        "one well"
    );
    assert_ne!(wells[0].delta, wells[1].delta, "two different decisions");
    // Java's guard is the op id alone: both apply.
    let applied: std::collections::HashSet<u64> = wells.iter().map(|x| x.op.0).collect();
    assert_eq!(applied.len(), 2, "two ops: {wells:?}");
}

// ---- no floor for the chunk epochs (P2-1 of the review, RP-259) ---------------------------

/// The chunks load with the epochs their attachments hold, and the mill is live again.
fn reload(at: u64, epochs: [u32; 2]) -> Vec<Timed> {
    let mut v: Vec<Timed> = epochs
        .iter()
        .enumerate()
        .map(|(i, &applied_epoch)| {
            t(
                at,
                Obs::ChunkLoaded {
                    district: MILL,
                    chunk: chunks()[i],
                    applied_epoch,
                },
            )
        })
        .collect();
    v.push(load_state(at, LoadState::Live, 2));
    v
}

/// Each chunk's reconcile epochs in `cmds`, by chunk index.
fn epochs_by_chunk(cmds: &[Cmd]) -> [Vec<u32>; 2] {
    let mut out = [Vec::new(), Vec::new()];
    for r in reconciles(cmds) {
        let i = chunks()
            .iter()
            .position(|c| *c == r.chunk)
            .expect("a mill chunk");
        out[i].push(r.epoch);
    }
    out
}

/// A round decided after the pages, applied by the world on the vault's chunk at epoch 1, which
/// Minecraft then saved with the chunk; the crash lost the journal's tail (review finding 13: the
/// chunk's save came before the journal's), so the replay has none of its records. After the
/// session start the chunks load with the vault's at epoch 1 in its attachment, and the engine
/// decides the same absence's round again, with other content (the iron of a longer span). It
/// takes epoch 1 again, so Java refuses it: the difference is lost, never applied twice (RP-272).
/// The review's P2-1 floor (IND_CHUNK_LOADED's applied epoch raising the chunk's counter) gave it
/// epoch 2, which Java applies: in rtsim-jni's
/// `a_reconcile_in_a_chunk_saved_ahead_of_the_journal_is_never_applied_twice` the world then made
/// 3,840 bars from 3,841 ore a second time. The same on 2a975cc3: a control, not a regression test.
#[test]
fn a_round_decided_again_after_a_crash_meets_the_epoch_its_chunk_holds() {
    let mut steps = base_steps();
    let mut load = Step::new(9_500);
    load.journaled = reload(9_200, [0, 0]);
    steps.push(load);
    let mut live = mill(1_000, 0.05);
    let mut sink = Vec::new();
    live_step(&mut live, &steps[0], &mut sink);
    let pages = live.snapshot();
    let mut decided = Vec::new();
    live_step(&mut live, &steps[1], &mut decided);
    assert_eq!(
        epochs_by_chunk(&decided),
        [vec![1], vec![]],
        "live, one round on the vault's chunk at epoch 1 (the frogport's moved nothing): {decided:?}"
    );
    let mut replay = Engine::restore(pages, Tunables::default()).expect("the pages restore");
    replay.session_start(Tick(20_000));
    let mut again = replay.drain_commands();
    for rec in reload(20_000, [1, 0]) {
        replay.ingest(&rec);
    }
    replay.advance_to(Tick(20_100));
    again.extend(replay.drain_commands());
    let live_deltas: Vec<Vec<i64>> = reconciles(&decided)
        .iter()
        .map(|r| r.deltas.iter().map(|d| d.delta).collect())
        .collect();
    let replayed_deltas: Vec<Vec<i64>> = reconciles(&again)
        .iter()
        .map(|r| r.deltas.iter().map(|d| d.delta).collect())
        .collect();
    assert_ne!(
        live_deltas, replayed_deltas,
        "the round decided again has other content"
    );
    assert_eq!(
        epochs_by_chunk(&again),
        [vec![1], vec![]],
        "the round decided again meets the epoch its chunk holds, which Java refuses: {:?}",
        chunk_rounds(&again)
    );
}

/// An IND_RECONCILE_RESULT for a round the engine does not wait for (`duplicate_results`: the
/// replay decides that round after the result's record, from a latest-wins record or in the held
/// step) raises no floor either: the round decided again meets the result's epoch, and Java
/// refuses it. A floor would give it epoch 2, applied a second time. The same on 2a975cc3.
#[test]
fn a_result_the_engine_does_not_wait_for_raises_no_epoch() {
    let mut e = mill(1_000, 0.05);
    e.ingest(&t(
        60,
        Obs::ReconcileResult(rtsim_industry::obs::ReconcileResult {
            op: rtsim_kernel::journal::OpId(0x5eed),
            district: MILL,
            epoch: 1,
            chunk: chunks()[0],
            entries: vec![],
            destroyed: vec![],
            blocks_skipped: 0,
        }),
    ));
    assert_eq!(
        e.stats.duplicate_results, 1,
        "an op the engine does not hold"
    );
    e.ingest(&load_state(100, LoadState::Unloaded, 0));
    e.advance_to(Tick(5_000));
    let _ = e.drain_commands();
    for rec in reload(5_000, [1, 0]) {
        e.ingest(&rec);
    }
    e.advance_to(Tick(5_100));
    let out = e.drain_commands();
    assert_eq!(
        epochs_by_chunk(&out),
        [vec![1], vec![]],
        "the vault's chunk at its own next epoch, the result's: {:?}",
        chunk_rounds(&out)
    );
}
