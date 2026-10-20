// SPDX-License-Identifier: AGPL-3.0-or-later
//! Actors: anyone a report can name as who or whom (PLAN.md §5.6). Players, settlements, notables
//! and groups (a caravan, a band) share one handle space, so a report row needs only two 6-byte
//! handles and no kind tags (SCALE.md §9).

use std::collections::BTreeMap;

use rtsim_kernel::handle::{Arena, Handle};

/// What kind of thing an actor is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ActorKind {
    /// A Minecraft player. `external` is a hash of the UUID chosen by the bridge.
    Player = 1,
    /// A market node: a village or a player base. `external` is the settlement's stable key.
    Settlement = 2,
    /// A notable individual with its own row (PLAN.md §5.7). `external` is its seed.
    Notable = 3,
    /// A group that moves as one unit (C25): a caravan, a band. `external` is its stable id.
    Group = 4,
}

impl ActorKind {
    pub fn from_u8(v: u8) -> Option<ActorKind> {
        Some(match v {
            1 => ActorKind::Player,
            2 => ActorKind::Settlement,
            3 => ActorKind::Notable,
            4 => ActorKind::Group,
            _ => return None,
        })
    }
}

/// One registered actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actor {
    pub kind: ActorKind,
    /// The id the actor has outside this crate (see [`ActorKind`]). Stable across saves.
    pub external: u64,
}

/// A 6-byte actor handle. A removed actor's handle never validates again (the arena's
/// generation check), so a report naming a dead notable reads as "nobody known", not as whoever
/// reused the slot.
pub type ActorId = Handle<Actor>;

/// The actor registry: an arena plus an index from (kind, external id) to handle, so the bridge's
/// ids (a player's UUID, a settlement's key) map to the same handle every time.
#[derive(Default)]
pub struct Actors {
    arena: Arena<Actor>,
    index: BTreeMap<(ActorKind, u64), ActorId>,
}

impl Actors {
    pub fn new() -> Self {
        Self::default()
    }

    /// The actor for (kind, external), registering it the first time. Idempotent.
    pub fn register(&mut self, kind: ActorKind, external: u64) -> ActorId {
        if let Some(&id) = self.index.get(&(kind, external)) {
            return id;
        }
        let id = self.arena.insert(Actor { kind, external });
        self.index.insert((kind, external), id);
        id
    }

    pub fn find(&self, kind: ActorKind, external: u64) -> Option<ActorId> {
        self.index.get(&(kind, external)).copied()
    }

    pub fn get(&self, id: ActorId) -> Option<&Actor> {
        self.arena.get(id)
    }

    pub fn kind(&self, id: ActorId) -> Option<ActorKind> {
        self.arena.get(id).map(|a| a.kind)
    }

    pub fn external(&self, id: ActorId) -> Option<u64> {
        self.arena.get(id).map(|a| a.external)
    }

    pub fn is_player(&self, id: ActorId) -> bool {
        self.kind(id) == Some(ActorKind::Player)
    }

    /// Forgets an actor. Its handle stops validating; reports that name it keep the stale handle.
    pub fn remove(&mut self, id: ActorId) -> Option<Actor> {
        let a = self.arena.remove(id)?;
        self.index.remove(&(a.kind, a.external));
        Some(a)
    }

    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Live actors in arena order (deterministic for a given history).
    pub fn iter(&self) -> impl Iterator<Item = (ActorId, &Actor)> {
        self.arena.iter()
    }

    pub fn heap_bytes(&self) -> usize {
        // A BTreeMap node holds up to 11 entries; count each entry with a share of its node.
        let entry = std::mem::size_of::<((ActorKind, u64), ActorId)>() + 8;
        self.arena.heap_bytes() + self.index.len() * entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_is_idempotent_and_removal_invalidates() {
        let mut a = Actors::new();
        let p = a.register(ActorKind::Player, 42);
        assert_eq!(a.register(ActorKind::Player, 42), p);
        let s = a.register(ActorKind::Settlement, 42);
        assert_ne!(
            p, s,
            "the same external id under another kind is another actor"
        );
        assert_eq!(a.kind(s), Some(ActorKind::Settlement));
        assert_eq!(a.external(p), Some(42));
        assert_eq!(a.find(ActorKind::Player, 42), Some(p));
        a.remove(p);
        assert_eq!(a.kind(p), None);
        assert_eq!(a.find(ActorKind::Player, 42), None);
        let p2 = a.register(ActorKind::Player, 42);
        assert_ne!(p2, p, "a re-registered actor gets a fresh generation");
    }
}
