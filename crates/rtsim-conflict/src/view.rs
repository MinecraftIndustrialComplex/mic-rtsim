// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only views of the conflict for other crates (TALK, slice 4, docs/choices-talk.md TK74).
//!
//! This is the only way TALK reads a faction: `rtsim-jni` answers a camp (holder kind 3) from
//! [`faction_facts`] and nothing else, so what the conflict keeps in its rows can change without
//! the talk engine knowing. A view reads with `&Conflict`, projects at the engine's own time
//! (O4's rule: a read commits nothing, so it never changes the trajectory) and holds plain values.
//! The Mowzie slices that change [`crate::engine::Faction`] (M4, M6) keep this file compiling and
//! meaning the same.

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;

use crate::culture::{SignalKind, Step};
use crate::engine::Conflict;
use crate::grievance::K;

/// One faction as a talker sees it, at the engine's time, and its feud against one player.
#[derive(Clone, Debug, PartialEq)]
pub struct FactionFacts {
    /// The faction itself as a knowledge actor (what villages feel toward it).
    pub actor: ActorId,
    /// Its haven's node in the world's node list. The conflict holds no world, so the node's own
    /// actor (`w.nodes[node].actor`, which `succeed` uses as the heir's home) is the caller's to
    /// resolve.
    pub node: usize,
    pub pos: Pos,
    pub chief: ActorId,
    pub heir: ActorId,
    /// Chiefs so far, counting the heir: a new faction has 2, and each succession adds one.
    pub chiefs: u32,
    pub chief_out: bool,
    pub cowed_until: Tick,
    /// Warriors now (the closed form at the engine's time).
    pub pool: f64,
    /// The culture's cap on warriors.
    pub pool_cap: f64,
    pub plunders: u32,
    /// The feud's rung at the player: [`Step::None`] with no feud or no player.
    pub step: Step,
    /// The feud's grievance at the player, the projected sum as `feud_view` sums it; 0 with no
    /// feud or no player.
    pub grievance: f64,
    /// The kind holding the most of that grievance (the first of equals, as the ladder's own
    /// notice picks it); `None` with no feud, no player, or nothing held.
    pub signal: Option<SignalKind>,
    /// The engine's time the view was taken at.
    pub now: Tick,
}

/// The faction under `faction` (its key, what `add_faction` was given) as it stands now, and its
/// feud against `player` when there is one. `None` for a key the conflict does not know.
pub fn faction_facts(c: &Conflict, faction: u64, player: Option<ActorId>) -> Option<FactionFacts> {
    let f = c.faction_of_key(faction)?;
    let x = &c.factions[f as usize];
    let culture = c.cultures.get(x.culture);
    let mut step = Step::None;
    let mut grievance = 0.0;
    let mut signal = None;
    if let Some(feud) = player.and_then(|p| c.feud(f, p)) {
        step = feud.step;
        let v = feud.g.project(c.now, culture.lambda);
        grievance = v.iter().sum();
        let mut best = None;
        let mut most = 0.0;
        for (k, kind) in SignalKind::ALL.iter().enumerate().take(K) {
            if v[k] > most {
                most = v[k];
                best = Some(*kind);
            }
        }
        signal = best;
    }
    Some(FactionFacts {
        actor: x.actor,
        node: x.node,
        pos: x.pos,
        chief: x.chief,
        heir: x.heir,
        chiefs: x.chiefs,
        chief_out: x.chief_out,
        cowed_until: x.cowed_until,
        pool: c.pool_at(f, c.now),
        pool_cap: f64::from(culture.def.pool_cap),
        plunders: x.plunders,
        step,
        grievance,
        signal,
        now: c.now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Signal;
    use crate::synth;

    fn setup(seed: u64) -> (rtsim_world::World, Conflict) {
        let (mut w, _) = synth::world(seed);
        let mut c = synth::conflict(seed, &w);
        c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
            .unwrap();
        (w, c)
    }

    #[test]
    fn a_key_the_conflict_does_not_know_gives_none() {
        let (_w, c) = setup(3);
        assert!(faction_facts(&c, synth::CAMP + 1, None).is_none());
        assert!(faction_facts(&c, 0, None).is_none());
    }

    #[test]
    fn a_found_faction_gives_its_actor_position_and_chief() {
        let (w, c) = setup(3);
        let f = c.faction_of_key(synth::CAMP).unwrap();
        let x = &c.factions[f as usize];
        let v = faction_facts(&c, synth::CAMP, None).unwrap();
        assert_eq!(v.actor, x.actor);
        assert_eq!(v.pos, synth::CAMP_POS);
        assert_eq!(v.chief, x.chief);
        assert_eq!(v.heir, x.heir);
        assert_ne!(v.chief, v.heir);
        assert_eq!(v.chiefs, 2, "a new camp has its chief and its heir");
        assert!(!v.chief_out);
        assert_eq!(v.cowed_until, Tick::ZERO);
        assert_eq!(v.plunders, x.plunders);
        assert!(
            v.pool > 0.0 && v.pool <= v.pool_cap,
            "{} of {}",
            v.pool,
            v.pool_cap
        );
        // The node is the camp's own, and the world knows its actor.
        assert_eq!(w.nodes[v.node].pos, synth::CAMP_POS);
        // No player, no feud.
        assert_eq!(v.step, Step::None);
        assert_eq!(v.grievance, 0.0);
        assert_eq!(v.signal, None);
    }

    #[test]
    fn a_stranger_has_no_feud_and_the_view_is_the_same_as_no_player() {
        let (mut w, mut c) = setup(3);
        let p = c.player(&mut w, synth::PLAYER);
        let none = faction_facts(&c, synth::CAMP, None).unwrap();
        let stranger = faction_facts(&c, synth::CAMP, Some(p)).unwrap();
        assert_eq!(stranger.step, Step::None);
        assert_eq!(stranger.grievance, 0.0);
        assert_eq!(stranger.signal, None);
        assert_eq!(none, stranger);
    }

    #[test]
    fn after_grievance_at_a_player_the_step_the_sum_and_the_signal_show() {
        let (mut w, mut c) = setup(3);
        let p = c.player(&mut w, synth::PLAYER);
        c.signal(
            &mut w,
            Signal {
                faction: Some(synth::CAMP),
                kind: SignalKind::Killing,
                amount: 3.0,
                actor: p,
                pos: synth::FOREST_POS,
                district: None,
            },
            Tick(100),
        );
        let fv = c.feud_view(synth::CAMP, p).unwrap();
        let v = faction_facts(&c, synth::CAMP, Some(p)).unwrap();
        assert_eq!(v.grievance, fv.grievance, "summed as feud_view sums it");
        assert!(v.grievance > 0.0);
        assert_eq!(v.step, fv.step);
        assert_eq!(v.signal, Some(SignalKind::Killing));
        assert_eq!(v.now, Tick(100));
        // Another player is still a stranger.
        let q = c.player(&mut w, (9, 9));
        let other = faction_facts(&c, synth::CAMP, Some(q)).unwrap();
        assert_eq!(other.step, Step::None);
        assert_eq!(other.signal, None);
    }

    #[test]
    fn the_largest_kind_wins_and_equals_go_to_the_lowest_index() {
        let (mut w, mut c) = setup(3);
        let p = c.player(&mut w, synth::PLAYER);
        let sig = |kind, amount| Signal {
            faction: Some(synth::CAMP),
            kind,
            amount,
            actor: p,
            pos: synth::FOREST_POS,
            district: None,
        };
        c.signal(&mut w, sig(SignalKind::Logging, 1.0), Tick(100));
        c.signal(&mut w, sig(SignalKind::Mining, 4.0), Tick(100));
        let v = faction_facts(&c, synth::CAMP, Some(p)).unwrap();
        assert_eq!(v.signal, Some(SignalKind::Mining));
    }

    #[test]
    fn a_view_commits_nothing() {
        let (mut w, mut c) = setup(3);
        let p = c.player(&mut w, synth::PLAYER);
        c.signal(
            &mut w,
            Signal {
                faction: Some(synth::CAMP),
                kind: SignalKind::Killing,
                amount: 2.0,
                actor: p,
                pos: synth::FOREST_POS,
                district: None,
            },
            Tick(100),
        );
        let before = c.feud_view(synth::CAMP, p).unwrap();
        for _ in 0..5 {
            let _ = faction_facts(&c, synth::CAMP, Some(p));
        }
        assert_eq!(c.feud_view(synth::CAMP, p).unwrap(), before);
    }
}
