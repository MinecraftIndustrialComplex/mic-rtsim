// SPDX-License-Identifier: AGPL-3.0-or-later
//! `sim mowzie`'s march runs (M10a): MOWZIE.md §4.16 and ledgers 23 and 25 against the engine.
//!
//! - **Run 11, an Invasion with its chief** (`invasion_marches`): the grove's Invasion on the
//!   march record, from a camp that is in and from one that is out (12.6); the chief killed away;
//!   a camp that comes in while its band is out; the throne body held and the seat held; the
//!   re-binds; the arrival with the only player 170 blocks off; a fight that loses its watcher; the
//!   dead of an abstract fight counted once. The chief's own travel is M11's, so the run's row
//!   keeps "M11".
//! - **Run 13, the march record** (`march_record`): runs 1, 5, 10 and 11 stepped 20 ticks at a time
//!   with the march audit (`rtsim_conflict::synth::march`, MZB-498) on, and the seams: a band that
//!   crosses an x-wrapped world's seam and routs, and one that crosses a glued world's.
//!
//! Players in run 11 are moved by `observe_player` (the host's own entry), not `set_player_pos`
//! (MZB-499). Where a band must stand in bodies with nobody tracking it, the run sets its fields as
//! a watched fight leaves them, as `crates/rtsim-conflict/tests/mowzie_march.rs` does.

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::engine::{Conflict, Party, Phase, SlotState, Target};
use rtsim_conflict::synth::{self, CampRun, march as audit};
use rtsim_conflict::{ConflictStats, Step};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_space::topology::{Topologies, Topology, layout};
use rtsim_world::World;

use super::{
    Check, RunOut, Walls, at_day, check, chief_bodies, chief_of, den_ladder, feud_at,
    grove_cultures, grove_ladder, heir_due, holds, quiet_grove, seat_first, send, step_of,
};

/// A second player, never the scripted one.
const OTHER: boundary::Uuid = (0x0B0B, 0xCAFE);

fn out_of(slice: &'static str, run: u32, checks: Vec<Check>) -> RunOut {
    RunOut {
        run,
        built: slice.is_empty(),
        slice,
        checks,
        rungs: Vec::new(),
        stats: None::<ConflictStats>,
        log: Vec::new(),
    }
}

fn days_between(a: Tick, b: Tick) -> f64 {
    (b.0 as f64 - a.0 as f64) / TICKS_PER_DAY as f64
}

// ---- helpers over a camp run ---------------------------------------------------------------------

/// Every march record the engine sent for `agent`, in order.
fn marches_of(r: &CampRun, agent: u64) -> Vec<boundary::BandMarch> {
    r.cmds
        .iter()
        .filter_map(|(_, c)| match c {
            Cmd::March(m) if m.agent == agent => Some(m.clone()),
            _ => None,
        })
        .collect()
}

/// The recalls sent for `agent` from command `mark` on: (epoch, reason).
fn recalls_since(r: &CampRun, agent: u64, mark: usize) -> Vec<(u32, u32)> {
    r.cmds
        .iter()
        .skip(mark)
        .filter_map(|(_, c)| match c {
            Cmd::Recall(x) if x.agent == agent => Some((x.epoch, x.reason)),
            _ => None,
        })
        .collect()
}

/// The steers sent for `agent` from command `mark` on: (epoch, flags).
fn steers_since(r: &CampRun, agent: u64, mark: usize) -> Vec<(u32, u32)> {
    r.cmds
        .iter()
        .skip(mark)
        .filter_map(|(_, c)| match c {
            Cmd::Steer(s) if s.agent == agent => Some((s.epoch, s.flags)),
            _ => None,
        })
        .collect()
}

/// Lines of `action` in the reconciles the engine sent.
fn lines_sent(r: &CampRun, action: u8) -> usize {
    r.cmds
        .iter()
        .map(|(_, c)| match c {
            Cmd::Reconcile(x) => x.lines.iter().filter(|l| l.action == action).count(),
            _ => 0,
        })
        .sum()
}

/// The outcomes of the chief slot's lines of `action` the plugin applied.
fn chief_lines(r: &CampRun, action: u8) -> Vec<u8> {
    r.plugin
        .applied
        .iter()
        .filter(|l| l.line.action == action && l.line.slot == rtsim_conflict::engine::CHIEF_SLOT)
        .map(|l| l.outcome)
        .collect()
}

fn refinery() -> Target {
    Target::District(synth::REFINERY, synth::REFINERY_POS)
}

/// A quiet grove whose chief's pressure has decayed to nothing by the time its first heir sits
/// (half-life 6 days against the 192 the first wait takes): one player's loss then makes a wait of
/// 6 x 2^1 days.
fn fast_grove(seed: u64) -> CampRun {
    let mut cultures = grove_cultures(6.0, 192.0, 6.0);
    for k in &mut cultures.list {
        k.def.plunder_mean_days = None;
    }
    CampRun::with_cultures(seed, cultures, "umvuthana", 9)
}

/// An Invasion led by the seated Umvuthi, ordered now for `feud`: the party and its generation.
fn led_invasion(r: &mut CampRun, feud: Option<ActorId>) -> Option<(Handle<Party>, u32)> {
    let seat = seat_first(r)?;
    if let Some(who) = feud {
        feud_at(r, who, Step::Invasion);
    }
    let at = r.c.now();
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, refinery(), feud, at)?;
    r.flush(at);
    Some((p, seat))
}

// ---- run 11 ---------------------------------------------------------------------------------------

/// Run 11: an Invasion with its chief, on the march record and the walk home.
pub(super) fn invasion_marches(slice: &'static str) -> RunOut {
    let mut out = Vec::new();
    with_the_chief(&mut out);
    killed_away(&mut out);
    comes_in_while_out(&mut out);
    held_throne_body(&mut out);
    rebinds(&mut out);
    let arrivals = arrivals(&mut out);
    watcher_lost(&mut out, arrivals);
    abstract_dead(&mut out);
    out_of(slice, 11, out)
}

/// MZB-413, MZB-451: an Invasion draws the Umvuthi and fights at 12.6, from a camp that is in and
/// from one that is out; its march record carries the chief's flag from the order, and the camp's
/// slot is left while it is away and back in its generation after.
fn with_the_chief(out: &mut Vec<Check>) {
    let (mut ok, mut why) = (true, String::new());
    let mut seen = Vec::new();
    for camp_out in [false, true] {
        let mut r = quiet_grove(8);
        let (walls, met) = Walls::new(1.0);
        r.infra = Box::new(walls);
        let Some(seat) = seat_first(&mut r) else {
            ok = false;
            why = "no heir".into();
            continue;
        };
        if camp_out {
            r.camp_out();
            let to = r.c.now() + 3;
            r.run(to);
        }
        let Some(p) = send(&mut r, PartyKind::Invasion) else {
            ok = false;
            why = format!("no Invasion (camp out: {camp_out})");
            continue;
        };
        let agent = Party::agent(p);
        let leads = r.c.parties[p].chief && chief_of(&r).map(|c| c.0) == Some(SlotState::Left);
        let first = marches_of(&r, agent);
        let flagged = first.first().is_some_and(|m| {
            m.flags & boundary::MARCH_CHIEF != 0 && m.kind == PartyKind::Invasion as u8
        });
        r.settle();
        let fights: Vec<(u32, f64)> = met.lock().expect("the record").clone();
        let home = chief_of(&r) == Some((SlotState::Body, seat)) && chief_bodies(&r).len() == 1;
        let strength = fights.last().map(|f| f.1);
        let books = holds(&mut r);
        seen.push((camp_out, strength));
        if !(leads && flagged && home && books && strength.is_some_and(|s| (s - 12.6).abs() < 1e-9))
        {
            ok = false;
            why = format!(
                "camp out {camp_out}: leads {leads}, record carries the chief {flagged}, home {home}, \
                 books {books}, fights {fights:?}"
            );
        }
    }
    check(
        out,
        "an Invasion with its chief from a camp that is in and from one that is out: the record \
         carries the chief, the fight is 12.6, the Umvuthi is home in its generation",
        ok,
        format!("(camp out, strength) {seen:?}; {why}"),
    );
}

/// MZB-412, MZB-451: the Umvuthi killed while it leads a band away is the slot dying away: the band
/// routs (one record, no chief, Home), the feud falls two rungs to Scouts, the slot is dead and its
/// heir is due after 12 days (6 x 2^1, one player's loss), and the band comes home without it.
fn killed_away(out: &mut Vec<Check>) {
    let mut r = fast_grove(8);
    let who = r.player;
    let Some((p, seat)) = led_invasion(&mut r, Some(who)) else {
        check(out, "the chief killed away", false, "no Invasion".into());
        return;
    };
    let agent = Party::agent(p);
    let until = r.c.parties[p].until;
    r.run(until + 50);
    let before = step_of(&r, who);
    let at = r.c.now() + 1;
    r.c.chief_killed_by(&mut r.w, p, None, boundary::BY_PLAYER, None, at);
    r.flush(at);
    let record = r.plugin.marches.get(&agent).cloned();
    let routed = record
        .as_ref()
        .is_some_and(|m| m.flags & boundary::MARCH_CHIEF == 0 && m.phase == boundary::MARCH_HOME);
    // The slot was Gone (its body set out with the band at the muster's end): dying away frees it;
    // a Left slot, killed before the set-out, would be Dead.
    let dead = matches!(chief_of(&r), None | Some((SlotState::Dead, _)));
    let wait = heir_due(&r).map(|d| days_between(at, d));
    let step = step_of(&r, who);
    r.settle();
    let home = !r.c.parties.contains(p);
    let none_home =
        chief_bodies(&r).is_empty() && chief_of(&r).map(|c| c.0) != Some(SlotState::Body);
    let due = heir_due(&r);
    if let Some(d) = due {
        r.run(d + at_day(1.5).0);
    }
    let seated = chief_of(&r).is_some_and(|c| c.0 == SlotState::Body && c.1 != seat);
    let books = holds(&mut r);
    check(
        out,
        "the Umvuthi killed away routs its band (one record, Home, no chief), steps the feud from \
         Invasion to Scouts, and the heir is due at 12 days",
        before == Some(Step::Invasion)
            && routed
            && dead
            && step == Some(Step::Scouts)
            && wait.is_some_and(|w| (w - 12.0).abs() < 0.1)
            && home
            && none_home
            && seated
            && books,
        format!(
            "feud {before:?} -> {step:?}; routed record {routed}; slot dead {dead}; heir after {wait:?} \
             days; band home without the chief {home}/{none_home}; another generation seated {seated}; \
             books hold {books}"
        ),
    );
}

/// MZB-452: a grove that comes in while its band is out finds the Umvuthi's throne body standing
/// with its slot left, and takes it; the Invasion goes on (no recall, no end), and the chief is
/// back on its throne when the band is home.
fn comes_in_while_out(out: &mut Vec<Check>) {
    let mut r = quiet_grove(8);
    let Some(seat) = seat_first(&mut r) else {
        check(
            out,
            "a grove that comes in while out",
            false,
            "no heir".into(),
        );
        return;
    };
    r.camp_out();
    let to = r.c.now() + 3;
    r.run(to);
    let Some(p) = send(&mut r, PartyKind::Invasion) else {
        check(
            out,
            "a grove that comes in while out",
            false,
            "no Invasion".into(),
        );
        return;
    };
    let agent = Party::agent(p);
    let bodies_out = chief_bodies(&r).len();
    let mark = r.cmds.len();
    r.camp_in();
    let until = r.c.parties[p].until;
    r.run(until + 100);
    let alive = r.c.parties.contains(p);
    let left = matches!(
        chief_of(&r),
        Some((SlotState::Left | SlotState::Gone, g)) if g == seat
    );
    let taken = chief_bodies(&r).is_empty();
    let recalls = recalls_since(&r, agent, mark);
    let ended = marches_of(&r, agent)
        .iter()
        .any(|m| m.flags & boundary::MARCH_ENDED != 0);
    r.settle();
    let home = chief_of(&r) == Some((SlotState::Body, seat)) && chief_bodies(&r).len() == 1;
    let books = holds(&mut r);
    check(
        out,
        "a grove that comes in while its Invasion is out takes the throne body; the Invasion goes \
         on and the Umvuthi is home after it",
        bodies_out == 1 && alive && left && taken && recalls.is_empty() && !ended && home && books,
        format!(
            "throne bodies before the census {bodies_out}; party alive {alive}, slot left {left}, \
             body taken {taken}; recalls {recalls:?}; ended early {ended}; home in its generation \
             {home}; books hold {books}"
        ),
    );
}

/// MZB-433, MZB-453: a player at the Umvuthi's body holds its set-out; the band marches on without
/// waiting, the Umvuthi dies away, the census answers the held set-out stale and the engine orders
/// the body removed, and the heir's seat goes at the wait's end. Then the same with the body in a
/// fight: the seat waits on the fight alone, past `HOLD_MAX`, and goes when it is over.
fn held_throne_body(out: &mut Vec<Check>) {
    let mut r = quiet_grove(8);
    let Some((p, seat)) = led_invasion(&mut r, None) else {
        check(out, "the held throne body", false, "no Invasion".into());
        return;
    };
    let now = r.c.now();
    r.plugin.hold_chief(r.key, true, false, now);
    let until = r.c.parties[p].until;
    r.run(until + 50);
    let waiting = chief_lines(&r, boundary::ACTION_SET_OUT).is_empty();
    let stands = chief_bodies(&r).len() == 1;
    let going = r.c.parties.contains(p) && r.c.parties[p].phase == Phase::Out;
    let at = r.c.now() + 1;
    r.c.chief_killed_by(&mut r.w, p, None, boundary::BY_PLAYER, None, at);
    r.run(at + 5);
    let dead = chief_of(&r) == Some((SlotState::Dead, seat));
    r.camp_in();
    let to = r.c.now() + 20;
    r.run(to);
    let stale = chief_lines(&r, boundary::ACTION_SET_OUT) == vec![boundary::LINE_STALE];
    let removals = lines_sent(&r, boundary::ACTION_REMOVE);
    check(
        out,
        "the Umvuthi's set-out held by a player at its body: the band marches on, and the census \
         answers the set-out stale and orders the dead chief's body removed",
        waiting && stands && going && dead && stale && removals == 1,
        format!(
            "set-out unanswered {waiting}, body stands {stands}, band out {going}; slot dead {dead}; \
             set-out stale {stale}; {removals} removal ordered"
        ),
    );

    // The seat: ordered at the wait's end over the old body, held on the fight alone.
    let pairs = lines_sent(&r, boundary::ACTION_SEAT_HEIR);
    let second = r
        .plugin
        .camp_tagged_body(r.key, 1, rtsim_conflict::engine::CHIEF_SLOT, 777);
    for body in &mut r.plugin.camp_world(r.key).bodies {
        if body.uuid == second {
            body.chief = true;
        }
    }
    let now = r.c.now();
    r.plugin.hold_chief(r.key, true, true, now);
    r.plugin.watch_throne(r.key, true, now);
    let Some(due) = heir_due(&r) else {
        check(
            out,
            "the seat waits for a fight",
            false,
            "no heir due".into(),
        );
        return;
    };
    r.run(due + 3 * synth::HOLD_MAX);
    let sent = lines_sent(&r, boundary::ACTION_SEAT_HEIR) == pairs + 1;
    let waits = chief_of(&r) == Some((SlotState::Dead, seat));
    let now = r.c.now();
    r.plugin.hold_chief(r.key, true, false, now);
    r.run(now + 10);
    let sits = chief_of(&r).is_some_and(|c| c.0 == SlotState::Body && c.1 != seat);
    check(
        out,
        "the heir's seat goes at the wait's end and waits on a fight alone, past HOLD_MAX, then sits",
        sent && waits && sits && r.failures.is_empty(),
        format!(
            "pair sent at the due tick {sent}; slot still dead through the fight {waits}; heir sits \
             when it ends {sits}; failures {:?}",
            r.failures
        ),
    );
}

/// A raid of the grove's ordered at the refinery for the scripted player, a muster later and out
/// on the road, and the player (observed at the camp, 100 ticks at a time) meeting it as it leaves.
/// Returns the band and the tick it walks out of the watcher's view.
fn met_leaving_its_camp(r: &mut CampRun) -> Option<(Handle<Party>, Tick)> {
    r.watch = false;
    let at = r.c.now();
    let who = r.player;
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Raid, refinery(), Some(who), at)?;
    r.flush(at);
    let muster = r.c.parties[p].until;
    r.run(muster + 1);
    let mut t = r.c.now();
    while r.c.stats.seen == 0 && t < at + 20 * TICKS_PER_DAY {
        r.c.observe_player(&mut r.w, synth::PLAYER, synth::CAMP_POS, t);
        t = t + 100;
        r.run(t);
    }
    let x = r.c.parties.get(p)?;
    (x.embodied && x.seen_by == Some(r.player)).then_some((p, x.unseen_due))
}

fn at_pos(p: (i32, i32)) -> Pos {
    Pos::new(0, p.0, p.1)
}

/// MZB-458, MZB-459: the watcher walks out of reach of a band, or follows it at 180 blocks, or logs
/// out with a second player near it: the band passes to a player near it, with no recall; with
/// nobody there it is recalled where it leaves, once.
fn rebinds(out: &mut Vec<Check>) {
    let mut report = Vec::new();
    let mut ok = true;
    for case in ["walked off", "followed", "logged out", "nobody"] {
        let mut r = quiet_grove(9);
        if seat_first(&mut r).is_none() {
            ok = false;
            report.push(format!("{case}: no heir"));
            continue;
        }
        let Some((band, exit)) = met_leaving_its_camp(&mut r) else {
            ok = false;
            report.push(format!("{case}: never met"));
            continue;
        };
        let agent = Party::agent(band);
        let epoch = r.c.parties[band].epoch;
        let here = r.c.party_pos(band, exit);
        let mark = r.cmds.len();
        let (kept, passed_to, recalls) = match case {
            "logged out" => {
                let other =
                    r.c.observe_player(&mut r.w, OTHER, at_pos((here.0 + 100, here.1)), r.c.now());
                let watcher = r.player;
                let t = r.c.now();
                r.c.player_left(&mut r.w, watcher);
                r.flush(t);
                let x = &r.c.parties[band];
                (
                    x.embodied,
                    x.seen_by == Some(other),
                    recalls_since(&r, agent, mark),
                )
            }
            _ => {
                r.run(Tick(exit.0 - 60));
                let near = at_pos((here.0 + 180, here.1));
                let other = match case {
                    "walked off" => Some(r.c.observe_player(&mut r.w, OTHER, near, r.c.now())),
                    "followed" => {
                        Some(r.c.observe_player(&mut r.w, synth::PLAYER, near, r.c.now()))
                    }
                    _ => None,
                };
                r.run(exit + 3);
                let x = &r.c.parties[band];
                (
                    x.embodied,
                    other.is_some() && x.seen_by == other,
                    recalls_since(&r, agent, mark),
                )
            }
        };
        let good = match case {
            "nobody" => !kept && recalls == vec![(epoch, boundary::RECALL_PASSING)],
            _ => kept && passed_to && recalls.is_empty(),
        };
        ok &= good;
        report.push(format!(
            "{case}: kept {kept}, passed on {passed_to}, recalls {recalls:?}"
        ));
    }
    check(
        out,
        "a band re-binds to a player near it when its watcher walks off, follows or logs out, with \
         no recall; with nobody near it is recalled once",
        ok,
        report.join("; "),
    );
}

/// A raid of the grove's in bodies that arrives at the refinery a tick from now, as a watched fight
/// leaves it (embodied, every member placed, nobody booked as its watcher), with a player `off`
/// blocks east of the refinery, reported fresh. The band, its epoch and the player's actor.
fn arriving_with_a_player(
    r: &mut CampRun,
    off: i32,
    uuid: boundary::Uuid,
) -> Option<(Handle<Party>, u32, ActorId)> {
    r.watch = false;
    let at = r.c.now();
    let who = r.player;
    let band =
        r.c.order_party(&mut r.w, r.key, PartyKind::Raid, refinery(), Some(who), at)?;
    r.flush(at);
    let muster = r.c.parties[band].until;
    r.run(muster + 1);
    let x = &mut r.c.parties[band];
    x.epoch += 1;
    x.embodied = true;
    x.spawned = x.units.clone();
    let epoch = x.epoch;
    let arrive = *r.c.parties[band].times.last()?;
    r.run(Tick(arrive.0 - 200));
    let player = r.c.observe_player(
        &mut r.w,
        uuid,
        at_pos((synth::REFINERY_POS.x + off, synth::REFINERY_POS.z)),
        Tick(arrive.0 - 200),
    );
    Some((band, epoch, player))
}

/// What run 11's arrival cases leave for the check after them: the 150-block fight, still standing.
struct Fighting {
    r: CampRun,
    band: Handle<Party>,
    epoch: u32,
    watcher: ActorId,
}

/// MZB-460: a band that walked to its target in bodies and finds no one within the view radius
/// (160) but a player within the circle (192), at 170 blocks, passes to that player: resolved
/// abstractly (no fight), its losses taken off the placed members, and walked home in the bodies.
/// At 150 blocks it is a fight, in the same bodies, told to attack where it stands.
fn arrivals(out: &mut Vec<Check>) -> Option<Fighting> {
    let (mut ok, mut why) = (true, String::new());
    let mut report = Vec::new();
    let mut stood = None;
    for (off, uuid) in [(170, OTHER), (150, OTHER)] {
        let mut r = quiet_grove(9);
        if seat_first(&mut r).is_none() {
            ok = false;
            why = "no heir".into();
            continue;
        }
        let Some((band, epoch, player)) = arriving_with_a_player(&mut r, off, uuid) else {
            ok = false;
            why = format!("{off}: no band");
            continue;
        };
        let agent = Party::agent(band);
        let mark = r.cmds.len();
        let arrive = *r.c.parties[band].times.last().expect("a route");
        r.run(arrive + 10);
        let x = &r.c.parties[band];
        let recalls = recalls_since(&r, agent, mark);
        let steers = steers_since(&r, agent, mark);
        let same = x.embodied && x.epoch == epoch && x.seen_by == Some(player);
        let fight = r.c.stats.raids_watched;
        report.push(format!(
            "{off} blocks: fights {fight}, phase {:?}, kept in its bodies {same}, recalls {}, {} \
             steers, {} of {} alive placed",
            x.phase,
            recalls.len(),
            steers.len(),
            x.spawned.iter().map(|s| s.1).sum::<u32>(),
            x.alive()
        ));
        let good = if off == 170 {
            let placed_fit = x
                .spawned
                .iter()
                .all(|s| s.1 <= x.units.iter().find(|e| e.0 == s.0).map_or(0, |e| e.1));
            fight == 0
                && recalls.is_empty()
                && same
                && x.phase == Phase::Home
                && !steers.is_empty()
                && placed_fit
        } else {
            fight == 1
                && x.phase == Phase::Fighting
                && same
                && recalls.is_empty()
                && steers
                    .iter()
                    .any(|s| s.0 == epoch && s.1 == boundary::STEER_ATTACK)
        };
        if !good {
            ok = false;
            why = format!(
                "{off} blocks: fights {fight}, phase {:?}, kept in its bodies {same}, recalls \
                 {recalls:?}, steers {steers:?}",
                x.phase
            );
        }
        if off == 150 && good {
            stood = Some(Fighting {
                r,
                band,
                epoch,
                watcher: player,
            });
        }
    }
    check(
        out,
        "an arrival with the only player 170 blocks off is resolved abstractly and walked home, \
         with the player 150 off it is a fight",
        ok,
        format!("{}; {why}", report.join("; ")),
    );
    stood
}

/// MZB-459: the fight's watcher leaves with a second player 100 blocks off: the fight turns to
/// waiting, the band keeps its bodies and the second player watches it; nothing is recalled.
fn watcher_lost(out: &mut Vec<Check>, fighting: Option<Fighting>) {
    let Some(Fighting {
        mut r,
        band,
        epoch,
        watcher,
    }) = fighting
    else {
        check(
            out,
            "a fight that loses its watcher",
            false,
            "no fight to lose".into(),
        );
        return;
    };
    let agent = Party::agent(band);
    let t = r.c.now();
    let here = r.c.party_pos(band, t);
    let other =
        r.c.observe_player(&mut r.w, synth::PLAYER, at_pos((here.0 + 100, here.1)), t);
    let mark = r.cmds.len();
    r.c.player_left(&mut r.w, watcher);
    r.flush(t);
    let x = &r.c.parties[band];
    let recalls = recalls_since(&r, agent, mark);
    check(
        out,
        "a fight that loses its watcher with a second player near keeps its bodies and turns to \
         waiting",
        x.phase == Phase::Waiting
            && x.embodied
            && x.epoch == epoch
            && x.seen_by == Some(other)
            && recalls.is_empty(),
        format!(
            "phase {:?}, embodied {}, watched by the second player {}, recalls {recalls:?}",
            x.phase,
            x.embodied,
            x.seen_by == Some(other)
        ),
    );
}

/// The survivors a recall would count for `band`: its units alive by body id.
fn alive_by_body(r: &CampRun, band: Handle<Party>) -> Vec<(u32, u32)> {
    let cu = r.c.cultures.get(r.c.factions[r.f as usize].culture).clone();
    r.c.parties[band]
        .units
        .iter()
        .filter(|e| e.1 > 0)
        .map(|&(u, n)| {
            (
                rtsim_conflict::engine::body_id(&r.c.cultures, &cu.unit(u).body),
                n,
            )
        })
        .collect()
}

/// MZB-462: a band that walked to a village in bodies and is resolved abstractly there (its watcher
/// is 170 blocks off) loses members to the village's defence; the bodies follow the members left.
/// When the watcher leaves the band is recalled with those bodies, and Java's despawn counts only
/// the one that died on the walk off. The same when the engine is saved after the recall and the
/// despawn is replayed from the pages.
fn abstract_dead(out: &mut Vec<Check>) {
    let (mut ok, mut why) = (true, String::new());
    let mut figures = Vec::new();
    for crash in [false, true] {
        let mut r = quiet_grove(63);
        if seat_first(&mut r).is_none() {
            ok = false;
            why = "no heir".into();
            continue;
        }
        let Some(ashby) = r.w.node_of_key(4) else {
            ok = false;
            why = "no Ashby".into();
            continue;
        };
        let there = r.w.nodes[ashby].pos;
        let m = r.w.market(ashby);
        let rich: BTreeMap<rtsim_trade::GoodId, i64> =
            r.w.goods
                .ids()
                .map(|g| (g, m.line(g).target + 20 * rtsim_trade::MILLI - m.stock(g)))
                .filter(|e| e.1 > 0)
                .collect();
        let at = r.c.now();
        r.w.stock_from_players(ashby, &rich, at);
        let who = r.player;
        let Some(band) = r.c.order_party(
            &mut r.w,
            r.key,
            PartyKind::Invasion,
            Target::Node(ashby),
            Some(who),
            at,
        ) else {
            ok = false;
            why = "no raid".into();
            continue;
        };
        r.flush(at);
        let muster = r.c.parties[band].until;
        r.run(muster + 1);
        let x = &mut r.c.parties[band];
        x.epoch += 1;
        x.embodied = true;
        x.spawned = x.units.clone();
        let epoch = x.epoch;
        let n = x.alive();
        let arrive = *x.times.last().expect("a route");
        r.run(Tick(arrive.0 - 200));
        let watcher = r.c.observe_player(
            &mut r.w,
            OTHER,
            at_pos((there.x + 170, there.z)),
            Tick(arrive.0 - 200),
        );
        r.run(arrive + 10);
        let x = &r.c.parties[band];
        let village = r.c.stats.raids_village;
        let left = x.alive();
        let follow =
            x.embodied && x.epoch == epoch && x.spawned.iter().map(|s| s.1).sum::<u32>() == left;
        let agent = Party::agent(band);
        let mark = r.cmds.len();
        let t = r.c.now();
        r.c.player_left(&mut r.w, watcher);
        r.flush(t);
        let recalls = recalls_since(&r, agent, mark);
        let x = &r.c.parties[band];
        let folded =
            !x.embodied && x.spawned.is_empty() && x.recalled == vec![(epoch, x.units.clone())];
        if crash {
            r.c = rtsim_conflict::engine::Conflict::load(
                r.c.tun.clone(),
                r.c.cultures.clone(),
                &r.c.save(),
            )
            .expect("loads");
            r.c.log_enabled = true;
            audit::session();
            r.c.begin_replay(true);
        }
        let mut survivors = alive_by_body(&r, band);
        if let Some(first) = survivors.first_mut() {
            first.1 -= 1;
        }
        let at = r.c.now() + 1;
        r.c.ingest(
            &mut r.w,
            &mut NoInfrastructure,
            &Obs::Dematerialized(boundary::BandDematerialized {
                agent,
                epoch,
                reason: boundary::RECALL_PLAYER_LEFT,
                survivors,
            }),
            at,
        );
        if crash {
            r.c.replay_done();
        }
        let x = &r.c.parties[band];
        let once = x.alive() == left - 1 && x.recalled.is_empty();
        figures.push((crash, n, left, x.alive()));
        if !(village == 1
            && left >= 2
            && left < n
            && follow
            && recalls.len() == 1
            && folded
            && once)
        {
            ok = false;
            why = format!(
                "crash {crash}: village raids {village}, {n} -> {left}, bodies follow {follow}, \
                 recalls {recalls:?}, recalled stack folded {folded}, alive after {} with {} \
                 recalled",
                x.alive(),
                x.recalled.len()
            );
        }
    }
    check(
        out,
        "the dead of an abstract fight in bodies are counted once at the recall, and across a \
         restart",
        ok,
        format!(
            "(crash, band, left after the village, alive after the despawn) {figures:?}; {why}"
        ),
    );
}

// ---- run 13 ---------------------------------------------------------------------------------------

/// What one audited run found, for the detail line.
fn audited<T>(f: impl FnOnce() -> T) -> (T, audit::Totals) {
    audit::on();
    let v = f();
    (v, audit::off())
}

/// Run 13: the march record (MOWZIE.md §4.16, MZB-454, MZB-498).
pub(super) fn march_record(seed: u64, slice: &'static str) -> RunOut {
    let mut out = Vec::new();
    let mut all = audit::Totals::default();
    let mut lines = Vec::new();
    // The stepped runs' own checks are not read: a step of 20 ticks moves the plugin's and the
    // watching player's clocks, so their figures differ. The audit's verdict is what counts.
    let runs: [(&str, Box<dyn FnOnce() + '_>); 4] = [
        (
            "1",
            Box::new(move || {
                let mut checks = Vec::new();
                let _ = den_ladder(seed, "");
                super::rail::meter(&mut checks);
            }),
        ),
        (
            "5",
            Box::new(move || {
                let _ = grove_ladder(seed, "");
            }),
        ),
        (
            "10",
            Box::new(move || {
                let _ = super::camp_reconciler(seed, "");
            }),
        ),
        (
            "11",
            Box::new(|| {
                let _ = invasion_marches("");
            }),
        ),
    ];
    for (name, f) in runs {
        let (_, t) = audited(f);
        lines.push(format!(
            "run {name}: {} steps, {} unchanged party-steps ({} ticks), {} changed, {} records \
             ({} ending), {} shell-steps",
            t.steps, t.still, t.ticks, t.changed, t.records, t.ended, t.shells
        ));
        all.add(&t);
    }
    check(
        &mut out,
        "every party unchanged across a step has its record's position equal the engine's at every \
         tick of the step, and a changed one at the step's end, in runs 1, 5, 10 and 11 stepped 20 \
         ticks at a time",
        all.failed == 0 && all.ticks > 0 && all.still > 0 && all.changed > 0,
        format!("{}; failures {:?}", lines.join("; "), all.failures),
    );
    check(
        &mut out,
        "a record follows each change of route, phase or count",
        all.unrecorded == 0 && all.changes > 0,
        format!(
            "{} changes seen, {} with no record in their step",
            all.changes, all.unrecorded
        ),
    );
    no_census_ends_a_party(&mut out);
    seam(
        &mut out,
        "an x-wrapped world",
        Topology::x_wrap(0, 1200),
        Pos::new(0, 100, 1000),
    );
    seam(
        &mut out,
        "a glued (Petroff-Guyou) world",
        layout::guyou(2200, false),
        Pos::new(0, -900, 1000),
    );
    out_of(slice, 13, out)
}

/// MZB-451: a census (the camp out and in again, the plugin's whole answer) ends no party: an
/// Invasion out for days across ten of them is still there and has sent no ENDED record, and it
/// sends exactly one when it is home.
fn no_census_ends_a_party(out: &mut Vec<Check>) {
    let mut r = quiet_grove(12);
    let Some((p, _)) = led_invasion(&mut r, None) else {
        check(out, "no census ends a party", false, "no Invasion".into());
        return;
    };
    let agent = Party::agent(p);
    let mut ok = true;
    let mut censuses = 0;
    for _ in 0..10 {
        r.camp_out();
        let to = r.c.now() + 3;
        r.run(to);
        r.camp_in();
        let to = r.c.now() + 400;
        r.run(to);
        censuses += 1;
        if !r.c.parties.contains(p) {
            break;
        }
        ok &= marches_of(&r, agent)
            .iter()
            .all(|m| m.flags & boundary::MARCH_ENDED == 0);
    }
    let still_out = r.c.parties.contains(p);
    r.settle();
    let ends = marches_of(&r, agent)
        .iter()
        .filter(|m| m.flags & boundary::MARCH_ENDED != 0)
        .count();
    check(
        out,
        "no census ends a party: an Invasion out across ten censuses sends no ENDED, and one when \
         it is home",
        ok && still_out && censuses == 10 && ends == 1 && !r.c.parties.contains(p),
        format!("{censuses} censuses, party still out {still_out}, ENDED records {ends} after"),
    );
}

/// A frostmaw den's raid on `target` in a world of topology `topo`, ordered at tick 20.
fn seam_raid(seed: u64, topo: Topology, target: Pos) -> Option<(World, Conflict, Handle<Party>)> {
    let (mut w, _) = synth::world(seed);
    let mut c = synth::conflict(seed, &w);
    let mut t = Topologies::new();
    t.set(0, topo);
    c.set_topologies(t.shared());
    c.add_faction(&mut w, synth::CAMP, "frostmaw", synth::CAMP_POS, Tick::ZERO)?;
    let who = c.player(&mut w, synth::PLAYER);
    let band = c.order_party(
        &mut w,
        synth::CAMP,
        PartyKind::Raid,
        Target::Pos(target),
        Some(who),
        Tick(20),
    )?;
    Some((w, c, band))
}

/// MZB-454, MZB-456: a band whose route crosses a seam. Out and home, at every tick, its position
/// (canonical, `party_pos`) is the fold of the chart cell's block centre, the way Java folds it, and
/// consecutive ticks are never further apart than the march's speed. Routed after it has crossed,
/// its way home starts at its chart position: the route is `here` and the route it came by, back to
/// front, with no lap through the world, and its first segment is as long as the walk it has made.
fn seam(out: &mut Vec<Check>, what: &'static str, topo: Topology, target: Pos) {
    let name = match what {
        "an x-wrapped world" => {
            "on an x-wrapped world a band that crosses the seam and routs walks home from its chart \
             position: no lap, a first segment as long as its walk, and its cell is the fold of its \
             block centre at every tick"
        }
        _ => {
            "on a glued world the canonical point of the block centre, floored, equals the band's \
             canonical cell at every tick across the seam, and a band routed after it walks home \
             from its chart position"
        }
    };
    let Some((mut w, mut c, band)) = seam_raid(21, topo.clone(), target) else {
        check(out, name, false, format!("{what}: no band"));
        return;
    };
    let speed = c
        .cultures
        .get(c.factions[0].culture)
        .def
        .march_blocks_per_tick;
    let muster = c.parties[band].until;
    let step = |c: &mut Conflict, w: &mut World, t: Tick| {
        let _ = audit::advance(c, w, &mut NoInfrastructure, t);
    };
    step(&mut c, &mut w, muster);
    let route = c.parties[band].route.clone();
    let end = *c.parties[band].times.last().expect("a route");
    let (mut prev, mut crossed, mut moved_ok, mut fold_ok) =
        (None::<(i32, i32)>, None::<Tick>, true, true);
    let mut t = muster;
    let mut stood = 0u64;
    while t < end {
        t = t + 1;
        step(&mut c, &mut w, t);
        if !c.parties.contains(band) {
            break;
        }
        let (cx, cz) = c.party_chart_pos(band, t);
        let cell = c.party_pos(band, t);
        let p = topo.canonical_point(f64::from(cx) + 0.5, f64::from(cz) + 0.5);
        fold_ok &= (p.0.floor() as i32, p.1.floor() as i32) == cell;
        if cell != (cx, cz) && crossed.is_none() {
            crossed = Some(t);
        }
        if let Some(q) = prev {
            let d = topo.dist(
                (i64::from(cell.0), i64::from(cell.1)),
                (i64::from(q.0), i64::from(q.1)),
            );
            moved_ok &= d <= speed + 2.0;
            stood += u64::from(q == cell);
        }
        prev = Some(cell);
    }
    let seam_crossed = crossed.is_some();

    // The same band, routed a quarter of the way past the seam.
    let (mut w, mut c, band) = seam_raid(21, topo.clone(), target).expect("the same raid");
    step(&mut c, &mut w, muster);
    let mut t = muster;
    let first_over = crossed.unwrap_or(end);
    while t < Tick(first_over.0 + (end.0 - first_over.0) / 4) {
        t = t + 1;
        step(&mut c, &mut w, t);
    }
    let (here, before) = (
        c.party_chart_pos(band, t),
        (
            c.parties[band].route.clone(),
            c.parties[band].seg,
            c.parties[band].times.clone(),
        ),
    );
    let canonical = c.party_pos(band, t);
    let had_chief = c.parties[band].chief;
    c.chief_killed(&mut w, band, None, t);
    let x = &c.parties[band];
    let mut want: Vec<(i32, i32)> = before.0[..=before.1 as usize].to_vec();
    want.reverse();
    if want.first() != Some(&here) {
        want.insert(0, here);
    }
    let walked = ((here.0 - before.0[before.1 as usize].0).pow(2) as f64
        + (here.1 - before.0[before.1 as usize].1).pow(2) as f64)
        .sqrt();
    let len = |r: &[(i32, i32)]| -> f64 {
        r.windows(2)
            .map(|s| (f64::from(s[1].0 - s[0].0)).hypot(f64::from(s[1].1 - s[0].1)))
            .sum()
    };
    let first = if x.route.len() >= 2 {
        (f64::from(x.route[1].0 - x.route[0].0)).hypot(f64::from(x.route[1].1 - x.route[0].1))
    } else {
        f64::NAN
    };
    let no_lap = x.route == want && len(&x.route) <= len(&route) + 2.0;
    let first_ok = (first - walked).abs() < 2.0 && x.phase == Phase::Home;
    let hop = x.times.get(1).map_or(u64::MAX, |h| h.0 - x.times[0].0) as f64;
    let walk_ticks = (t.0 - before.2[before.1 as usize].0) as f64;
    let time_ok = (hop - walk_ticks).abs() <= 2.0 / speed.max(1e-6) + 1.0;
    let over = here != canonical;
    // The walk home, tick by tick, is as continuous.
    let home_end = *c.parties[band].times.last().expect("a route");
    let mut home_prev = Some(c.party_pos(band, t));
    let mut home_ok = true;
    let mut u = t;
    while u < home_end {
        u = u + 1;
        step(&mut c, &mut w, u);
        if !c.parties.contains(band) {
            break;
        }
        let cell = c.party_pos(band, u);
        if let Some(q) = home_prev {
            home_ok &= topo.dist(
                (i64::from(cell.0), i64::from(cell.1)),
                (i64::from(q.0), i64::from(q.1)),
            ) <= speed + 2.0;
        }
        home_prev = Some(cell);
    }
    let unchanged_steps = stood;
    check(
        out,
        name,
        seam_crossed
            && fold_ok
            && moved_ok
            && had_chief
            && over
            && no_lap
            && first_ok
            && time_ok
            && home_ok,
        format!(
            "{what}: crossed the seam at tick {:?}; every cell the fold of its centre {fold_ok}; no \
             jump out or home {moved_ok}/{home_ok} ({unchanged_steps} still ticks); routed at chart \
             {here:?} (canonical {canonical:?}, over the seam {over}, chief rode {had_chief}); the \
             way home is `here` and the road it came by {no_lap}; first segment {first:.1} blocks \
             for a walk of {walked:.1}, {hop} ticks for {walk_ticks}",
            crossed.map(|c| c.0)
        ),
    );
}
