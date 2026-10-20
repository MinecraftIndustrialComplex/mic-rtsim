// SPDX-License-Identifier: AGPL-3.0-or-later
//! Latent members and derived identity (SCALE.md §2's `sample` and `fold`; PLAN.md §4.4).
//!
//! **Identity is derived.** A member is its index in the herd; its seed is a pure function of
//! (herd seed, index), and its look (a sheep's colour, a horse's coat and markings, a screaming
//! goat) is rolled from that seed with vanilla's own odds. Its age is its year-class's. So the
//! same ewe with the same colour comes back every time, and an animal nobody has seen costs no
//! bytes.
//!
//! **Which members are alive** is a convention over counts. In a class with `alive` members, the
//! seen ones (in the herd's `seen` map) are alive, and of the unseen ones, not dead, the
//! `alive − seen` lowest indices are. Anonymous deaths (the dyadic counts, predators taking
//! latent animals) therefore always fall on unseen members: a member you saw never quietly dies
//! off-screen and comes back. Only if a class's count falls below its seen members (old age
//! reaching them) do seen members die, highest index first, and they go to the dead set.
//!
//! **Sampling** (`pick`): seen members first, most recently seen first (the group you met comes
//! back as it was), then fresh members spread over the classes in proportion to their unseen
//! counts (largest remainder), lowest unseen index first. Every pick becomes seen.
//!
//! **The dead set** holds members a player saw that died or left the sim (killed, taken, eaten
//! while watched): never sampled again. Entries go when their class has no one left.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::rng::{StreamKey, SystemId};

use super::herd::{Herd, Seen};
use super::species::{Cohort, HerdSpeciesData, Look, look};

pub const SYS_MEMBER: SystemId = SystemId::named("herds.member");

/// A member's identity seed: (herd seed, index) only.
pub fn member_seed(herd_seed: u64, index: u64) -> u64 {
    StreamKey::new(herd_seed, SYS_MEMBER, index).block(0, 0)[0]
}

/// A member as it would materialize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    pub index: u64,
    pub seed: u64,
    pub look: Look,
    pub cohort: Cohort,
    pub age_days: u32,
    pub sheared: bool,
}

/// The member at `index` on `day` (whether or not it is alive: the caller knows).
pub fn member(h: &Herd, sp: &HerdSpeciesData, index: u64, day: i64, now: Tick, regrow: u64) -> Option<Member> {
    let ci = h.class_of(index)?;
    let c = &h.classes[ci];
    let cohort = c.cohort_on(sp, day)?;
    let seed = member_seed(h.seed, index);
    let sheared = h
        .seen
        .get(&index)
        .and_then(|s| s.sheared_at)
        .is_some_and(|t| now.0 < t.0 + regrow);
    Some(Member {
        index,
        seed,
        look: look(h.species, seed),
        cohort,
        age_days: c.age_on(day).max(0) as u32,
        sheared,
    })
}

/// Brings `seen` in line with the counts on `day`: a class whose count fell below its seen
/// members loses its highest seen indices (not embodied ones first) to the dead set. A
/// deterministic function of the state and the day, run only at events. Returns how many died
/// this way.
pub fn reconcile(h: &mut Herd, sp: &HerdSpeciesData, year_days: u32, day: i64) -> u32 {
    let mut died = 0;
    let embodied: Vec<u64> = h
        .embodied
        .as_ref()
        .map(|e| e.members.clone())
        .unwrap_or_default();
    for i in 0..h.classes.len() {
        let alive = h.classes[i].alive_on(sp, year_days, h.seed, day) as usize;
        let seen: Vec<u64> = h.seen_in(i).map(|(k, _)| *k).collect();
        if seen.len() <= alive {
            continue;
        }
        let mut excess = seen.len() - alive;
        let (mut quiet, mut watched): (Vec<u64>, Vec<u64>) =
            seen.into_iter().partition(|k| !embodied.contains(k));
        quiet.reverse();
        watched.reverse();
        for k in quiet.into_iter().chain(watched) {
            if excess == 0 {
                break;
            }
            h.seen.remove(&k);
            h.dead.insert(k);
            excess -= 1;
            died += 1;
        }
    }
    died
}

/// Picks up to `n` members to materialize on `day` (seen first, then fresh), marks them seen at
/// `now`, and returns their indices. Call [`reconcile`] first.
pub fn pick(
    h: &mut Herd,
    sp: &HerdSpeciesData,
    year_days: u32,
    day: i64,
    n: usize,
    now: Tick,
    seen_cap: usize,
) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::with_capacity(n);
    // Seen members, most recently seen first, then by index.
    let mut seen: Vec<(Tick, u64)> = h
        .seen
        .iter()
        .filter(|(k, _)| h.class_of(**k).is_some())
        .map(|(k, s)| (s.last, *k))
        .collect();
    seen.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    out.extend(seen.iter().take(n).map(|x| x.1));
    // Fresh members: largest remainder over the classes' unseen counts.
    let want = n - out.len();
    if want > 0 {
        let unseen: Vec<u64> = (0..h.classes.len())
            .map(|i| {
                let alive = u64::from(h.classes[i].alive_on(sp, year_days, h.seed, day));
                let s = h.seen_in(i).count() as u64;
                if h.classes[i].cohort_on(sp, day).is_none() {
                    0
                } else {
                    alive.saturating_sub(s)
                }
            })
            .collect();
        let total: u64 = unseen.iter().sum();
        let want = (want as u64).min(total);
        if want > 0 {
            let mut share: Vec<(u64, u64, usize)> = unseen
                .iter()
                .enumerate()
                .map(|(i, &u)| {
                    let x = u128::from(u) * u128::from(want);
                    (
                        (x / u128::from(total)) as u64,
                        (x % u128::from(total)) as u64,
                        i,
                    )
                })
                .collect();
            let mut given: u64 = share.iter().map(|s| s.0).sum();
            let mut order: Vec<usize> = (0..share.len()).collect();
            order.sort_by(|&a, &b| share[b].1.cmp(&share[a].1).then(a.cmp(&b)));
            for i in order {
                if given >= want {
                    break;
                }
                if share[i].0 < unseen[i] {
                    share[i].0 += 1;
                    given += 1;
                }
            }
            for (k, _, i) in share {
                let c = &h.classes[i];
                let mut idx = c.first;
                let mut got = 0;
                while got < k && idx < c.end() {
                    if !h.seen.contains_key(&idx) && !h.dead.contains(&idx) {
                        out.push(idx);
                        got += 1;
                    }
                    idx += 1;
                }
            }
        }
    }
    for &k in &out {
        let s = h.seen.entry(k).or_insert(Seen {
            last: now,
            sheared_at: None,
        });
        s.last = now;
    }
    // Cap the seen map: forget the least recently seen (demotion by importance, PLAN.md §4.4).
    while h.seen.len() > seen_cap.max(n) {
        let Some((&k, _)) = h
            .seen
            .iter()
            .filter(|(k, _)| !out.contains(k))
            .min_by(|a, b| a.1.last.cmp(&b.1.last).then(a.0.cmp(b.0)))
        else {
            break;
        };
        h.seen.remove(&k);
    }
    out
}

/// Drops dead-set and seen entries of classes that no longer exist.
pub fn prune(h: &mut Herd) {
    let (classes, seen, dead) = (&h.classes, &mut h.seen, &mut h.dead);
    let live = |k: &u64| {
        let i = classes.partition_point(|c| c.end() <= *k);
        i < classes.len() && classes[i].contains(*k)
    };
    seen.retain(|k, _| live(k));
    dead.retain(live);
}
