// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M11 (ledger 24, §4.5 "When it has a body"): the grove's
//! chief goes with its band as a stack of its own behind an away CHIEF_BODY, comes home by a
//! HAND_BACK, is paid as a puppet, and is booked once whatever ends it. Played against the emulated
//! plugin (`synth::CampRun`) on the shipped cultures.

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{Cultures, PartyKind};
use rtsim_conflict::engine::{CHIEF_SLOT, Conflict, Party, Phase, SlotState, Target, body_id};
use rtsim_conflict::synth::{self, CampRun};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_settle::Pos;

// ---- set-up -------------------------------------------------------------------------------------

/// A grove that never plunders, so the only parties are the ones a test orders.
fn quiet_grove(seed: u64) -> CampRun {
    let mut cultures = Cultures::builtin();
    for k in &mut cultures.list {
        k.def.plunder_mean_days = None;
    }
    CampRun::with_cultures(seed, cultures, "umvuthana", 9)
}

fn ok(r: &mut CampRun) {
    let bad = r.check();
    assert!(bad.is_empty(), "the invariants break: {bad:#?}");
    assert!(r.failures.is_empty(), "{:#?}", r.failures);
}

/// The camp comes in and its first lines are answered.
fn come_in(r: &mut CampRun) {
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
}

/// The chief's slot: its state and generation.
fn chief(r: &CampRun) -> Option<(SlotState, u32)> {
    r.c.camp(r.f)
        .and_then(|c| c.chief())
        .map(|s| (s.state, s.generation))
}

/// The camp comes in and waits out its first heir: a seated Umvuthi, its generation.
fn seat_chief(r: &mut CampRun) -> u32 {
    come_in(r);
    let due = r.c.camp(r.f).and_then(|c| c.heir).expect("an heir").due;
    r.run(due + 100);
    let (state, generation) = chief(r).expect("seated");
    assert_eq!(state, SlotState::Body, "seated");
    generation
}

/// A player's payment of 9 gold nuggets to the seated Umvuthi at home: the payer joins the camp's
/// memory (MZB-418), so a CHIEF_BODY that carries it is told apart from one that carries nothing.
fn pay_at_home(r: &mut CampRun, generation: u32) {
    let g =
        r.w.goods
            .find("gold_nugget")
            .expect("a good of the synthetic world");
    let id = rtsim_conflict::engine::item_of(g);
    r.c.define(2, id, "gold_nugget");
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::ChiefPaid(boundary::ChiefPaid {
            faction: r.key,
            slot: CHIEF_SLOT,
            generation,
            flags: 0,
            player: synth::PLAYER,
            op: 0x5111,
            items: vec![boundary::Item { item: id, count: 9 }],
        }),
        at,
    );
    let traded = &r.c.camp(r.f).expect("a camp").traded;
    assert_eq!(traded, &vec![synth::PLAYER], "the payer is remembered");
}

/// An Invasion led by the seated Umvuthi, paid once at home, ordered now on the refinery and out on
/// the road past its muster: the party.
fn led_invasion_out(seed: u64) -> (CampRun, Handle<Party>) {
    let mut r = quiet_grove(seed);
    let generation = seat_chief(&mut r);
    pay_at_home(&mut r, generation);
    let at = r.c.now();
    let target = Target::District(synth::REFINERY, synth::REFINERY_POS);
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, target, None, at)
            .expect("an invasion");
    r.flush(at);
    assert!(r.c.parties[p].chief, "the chief leads");
    let muster = r.c.parties[p].until;
    r.run(muster + 200);
    assert_eq!(r.c.parties[p].phase, Phase::Out, "on the road");
    (r, p)
}

/// The engine's body id of the grove's chief.
fn chief_body(r: &CampRun) -> u32 {
    let ci = r.c.factions[r.f as usize].culture;
    let name =
        r.c.cultures
            .get(ci)
            .def
            .chief_body
            .clone()
            .expect("a chief body");
    body_id(&r.c.cultures, &name)
}

fn here(r: &CampRun, p: Handle<Party>) -> Pos {
    let at = r.c.now();
    let (x, z) = r.c.party_pos(p, at);
    Pos::new(synth::CAMP_POS.frame, x, z)
}

/// The scripted player meets the band where it is now, reported to the engine as the bridge
/// reports a player: the commands that took.
fn meet(r: &mut CampRun, p: Handle<Party>) -> Vec<Cmd> {
    let at = r.c.now();
    let pos = here(r, p);
    let mark = r.cmds.len();
    r.c.observe_player(&mut r.w, synth::PLAYER, pos, at);
    // The meeting is an event of the engine's wheel, due now.
    r.run(at + 2);
    r.cmds[mark..].iter().map(|c| c.1.clone()).collect()
}

/// The commands sent from `mark` on.
fn since(r: &CampRun, mark: usize) -> Vec<Cmd> {
    r.cmds[mark..].iter().map(|c| c.1.clone()).collect()
}

/// The away CHIEF_BODYs of `agent` in `cmds`, checked, with the MATERIALIZE each leads. Each is the
/// party's (the camp's key, health 0, flags 0, the camp's memory) and goes just ahead of the
/// MATERIALIZE of its agent and epoch, which has BAND_CHIEF and, last, the stack `{ chief body, 0,
/// MEMBER_CHIEF }`; and a MATERIALIZE of the agent has a chief stack exactly when a CHIEF_BODY leads
/// it. Returns the MATERIALIZEs led.
fn chief_pairs(r: &CampRun, cmds: &[Cmd], agent: u64) -> Vec<boundary::BandMaterialize> {
    let chief = boundary::Stack {
        body: chief_body(r),
        count: 0,
        flags: boundary::MEMBER_CHIEF,
    };
    let traded = r.c.camp(r.f).map(|c| c.traded.clone()).unwrap_or_default();
    let mut out = Vec::new();
    for (i, c) in cmds.iter().enumerate() {
        match c {
            Cmd::ChiefBody(b) if b.agent == agent => {
                let Some(Cmd::Materialize(m)) = cmds.get(i + 1) else {
                    panic!("an away CHIEF_BODY not just ahead of a MATERIALIZE: {cmds:#?}");
                };
                assert_eq!(
                    (m.agent, u64::from(m.epoch)),
                    (b.agent, b.epoch),
                    "one band, one epoch"
                );
                assert_eq!(b.faction, r.key);
                assert_eq!(
                    (b.health, b.flags),
                    (0.0, 0),
                    "health 0, the body's own; no flags"
                );
                assert_eq!(b.traded, traded, "the camp's memory");
                assert_ne!(m.flags & boundary::BAND_CHIEF, 0, "BAND_CHIEF");
                assert_eq!(m.members.last(), Some(&chief), "the chief's stack, last");
                out.push(m.clone());
            }
            Cmd::Materialize(m) if m.agent == agent => {
                let led = i > 0 && matches!(&cmds[i - 1], Cmd::ChiefBody(b) if b.agent == agent);
                let stacks = m
                    .members
                    .iter()
                    .filter(|s| s.flags & boundary::MEMBER_CHIEF != 0)
                    .count();
                assert_eq!(
                    stacks,
                    usize::from(led),
                    "a chief stack only behind a CHIEF_BODY"
                );
            }
            _ => {}
        }
    }
    out
}

/// The members a MATERIALIZE carries that are units: every stack but the chief's.
fn units_in(m: &boundary::BandMaterialize) -> u32 {
    m.members
        .iter()
        .filter(|s| s.flags & boundary::MEMBER_CHIEF == 0)
        .map(|s| s.count)
        .sum()
}

fn placed(r: &CampRun, p: Handle<Party>) -> u32 {
    r.c.parties[p].spawned.iter().map(|e| e.1).sum()
}

/// The band arrives at its target a tick from now with the scripted player standing there, set
/// through the world as the bridge reports a player who has not met it on the road: a watched
/// fight. Returns the tick of the arrival.
fn player_at_the_target(r: &mut CampRun, p: Handle<Party>) -> Tick {
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(Tick(arrive.0 - 1));
    let at = r.c.now();
    let player = r.player;
    r.w.set_player_pos(player, synth::REFINERY_POS, at);
    arrive
}

// ---- A1: the chief's stack and its CHIEF_BODY ---------------------------------------------------

/// A1 (MZB-801, MZB-803): each first MATERIALIZE with bodies of an epoch of a band its Umvuthi
/// leads goes just behind an away CHIEF_BODY (the band's agent and epoch, health 0, flags 0, the
/// camp's memory) and carries BAND_CHIEF and, last, the chief alone, `{ umvuthi, 0, MEMBER_CHIEF
/// }`: met on the road (`seen`), at its target (`start_fight`), at a later wave after Java refused
/// the first, and on its way home. The chief is in no fill: `spawned` and the waves count the
/// units only. A walk-in and a second wave of the epoch send neither. Without the change no
/// CHIEF_BODY and no chief stack goes (the pairs found are none).
#[test]
fn the_chief_body_goes_ahead_of_the_chiefs_stack() {
    // 1. On the road, then walked into its fight.
    let (mut r, p) = led_invasion_out(81);
    let agent = Party::agent(p);
    let waves = r.c.stats.waves;
    let cmds = meet(&mut r, p);
    let pairs = chief_pairs(&r, &cmds, agent);
    assert_eq!(pairs.len(), 1, "met on the road: one pair");
    assert_eq!(pairs[0].epoch, r.c.parties[p].epoch);
    assert_eq!(
        units_in(&pairs[0]),
        placed(&r, p),
        "the chief is in no fill"
    );
    assert_eq!(
        r.c.stats.waves,
        waves + 1,
        "one wave, the chief not counted apart"
    );
    // The player follows the band to its target: it walks into its fight in the same bodies.
    let arrive = *r.c.parties[p].times.last().expect("a route");
    let mark = r.cmds.len();
    while r.c.now() < arrive + 5 {
        let at = r.c.now();
        let pos = here(&r, p);
        r.c.observe_player(&mut r.w, synth::PLAYER, pos, at);
        let to = at + 50;
        r.run(to);
    }
    assert_eq!(r.c.parties[p].phase, Phase::Fighting, "a fight");
    let walked = since(&r, mark);
    assert!(
        walked
            .iter()
            .any(|c| matches!(c, Cmd::Materialize(m) if m.agent == agent)),
        "the walked-in fight's marker went"
    );
    assert!(
        chief_pairs(&r, &walked, agent).is_empty(),
        "a walk-in sends no chief"
    );
    ok(&mut r);

    // 2. At its target, unmet on the road; 3. the fight's later waves, one after Java refused the
    // first. With a quota of 2 the scripted player's kills make room for a second wave.
    for refused in [false, true] {
        let (mut r, p) = led_invasion_out(82);
        r.c.tun.spawn_quota = 2;
        r.plugin.fight = Some(synth::Fight {
            kill_share: 1.0,
            after: 10,
            kill_chief: false,
            only_at_base: false,
        });
        let agent = Party::agent(p);
        let arrive = player_at_the_target(&mut r, p);
        let mark = r.cmds.len();
        let first = if refused {
            let cmds = r.decide(arrive + 1);
            let pairs = chief_pairs(&r, &cmds, agent);
            assert_eq!(pairs.len(), 1, "the fight's first wave");
            let m = &pairs[0];
            let at = r.c.now();
            r.c.ingest(
                &mut r.w,
                &mut NoInfrastructure,
                &Obs::SpawnRejected(boundary::BandSpawnRejected {
                    agent,
                    epoch: m.epoch,
                    wave: 0,
                    reason: 1,
                    cmd_seq: 0,
                }),
                at,
            );
            assert_eq!(placed(&r, p), 0, "refused: nothing placed");
            m.clone()
        } else {
            r.run(arrive + 1);
            let cmds = since(&r, mark);
            let pairs = chief_pairs(&r, &cmds, agent);
            assert_eq!(pairs.len(), 1, "the fight's first wave");
            assert_eq!(units_in(&pairs[0]), 2, "the quota's 2");
            assert_eq!(placed(&r, p), 2, "the chief is in no fill");
            pairs[0].clone()
        };
        assert_eq!(r.c.parties[p].phase, Phase::Fighting, "refused {refused}");
        let epoch = first.epoch;
        // The next wave: the scripted player's kills put it a wave interval after them.
        let mark = r.cmds.len();
        let limit = r.c.now() + 2 * r.c.tun.wave_interval_ticks;
        let materialized = |r: &CampRun| {
            r.cmds[mark..]
                .iter()
                .any(|(_, c)| matches!(c, Cmd::Materialize(m) if m.agent == agent))
        };
        while !materialized(&r) && r.c.now() < limit {
            let to = r.c.now() + 10;
            r.run(to);
        }
        let later = since(&r, mark);
        let pairs = chief_pairs(&r, &later, agent);
        let waves: Vec<&boundary::BandMaterialize> = later
            .iter()
            .filter_map(|c| match c {
                Cmd::Materialize(m) if m.agent == agent => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(waves.len(), 1, "refused {refused}: one later wave");
        assert_eq!(waves[0].epoch, epoch, "refused {refused}: the same epoch");
        assert_ne!(
            waves[0].flags & boundary::BAND_CHIEF,
            0,
            "BAND_CHIEF on every wave"
        );
        if refused {
            assert_eq!(
                pairs.len(),
                1,
                "after the refusal the chief goes with the next wave"
            );
            assert_eq!(units_in(waves[0]), 2, "the quota's 2");
        } else {
            assert!(
                pairs.is_empty(),
                "a second wave of the epoch sends no chief"
            );
            assert_eq!(units_in(waves[0]), 2, "the second wave's room");
        }
    }

    // 4. On its way home, unmet before.
    let (mut r, p) = led_invasion_out(83);
    let agent = Party::agent(p);
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(arrive + 400);
    assert_eq!(r.c.parties[p].phase, Phase::Home, "walking home");
    assert!(!r.c.parties[p].embodied, "unmet");
    let cmds = meet(&mut r, p);
    let pairs = chief_pairs(&r, &cmds, agent);
    assert_eq!(pairs.len(), 1, "met on its way home: one pair");
    assert_eq!(units_in(&pairs[0]), placed(&r, p));
    ok(&mut r);
}

/// A1's controls: the Frostmaw, whose chief is its band's one body, sends no CHIEF_BODY and keeps
/// MEMBER_CHIEF on its first stack; a grove's raid, which its Umvuthi does not lead, sends neither.
#[test]
fn a_chief_that_is_a_band_body_and_a_band_without_its_chief_send_no_chief_body() {
    let mut r = CampRun::new(67, "frostmaw", 1);
    come_in(&mut r);
    r.watch = true;
    let p = r.raid().expect("a raid goes");
    r.camp_out();
    let agent = Party::agent(p);
    let limit = r.c.now() + 4 * TICKS_PER_DAY;
    let first = loop {
        let m = r.cmds.iter().find_map(|(_, c)| match c {
            Cmd::Materialize(m) if m.agent == agent => Some(m.clone()),
            _ => None,
        });
        if let Some(m) = m {
            break m;
        }
        assert!(r.c.now() < limit, "the Frostmaw is seen");
        let to = r.c.now() + 100;
        r.run(to);
    };
    assert!(
        !r.cmds.iter().any(|(_, c)| matches!(c, Cmd::ChiefBody(_))),
        "no CHIEF_BODY"
    );
    assert_ne!(first.flags & boundary::BAND_CHIEF, 0);
    assert_eq!(
        first.members[0].flags,
        boundary::MEMBER_CHIEF,
        "its first stack is the chief"
    );

    let mut r = quiet_grove(84);
    seat_chief(&mut r);
    let p = r.raid().expect("a raid");
    assert!(!r.c.parties[p].chief, "the Umvuthi does not lead a raid");
    let muster = r.c.parties[p].until;
    r.run(muster + 200);
    let cmds = meet(&mut r, p);
    let agent = Party::agent(p);
    let m = cmds
        .iter()
        .find_map(|c| match c {
            Cmd::Materialize(m) if m.agent == agent => Some(m.clone()),
            _ => None,
        })
        .expect("the raid is seen");
    assert!(
        !cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))),
        "no CHIEF_BODY"
    );
    assert_eq!(m.flags & boundary::BAND_CHIEF, 0, "no BAND_CHIEF");
    assert!(
        m.members
            .iter()
            .all(|s| s.flags & boundary::MEMBER_CHIEF == 0)
    );
    ok(&mut r);
}

// ---- A2: a chief never appears alone ------------------------------------------------------------

/// A2 (MZB-802): an Invasion whose warriors are all dead while its chief lives sends nothing when
/// met: no MATERIALIZE and no CHIEF_BODY, its Umvuthi abstract until its band has a body to show.
/// The control, one warrior left, sends both (and fails without the change: no CHIEF_BODY).
#[test]
fn a_band_with_only_its_chief_left_sends_nothing() {
    for left in [0, 1] {
        let (mut r, p) = led_invasion_out(85);
        let agent = Party::agent(p);
        let mut keep = left;
        for u in &mut r.c.parties[p].units {
            let k = u.1.min(keep);
            keep -= k;
            u.1 = k;
        }
        assert_eq!(r.c.parties[p].alive(), left);
        assert!(r.c.parties[p].chief, "its chief lives");
        let cmds = meet(&mut r, p);
        let materializes = cmds
            .iter()
            .filter(|c| matches!(c, Cmd::Materialize(m) if m.agent == agent))
            .count();
        let pairs = chief_pairs(&r, &cmds, agent);
        if left == 0 {
            assert_eq!(materializes, 0, "no MATERIALIZE");
            assert!(
                !cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))),
                "no CHIEF_BODY"
            );
        } else {
            assert_eq!((materializes, pairs.len()), (1, 1), "one warrior: both");
        }
    }
}

// ---- A3: the gate drops the pair ----------------------------------------------------------------

/// A3 (MZB-804): an engine with an away CHIEF_BODY and its MATERIALIZE queued (its band met on its
/// way home, the commands not taken), and a home CHIEF_BODY queued after them (the band home at a
/// grove that is in), saved to pages and loaded: the queue holds all three, and at `begin_replay`
/// the pair is dropped and the home record kept. A meeting while replaying pushes neither
/// (`deferred` rises, `never` stays 0). Without the split the away CHIEF_BODY took the home row's
/// keep and outlived its MATERIALIZE.
#[test]
fn an_away_chief_body_is_dropped_with_its_materialize() {
    let (mut r, p) = led_invasion_out(86);
    let agent = Party::agent(p);
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(arrive + 400);
    assert_eq!(r.c.parties[p].phase, Phase::Home, "walking home");
    // Met on its way home, near enough to the camp that it is home before the met band's bodies are
    // gone; nothing is taken from here on.
    let home = *r.c.parties[p].times.last().expect("a route home");
    r.run(Tick(home.0 - 300));
    let at = r.c.now();
    let pos = here(&r, p);
    r.c.observe_player(&mut r.w, synth::PLAYER, pos, at);
    r.c.run_until(&mut r.w, &mut NoInfrastructure, at + 2);
    assert!(r.c.parties[p].embodied, "met");
    let epoch = r.c.parties[p].epoch;
    r.c.run_until(&mut r.w, &mut NoInfrastructure, home + 5);
    assert!(!r.c.parties.contains(p), "home");
    let pages = r.c.save();
    let load = || Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).expect("loads");

    let queued = load().take_commands();
    let away = |c: &Cmd| matches!(c, Cmd::ChiefBody(b) if b.agent == agent);
    let home_body = |c: &Cmd| matches!(c, Cmd::ChiefBody(b) if b.agent == 0);
    let band = |c: &Cmd| matches!(c, Cmd::Materialize(m) if m.agent == agent);
    let i = queued
        .iter()
        .position(away)
        .expect("the away CHIEF_BODY is queued");
    assert!(band(&queued[i + 1]), "just ahead of its MATERIALIZE");
    match &queued[i] {
        Cmd::ChiefBody(b) => assert_eq!(b.epoch, u64::from(epoch)),
        _ => unreachable!(),
    }
    assert!(
        queued[i..].iter().any(home_body),
        "the home CHIEF_BODY is queued after them: {queued:#?}"
    );

    let mut c = load();
    c.begin_replay(true);
    let kept = c.take_commands();
    assert!(!kept.iter().any(away), "the away CHIEF_BODY is dropped");
    assert!(!kept.iter().any(band), "with its MATERIALIZE");
    assert_eq!(
        kept.iter().filter(|c| home_body(c)).count(),
        1,
        "the home one is kept"
    );
    assert_eq!(c.replay_gate_counts().never, 0);

    // A meeting while replaying: a band out on the road, met before the replay ends.
    let (mut r, p) = led_invasion_out(87);
    let agent = Party::agent(p);
    r.restart_to_replay();
    let gate = r.c.replay_gate_counts();
    let cmds = meet(&mut r, p);
    let after = r.c.replay_gate_counts();
    assert!(after.deferred > gate.deferred, "the meeting is deferred");
    assert_eq!(after.never, 0, "never decided");
    assert!(
        !cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))
            || matches!(c, Cmd::Materialize(m) if m.agent == agent)),
        "neither is pushed while replaying"
    );
    assert!(!r.c.parties[p].embodied);
}

// ---- A4: the chief comes home by a hand-back ----------------------------------------------------

/// The scripted player follows the band, looking where it is every 50 ticks, until `to` or until
/// it is home.
fn follow(r: &mut CampRun, p: Handle<Party>, to: Tick) {
    while r.c.now() < to && r.c.parties.contains(p) {
        let at = r.c.now();
        let pos = here(r, p);
        r.c.observe_player(&mut r.w, synth::PLAYER, pos, at);
        let next = (at + 50).min(to);
        r.run(next);
    }
}

/// The band, past its fight and on its way home, met 300 ticks short of its camp and followed to
/// a tick before it is there: embodied as it comes home. The tick it is home.
fn followed_home(r: &mut CampRun, p: Handle<Party>) -> Tick {
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(arrive + 400);
    assert_eq!(r.c.parties[p].phase, Phase::Home, "walking home");
    let home = *r.c.parties[p].times.last().expect("a route home");
    r.run(Tick(home.0 - 300));
    follow(r, p, Tick(home.0 - 1));
    assert!(r.c.parties[p].embodied, "embodied as it comes home");
    home
}

/// The chief's line in `cmds` (a CAMP_RECONCILE's line on `CHIEF_SLOT`): the reconcile, the line's
/// index there, and the line; and the command just ahead of the reconcile.
fn chief_line(
    cmds: &[Cmd],
) -> Option<(
    boundary::CampReconcile,
    u32,
    boundary::ReconcileLine,
    Option<Cmd>,
)> {
    cmds.iter().enumerate().find_map(|(i, c)| match c {
        Cmd::Reconcile(r) => r.lines.iter().position(|l| l.slot == CHIEF_SLOT).map(|k| {
            (
                r.clone(),
                k as u32,
                r.lines[k],
                i.checked_sub(1).map(|j| cmds[j].clone()),
            )
        }),
        _ => None,
    })
}

/// The outcome the plugin gave the line `index` of reconcile `op`, once applied.
fn applied(r: &CampRun, op: u64, index: u32) -> Option<u8> {
    r.plugin
        .applied
        .iter()
        .find(|a| a.op == op && a.index == index)
        .map(|a| a.outcome)
}

/// Runs until the plugin has answered the line, a day at most: its outcome.
fn answered(r: &mut CampRun, op: u64, index: u32) -> u8 {
    let limit = r.c.now() + TICKS_PER_DAY;
    while applied(r, op, index).is_none() && r.c.now() < limit {
        let to = r.c.now() + 10;
        r.run(to);
    }
    let outcome = applied(r, op, index).expect("answered");
    // The answer reaches the engine `answer_ticks` after the plugin applied it.
    let to = r.c.now() + r.plugin.answer_ticks + 2;
    r.run(to);
    outcome
}

/// Asserts the reconcile at home is the chief's HAND_BACK of `p`'s agent and epoch (LINE_CHIEF),
/// with the camp's memory just ahead of it as a home CHIEF_BODY (agent 0, the camp's epoch).
fn assert_handed_back(r: &CampRun, cmds: &[Cmd], agent: u64, epoch: u32, generation: u32) {
    let (rec, _, l, ahead) = chief_line(cmds).expect("a chief line at home");
    assert_eq!(
        l.action,
        boundary::ACTION_HAND_BACK,
        "the chief is handed back"
    );
    assert_eq!((l.a, l.b), (agent, u64::from(epoch)), "its band and epoch");
    assert_eq!(l.generation, generation);
    assert_ne!(l.flags & boundary::LINE_CHIEF, 0, "LINE_CHIEF");
    let camp = r.c.camp(r.f).expect("a camp");
    match ahead {
        Some(Cmd::ChiefBody(b)) => {
            assert_eq!((b.faction, b.agent, b.epoch), (r.key, 0, rec.epoch), "home");
            assert_eq!(b.traded, vec![synth::PLAYER], "the camp's memory");
            assert_eq!(rec.epoch, camp.epoch, "the camp's epoch");
        }
        other => panic!("no CHIEF_BODY ahead of the hand-back: {other:#?}"),
    }
}

/// A4 (MZB-821): an embodied Invasion its Umvuthi leads comes home, and the chief comes home by a
/// HAND_BACK of its own, the camp's memory just ahead of it: (1) to a grove that is in, its slot
/// Gone, answered DONE (the throne body made from the puppet), the slot Body; (2) the slot Left,
/// its throne body standing (the grove was out at the muster's end, and the TAKE its census sent
/// was held on a player at the throne and answered STALE as the puppet came near, MZB-823),
/// answered NOTHING (case B), the slot Body and one throne body; (3) to a grove that is out, the
/// band recalled with RECALL_DONE and no line, the slot Owed and spawned by the next census. Without
/// the change (1) is a SPAWN and (2) has no chief line at all.
#[test]
fn the_chief_is_handed_back_at_home() {
    // 1. The grove in, the slot Gone (its body set out at the muster's end).
    let (mut r, p) = led_invasion_out(88);
    let agent = Party::agent(p);
    let generation = chief(&r).expect("a chief").1;
    assert_eq!(chief(&r), Some((SlotState::Gone, generation)), "set out");
    let home = followed_home(&mut r, p);
    let epoch = r.c.parties[p].epoch;
    let mark = r.cmds.len();
    r.run(home + 1);
    assert!(!r.c.parties.contains(p), "home");
    let cmds = since(&r, mark);
    assert_handed_back(&r, &cmds, agent, epoch, generation);
    let (rec, index, ..) = chief_line(&cmds).expect("a chief line");
    assert_eq!(answered(&mut r, rec.op, index), boundary::LINE_DONE);
    assert_eq!(chief(&r), Some((SlotState::Body, generation)), "body again");
    assert_eq!(
        r.plugin.camp_held(r.key, CHIEF_SLOT, generation).len(),
        1,
        "one throne body"
    );
    ok(&mut r);

    // 2. The grove out at the muster's end, in again while the band is away, the slot Left.
    let mut r = quiet_grove(89);
    let generation = seat_chief(&mut r);
    pay_at_home(&mut r, generation);
    let at = r.c.now();
    let target = Target::District(synth::REFINERY, synth::REFINERY_POS);
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, target, None, at)
            .expect("an invasion");
    r.flush(at);
    r.camp_out();
    let muster = r.c.parties[p].until;
    r.run(muster + 200);
    assert_eq!(r.c.parties[p].phase, Phase::Out, "on the road");
    assert_eq!(
        chief(&r),
        Some((SlotState::Left, generation)),
        "left, its body standing"
    );
    let agent = Party::agent(p);
    // A player at the throne as the grove comes in: the census's TAKE of the throne body is held
    // (Java's `holdRemoval`).
    let at = r.c.now();
    r.plugin.hold_chief(r.key, true, false, at);
    let mark = r.cmds.len();
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    let (take, take_index, l, _) = chief_line(&since(&r, mark)).expect("the census's chief line");
    assert_eq!(
        l.action,
        boundary::ACTION_TAKE,
        "the census takes the throne body"
    );
    assert!(applied(&r, take.op, take_index).is_none(), "held");
    let home = followed_home(&mut r, p);
    let epoch = r.c.parties[p].epoch;
    // Java answers the held TAKE stale once the puppet is within 32 blocks of the throne (MZB-823).
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &Obs::CampBodies(boundary::CampBodies {
            faction: r.key,
            epoch: take.epoch,
            op: take.op,
            lines: vec![boundary::LineAnswer {
                line: take_index,
                slot: CHIEF_SLOT,
                generation,
                action: boundary::ACTION_TAKE,
                outcome: boundary::LINE_STALE,
            }],
        }),
        at,
    );
    let s = r.c.camp(r.f).and_then(|c| c.chief()).expect("a chief");
    assert_eq!(
        (s.state, s.line.is_none()),
        (SlotState::Left, true),
        "left, no line"
    );
    let mark = r.cmds.len();
    r.run(home + 1);
    assert!(!r.c.parties.contains(p), "home");
    let cmds = since(&r, mark);
    assert_handed_back(&r, &cmds, agent, epoch, generation);
    assert_eq!(chief(&r), Some((SlotState::Body, generation)), "body");
    let books = (
        r.c.stats.chiefs_killed,
        r.c.camp(r.f).expect("a camp").slots.len(),
    );
    let (rec, index, ..) = chief_line(&cmds).expect("a chief line");
    assert_eq!(
        answered(&mut r, rec.op, index),
        boundary::LINE_NOTHING,
        "case B: the throne body stands"
    );
    assert_eq!(chief(&r), Some((SlotState::Body, generation)), "still body");
    assert_eq!(
        (
            r.c.stats.chiefs_killed,
            r.c.camp(r.f).expect("a camp").slots.len()
        ),
        books,
        "the books unchanged"
    );
    assert_eq!(
        r.plugin.camp_held(r.key, CHIEF_SLOT, generation).len(),
        1,
        "one throne body"
    );

    // 3. The grove out at home.
    let (mut r, p) = led_invasion_out(90);
    let agent = Party::agent(p);
    let generation = chief(&r).expect("a chief").1;
    r.camp_out();
    let home = followed_home(&mut r, p);
    let epoch = r.c.parties[p].epoch;
    let mark = r.cmds.len();
    r.run(home + 1);
    assert!(!r.c.parties.contains(p), "home");
    let cmds = since(&r, mark);
    assert!(
        chief_line(&cmds).is_none(),
        "no line at a grove that is out"
    );
    assert!(
        cmds.iter().any(|c| matches!(c, Cmd::Recall(x)
            if x.agent == agent && x.epoch == epoch && x.reason == boundary::RECALL_DONE)),
        "recalled: {cmds:#?}"
    );
    assert_eq!(chief(&r), Some((SlotState::Owed, generation)), "owed");
    let mark = r.cmds.len();
    r.camp_in();
    let to = r.c.now() + r.plugin.leave_ticks + 50;
    r.run(to);
    let (_, _, l, ahead) = chief_line(&since(&r, mark)).expect("the census's chief line");
    assert_eq!(
        l.action,
        boundary::ACTION_SPAWN,
        "the next census spawns it"
    );
    assert!(matches!(ahead, Some(Cmd::ChiefBody(b)) if b.agent == 0));
    ok(&mut r);
}

/// A4's control: a band never embodied comes home to a grove that is in, and its chief, Gone, gets
/// a SPAWN as before (`chief_puppet` false).
#[test]
fn an_unmet_chief_comes_home_as_a_spawn() {
    let (mut r, p) = led_invasion_out(91);
    let arrive = *r.c.parties[p].times.last().expect("a route");
    r.run(arrive + 400);
    assert_eq!(r.c.parties[p].phase, Phase::Home, "walking home");
    let home = *r.c.parties[p].times.last().expect("a route home");
    r.run(Tick(home.0 - 1));
    assert!(!r.c.parties[p].embodied, "unmet");
    let mark = r.cmds.len();
    r.run(home + 1);
    let (_, _, l, ahead) = chief_line(&since(&r, mark)).expect("a chief line at home");
    assert_eq!(l.action, boundary::ACTION_SPAWN, "a spawn, as before");
    assert!(matches!(ahead, Some(Cmd::ChiefBody(b)) if b.agent == 0));
    ok(&mut r);
}

// ---- A6: a death after home ---------------------------------------------------------------------

/// The books of a chief's death: (falls, player losses, an heir due, the memory).
fn death_books(r: &CampRun) -> (u64, u64, bool, usize) {
    let camp = r.c.camp(r.f).expect("a camp");
    (
        r.c.stats.chiefs_killed,
        r.c.stats.renewal_losses,
        camp.heir.is_some(),
        camp.traded.len(),
    )
}

fn chief_death(r: &CampRun, generation: u32) -> Obs {
    Obs::CampDeath(boundary::CampDeath {
        faction: r.key,
        by: boundary::BY_PLAYER,
        body: chief_body(r),
        flags: boundary::DEATH_CHIEF,
        slot: CHIEF_SLOT,
        generation,
        killer: synth::PLAYER,
        op: 0x6001,
    })
}

/// A6 (MZB-824): home with the grove in, the chief's HAND_BACK in flight (the puppet floating to
/// the throne), the puppet is killed: CAMP_DEATH of the chief's slot and generation, BY_PLAYER, and
/// the line's STALE, in both orders. The chief dies once: one fall at its camp, one player's loss,
/// the heir due and the memory cleared; no CHIEF_KILLED goes into it, and the slot is freed, its
/// generation retired (`chief_death` frees an Owed slot; the design's "Dead" is the state of a
/// chief that died away while its body stood). Without the change the chief's line is a SPAWN.
/// The control, a throne death at home with no line, books the same.
#[test]
fn a_camp_death_after_home_books_once() {
    for death_first in [true, false] {
        let (mut r, p) = led_invasion_out(92);
        let generation = chief(&r).expect("a chief").1;
        let home = followed_home(&mut r, p);
        // The hand-back is decided and never reaches the plugin: in flight.
        let cmds = r.decide(home + 1);
        let (rec, index, l, _) = chief_line(&cmds).expect("a chief line at home");
        assert_eq!(
            l.action,
            boundary::ACTION_HAND_BACK,
            "a hand-back in flight"
        );
        let s = r.c.camp(r.f).and_then(|c| c.chief()).expect("a chief");
        assert!(s.line.is_some(), "in flight");
        let before = death_books(&r);
        assert_eq!(before.2, false, "no heir yet");
        assert_eq!(before.3, 1, "the payer remembered");
        let stale = Obs::CampBodies(boundary::CampBodies {
            faction: r.key,
            epoch: rec.epoch,
            op: rec.op,
            lines: vec![boundary::LineAnswer {
                line: index,
                slot: CHIEF_SLOT,
                generation,
                action: boundary::ACTION_HAND_BACK,
                outcome: boundary::LINE_STALE,
            }],
        });
        let death = chief_death(&r, generation);
        let order = if death_first {
            [death, stale]
        } else {
            [stale, death]
        };
        for o in &order {
            let at = r.c.now();
            r.c.ingest(&mut r.w, &mut NoInfrastructure, o, at);
        }
        let after = death_books(&r);
        assert_eq!(
            after,
            (before.0 + 1, before.1 + 1, true, 0),
            "death first {death_first}: one fall, one loss, the heir due, the memory cleared"
        );
        assert!(
            r.c.camp(r.f)
                .expect("a camp")
                .index(CHIEF_SLOT, generation)
                .is_none(),
            "the slot freed"
        );
        let bad = r.check();
        assert!(bad.is_empty(), "{bad:#?}");
    }

    // The control: the seated chief killed on its throne, no line in flight.
    let mut r = quiet_grove(93);
    let generation = seat_chief(&mut r);
    pay_at_home(&mut r, generation);
    let before = death_books(&r);
    let death = chief_death(&r, generation);
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &death, at);
    assert_eq!(death_books(&r), (before.0 + 1, before.1 + 1, true, 0));
    assert!(chief(&r).is_none(), "the slot freed");
}
