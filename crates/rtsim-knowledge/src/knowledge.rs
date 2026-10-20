// SPDX-License-Identifier: AGPL-3.0-or-later
//! The knowledge module's front: holders, learning, gossip by packet, expiry, notables and heirs.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{Encode, Writer};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::wheel::TimingWheel;

use crate::actor::{ActorId, ActorKind, Actors};
use crate::board::{BoardCursor, Noticeboard, Packet};
use crate::gossip::{VanillaGossip, vanilla_mapping, vanilla_sees_first_hand};
use crate::notable::Notable;
use crate::report::{
    FLAG_PLAYER, Inserted, Place, Report, ReportEntry, ReportId, ReportKind, ReportTable,
};
use crate::sentiment::Sentiments;
use crate::tunables::{SEVERITY_UNIT, Tunables};

/// What kind of holder an actor is: decides its caps and whether news writes vanilla gossip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum HolderKind {
    /// A vanilla village: news about players also goes to vanilla gossip (C20).
    VanillaVillage = 0,
    /// A ribbit village: no gossip of its own, so its feelings show in the sim's quotes.
    RibbitVillage = 1,
    /// A player base (C26): carries prices and news like any market node.
    PlayerBase = 2,
    /// A caravan or band that became an individual row: it carries what it saw home.
    Caravan = 3,
    Notable = 4,
}

impl HolderKind {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_u8(v: u8) -> Option<HolderKind> {
        Some(match v {
            0 => HolderKind::VanillaVillage,
            1 => HolderKind::RibbitVillage,
            2 => HolderKind::PlayerBase,
            3 => HolderKind::Caravan,
            4 => HolderKind::Notable,
            _ => return None,
        })
    }
}

pub(crate) struct Holder {
    pub(crate) kind: HolderKind,
    pub(crate) board: Noticeboard,
    pub(crate) sentiment: Sentiments,
}

/// Holders that saw an event: those in the same frame within `radius_blocks` of `place`
/// (horizontal distance), in candidate order. A settlement's awareness of events inside it is the
/// caller's: pass the settlement with the event's own place.
pub fn witnesses(
    place: Place,
    radius_blocks: u32,
    candidates: impl IntoIterator<Item = (ActorId, Place)>,
) -> Vec<ActorId> {
    let r2 = i64::from(radius_blocks) * i64::from(radius_blocks);
    candidates
        .into_iter()
        .filter(|(_, p)| place.dist2(p).is_some_and(|d| d <= r2))
        .map(|(a, _)| a)
        .collect()
}

pub(crate) type CounterKey = (ActorId, ActorId, ReportKind);

/// What expired reports leave behind: how many of each kind about each actor a holder knew
/// (SCALE.md §6). Capped; the smallest counts go first.
#[derive(Default)]
pub(crate) struct Counters {
    pub(crate) map: BTreeMap<CounterKey, u32>,
    pub(crate) by_count: BTreeSet<(u32, CounterKey)>,
    pub(crate) cap: u32,
}

impl Counters {
    fn bump(&mut self, key: CounterKey) {
        if let Some(c) = self.map.get_mut(&key) {
            self.by_count.remove(&(*c, key));
            *c = c.saturating_add(1);
            self.by_count.insert((*c, key));
            return;
        }
        if self.cap == 0 {
            return;
        }
        if self.map.len() >= self.cap as usize
            && let Some(least) = self.by_count.pop_first()
        {
            self.map.remove(&least.1);
        }
        self.map.insert(key, 1);
        self.by_count.insert((1, key));
    }

    fn get(&self, key: &CounterKey) -> u32 {
        self.map.get(key).copied().unwrap_or(0)
    }
}

/// Reports, holders, notables: everything the knowledge module keeps.
pub struct Knowledge {
    pub(crate) world_seed: u64,
    pub(crate) tunables: Tunables,
    pub(crate) actors: Actors,
    pub(crate) reports: ReportTable,
    pub(crate) holders: BTreeMap<ActorId, Holder>,
    pub(crate) notables: BTreeMap<ActorId, Notable>,
    pub(crate) deaths: TimingWheel<ActorId>,
    pub(crate) counters: Counters,
    pub(crate) vanilla: Vec<VanillaGossip>,
}

impl Knowledge {
    pub fn new(world_seed: u64) -> Self {
        Self::with_tunables(world_seed, Tunables::default())
    }

    pub fn with_tunables(world_seed: u64, tunables: Tunables) -> Self {
        Self {
            world_seed,
            reports: ReportTable::new(tunables.report_cap),
            counters: Counters {
                cap: tunables.counter_cap,
                ..Counters::default()
            },
            tunables,
            actors: Actors::new(),
            holders: BTreeMap::new(),
            notables: BTreeMap::new(),
            deaths: TimingWheel::new(Tick::ZERO),
            vanilla: Vec::new(),
        }
    }

    pub fn world_seed(&self) -> u64 {
        self.world_seed
    }

    pub fn tunables(&self) -> &Tunables {
        &self.tunables
    }

    pub fn actors(&self) -> &Actors {
        &self.actors
    }

    pub fn actors_mut(&mut self) -> &mut Actors {
        &mut self.actors
    }

    pub fn reports(&self) -> &ReportTable {
        &self.reports
    }

    pub fn report(&self, id: ReportId) -> Option<Report> {
        self.reports.report(id)
    }

    // ---- Holders ----

    /// Makes `actor` a holder of `kind`. Idempotent: an existing holder keeps its board and
    /// feelings.
    pub fn add_holder(&mut self, actor: ActorId, kind: HolderKind) {
        let t = &self.tunables;
        self.holders.entry(actor).or_insert_with(|| Holder {
            kind,
            board: Noticeboard::new(t.board_cap[kind.index()]),
            sentiment: Sentiments::new(actor, t.sentiment_cap[kind.index()], Tick::ZERO),
        });
    }

    /// Forgets a holder, with no inheritance (use [`Knowledge::die`] for a notable's death).
    pub fn remove_holder(&mut self, actor: ActorId) {
        if let Some(h) = self.holders.remove(&actor) {
            for id in h.board.iter() {
                self.reports.remove_knower(id, actor);
            }
        }
        self.notables.remove(&actor);
    }

    pub fn is_holder(&self, actor: ActorId) -> bool {
        self.holders.contains_key(&actor)
    }

    pub fn holder_kind(&self, actor: ActorId) -> Option<HolderKind> {
        self.holders.get(&actor).map(|h| h.kind)
    }

    pub fn holders(&self) -> impl Iterator<Item = (ActorId, HolderKind)> + '_ {
        self.holders.iter().map(|(&a, h)| (a, h.kind))
    }

    pub fn board(&self, holder: ActorId) -> Option<&Noticeboard> {
        self.holders.get(&holder).map(|h| &h.board)
    }

    pub fn knows(&self, holder: ActorId, id: ReportId) -> bool {
        self.reports.knows(id, holder)
    }

    // ---- Reports and learning ----

    /// Stores a fact; nobody knows it yet. Sets [`FLAG_PLAYER`] when a player is involved. Returns
    /// [`Handle::NULL`] when the table is full of reports that all matter more (never known by
    /// anyone, so every later call with it is a no-op).
    pub fn record(&mut self, mut report: Report) -> ReportId {
        if self.actors.is_player(report.who) || self.actors.is_player(report.whom) {
            report.flags |= FLAG_PLAYER;
        }
        let expires = report.at + self.tunables.lifetime(report.kind);
        match self.reports.insert(report.pack(), expires) {
            Inserted::Stored(id) => id,
            Inserted::Evicted(id, old, entry) => {
                self.fold(old, entry);
                id
            }
            Inserted::Dropped => Handle::NULL,
        }
    }

    /// First-hand: the holder saw it. Full delta toward `who`. False if not new to the holder.
    pub fn witness(&mut self, holder: ActorId, id: ReportId, at: Tick) -> bool {
        self.learn(holder, id, at, false)
    }

    /// Second-hand news a traveller carried: a weaker delta. Returns how many reports were new.
    pub fn deliver(&mut self, holder: ActorId, packet: &Packet, at: Tick) -> usize {
        packet
            .reports
            .iter()
            .filter(|&&id| self.learn(holder, id, at, true))
            .count()
    }

    fn learn(&mut self, holder: ActorId, id: ReportId, at: Tick, hearsay: bool) -> bool {
        if !self.holders.contains_key(&holder) {
            return false;
        }
        let Some(report) = self.reports.report(id) else {
            return false;
        };
        if !self.reports.add_knower(id, holder) {
            return false;
        }
        let h = self.holders.get_mut(&holder).expect("checked");
        // A board pushing off an old entry keeps that report's knower record, so it is never
        // relearned as news.
        h.board.push(id);
        let who_kind = self.actors.kind(report.who);
        if who_kind.is_some() && report.who != holder {
            let d = self.tunables.delta(report.kind, report.severity, hearsay);
            h.sentiment.add(
                at,
                report.who,
                d,
                who_kind == Some(ActorKind::Player),
                self.world_seed,
                &self.tunables,
            );
        }
        if h.kind == HolderKind::VanillaVillage
            && who_kind == Some(ActorKind::Player)
            && (hearsay || !vanilla_sees_first_hand(report.kind))
        {
            let (gossip, base) = vanilla_mapping(report.kind);
            let mut amount = base * i32::from(report.severity) / i32::from(SEVERITY_UNIT);
            if hearsay {
                amount = amount * i32::from(self.tunables.hearsay_percent) / 100;
            }
            if amount > 0 {
                self.vanilla.push(VanillaGossip {
                    village: holder,
                    player: report.who,
                    gossip,
                    amount,
                    report: id,
                    kind: report.kind,
                    at: report.at,
                    first_hand: !hearsay,
                });
            }
        }
        if let Some(n) = self.notables.get_mut(&holder) {
            let mut salience = u32::from(report.severity);
            if hearsay {
                salience = salience * u32::from(self.tunables.hearsay_percent) / 100;
            }
            n.remember(id, salience.clamp(1, 255) as u8);
        }
        true
    }

    /// The reports `holder` learned since `cursor`, up to the packet cap; the cursor advances past
    /// what was taken. Empty for a non-holder.
    pub fn packet_since(&self, holder: ActorId, cursor: &mut BoardCursor) -> Packet {
        match self.holders.get(&holder) {
            Some(h) => h.board.take_since(cursor, self.tunables.packet_cap),
            None => Packet::default(),
        }
    }

    // ---- Sentiment ----

    /// The holder's feeling toward `target` at `at`, projected; commits nothing.
    pub fn sentiment(&self, holder: ActorId, target: ActorId, at: Tick) -> i8 {
        self.holders.get(&holder).map_or(0, |h| {
            h.sentiment.get(target, at, self.world_seed, &self.tunables)
        })
    }

    /// Every non-zero feeling the holder has at `at`, projected, for the bridge's snapshot.
    pub fn sentiment_snapshot(&self, holder: ActorId, at: Tick) -> Vec<(ActorId, i8)> {
        self.holders.get(&holder).map_or(Vec::new(), |h| {
            h.sentiment.snapshot(at, self.world_seed, &self.tunables)
        })
    }

    /// The holder's sentiment table as stored (for saving and inspection).
    pub fn sentiments(&self, holder: ActorId) -> Option<&Sentiments> {
        self.holders.get(&holder).map(|h| &h.sentiment)
    }

    /// A direct change (a fair trade the module scores itself, a scripted test), saturating.
    pub fn adjust(&mut self, holder: ActorId, target: ActorId, delta: i16, at: Tick) {
        let is_player = self.actors.is_player(target);
        if let Some(h) = self.holders.get_mut(&holder) {
            h.sentiment.add(
                at,
                target,
                delta,
                is_player,
                self.world_seed,
                &self.tunables,
            );
        }
    }

    /// Gossip writes for the bridge since the last call (C20).
    pub fn take_vanilla_gossip(&mut self) -> Vec<VanillaGossip> {
        std::mem::take(&mut self.vanilla)
    }

    // ---- Expiry ----

    /// Drops every report whose lifetime ended at or before `at`, folding each into its knowers'
    /// counters. Costs per expiring report.
    pub fn expire_due(&mut self, at: Tick) -> usize {
        let mut n = 0;
        while let Some((id, entry)) = self.reports.pop_expired(at) {
            self.fold(id, entry);
            n += 1;
        }
        n
    }

    pub fn next_expiry(&self) -> Option<Tick> {
        self.reports.next_expiry()
    }

    /// Detail is gone; the count stays (SCALE.md §6). Boards and memories drop the handle.
    fn fold(&mut self, id: ReportId, entry: ReportEntry) {
        let Some(report) = entry.row.unpack() else {
            return;
        };
        for &k in entry.knowers() {
            if let Some(h) = self.holders.get_mut(&k) {
                h.board.remove(id);
                self.counters.bump((report.who, k, report.kind));
            }
            if let Some(n) = self.notables.get_mut(&k) {
                n.forget(id);
            }
        }
    }

    /// How many reports of `kind` about `actor` the holder `place` knew that have since expired.
    pub fn counter(&self, actor: ActorId, place: ActorId, kind: ReportKind) -> u32 {
        self.counters.get(&(actor, place, kind))
    }

    // ---- Notables ----

    /// A new notable: registers its actor (by seed) and its holder, and books its death of age.
    pub fn create_notable(
        &mut self,
        seed: u64,
        home: ActorId,
        heir: ActorId,
        born: Tick,
        lifespan_ticks: u64,
    ) -> ActorId {
        let id = self.actors.register(ActorKind::Notable, seed);
        self.add_holder(id, HolderKind::Notable);
        let dies = born + lifespan_ticks;
        self.notables.insert(
            id,
            Notable::new(seed, home, heir, born, dies, self.tunables.memory_slots),
        );
        self.deaths.schedule(dies.max(self.deaths.now()), id);
        id
    }

    pub fn notable(&self, id: ActorId) -> Option<&Notable> {
        self.notables.get(&id)
    }

    pub fn notables(&self) -> impl Iterator<Item = (ActorId, &Notable)> {
        self.notables.iter().map(|(&a, n)| (a, n))
    }

    pub fn set_heir(&mut self, notable: ActorId, heir: ActorId) {
        if let Some(n) = self.notables.get_mut(&notable) {
            n.heir = heir;
        }
    }

    pub fn remember(&mut self, notable: ActorId, report: ReportId, salience: u8) {
        if self.reports.get(report).is_none() {
            return;
        }
        if let Some(n) = self.notables.get_mut(&notable) {
            n.remember(report, salience);
        }
    }

    /// The next notable whose death of age is due at or before `at`. The caller then calls
    /// [`Knowledge::die`]; a notable that already died by violence is skipped.
    pub fn death_due(&mut self, at: Tick) -> Option<ActorId> {
        while let Some((_, id)) = self.deaths.pop_until(at) {
            if self.notables.get(&id).is_some_and(|n| n.dies <= at) {
                return Some(id);
            }
        }
        None
    }

    pub fn next_death(&self) -> Option<Tick> {
        self.deaths.next_due()
    }

    /// A notable dies, of age or by violence (C24). Its strongest memories and feelings pass to
    /// its heir, or to its home when the heir is gone; then it is removed, actor and all. Returns
    /// who inherited.
    pub fn die(&mut self, notable: ActorId, at: Tick) -> Option<ActorId> {
        let n = self.notables.remove(&notable)?;
        let holder = self.holders.remove(&notable);
        let heir = [n.heir, n.home]
            .into_iter()
            .find(|h| *h != notable && self.holders.contains_key(h));
        if let (Some(heir), Some(holder)) = (heir, holder.as_ref()) {
            let feelings = holder.sentiment.strongest(
                usize::from(self.tunables.heir_sentiments),
                at,
                self.world_seed,
                &self.tunables,
            );
            let hh = self.holders.get_mut(&heir).expect("checked");
            for (target, v, is_player) in feelings {
                if target != heir {
                    hh.sentiment
                        .merge(at, target, v, is_player, self.world_seed, &self.tunables);
                }
            }
            for m in n.strongest_memories(usize::from(self.tunables.heir_memories)) {
                let id = m.report();
                if self.reports.get(id).is_none() {
                    continue;
                }
                // The feeling came across above, so the memory goes on the board without a
                // second delta: the heir can tell it on, but does not count it twice.
                if self.reports.add_knower(id, heir) {
                    hh.board.push(id);
                }
                if let Some(hn) = self.notables.get_mut(&heir) {
                    hn.remember(id, m.salience());
                }
            }
        }
        if let Some(h) = holder {
            for id in h.board.iter() {
                self.reports.remove_knower(id, notable);
            }
        }
        self.actors.remove(notable);
        heir
    }

    // ---- Accounting ----

    pub fn heap_bytes(&self) -> usize {
        let holders: usize = self
            .holders
            .values()
            .map(|h| h.board.heap_bytes() + h.sentiment.heap_bytes() + 64)
            .sum();
        let notables = self.notables.len() * (std::mem::size_of::<Notable>() + 16 * 8 + 16);
        self.actors.heap_bytes()
            + self.reports.heap_bytes()
            + holders
            + notables
            + self.deaths.heap_bytes()
            + self.counters.map.len() * 2 * (std::mem::size_of::<(u32, CounterKey)>() + 8)
            + self.vanilla.capacity() * std::mem::size_of::<VanillaGossip>()
    }
}

/// Deterministic encoding of everything, for state hashes (the determinism gate) and inspection.
/// Wheels are not written: their contents are the expiry and death times written here.
impl Encode for Knowledge {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.world_seed);
        w.var_u64(self.actors.len() as u64);
        for (id, a) in self.actors.iter() {
            w.u64(id.to_bits());
            w.u8(a.kind as u8);
            w.u64(a.external);
        }
        self.reports.encode(w);
        w.var_u64(self.holders.len() as u64);
        for (id, h) in &self.holders {
            w.u64(id.to_bits());
            w.u8(h.kind as u8);
            h.board.encode(w);
            h.sentiment.encode(w);
        }
        w.var_u64(self.notables.len() as u64);
        for (id, n) in &self.notables {
            w.u64(id.to_bits());
            n.encode(w);
        }
        w.var_u64(self.counters.map.len() as u64);
        for (&(a, p, k), &c) in &self.counters.map {
            w.u64(a.to_bits());
            w.u64(p.to_bits());
            w.u16(k as u16);
            w.u32(c);
        }
        w.var_u64(self.vanilla.len() as u64);
        for g in &self.vanilla {
            w.u64(g.village.to_bits());
            w.u64(g.player.to_bits());
            w.u8(g.gossip as u8);
            w.var_i64(i64::from(g.amount));
            w.u64(g.report.to_bits());
        }
    }
}
