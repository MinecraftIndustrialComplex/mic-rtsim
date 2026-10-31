// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M11 (ledger 24, §4.5 "When it has a body"): the grove's
//! chief goes with its band as a stack of its own behind an away CHIEF_BODY, comes home by a
//! HAND_BACK, is paid as a puppet, and is booked once whatever ends it. Played against the emulated
//! plugin (`synth::CampRun`) on the shipped cultures.

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{Cultures, PartyKind};
use rtsim_conflict::engine::{
    CHIEF_SLOT, Conflict, Ended, Party, Phase, SlotState, Target, body_id,
};
use rtsim_conflict::synth::{self, CampRun};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::renewal::LossCause;
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
    // Nor does it hold the chief (MZB-853): no puppet of it was ever in the world.
    assert_eq!(ended(&r), None, "an unmet band holds nothing");
    ok(&mut r);
}

// ---- A5: a payment to the puppet ----------------------------------------------------------------

/// A second player, who pays the puppet.
const PAYER: boundary::Uuid = (0xA5A5, 0x5A5A);

/// A payment of 9 gold nuggets to an Umvuthi body, as the plugin reports it: the slot, generation
/// and flags, and the op id.
fn pay(r: &mut CampRun, generation: u32, flags: u32, op: u64) {
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
            flags,
            player: PAYER,
            op,
            items: vec![boundary::Item { item: id, count: 9 }],
        }),
        at,
    );
}

/// The tribute the engine has booked, every good.
fn tribute(r: &CampRun) -> i128 {
    r.c.ledger.tribute.values().sum()
}

/// The camp's memory.
fn traded(r: &CampRun) -> Vec<boundary::Uuid> {
    r.c.camp(r.f).expect("a camp").traded.clone()
}

/// A5 (MZB-819): a payment to the chief's puppet (CHIEF_PAID with PAID_PUPPET, the chief's slot,
/// generation 0) out on the road is booked as tribute once (its copy, the same op id, books
/// nothing), the payer joins the camp's memory, and the next CHIEF_BODY carries it, away (the band
/// met again) and home (ahead of the chief's HAND_BACK). Without the change the payer is not
/// remembered: `index` finds no slot of generation 0. The controls: the chief killed away (its slot
/// Dead, or freed when its body had set out), a puppet's payment is tribute and the heir's memory
/// stays empty; a payment with flags 0 and generation 0 is a stray's tribute, as before.
#[test]
fn a_payment_to_the_puppet_is_remembered() {
    let (mut r, p) = led_invasion_out(111);
    let agent = Party::agent(p);
    let generation = chief(&r).expect("a chief").1;
    let t0 = tribute(&r);
    pay(&mut r, 0, boundary::PAID_PUPPET, 0x8541);
    let t1 = tribute(&r);
    assert!(t1 > t0, "booked as tribute");
    assert_eq!(traded(&r), {
        let mut v = vec![synth::PLAYER, PAYER];
        v.sort();
        v
    });
    pay(&mut r, 0, boundary::PAID_PUPPET, 0x8541);
    assert_eq!(tribute(&r), t1, "a copy books nothing");
    assert_eq!(traded(&r).len(), 2, "remembered once");
    // Away: the band met on the road.
    let cmds = meet(&mut r, p);
    let away = cmds
        .iter()
        .find_map(|c| match c {
            Cmd::ChiefBody(b) if b.agent == agent => Some(b.clone()),
            _ => None,
        })
        .expect("an away CHIEF_BODY");
    assert!(
        away.traded.contains(&PAYER),
        "the away CHIEF_BODY carries the payer"
    );
    // Home, the grove in.
    let home = followed_home(&mut r, p);
    let mark = r.cmds.len();
    r.run(home + 1);
    let (_, _, l, ahead) = chief_line(&since(&r, mark)).expect("a chief line at home");
    assert_eq!(
        (l.action, l.generation),
        (boundary::ACTION_HAND_BACK, generation)
    );
    match ahead {
        Some(Cmd::ChiefBody(b)) => {
            assert_eq!(b.agent, 0, "home");
            assert!(
                b.traded.contains(&PAYER),
                "the home CHIEF_BODY carries the payer"
            );
        }
        other => panic!("no CHIEF_BODY ahead: {other:#?}"),
    }
    ok(&mut r);

    // The controls: the chief killed away, its slot Dead (Left) or freed (Gone).
    for left in [true, false] {
        let (mut r, _) = if left {
            left_invasion_out(112)
        } else {
            led_invasion_out(113)
        };
        let at = r.c.now();
        assert!(
            r.c.chief_died_away(r.f, LossCause::Player, at),
            "killed away"
        );
        let state = chief(&r).map(|s| s.0);
        assert_eq!(state, left.then_some(SlotState::Dead), "left {left}");
        assert!(traded(&r).is_empty(), "the memory cleared");
        let t0 = tribute(&r);
        pay(&mut r, 0, boundary::PAID_PUPPET, 0x8542);
        assert!(tribute(&r) > t0, "left {left}: booked as tribute");
        assert!(
            traded(&r).is_empty(),
            "left {left}: the heir's memory stays empty"
        );
    }
    // A stray's: flags 0, generation 0.
    let (mut r, _) = led_invasion_out(114);
    let before = traded(&r);
    let t0 = tribute(&r);
    pay(&mut r, 0, 0, 0x8543);
    assert!(tribute(&r) > t0, "booked as tribute");
    assert_eq!(traded(&r), before, "not remembered");
}

/// MZB-855: the Umvuthi killed away (`chief_killed_by`, the CHIEF_KILLED of the band it leads)
/// while its grove is out and the pool holds a whole warrior the ledger has not followed yet (the
/// band left with the camp out, so no census has added its slot). `chief_died_away` follows the
/// ledger first, and the slot it adds sorts below the chief's: the chief's own slot is made Dead,
/// its throne body standing, and no warrior's slot changes. On the base the index was taken before
/// the ledger moved, so the new warrior's slot was freed in the chief's place and the chief's slot
/// stayed Left with its party routed. The control: a band whose chief's slot had set out (Gone),
/// with the ledger up to date, frees the chief's slot on both.
#[test]
fn a_chief_killed_away_after_the_pool_grew_dies_in_its_own_slot() {
    for (left, seed) in [(true, 112), (false, 116)] {
        let (mut r, p) = if left {
            left_invasion_out(seed)
        } else {
            led_invasion_out(seed)
        };
        let (_, generation) = chief(&r).expect("a chief");
        let warriors = |r: &CampRun| -> Vec<(u32, u32, SlotState)> {
            r.c.camp(r.f)
                .map(|c| {
                    c.slots
                        .iter()
                        .filter(|s| s.id != CHIEF_SLOT)
                        .map(|s| (s.id, s.generation, s.state))
                        .collect()
                })
                .unwrap_or_default()
        };
        let before = warriors(&r);
        // Days on, the band still out.
        let at = r.c.now() + 2 * TICKS_PER_DAY;
        r.c.run_until(&mut r.w, &mut NoInfrastructure, at);
        r.flush(at);
        assert!(r.c.parties.contains(p), "left {left}: still out");
        assert_eq!(
            warriors(&r),
            before,
            "left {left}: the ledger has not followed yet"
        );
        let who = r.player;
        r.c.chief_killed_by(&mut r.w, p, Some(who), boundary::BY_PLAYER, None, at);
        r.flush(at);
        let after = warriors(&r);
        assert_eq!(
            after.len() > before.len(),
            left,
            "left {left}: the ledger followed a gain ({after:?})"
        );
        for s in &before {
            assert!(
                after.contains(s),
                "left {left}: warrior slot {s:?} changed: {after:?}"
            );
        }
        let want = left.then_some((SlotState::Dead, generation));
        assert_eq!(chief(&r), want, "left {left}: the chief's own slot");
        ok(&mut r);
    }
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
        uuid: (0, 0),
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
        assert!(!before.2, "no heir yet");
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

// ---- MZB-853: the end of the chief's band -------------------------------------------------------

/// The camp's end of its chief's band (MZB-853).
fn ended(r: &CampRun) -> Option<Ended> {
    r.c.camp(r.f).and_then(|c| c.ended)
}

/// The end and the index agree on the band, or there is neither.
fn assert_end(r: &CampRun, want: Option<Ended>) {
    assert_eq!(ended(r), want, "the camp's end");
    let index: Vec<(u64, u32)> = r.c.chief_ended.iter().map(|(a, f)| (*a, *f)).collect();
    let want_index: Vec<(u64, u32)> = want.iter().map(|e| (e.agent, r.f)).collect();
    assert_eq!(index, want_index, "the index by agent");
}

/// The puppet of `agent`'s chief killed by the scripted player.
fn puppet_killed(agent: u64, epoch: u32, op: u64) -> Obs {
    Obs::ChiefKilled(boundary::ChiefKilled {
        agent,
        epoch,
        by: boundary::BY_PLAYER,
        killer: synth::PLAYER,
        op,
    })
}

/// Takes `agent`'s BAND_DEMATERIALIZED out of the plugin's queue for the test to deliver when it
/// likes; the rest of the queue goes on a tick from now.
fn hold_despawn(r: &mut CampRun, agent: u64) -> Obs {
    let mut found = None;
    let next = r.c.now() + 1;
    for o in r.plugin.drop_queued() {
        match o {
            Obs::Dematerialized(ref d) if d.agent == agent && found.is_none() => found = Some(o),
            o => r.plugin.push(next, o),
        }
    }
    found.expect("the band's despawn queued")
}

/// A4 case 2's band: the grove goes out before the muster's end, so the chief's slot is Left, its
/// throne body standing, and stays out. On the road.
fn left_invasion_out(seed: u64) -> (CampRun, Handle<Party>) {
    let mut r = quiet_grove(seed);
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
    assert_eq!(chief(&r), Some((SlotState::Left, generation)), "left");
    (r, p)
}

/// A led band followed home to a grove that is out (A4 case 3), its chief's slot Gone, or Left
/// with its throne body standing (`left`): recalled, its puppet still in the world, so the end
/// holds the chief. The band's agent, its epoch, the chief's generation, and the recall's
/// BAND_DEMATERIALIZED, held back from the engine.
fn home_out(seed: u64, left: bool) -> (CampRun, u64, u32, u32, Obs) {
    let (mut r, p) = if left {
        left_invasion_out(seed)
    } else {
        let (mut r, p) = led_invasion_out(seed);
        r.camp_out();
        (r, p)
    };
    let agent = Party::agent(p);
    let generation = chief(&r).expect("a chief").1;
    let home = followed_home(&mut r, p);
    let epoch = r.c.parties[p].epoch;
    r.run(home + 1);
    assert!(!r.c.parties.contains(p), "home");
    let despawn = hold_despawn(&mut r, agent);
    let state = if left {
        SlotState::Body
    } else {
        SlotState::Owed
    };
    assert_eq!(chief(&r), Some((state, generation)), "home in the books");
    assert_end(
        &r,
        Some(Ended {
            agent,
            epoch,
            generation,
        }),
    );
    (r, agent, epoch, generation, despawn)
}

/// An Invasion ordered now: whether its chief leads it.
fn led_now(r: &mut CampRun) -> bool {
    let at = r.c.now();
    let target = Target::District(synth::REFINERY, synth::REFINERY_POS);
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, target, None, at)
            .expect("an invasion");
    r.flush(at);
    r.c.parties[p].chief
}

/// The band's BAND_DEMATERIALIZED of `epoch`, as Java sends it at a recall.
fn despawned(agent: u64, epoch: u32) -> Obs {
    Obs::Dematerialized(boundary::BandDematerialized {
        agent,
        epoch,
        reason: 0,
        survivors: Vec::new(),
    })
}

/// MZB-853, release 1: a led band comes home to a grove that is out with its puppet still in the
/// world (recalled, the despawn not yet in). The end holds the chief: the grove comes in and its
/// census does not spawn the chief (the slot is Owed), and an Invasion ordered then goes without
/// it. The band's BAND_DEMATERIALIZED releases it: the chief's SPAWN goes at once, its memory
/// ahead, and it leads again. The controls: a CHIEF_KILLED of the band after the release books
/// nothing, and the same band's despawn in before the grove comes in leaves an Invasion led.
/// Without the hold the census sends the chief's SPAWN while the puppet stands.
#[test]
fn the_end_holds_the_chief_until_its_band_is_gone() {
    let (mut r, agent, epoch, generation, despawn) = home_out(94, false);
    let mark = r.cmds.len();
    come_in(&mut r);
    let to = r.c.now() + 50;
    r.run(to);
    assert!(
        chief_line(&since(&r, mark)).is_none(),
        "no chief line while the end holds it: {:#?}",
        since(&r, mark)
    );
    assert_eq!(chief(&r), Some((SlotState::Owed, generation)), "still owed");
    ok(&mut r);

    let mark = r.cmds.len();
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at);
    r.flush(at);
    assert_end(&r, None);
    let cmds = since(&r, mark);
    let (rec, index, l, ahead) = chief_line(&cmds).expect("the chief spawned at the release");
    assert_eq!(l.action, boundary::ACTION_SPAWN);
    assert_eq!(l.generation, generation);
    assert!(
        matches!(ahead, Some(Cmd::ChiefBody(b)) if b.agent == 0 && b.traded == vec![synth::PLAYER]),
        "its memory ahead"
    );
    assert_eq!(answered(&mut r, rec.op, index), boundary::LINE_DONE);
    assert_eq!(chief(&r), Some((SlotState::Body, generation)), "home, body");
    assert_eq!(
        r.plugin.camp_held(r.key, CHIEF_SLOT, generation).len(),
        1,
        "one throne body"
    );
    ok(&mut r);

    // A death of the band's puppet after the release books nothing.
    let before = death_books(&r);
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &puppet_killed(agent, epoch, 0x8531),
        at,
    );
    assert_eq!(death_books(&r), before, "a straggler books nothing");
    assert!(led_now(&mut r), "the chief leads again");
    ok(&mut r);

    // The hold: an Invasion ordered while the end stands goes without the chief.
    let (mut r, _, _, generation, _) = home_out(94, false);
    come_in(&mut r);
    let to = r.c.now() + 50;
    r.run(to);
    assert!(
        !led_now(&mut r),
        "the chief does not lead while the end holds it"
    );
    assert_eq!(
        chief(&r),
        Some((SlotState::Owed, generation)),
        "nor is it drawn"
    );
    ok(&mut r);
}

/// MZB-853, release 2: home to a grove that is in, the chief's HAND_BACK answered DONE (the puppet
/// consumed into the throne body) releases the end before the band's despawn, and NOTHING (case B,
/// the throne body stood) does too. The control: a HAND_BACK answered STALE (the puppet was not
/// there to take) releases nothing, and the owed chief is not spawned while the end holds it.
/// Without the release the end stands until the despawn.
#[test]
fn a_hand_back_answered_releases_the_end() {
    for (seed, outcome) in [
        (95, boundary::LINE_DONE),
        (96, boundary::LINE_NOTHING),
        (97, boundary::LINE_STALE),
    ] {
        let (mut r, p) = led_invasion_out(seed);
        let agent = Party::agent(p);
        let generation = chief(&r).expect("a chief").1;
        let home = followed_home(&mut r, p);
        let epoch = r.c.parties[p].epoch;
        // Decided and held back: the test answers it.
        let cmds = r.decide(home + 1);
        assert_handed_back(&r, &cmds, agent, epoch, generation);
        let end = Some(Ended {
            agent,
            epoch,
            generation,
        });
        assert_end(&r, end);
        let (rec, index, ..) = chief_line(&cmds).expect("a chief line");
        let answer = Obs::CampBodies(boundary::CampBodies {
            faction: r.key,
            epoch: rec.epoch,
            op: rec.op,
            lines: vec![boundary::LineAnswer {
                line: index,
                slot: CHIEF_SLOT,
                generation,
                action: boundary::ACTION_HAND_BACK,
                outcome,
            }],
        });
        let at = r.c.now();
        r.c.ingest(&mut r.w, &mut NoInfrastructure, &answer, at);
        let cmds = r.decide(at + 1);
        if outcome == boundary::LINE_STALE {
            assert_end(&r, end);
            let s = r.c.camp(r.f).and_then(|c| c.chief()).expect("a chief");
            assert_eq!((s.state, s.line), (SlotState::Owed, None), "owed, no line");
            assert!(chief_line(&cmds).is_none(), "nothing spawned");
            // The despawn releases it, and the chief is spawned.
            r.c.ingest(
                &mut r.w,
                &mut NoInfrastructure,
                &despawned(agent, epoch),
                at + 1,
            );
            let cmds = r.decide(at + 2);
            let (_, _, l, _) = chief_line(&cmds).expect("spawned at the despawn");
            assert_eq!(
                (l.action, l.generation),
                (boundary::ACTION_SPAWN, generation)
            );
            assert_end(&r, None);
        } else {
            assert_end(&r, None);
            assert_eq!(
                chief(&r),
                Some((SlotState::Body, generation)),
                "outcome {outcome}"
            );
            assert!(chief_line(&cmds).is_none(), "nothing more to send");
            // The despawn after it changes nothing.
            r.c.ingest(
                &mut r.w,
                &mut NoInfrastructure,
                &despawned(agent, epoch),
                at + 1,
            );
            assert!(chief_line(&r.decide(at + 2)).is_none());
            assert_eq!(chief(&r), Some((SlotState::Body, generation)));
        }
        let bad = r.check();
        assert!(bad.is_empty(), "outcome {outcome}: {bad:#?}");
    }
}

/// MZB-853, release 3: the band's puppet killed after its end (CHIEF_KILLED, the party gone) is
/// the chief's death at its camp, booked once: one fall, one player's loss, the heir due, the
/// memory cleared. (1) The slot Owed (its body went with the band): it is freed, its generation
/// retired, and no census spawns it; the despawn after sends nothing, and the record's copy (the
/// Sol Visage's) books nothing more. (2) The slot Body, its throne body standing, the grove in by
/// then: the slot is dead and its throne body removed by a REMOVE at once; the answer frees the
/// slot. Without the booking the death counts for nothing and the census spawns the chief again.
#[test]
fn a_puppet_killed_after_its_end_is_the_chiefs_death() {
    // 1. Owed.
    let (mut r, agent, epoch, generation, despawn) = home_out(97, false);
    let before = death_books(&r);
    let killed = puppet_killed(agent, epoch, 0x8532);
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &killed, at);
    assert_eq!(
        death_books(&r),
        (before.0 + 1, before.1 + 1, true, 0),
        "one fall, one loss, the heir due, the memory cleared"
    );
    assert_end(&r, None);
    let camp = r.c.camp(r.f).expect("a camp");
    assert!(
        camp.index(CHIEF_SLOT, generation).is_none(),
        "the slot freed"
    );
    // The copy, and the despawn after it.
    let at = r.c.now();
    r.c.ingest_copy(&mut r.w, &mut NoInfrastructure, &killed, at);
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at);
    assert_eq!(death_books(&r).0, before.0 + 1, "booked once");
    let mark = r.cmds.len();
    come_in(&mut r);
    let to = r.c.now() + 50;
    r.run(to);
    assert!(
        since(&r, mark)
            .iter()
            .all(|c| !matches!(c, Cmd::Reconcile(x)
            if x.lines.iter().any(|l| l.slot == CHIEF_SLOT && l.generation == generation))),
        "the dead chief is not spawned"
    );
    assert!(r.plugin.camp_held(r.key, CHIEF_SLOT, generation).is_empty());
    ok(&mut r);

    // 2. Body, the grove in by the death.
    let (mut r, agent, epoch, generation, despawn) = home_out(98, true);
    come_in(&mut r);
    let to = r.c.now() + 50;
    r.run(to);
    assert_eq!(chief(&r), Some((SlotState::Body, generation)));
    let before = death_books(&r);
    let mark = r.cmds.len();
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &puppet_killed(agent, epoch, 0x8533),
        at,
    );
    r.flush(at);
    assert_eq!(death_books(&r), (before.0 + 1, before.1 + 1, true, 0));
    assert_end(&r, None);
    assert_eq!(chief(&r), Some((SlotState::Dead, generation)), "dead");
    let (rec, index, l, _) = chief_line(&since(&r, mark)).expect("the throne body's line");
    assert_eq!(
        (l.action, l.generation),
        (boundary::ACTION_REMOVE, generation),
        "the throne body removed"
    );
    assert_eq!(answered(&mut r, rec.op, index), boundary::LINE_DONE);
    assert!(chief(&r).is_none(), "the slot freed by the answer");
    assert!(r.plugin.camp_held(r.key, CHIEF_SLOT, generation).is_empty());
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at);
    ok(&mut r);
}

/// MZB-853, release 4: the chief's slot freed by anything else prunes the end: its throne body
/// (the slot Body, the band home to a grove that is out) is killed, a CAMP_DEATH booked by
/// `chief_death`. The index entry goes at the band's despawn, which spawns nothing. Without the
/// prune the end names a generation the chief's slot does not hold (the audit's MZB-853).
#[test]
fn a_camp_death_prunes_the_end() {
    let (mut r, agent, _, generation, despawn) = home_out(99, true);
    let before = death_books(&r);
    let at = r.c.now();
    let death = chief_death(&r, generation);
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &death, at);
    assert_eq!(death_books(&r), (before.0 + 1, before.1 + 1, true, 0));
    assert_eq!(ended(&r), None, "pruned with the generation");
    assert!(chief(&r).is_none(), "freed");
    assert!(r.check().is_empty(), "{:#?}", r.check());
    assert_eq!(
        r.c.chief_ended.get(&agent),
        Some(&r.f),
        "the index waits for its agent"
    );
    let mark = r.cmds.len();
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at);
    r.flush(at);
    assert_end(&r, None);
    assert!(chief_line(&since(&r, mark)).is_none(), "nothing spawned");
    // A puppet death after it books no second fall.
    r.c.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &puppet_killed(agent, 1, 0x8534),
        at,
    );
    assert_eq!(death_books(&r).0, before.0 + 1, "one fall");
    ok(&mut r);
}

/// MZB-853, release 5: the session ends with the end standing (the journal lost the band's
/// despawn, a crash). At the replay end the chief is lost to no one: the slot as at a puppet's
/// death (Owed freed; Body dead, its throne body removed by the next census), the heir due and
/// the memory cleared, with no fall and no pressure step. The CHIEF_KILLED that comes later as a
/// copy (on the Sol Visage) books nothing. The control: a clean stop, the despawn journaled before
/// the restart, keeps the chief, spawned by the next census. Without the loss the end stands
/// after the replay and the chief is held for good.
#[test]
fn an_end_standing_at_the_replay_end_is_a_loss() {
    for left in [false, true] {
        let (mut r, agent, epoch, generation, _despawn) = home_out(100 + u64::from(left), left);
        let before = death_books(&r);
        r.restart();
        assert_end(&r, None);
        assert_eq!(
            death_books(&r),
            (before.0, before.1, true, 0),
            "left {left}: lost to no one, the heir due, the memory cleared"
        );
        let state = chief(&r).map(|s| s.0);
        assert_eq!(state, left.then_some(SlotState::Dead), "left {left}");
        let at = r.c.now();
        r.c.ingest_copy(
            &mut r.w,
            &mut NoInfrastructure,
            &puppet_killed(agent, epoch, 0x8535),
            at,
        );
        assert_eq!(
            death_books(&r).0,
            before.0,
            "left {left}: the copy books nothing"
        );
        come_in(&mut r);
        let to = r.c.now() + 200;
        r.run(to);
        assert!(
            r.plugin.camp_held(r.key, CHIEF_SLOT, generation).is_empty(),
            "left {left}: no body of the lost chief"
        );
        ok(&mut r);
    }

    // The control: a clean stop.
    let (mut r, _, _, generation, despawn) = home_out(102, false);
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at);
    let before = death_books(&r);
    r.restart();
    assert_eq!(death_books(&r), before, "nothing lost");
    come_in(&mut r);
    let to = r.c.now() + 200;
    r.run(to);
    assert_eq!(chief(&r), Some((SlotState::Body, generation)), "spawned");
    ok(&mut r);
}

/// A4 case 2's band followed home: the grove out at the muster's end (the chief's slot Left, its
/// throne body standing), in again while the band is away with the census's TAKE of the throne
/// body held and answered STALE as the puppet came near (MZB-823). The tick it is home.
fn left_home_in(seed: u64) -> (CampRun, Handle<Party>, Tick) {
    let (mut r, p) = left_invasion_out(seed);
    let generation = chief(&r).expect("a chief").1;
    let at = r.c.now();
    r.plugin.hold_chief(r.key, true, false, at);
    let mark = r.cmds.len();
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    let (take, take_index, l, _) = chief_line(&since(&r, mark)).expect("the census's chief line");
    assert_eq!(l.action, boundary::ACTION_TAKE);
    let home = followed_home(&mut r, p);
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
    assert_eq!(chief(&r), Some((SlotState::Left, generation)), "left");
    (r, p, home)
}

/// The chief's line of `rec` answered `outcome`.
fn chief_answer(
    r: &CampRun,
    rec: &boundary::CampReconcile,
    index: u32,
    l: &boundary::ReconcileLine,
    outcome: u8,
) -> Obs {
    Obs::CampBodies(boundary::CampBodies {
        faction: r.key,
        epoch: rec.epoch,
        op: rec.op,
        lines: vec![boundary::LineAnswer {
            line: index,
            slot: CHIEF_SLOT,
            generation: l.generation,
            action: l.action,
            outcome,
        }],
    })
}

/// MZB-853 with the chief's HAND_BACK in flight (M11b): the puppet killed before the hand-back
/// reaches it journals CHIEF_KILLED for the ended band, which is release 3, booked once; the
/// hand-back's answer after it (DONE or NOTHING) finds no end to release and books nothing more.
/// (1) The slot Owed (Gone at home): freed at the death, the answer's generation retired. (2) The
/// slot Body (Left at home, its throne body standing): dead at the death, its line still in
/// flight so no REMOVE yet; the answer, NOTHING (a merge) or STALE (Java's answer since the M11
/// review's P1), clears the line and sends the throne body's REMOVE at once (the review's P3,
/// MZB-1304; before it the REMOVE waited for the next census), and the slot is freed once the
/// held lines play. Without release 3 the death books nothing and the chief lives.
#[test]
fn a_puppet_killed_while_its_hand_back_is_in_flight_dies_once() {
    for (seed, outcome) in [(103, boundary::LINE_DONE), (104, boundary::LINE_NOTHING)] {
        let (mut r, p) = led_invasion_out(seed);
        let agent = Party::agent(p);
        let home = followed_home(&mut r, p);
        let epoch = r.c.parties[p].epoch;
        let cmds = r.decide(home + 1);
        let (rec, index, l, _) = chief_line(&cmds).expect("a chief line");
        assert_eq!(l.action, boundary::ACTION_HAND_BACK, "in flight");
        let before = death_books(&r);
        let at = r.c.now();
        r.c.ingest(
            &mut r.w,
            &mut NoInfrastructure,
            &puppet_killed(agent, epoch, 0x8536),
            at,
        );
        assert_eq!(
            death_books(&r),
            (before.0 + 1, before.1 + 1, true, 0),
            "outcome {outcome}: once"
        );
        assert_end(&r, None);
        assert!(chief(&r).is_none(), "the slot freed");
        let answer = chief_answer(&r, &rec, index, &l, outcome);
        r.c.ingest(&mut r.w, &mut NoInfrastructure, &answer, at);
        r.c.ingest(
            &mut r.w,
            &mut NoInfrastructure,
            &despawned(agent, epoch),
            at,
        );
        assert_eq!(
            death_books(&r),
            (before.0 + 1, before.1 + 1, true, 0),
            "outcome {outcome}: nothing more"
        );
        assert!(chief(&r).is_none(), "outcome {outcome}: nothing comes back");
        assert!(
            chief_line(&r.decide(at + 1)).is_none(),
            "outcome {outcome}: no line"
        );
        let bad = r.check();
        assert!(bad.is_empty(), "outcome {outcome}: {bad:#?}");
    }

    // 2. Body.
    for (seed, outcome) in [(105, boundary::LINE_NOTHING), (106, boundary::LINE_STALE)] {
        let (mut r, p, home) = left_home_in(seed);
        let agent = Party::agent(p);
        let generation = chief(&r).expect("a chief").1;
        let epoch = r.c.parties[p].epoch;
        let cmds = r.decide(home + 1);
        let (rec, index, l, _) = chief_line(&cmds).expect("a chief line");
        assert_eq!(l.action, boundary::ACTION_HAND_BACK, "in flight");
        assert_eq!(chief(&r), Some((SlotState::Body, generation)));
        let before = death_books(&r);
        let at = r.c.now();
        r.c.ingest(
            &mut r.w,
            &mut NoInfrastructure,
            &puppet_killed(agent, epoch, 0x8537),
            at,
        );
        assert_eq!(
            death_books(&r),
            (before.0 + 1, before.1 + 1, true, 0),
            "outcome {outcome}: once"
        );
        assert_end(&r, None);
        assert_eq!(chief(&r), Some((SlotState::Dead, generation)), "dead");
        assert!(
            chief_line(&r.decide(at + 1)).is_none(),
            "outcome {outcome}: no REMOVE over a line in flight"
        );
        let answer = chief_answer(&r, &rec, index, &l, outcome);
        let mark = r.cmds.len();
        let at = r.c.now();
        r.c.ingest(&mut r.w, &mut NoInfrastructure, &answer, at);
        r.flush(at);
        let s = r.c.camp(r.f).and_then(|c| c.chief()).expect("a chief");
        assert_eq!(s.state, SlotState::Dead, "outcome {outcome}: dead");
        let removal = chief_line(&since(&r, mark));
        assert!(
            removal.as_ref().is_some_and(
                |x| (x.2.action, x.2.generation) == (boundary::ACTION_REMOVE, generation)
            ),
            "outcome {outcome}: the answer sends the throne body's REMOVE at once, not at the next \
             census (line in flight {:?}, sent {:?})",
            s.line,
            removal.map(|x| x.2)
        );
        assert_eq!(
            death_books(&r),
            (before.0 + 1, before.1 + 1, true, 0),
            "outcome {outcome}: nothing more"
        );
        r.c.ingest(
            &mut r.w,
            &mut NoInfrastructure,
            &despawned(agent, epoch),
            at,
        );
        // The player at the throne goes: the held lines play and the slot is freed. The census's
        // TAKE that `left_home_in` answered STALE is still held by the plugin and plays first,
        // taking the throne body, so the REMOVE finds none: NOTHING, which frees a dead slot as
        // DONE does.
        let (rec, index, _, _) = removal.expect("the REMOVE");
        let at = r.c.now();
        r.plugin.hold_chief(r.key, false, false, at);
        let got = answered(&mut r, rec.op, index);
        assert!(
            got == boundary::LINE_DONE || got == boundary::LINE_NOTHING,
            "outcome {outcome}: the REMOVE is answered ({got})"
        );
        assert_eq!(
            chief(&r),
            None,
            "outcome {outcome}: freed by the REMOVE's answer"
        );
        assert!(
            r.plugin.camp_held(r.key, CHIEF_SLOT, generation).is_empty(),
            "outcome {outcome}: no throne body"
        );
        assert_eq!(death_books(&r).0, before.0 + 1);
        ok(&mut r);
    }
}

/// The journal after a led band's home, for the crash tests: a scenario, the records the last
/// session journaled from home on, and the copy a holder brings in this session.
struct Journal {
    r: CampRun,
    generation: u32,
    records: Vec<Obs>,
    copy: Option<Obs>,
}

/// K: the grove out at home, the slot Owed (or Body, `left`), the puppet killed by a player, then
/// the band despawned; the CHIEF_KILLED's copy is the Sol Visage's.
fn journal_killed(seed: u64, left: bool) -> Journal {
    let (r, agent, epoch, generation, despawn) = home_out(seed, left);
    let killed = puppet_killed(agent, epoch, 0x8538);
    Journal {
        r,
        generation,
        records: vec![killed.clone(), despawn],
        copy: Some(killed),
    }
}

/// D: the grove out at home, the slot Owed, the band despawned with its chief alive.
fn journal_despawned(seed: u64) -> Journal {
    let (r, _, _, generation, despawn) = home_out(seed, false);
    Journal {
        r,
        generation,
        records: vec![despawn],
        copy: None,
    }
}

/// H: the grove in at home, the chief's HAND_BACK applied by the plugin (the puppet taken into
/// a throne body), its answer then the band's despawn journaled.
fn journal_handed_back(seed: u64) -> Journal {
    let (mut r, p) = led_invasion_out(seed);
    let agent = Party::agent(p);
    let generation = chief(&r).expect("a chief").1;
    r.plugin.answer_ticks = 100;
    let home = followed_home(&mut r, p);
    r.run(home + 2);
    assert!(!r.c.parties.contains(p), "home");
    assert!(ended(&r).is_some(), "the end holds the chief");
    let mut answer = None;
    let mut despawn = None;
    for o in r.plugin.drop_queued() {
        match o {
            Obs::CampBodies(ref b) if b.lines.iter().any(|a| a.slot == CHIEF_SLOT) => {
                answer = Some(o)
            }
            Obs::Dematerialized(ref d) if d.agent == agent => despawn = Some(o),
            _ => {}
        }
    }
    Journal {
        r,
        generation,
        records: vec![
            answer.expect("the hand-back's answer"),
            despawn.expect("the despawn"),
        ],
        copy: None,
    }
}

/// MZB-853's crash tests: the session ends with a led band home and its end standing, and the
/// journal reaches the next session up to each of its records (none, the first, all). The pages
/// keep the end; the replay books what the journal kept; the replay end loses a chief whose
/// puppet was not accounted for; a holder's copy then comes. Whatever the cut, the chief dies at
/// most once (one fall and one player's loss, only when the journal kept the CHIEF_KILLED), is
/// either kept or lost (an heir due, no throne body of its generation), never both; nothing is
/// pushed while replaying; no end or index entry is left; and the camp's books and world agree
/// once it is in. Without release 5 a cut before the despawn strands the chief (the end stands);
/// without release 3 the killed chief is kept.
#[test]
fn a_crash_at_any_record_after_home_books_the_chief_once() {
    type Make = fn(u64) -> Journal;
    let cases: [(&str, Make, u64); 4] = [
        ("killed, owed", |s| journal_killed(s, false), 106),
        ("killed, body", |s| journal_killed(s, true), 107),
        ("despawned", journal_despawned, 108),
        ("handed back", journal_handed_back, 109),
    ];
    for (name, make, seed) in cases {
        let n = make(seed).records.len();
        for cut in 0..=n {
            let Journal {
                mut r,
                generation,
                records,
                copy,
            } = make(seed);
            let end = ended(&r);
            r.round_trip();
            assert_end(&r, end);
            let before = death_books(&r);
            r.restart_to_replay();
            let gate = r.c.replay_gate_counts();
            for o in &records[..cut] {
                let at = r.c.now();
                r.c.ingest(&mut r.w, &mut NoInfrastructure, o, at);
            }
            let pushed = r.c.take_commands();
            assert!(
                !pushed
                    .iter()
                    .any(|c| matches!(c, Cmd::Reconcile(_) | Cmd::ChiefBody(_))),
                "{name}, cut {cut}: pushed while replaying: {pushed:#?}"
            );
            assert_eq!(
                r.c.replay_gate_counts().never,
                gate.never,
                "{name}, cut {cut}"
            );
            r.c.replay_done();
            let present = r.c.now();
            r.c.replay_present(present);
            assert_end(&r, None);
            if let Some(o) = &copy {
                let at = r.c.now();
                r.c.ingest_copy(&mut r.w, &mut NoInfrastructure, o, at);
            }
            let killed = records[..cut]
                .iter()
                .any(|o| matches!(o, Obs::ChiefKilled(_)));
            let released = records[..cut]
                .iter()
                .any(|o| !matches!(o, Obs::ChiefKilled(_)));
            let kept = released && !killed;
            let after = death_books(&r);
            let fall = u64::from(killed);
            assert_eq!(
                (after.0, after.1, after.2),
                (before.0 + fall, before.1 + fall, !kept),
                "{name}, cut {cut}: the falls, the losses, the heir"
            );
            come_in(&mut r);
            let to = r.c.now() + 300;
            r.run(to);
            let held = r.plugin.camp_held(r.key, CHIEF_SLOT, generation).len();
            assert_eq!(
                held,
                usize::from(kept),
                "{name}, cut {cut}: the throne body"
            );
            assert_eq!(
                chief(&r).is_some_and(|s| s.1 == generation),
                kept,
                "{name}, cut {cut}: the chief kept"
            );
            assert_eq!(
                death_books(&r).0,
                before.0 + fall,
                "{name}, cut {cut}: no fall later"
            );
            ok(&mut r);
        }
    }
}

/// MZB-853's ending epoch: the end names the epoch its band ended in, and only that epoch's
/// BAND_DEMATERIALIZED releases it. Replayed, it releases the end while the camp is out, pushing
/// nothing (the census after the replay spawns the owed chief); a despawn of another epoch of
/// the agent, or of the epoch for another agent, releases nothing, and the end is then lost at
/// the replay end. The band that leads next is a new party: its agent is not the ended one, so no
/// epoch the end named is used again for it. Without the epoch's check the other epoch releases.
#[test]
fn only_the_ending_epochs_despawn_releases_the_end() {
    for (which, release) in [
        ("the epoch", true),
        ("a later epoch", false),
        ("another agent", false),
    ] {
        let (mut r, agent, epoch, generation, _) = home_out(110, false);
        r.round_trip();
        r.restart_to_replay();
        let despawn = match which {
            "the epoch" => despawned(agent, epoch),
            "a later epoch" => despawned(agent, epoch + 1),
            _ => despawned(agent ^ 1, epoch),
        };
        let at = r.c.now();
        r.c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at);
        assert_eq!(
            ended(&r).is_none(),
            release,
            "{which}: released while replaying"
        );
        let pushed = r.c.take_commands();
        assert!(
            !pushed
                .iter()
                .any(|c| matches!(c, Cmd::Reconcile(_) | Cmd::ChiefBody(_))),
            "{which}: pushed while replaying: {pushed:#?}"
        );
        r.c.replay_done();
        let present = r.c.now();
        r.c.replay_present(present);
        assert_end(&r, None);
        assert_eq!(
            death_books(&r).2,
            !release,
            "{which}: lost at the replay end"
        );
        come_in(&mut r);
        let to = r.c.now() + 300;
        r.run(to);
        assert_eq!(
            chief(&r).is_some_and(|s| s == (SlotState::Body, generation)),
            release,
            "{which}: spawned by the census"
        );
        if release {
            let at = r.c.now();
            let target = Target::District(synth::REFINERY, synth::REFINERY_POS);
            let q =
                r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, target, None, at)
                    .expect("an invasion");
            r.flush(at);
            assert!(r.c.parties[q].chief, "the chief leads the next band");
            assert_ne!(Party::agent(q), agent, "a new agent");
        }
        ok(&mut r);
    }
}
