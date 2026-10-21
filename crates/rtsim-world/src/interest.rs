// SPDX-License-Identifier: AGPL-3.0-or-later
//! Players and what they can see (PLAN.md §4.3, §6.1; SCALE.md §3; A15).
//!
//! The bridge reports where players are ([`World::set_player_pos`]); each player's interest area is
//! booked as a leg in the shared [`rtsim_travel::LegIndex`], next to the individual caravans' legs,
//! so a crossing is found by looking up the cells the area covers, never by a scan. Flows are found
//! through the static [`rtsim_travel::RoadIndex`]: the roads crossing the regions around a player
//! are the ones to sample travellers from. [`World::visible`] answers "which travellers are visible
//! near this player now"; [`World::encounters`] answers "which individual caravans will come within
//! sight during the window the player's area is booked for".

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::{ActorId, ActorKind};
use rtsim_settle::Pos;
use rtsim_travel::caravan::{CaravanKey, Traveller};
use rtsim_travel::legs::{LegId, LegIndex, Meeting, Owner, Path, Stamp};

use crate::world::{CaravanId, World};

/// The canonical regions a square of half-side `r` around a block covers, across seams (TP8).
fn regions_near(t: &rtsim_space::Topology, x: i64, z: i64, r: i64) -> Vec<(i64, i64)> {
    if t.is_flat() {
        let (rx0, rz0) = LegIndex::region_of(x - r, z - r);
        let (rx1, rz1) = LegIndex::region_of(x + r, z + r);
        let mut out = Vec::new();
        for rx in rx0..=rx1 {
            for rz in rz0..=rz1 {
                out.push((rx, rz));
            }
        }
        return out;
    }
    let mut out = Vec::new();
    for part in t.rects_near(x, z, r) {
        let (x0, z0, x1, z1) = part.grid_cells(rtsim_travel::legs::REGION_BLOCKS);
        for rx in x0..=x1 {
            for rz in z0..=z1 {
                out.push((rx, rz));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The top bit of a leg owner marks a player's interest area.
pub const PLAYER_OWNER: u64 = 1 << 63;

/// A caravan row's leg owner is its 48-bit handle; anything above is another module's (a
/// player's interest area, a dragon's flight: rtsim-ecology tags its legs with bit 62).
fn caravan_owner(owner: u64) -> Option<CaravanId> {
    (owner >> 48 == 0).then(|| CaravanId::from_bits(owner))
}

/// What the engine knows of a player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerState {
    /// The player's UUID as (most, least) significant halves; (0, hash) for a player registered
    /// by hash only.
    pub uuid: (u64, u64),
    /// Where the bridge last reported the player, and when.
    pub pos: Option<(Pos, Tick)>,
    /// The player's interest area in the leg index.
    pub(crate) interest: Option<LegId>,
}

/// Something a player can see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Visible {
    /// A traveller sampled from a flow (it has no row until something happens to it).
    Traveller {
        traveller: Traveller,
        x: i64,
        z: i64,
    },
    /// An individual caravan row.
    Caravan { id: CaravanId, x: i64, z: i64 },
    /// A touring band (P16).
    Band { band: usize, x: i64, z: i64 },
}

impl Visible {
    pub fn pos(&self) -> (i64, i64) {
        match self {
            Visible::Traveller { x, z, .. }
            | Visible::Caravan { x, z, .. }
            | Visible::Band { x, z, .. } => (*x, *z),
        }
    }
}

/// A 64-bit stand-in for a 128-bit UUID (the knowledge actors' external id).
pub fn uuid_hash(most: u64, least: u64) -> u64 {
    let k = rtsim_kernel::rng::StreamKey::new(
        most,
        rtsim_kernel::rng::SystemId::named("player.uuid"),
        least,
    );
    k.block(0, 0)[0]
}

impl World {
    /// Registers a player by UUID (idempotent) and returns its actor.
    pub fn player_uuid(&mut self, most: u64, least: u64) -> ActorId {
        let id = self
            .knowledge
            .actors_mut()
            .register(ActorKind::Player, uuid_hash(most, least));
        self.players.entry(id).or_insert(PlayerState {
            uuid: (most, least),
            pos: None,
            interest: None,
        });
        id
    }

    /// Where a player is (the bridge reports it as players move). Books the player's interest
    /// area, a circle of the configured view radius at `pos`, into the leg index for the current
    /// window, replacing the last one (cascade cancellation drops what was booked for it).
    pub fn set_player_pos(&mut self, player: ActorId, pos: Pos, at: Tick) {
        let window = self.cfg.trade.quantum.len();
        let path = Path {
            frame: pos.frame,
            x0: pos.x,
            z0: pos.z,
            x1: pos.x,
            z1: pos.z,
            t0: at,
            t1: Tick(at.0 + window),
            radius: self.cfg.view_blocks,
        };
        let uuid = (0, self.knowledge.actors().external(player).unwrap_or(0));
        let st = self.players.entry(player).or_insert(PlayerState {
            uuid,
            pos: None,
            interest: None,
        });
        st.pos = Some((pos, at));
        let leg = st.interest;
        let stamp = match leg.and_then(|l| self.legs.replan(l, path)) {
            Some(s) => s,
            None => self.legs.book(path, Owner(PLAYER_OWNER | player.to_bits())),
        };
        if let Some(st) = self.players.get_mut(&player) {
            st.interest = Some(stamp.leg);
        }
    }

    /// A player left (logged out, or changed dimension and will report again).
    pub fn forget_player_pos(&mut self, player: ActorId) {
        if let Some(st) = self.players.get_mut(&player) {
            st.pos = None;
            if let Some(l) = st.interest.take() {
                self.legs.cancel(l);
            }
        }
    }

    /// Where a sampled traveller is now, in blocks.
    pub fn traveller_pos(&self, t: &Traveller) -> (i64, i64) {
        let (a, b) = (self.nodes[t.from].pos, self.nodes[t.to].pos);
        // On the road the short way, canonical (topology choice TP21).
        self.road_point(a, b, i64::from(t.progress_milli))
    }

    /// Everything travelling within `radius` blocks of `pos` now: travellers sampled from the
    /// flows on the roads around it (found through the road index), individual caravans (found
    /// through the leg index) and touring bands. Sorted by distance, nearest first.
    pub fn visible(&mut self, pos: Pos, radius: u32) -> Vec<Visible> {
        let now = self.now;
        let (x, z, r) = (i64::from(pos.x), i64::from(pos.z), i64::from(radius));
        let r2 = (r * r) as f64;
        let mut out: Vec<(f64, Visible)> = Vec::new();
        let topo = self.legs.topologies().clone();
        let t = topo.of(pos.frame);
        // Through every seam (TP8): the plain squared distance on a flat frame.
        let near = |a: (i64, i64)| {
            let d = if t.is_flat() {
                ((a.0 - x).pow(2) + (a.1 - z).pow(2)) as f64
            } else {
                t.dist_sq((x, z), a) as f64
            };
            (d <= r2).then_some(d)
        };
        // Flows.
        for e in self.roads.near_on(t, pos.frame, x, z, r) {
            for dir in 0..2u8 {
                for t in self.travellers(e, dir) {
                    let p = self.traveller_pos(&t);
                    if let Some(d) = near(p) {
                        out.push((
                            d,
                            Visible::Traveller {
                                traveller: t,
                                x: p.0,
                                z: p.1,
                            },
                        ));
                    }
                }
            }
        }
        // Individual caravans: the legs booked in the regions around.
        let mut seen: Vec<CaravanId> = Vec::new();
        for (rx, rz) in regions_near(t, x, z, r) {
            {
                for s in self.legs.in_region(pos.frame, rx, rz, now) {
                    let Some(row) = self.legs.get(s.leg) else {
                        continue;
                    };
                    let Some(id) = caravan_owner(row.owner.0) else {
                        continue;
                    };
                    if seen.contains(&id) {
                        continue;
                    }
                    seen.push(id);
                    if let Some(c) = self.caravans.get(id) {
                        let p = self.caravan_place(c, now);
                        if let Some(d) = near((i64::from(p.x), i64::from(p.z))) {
                            out.push((
                                d,
                                Visible::Caravan {
                                    id,
                                    x: i64::from(p.x),
                                    z: i64::from(p.z),
                                },
                            ));
                        }
                    }
                }
            }
        }
        // Bands.
        for b in 0..self.bands.len() {
            let v = self.band_view(b, now);
            if v.frame == pos.frame
                && let Some(d) = near((v.x, v.z))
            {
                out.push((
                    d,
                    Visible::Band {
                        band: b,
                        x: v.x,
                        z: v.z,
                    },
                ));
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().map(|x| x.1).collect()
    }

    /// The visible things near a registered player's last reported position.
    pub fn visible_to(&mut self, player: ActorId) -> Vec<Visible> {
        let Some((pos, _)) = self.players.get(&player).and_then(|p| p.pos) else {
            return Vec::new();
        };
        let r = self.cfg.view_blocks;
        self.visible(pos, r)
    }

    /// Individual caravans whose legs come within sight of the player's booked interest area
    /// during its window, with when: the scheduled crossings of A15, found through the cells
    /// both are booked in. Stale bookings (replanned or finished legs) never show.
    pub fn encounters(&mut self, player: ActorId) -> Vec<(Tick, CaravanId)> {
        let Some(leg) = self.players.get(&player).and_then(|p| p.interest) else {
            return Vec::new();
        };
        let Some(me) = self.legs.stamp(leg) else {
            return Vec::new();
        };
        let Some(path) = self.legs.get(leg).map(|r| r.path) else {
            return Vec::new();
        };
        let r = i64::from(path.radius);
        let cells = regions_near(
            self.legs.topo(path.frame),
            i64::from(path.x0),
            i64::from(path.z0),
            r,
        );
        let mut stamps: Vec<Stamp> = Vec::new();
        let window = self.cfg.trade.quantum.len();
        let mut t = path.t0.0;
        while t <= path.t1.0 {
            for &(rx, rz) in &cells {
                stamps.extend(self.legs.in_region(path.frame, rx, rz, Tick(t)));
            }
            t += window;
        }
        stamps.sort();
        stamps.dedup();
        let mut out: Vec<(Tick, CaravanId)> = stamps
            .into_iter()
            .filter(|s| s.leg != me.leg)
            .filter_map(|s| {
                let id = caravan_owner(self.legs.get(s.leg)?.owner.0)?;
                let m = self.legs.meeting(me, s)?;
                Some((m.at, id))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The sampled traveller with this key, if it is on the road now.
    pub fn traveller(&self, key: CaravanKey) -> Option<Traveller> {
        self.travellers(key.edge, key.dir)
            .into_iter()
            .find(|t| t.key == key)
    }
}

// ---- one interest area per player, shared by every module (overnight choices U2 to U6) --------

/// A module whose travellers players can see (the dragons, the ships; next, rtsim-ecology's
/// herds): it owns the legs whose owner it claims, and hears of each player's **shared** interest
/// area whenever that area is booked or replanned ([`World::route_area`]).
///
/// One area per player serves every module: the player's position goes in once
/// ([`World::observe_area`]), each module's travellers are booked against the same area, and
/// each crossing found in the cells the area covers goes to the module that owns the traveller.
/// A module's own travellers look the area up the same way when they are booked (a dragon's
/// flight, a pirate's leg: `LegIndex::sharing` finds the one area, so one meeting).
///
/// **Leg owners** (the index treats them as opaque; this is the registry): bit 63 a player's
/// area ([`PLAYER_OWNER`] over the player's actor), bit 62 a dragon's flight (rtsim-ecology),
/// bit 61 a ship's voyage or pirate leg (rtsim-ships), bit 60 a herd's leg (rtsim-ecology's herds),
/// bit 59 a conflict party's march or wait (rtsim-conflict);
/// none of them, a caravan row's 48-bit handle (this crate's own: [`World::encounters`] answers for
/// those, and [`World::route_area`] hands them back).
pub trait Viewer {
    /// Whether a leg with this owner is one of this module's travellers.
    fn owns(&self, owner: Owner) -> bool;

    /// A player's area was booked or replanned (`area` is the new stamp). Book here what only an
    /// area makes worth booking (the ships book the voyages it could see), before the crossings
    /// are looked up.
    fn area_booked(&mut self, w: &mut World, player: ActorId, uuid: (u64, u64), area: Stamp);

    /// This module's legs that share a cell with the area, each with its first meeting (`a` the
    /// traveller's leg, `b` the area), in the order of the travellers' stamps. Also where a
    /// module sees what has no leg (a dragon at rest in its lair).
    fn crossings(&mut self, w: &mut World, player: ActorId, area: Stamp, found: &[Meeting]);

    /// The player left (or a session ended): its area is gone.
    fn player_left(&mut self, w: &mut World, player: ActorId) {
        let _ = (w, player);
    }
}

impl World {
    /// Where a player is, for the shared interest area of every module (one PLAYER record per
    /// hand-over goes in here once). The player is registered by UUID; its area, a circle of
    /// `radius` blocks, is kept while the player stays within a quarter of the radius and it still
    /// covers the next window, otherwise booked anew for two windows (the rule the dragons' and
    /// the ships' own areas follow, R11 and T17). Returns the player's actor and the area's new
    /// stamp when it was booked or replanned (route it with [`World::route_area`]), `None` when
    /// it was kept.
    ///
    /// The area is the world's own interest leg ([`PlayerState`], saved with the world), so
    /// [`World::encounters`] and [`World::visible_to`] read it too. Use this or
    /// [`World::set_player_pos`] for a player, not both: they replan the same leg.
    pub fn observe_area(
        &mut self,
        uuid: (u64, u64),
        pos: Pos,
        at: Tick,
        radius: u32,
    ) -> (ActorId, Option<Stamp>) {
        let player = self.player_uuid(uuid.0, uuid.1);
        let window = self.legs.window_ticks();
        let leg = self.players.get(&player).and_then(|p| p.interest);
        let keep = leg.and_then(|l| self.legs.get(l)).is_some_and(|row| {
            let moved = self.legs.topo(pos.frame).dist_sq(
                (i64::from(row.path.x0), i64::from(row.path.z0)),
                (i64::from(pos.x), i64::from(pos.z)),
            );
            let quarter = u64::from(radius / 4).pow(2);
            row.path.frame == pos.frame
                && row.path.radius == radius
                && moved <= quarter
                && row.path.t1.0 >= at.0 + window
        });
        if let Some(st) = self.players.get_mut(&player) {
            st.pos = Some((pos, at));
        }
        if keep {
            return (player, None);
        }
        let path = Path {
            frame: pos.frame,
            x0: pos.x,
            z0: pos.z,
            x1: pos.x,
            z1: pos.z,
            t0: at,
            t1: Tick(at.0 + 2 * window),
            radius,
        };
        let stamp = match leg.and_then(|l| self.legs.replan(l, path)) {
            Some(s) => s,
            None => self.legs.book(path, Owner(PLAYER_OWNER | player.to_bits())),
        };
        if let Some(st) = self.players.get_mut(&player) {
            st.interest = Some(stamp.leg);
        }
        (player, Some(stamp))
    }

    /// The player's shared interest area as it stands, if it has one.
    pub fn interest_area(&self, player: ActorId) -> Option<Stamp> {
        let leg = self.players.get(&player)?.interest?;
        self.legs.stamp(leg)
    }

    /// Routes a player's newly booked area to the modules (A15): each viewer books what the area
    /// needs first ([`Viewer::area_booked`]), then the legs sharing its cells are looked up once
    /// and each crossing goes to the viewer that owns the traveller ([`Viewer::crossings`], every
    /// viewer called, in the order given). Returns the crossings no viewer claimed (caravan rows,
    /// a module not passed in), in stamp order. A stale `area` routes nothing.
    pub fn route_area(
        &mut self,
        player: ActorId,
        area: Stamp,
        viewers: &mut [&mut dyn Viewer],
    ) -> Vec<Meeting> {
        if !self.legs.is_live(area) {
            return Vec::new();
        }
        let uuid = self.players.get(&player).map_or((0, 0), |p| p.uuid);
        for v in viewers.iter_mut() {
            v.area_booked(self, player, uuid, area);
        }
        let mut found: Vec<Vec<Meeting>> = vec![Vec::new(); viewers.len()];
        let mut rest = Vec::new();
        for other in self.legs.sharing(area) {
            let Some(owner) = self.legs.get(other.leg).map(|r| r.owner) else {
                continue;
            };
            if owner.0 & PLAYER_OWNER != 0 {
                continue; // another player's area
            }
            let Some(m) = self.legs.meeting(other, area) else {
                continue;
            };
            match viewers.iter().position(|v| v.owns(owner)) {
                Some(k) => found[k].push(m),
                None => rest.push(m),
            }
        }
        for (v, f) in viewers.iter_mut().zip(&found) {
            v.crossings(self, player, area, f);
        }
        rest
    }

    /// A player left (logged out, stale, a session ended): its shared area goes, with every
    /// meeting booked against it, and each viewer hears of it.
    pub fn leave_area(&mut self, player: ActorId, viewers: &mut [&mut dyn Viewer]) {
        self.forget_player_pos(player);
        for v in viewers.iter_mut() {
            v.player_left(self, player);
        }
    }
}
