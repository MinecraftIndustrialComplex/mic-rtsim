// SPDX-License-Identifier: AGPL-3.0-or-later
//! The works purse: surplus money buys real things (SINK.md; Ben's MS-1 to MS-11).
//!
//! - **The purse** (MS-1): what E43 sinks on a currency line above twice its target leaves
//!   circulation into the market's works purse. The purse is a closed form of the line's books:
//!   `book.sunk − spent − tribute`, per currency. Nothing in the market reads it, so the market's
//!   trajectory, its news and its breakpoints are exactly what they were (round 2's quiet worlds
//!   hold by construction).
//! - **The catalogue** (MS-2): works are data ([`WorkDef`]), one kind per JSON file in game
//!   (`data/<ns>/mic_rtsim/work/<id>.json`), each with its variants (templates or a body) and the
//!   items each variant is made of. A village buys only its own culture's works, and a work that
//!   names a village type (its `style`) only in villages of that type.
//! - **Order** (MS-6): need first. A village paying tribute keeps its purse for the dragon; a
//!   village threatened within the last season buys a golem while under the golem cap; then
//!   lamps and decor, cheapest first, ties by the village's seed.
//! - **Booking** (SINK.md §4.4): when the next work's price is known, the engine books one wheel
//!   entry ([`crate::world::Ev::Works`]) at the boundary the purse reaches it, from the currency
//!   lines' plans (`Line::steps_to_sink`, one division on the common one-coin path). A market
//!   event rebooks only when the due time moves (lazy cancellation by stamp). A works event
//!   touches only the purse and the pointer: it never runs a market event, so it wakes no market
//!   and sends no news.
//! - **Buying** (MS-3): the price leaves the purse as `works` (a burn: the builders' wages and
//!   materials), and the engine sends PLACE_HOUSE with the work's template, or SPAWN_RESIDENT
//!   with its body (a golem), under an identity op id (the village and its nth work).
//! - **No plot** (MS-11): a work the bridge cannot place (NOT_FREE, FAILED) marks its kind full
//!   for the village; the coin stays spent.
//! - **Tribute** (MS-5, MS-10): `rtsim-ecology` asks for a share of the purse each season, paid
//!   in the coin the purse holds most of ([`World::pay_tribute`]), booked as `tribute`.
//! - **Foreign coin** (MS-10): counts toward a work at the market's exchange anchor (mint parity,
//!   C13).
//!
//! Conservation per currency (SINK.md §4.5): treasuries + in transit + caravan cargo + purses =
//! genesis + minted + player in − player out − destroyed − works − tribute (`check.rs`).

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_settle::{Culture, NodeKind};
use rtsim_trade::{Amount, GoodId, Line, MILLI, Market, PRICE_ONE};

use crate::ribbits::identity_op;
use crate::world::{Command, Ev, World};

/// The works' RNG system: variants, ties, rotations, op identities.
pub const SYS_WORKS: SystemId = SystemId::named("world.works");

/// What a work is for: the purchase order (MS-6) treats golems as need.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkGroup {
    Lamp = 0,
    Decor = 1,
    Golem = 2,
}

impl WorkGroup {
    pub fn from_u32(v: u32) -> Option<WorkGroup> {
        match v {
            0 => Some(WorkGroup::Lamp),
            1 => Some(WorkGroup::Decor),
            2 => Some(WorkGroup::Golem),
            _ => None,
        }
    }
}

/// How many of a work kind a village may buy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkCap {
    /// One per `n` residents the census counts (houses are not observed; overnight choice MSC3).
    PerResidents(u32),
    /// At most `n`.
    Fixed(u32),
}

/// What the bridge does for a bought work.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WorkPlace {
    /// PLACE_HOUSE with this template and processor list ("" for none), SEARCH from the meeting
    /// point.
    Template {
        template: String,
        processors: String,
    },
    /// SPAWN_RESIDENT of this entity type at the meeting point (a golem).
    Spawn { body: String },
}

/// One variant of a work kind: what it places and what it is made of.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorkVariant {
    pub place: WorkPlace,
    /// Items of the world's goods it is made of, whole items: priced at the market's anchors.
    pub goods: Vec<(GoodId, i64)>,
    /// Items that are none of the world's goods: priced as material labour
    /// (`WorldConfig::works_material_wd_milli`).
    pub other_items: i64,
    /// Blocks (or bodies) placed: priced as placement labour (`works_place_wd_milli`).
    pub blocks: i64,
}

/// A work kind of the catalogue (MS-2): a lamp of one village type, a decor set, a golem.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorkDef {
    /// Its resource id (`mic_rtsim:plains_lamp`): the kind's identity for caps and fullness.
    pub id: String,
    pub group: WorkGroup,
    pub culture: Culture,
    /// The village type it belongs to ([`style_of`] of the village's structure), 0 for every
    /// village of the culture.
    pub style: u64,
    pub cap: WorkCap,
    pub variants: Vec<WorkVariant>,
}

impl WorkDef {
    /// The kind's key in a village's counters.
    pub fn key(&self) -> u64 {
        fnv1a64(self.id.as_bytes())
    }
}

/// A village type's key, from its structure's resource id (`minecraft:village_plains`).
pub fn style_of(structure: &str) -> u64 {
    if structure.is_empty() {
        0
    } else {
        fnv1a64(structure.as_bytes())
    }
}

/// The next work a village wants, as last chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Next {
    /// Not chosen since something changed what it may buy.
    #[default]
    Unknown,
    /// Nothing to buy: every kind capped or full, or the purse is the dragon's (tribute).
    Nothing,
    /// Catalogue index, variant, price in home-coin thousandths.
    Work {
        def: u32,
        variant: u16,
        price: Amount,
    },
}

/// A village's works: its purse's outflows, its counters and its booking (SINK.md §7: a purse
/// per currency, a pointer and a stamp).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorksState {
    /// Its village type ([`style_of`]).
    pub style: u64,
    /// Coin spent on works, per currency (thousandths), sorted by good.
    pub spent: Vec<(GoodId, Amount)>,
    /// Coin paid as tribute, per currency (thousandths), sorted by good.
    pub tribute: Vec<(GoodId, Amount)>,
    /// Coin a test put straight into the purse ([`World::grant_purse`]), ledgered as minted.
    pub granted: Vec<(GoodId, Amount)>,
    /// Works bought, per kind key, sorted.
    pub bought: Vec<(u64, u32)>,
    /// Kinds the bridge found no plot for (MS-11), sorted.
    pub full: Vec<u64>,
    /// Works bought in all: the next one's identity.
    pub nth: u32,
    /// Bumped whenever the booking changes: a works event with an older stamp is stale.
    pub stamp: u32,
    /// When the booked works event runs.
    pub due: Option<Tick>,
    pub next: Next,
    /// The last raid on it, or the last time its tribute lapsed: golems come first for a season.
    pub threat_at: Option<Tick>,
    /// A dragon's lair takes its tribute: the purse is kept for it (MS-6, need first).
    pub tributary: bool,
    /// Golems the census counts (MS-8: dragons fear them).
    pub golems: u32,
    /// Bought works the bridge has not answered yet: a village buys its next work only after the
    /// last one was answered (choice MSC13), so a village with no plot loses one work's price,
    /// not its cap's, and an unvisited one waits on one command.
    pub in_flight: u32,
}

fn get(v: &[(GoodId, Amount)], g: GoodId) -> Amount {
    v.binary_search_by_key(&g, |e| e.0).map_or(0, |i| v[i].1)
}

fn add(v: &mut Vec<(GoodId, Amount)>, g: GoodId, a: Amount) {
    match v.binary_search_by_key(&g, |e| e.0) {
        Ok(i) => v[i].1 += a,
        Err(i) => v.insert(i, (g, a)),
    }
}

impl WorksState {
    pub fn spent(&self, g: GoodId) -> Amount {
        get(&self.spent, g)
    }

    pub fn tribute(&self, g: GoodId) -> Amount {
        get(&self.tribute, g)
    }

    pub fn granted(&self, g: GoodId) -> Amount {
        get(&self.granted, g)
    }

    pub fn bought(&self, kind: u64) -> u32 {
        self.bought
            .binary_search_by_key(&kind, |e| e.0)
            .map_or(0, |i| self.bought[i].1)
    }

    fn bump(&mut self, kind: u64) {
        match self.bought.binary_search_by_key(&kind, |e| e.0) {
            Ok(i) => self.bought[i].1 += 1,
            Err(i) => self.bought.insert(i, (kind, 1)),
        }
    }

    pub fn is_full(&self, kind: u64) -> bool {
        self.full.binary_search(&kind).is_ok()
    }

    fn mark_full(&mut self, kind: u64) {
        if let Err(i) = self.full.binary_search(&kind) {
            self.full.insert(i, kind);
        }
    }

    /// The purse in currency `g` given the line's books: sunk, less what left it.
    pub fn purse(&self, g: GoodId, line: &Line) -> Amount {
        line.book.sunk + self.granted(g) - self.spent(g) - self.tribute(g)
    }
}

/// A currency line at a market, with what one of its coins is worth in home coin (millionths).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CoinLine {
    pub good: GoodId,
    pub line: Line,
    pub par: i64,
}

/// The currency lines of a market that may sink, with their parity (MS-10): the home coin at
/// one, a foreign coin at the market's exchange anchor (mint parity, C13), or by the cultures'
/// mint costs where the market keeps no exchange line.
pub(crate) fn coin_lines(m: &Market, mint_par: impl Fn(GoodId) -> i64) -> Vec<CoinLine> {
    let Some(home) = m.currency else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (gi, gs) in m.goods.iter().enumerate() {
        if !gs.currency {
            continue;
        }
        let g = GoodId(gi as u16);
        let par = if g == home {
            PRICE_ONE
        } else if m.anchor(g) > 0 {
            m.anchor(g)
        } else {
            mint_par(g)
        };
        out.push(CoinLine {
            good: g,
            line: m.lines[gi],
            par,
        });
    }
    out
}

/// A foreign coin's mint parity in home coin (millionths): `L_mint(foreign) / L_mint(home)`
/// (C13), for a market that keeps no exchange line for it.
pub(crate) fn mint_par(
    goods: &rtsim_trade::Goods,
    mint_cost: [f64; 2],
    home: Culture,
    g: GoodId,
) -> i64 {
    let Some(c) = goods.currency_culture(g) else {
        return 0;
    };
    let (lf, lh) = (mint_cost[c.index()], mint_cost[home.index()]);
    if lf.is_finite() && lh.is_finite() && lh > 0.0 {
        (lf / lh * PRICE_ONE as f64).round() as i64
    } else {
        0
    }
}

/// Home-coin thousandths `a` thousandths of a coin at parity `par` are worth (rounded down).
fn value(a: Amount, par: i64) -> Amount {
    (i128::from(a) * i128::from(par) / i128::from(PRICE_ONE)) as Amount
}

impl World {
    /// The works catalogue (MS-2), replacing the last one: every village chooses again.
    pub fn set_works(&mut self, mut defs: Vec<WorkDef>, at: Tick) {
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        defs.dedup_by(|a, b| a.id == b.id);
        if defs == self.works_catalog {
            return;
        }
        self.works_catalog = defs;
        let nodes: Vec<usize> = self.works.keys().copied().collect();
        for i in nodes {
            self.works_touch(i, at);
        }
        for i in 0..self.nodes.len() {
            if !self.works.contains_key(&i) && self.works_village(i) {
                self.works_touch(i, at);
            }
        }
    }

    /// A village's type (its structure's [`style_of`]): which styled works it buys.
    pub fn set_work_style(&mut self, node: usize, style: u64, at: Tick) {
        // Kept for a village even before a catalogue arrives (a later session may bring one).
        if !self.is_village(node) {
            return;
        }
        let s = self.works.entry(node).or_default();
        if s.style != style {
            s.style = style;
            self.works_touch(node, at);
        }
    }

    /// The golems a village's census counts (MS-8).
    pub fn set_golems(&mut self, node: usize, golems: u32) {
        if !self.is_village(node) {
            return;
        }
        self.works.entry(node).or_default().golems = golems;
    }

    pub fn golems(&self, node: usize) -> u32 {
        self.works.get(&node).map_or(0, |s| s.golems)
    }

    /// A village's works state, if it has one.
    pub fn works_of(&self, node: usize) -> Option<&WorksState> {
        self.works.get(&node)
    }

    /// Whether `op` is a bought work the bridge has not answered yet.
    pub fn is_work_op(&self, op: u64) -> bool {
        self.works_pending.contains_key(&op)
    }

    /// The village's purse per currency at `at` (thousandths), a projection.
    pub fn works_purse(&self, node: usize, at: Tick) -> Vec<(GoodId, Amount)> {
        if node >= self.nodes.len() {
            return Vec::new();
        }
        let m = self.market_at(node, at);
        let s = self.works.get(&node);
        coin_lines(&m, |_| 0)
            .into_iter()
            .map(|c| {
                let p = s.map_or(c.line.book.sunk, |s| s.purse(c.good, &c.line));
                (c.good, p)
            })
            .collect()
    }

    /// Coin spent on works and paid as tribute in the whole world, per currency.
    #[allow(clippy::type_complexity)]
    pub fn works_totals(&self) -> (Vec<(GoodId, i128)>, Vec<(GoodId, i128)>) {
        let mut spent: std::collections::BTreeMap<GoodId, i128> = Default::default();
        let mut tribute: std::collections::BTreeMap<GoodId, i128> = Default::default();
        for s in self.works.values() {
            for &(g, a) in &s.spent {
                *spent.entry(g).or_default() += i128::from(a);
            }
            for &(g, a) in &s.tribute {
                *tribute.entry(g).or_default() += i128::from(a);
            }
        }
        (spent.into_iter().collect(), tribute.into_iter().collect())
    }

    fn is_village(&self, i: usize) -> bool {
        matches!(
            self.nodes.get(i).map(|n| n.kind),
            Some(NodeKind::Village(_))
        )
    }

    /// Whether node `i` keeps works: a village with a treasury, and a catalogue to buy from.
    pub(crate) fn works_village(&self, i: usize) -> bool {
        self.cfg.works
            && !self.works_catalog.is_empty()
            && matches!(
                self.nodes.get(i).map(|n| n.kind),
                Some(NodeKind::Village(_))
            )
    }

    /// Whether a market event at `i` must hand its currency lines to [`World::works_rebook`]:
    /// not for a village with nothing to buy (its purse is a closed form nobody reads).
    pub(crate) fn works_wanted(&self, i: usize) -> bool {
        self.works_village(i) && self.works.get(&i).is_none_or(|s| s.next != Next::Nothing)
    }

    pub(crate) fn coin_lines_of(&self, i: usize, m: &Market) -> Vec<CoinLine> {
        match self.nodes[i].kind.culture() {
            Some(c) => coin_lines(m, |g| mint_par(&self.goods, self.mint_cost, c, g)),
            None => Vec::new(),
        }
    }

    /// The price of a work variant at market `m` of culture `c`, in home-coin thousandths
    /// (SINK.md §4.3): its goods at the market's anchors, plus material labour for the items that
    /// are none of the world's goods and placement labour per block, at the culture's mint cost.
    pub fn work_price(&self, m: &Market, c: Culture, v: &WorkVariant) -> Amount {
        let mut milli: i128 = 0;
        let mut labour_milli_wd: i128 = i128::from(v.other_items.max(0))
            * i128::from(self.cfg.works_material_wd_milli)
            + i128::from(v.blocks.max(0)) * i128::from(self.cfg.works_place_wd_milli);
        for &(g, n) in &v.goods {
            let p = if g.index() < m.goods.len() {
                m.anchor(g)
            } else {
                0
            };
            if p > 0 {
                milli += i128::from(p) * i128::from(n.max(0)) / i128::from(PRICE_ONE / MILLI);
            } else {
                // A good the market has no anchor for is material like any other item.
                labour_milli_wd +=
                    i128::from(n.max(0)) * i128::from(self.cfg.works_material_wd_milli);
            }
        }
        let l_mint = self.mint_cost[c.index()];
        if l_mint.is_finite() && l_mint > 0.0 {
            milli += (labour_milli_wd as f64 / l_mint).round() as i128;
        }
        milli.clamp(1, i128::from(i64::MAX / 4)) as Amount
    }

    /// The next work village `i` wants (MS-6), from the catalogue and its counters.
    fn works_choose(&self, i: usize, m: &Market, at: Tick) -> Next {
        let Some(NodeKind::Village(culture)) = self.nodes.get(i).map(|n| n.kind) else {
            return Next::Nothing;
        };
        let s = self.works.get(&i).cloned().unwrap_or_default();
        if s.tributary || s.in_flight > 0 {
            return Next::Nothing;
        }
        let threat = TICKS_PER_DAY * self.cfg.works_threat_days;
        let threatened = s
            .threat_at
            .is_some_and(|t| at.0 < t.0.saturating_add(threat));
        let pop = self.nodes[i].local.pop;
        let key = self.nodes[i].key;
        let mut best: Option<(u8, Amount, u64, Next)> = None;
        for (di, d) in self.works_catalog.iter().enumerate() {
            if d.culture != culture || (d.style != 0 && d.style != s.style) || d.variants.is_empty()
            {
                continue;
            }
            let kind = d.key();
            if s.is_full(kind) {
                continue;
            }
            let bought = s.bought(kind);
            let cap = match d.cap {
                WorkCap::PerResidents(n) => pop / n.max(1),
                WorkCap::Fixed(n) => n,
            };
            if bought >= cap {
                continue;
            }
            let prio = match d.group {
                WorkGroup::Golem if threatened => 0,
                WorkGroup::Golem => continue,
                WorkGroup::Lamp | WorkGroup::Decor => 1,
            };
            let roll = StreamKey::new(self.cfg.seed, SYS_WORKS, key).block(kind, u64::from(bought));
            let variant = (roll[0] % d.variants.len() as u64) as u16;
            let price = self.work_price(m, culture, &d.variants[variant as usize]);
            let tie = roll[1];
            let cand = (prio, price, tie);
            if best.is_none_or(|b| cand < (b.0, b.1, b.2)) {
                best = Some((
                    prio,
                    price,
                    tie,
                    Next::Work {
                        def: di as u32,
                        variant,
                        price,
                    },
                ));
            }
        }
        best.map_or(Next::Nothing, |b| b.3)
    }

    /// Something changed what village `i` may buy or what it costs (a catalogue, its type, a raid,
    /// a lapse, a population, an exchange line): it chooses again and rebooks, from its market
    /// now.
    pub(crate) fn works_touch(&mut self, i: usize, at: Tick) {
        if !self.works_village(i) {
            return;
        }
        let m = self.market_at(i, at);
        let next = self.works_choose(i, &m, at);
        self.works.entry(i).or_default().next = next;
        let lines = self.coin_lines_of(i, &m);
        self.works_book(i, at, &lines);
    }

    /// After a market event at `i` (its rates may have changed): rebook the next work from the
    /// currency lines as they now stand.
    pub(crate) fn works_rebook(&mut self, i: usize, at: Tick, lines: &[CoinLine]) {
        let unknown = self.works.get(&i).is_none_or(|s| s.next == Next::Unknown);
        if unknown {
            self.works_touch(i, at);
        } else {
            self.works_book(i, at, lines);
        }
    }

    /// Books (or cancels) the works event at the boundary the purse reaches the next work's
    /// price; leaves an entry that is still right where it is.
    fn works_book(&mut self, i: usize, at: Tick, lines: &[CoinLine]) {
        let q = self.cfg.trade.quantum;
        let s = self.works.entry(i).or_default();
        let due = match s.next {
            Next::Work { price, .. } => {
                let have: Amount = lines
                    .iter()
                    .map(|c| value(s.purse(c.good, &c.line).max(0), c.par))
                    .sum();
                if have >= price {
                    Some(at)
                } else {
                    steps_to_value(lines, price - have).map(|k| q.at(q.index(at).saturating_add(k)))
                }
            }
            Next::Unknown | Next::Nothing => None,
        };
        if s.due == due {
            return;
        }
        s.stamp = s.stamp.wrapping_add(1);
        s.due = due;
        if let Some(t) = due {
            let stamp = s.stamp;
            self.schedule(t, Ev::Works { node: i, stamp });
        }
    }

    /// The booked works event: buy the next work if the purse holds its price.
    pub(crate) fn works_due(&mut self, i: usize, stamp: u32, at: Tick) {
        let Some(s) = self.works.get(&i) else {
            return;
        };
        if s.stamp != stamp || s.due.is_none() {
            return;
        }
        self.stats.works_events += 1;
        self.works.get_mut(&i).expect("checked").due = None;
        let m = self.market_at(i, at);
        let mut lines = self.coin_lines_of(i, &m);
        if self.works[&i].next == Next::Unknown {
            let next = self.works_choose(i, &m, at);
            self.works.get_mut(&i).expect("checked").next = next;
        }
        let Next::Work {
            def,
            variant,
            price,
        } = self.works[&i].next
        else {
            return;
        };
        let s = self.works.get_mut(&i).expect("checked");
        let have: Amount = lines
            .iter()
            .map(|c| value(s.purse(c.good, &c.line).max(0), c.par))
            .sum();
        if have < price {
            // A change the booking did not see (a projection's rounding): book again.
            self.works_book(i, at, &lines);
            return;
        }
        // Pay: the home coin first, then foreign coin in good order at parity (MS-10).
        lines.sort_by_key(|c| (c.par != PRICE_ONE, c.good));
        let mut left = price;
        for c in &lines {
            if left <= 0 {
                break;
            }
            let purse = s.purse(c.good, &c.line).max(0);
            if purse <= 0 || c.par <= 0 {
                continue;
            }
            // Thousandths of this coin that cover `left` home thousandths, rounded up.
            let need = ((i128::from(left) * i128::from(PRICE_ONE) + i128::from(c.par) - 1)
                / i128::from(c.par)) as Amount;
            let take = need.min(purse);
            add(&mut s.spent, c.good, take);
            left -= value(take, c.par).min(left);
            if take == need {
                left = 0;
            }
        }
        let d = self.works_catalog[def as usize].clone();
        let kind = d.key();
        s.bump(kind);
        s.nth += 1;
        s.in_flight += 1;
        let nth = s.nth;
        s.next = Next::Unknown;
        let node = &self.nodes[i];
        let (village, pos) = (node.key, node.pos);
        // The work's identity: this village's nth work (a crash re-deciding it gets the same id).
        let seed = StreamKey::new(self.cfg.seed, SYS_WORKS, village).block(u64::from(nth), 1)[0];
        let op = identity_op(seed, 0);
        match &d.variants[variant as usize].place {
            WorkPlace::Template {
                template,
                processors,
            } => self.outbox.push(Command::PlaceWork(PlaceWork {
                op,
                village,
                frame: pos.frame,
                template: template.clone(),
                processors: processors.clone(),
                x: pos.x,
                z: pos.z,
                radius: self.cfg.works_radius,
                rotation: ((seed >> 32) % 4) as u8,
                seed,
            })),
            WorkPlace::Spawn { body } => self.outbox.push(Command::SpawnWork(SpawnWork {
                op,
                village,
                body: body.clone(),
            })),
        }
        self.works_pending.insert(op, (i, kind));
        self.stats.works += 1;
        let next = self.works_choose(i, &m, at);
        self.works.get_mut(&i).expect("checked").next = next;
        self.works_book(i, at, &lines);
    }

    /// The bridge answered a bought work: placed (APPLIED, DUPLICATE), or not (NOT_FREE, FAILED:
    /// the kind is full for the village and the coin stays spent, MS-11).
    pub fn work_result(&mut self, op: u64, placed: bool, at: Tick) {
        let Some((i, kind)) = self.works_pending.remove(&op) else {
            return;
        };
        if let Some(s) = self.works.get_mut(&i) {
            s.in_flight = s.in_flight.saturating_sub(1);
            if !placed {
                s.mark_full(kind);
            }
        }
        if !placed {
            self.stats.works_refused += 1;
        }
        // The next work, now that this one is answered (MSC13).
        self.works_touch(i, at.max(self.now));
    }

    /// A dragon raided village `node` (`rtsim-ecology`): golems come first for a season, and
    /// with `tribute` its purse is kept for the raider's lair from now on (MS-5, MS-6).
    pub fn village_raided(&mut self, node: usize, tribute: bool, at: Tick) {
        if !self.works_village(node) {
            return;
        }
        let s = self.works.entry(node).or_default();
        s.threat_at = Some(at);
        s.tributary |= tribute;
        self.works_touch(node, at);
    }

    /// A season's tribute from village `node` (MS-5): `share_milli` thousandths of its purse, in
    /// the currency it holds most of (MS-10), in whole coins (at least one). `None` when the purse
    /// holds less than a coin: the tribute lapses ([`World::tribute_lapsed`]).
    pub fn pay_tribute(
        &mut self,
        node: usize,
        share_milli: i64,
        at: Tick,
    ) -> Option<(GoodId, Amount)> {
        if node >= self.nodes.len() || !self.works_village(node) {
            return None;
        }
        let m = self.market_at(node, at);
        let lines = self.coin_lines_of(node, &m);
        let s = self.works.entry(node).or_default();
        let (g, purse) = lines
            .iter()
            .map(|c| (c.good, s.purse(c.good, &c.line)))
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?;
        if purse < MILLI {
            return None;
        }
        let share = i128::from(purse) * i128::from(share_milli.clamp(0, 1000)) / 1000;
        let coins = (share / i128::from(MILLI)).max(1) as Amount;
        let amount = (coins * MILLI).min(purse / MILLI * MILLI);
        add(&mut s.tribute, g, amount);
        self.stats.tributes += 1;
        self.works_book(node, at, &lines);
        Some((g, amount))
    }

    /// TEST: `amount` thousandths of village `node`'s home coin straight into its works purse,
    /// booked as minted (GameTests give a village a purse without running it to its ceiling).
    /// Returns false for a node with no works purse.
    pub fn grant_purse(&mut self, node: usize, amount: Amount, at: Tick) -> bool {
        if amount <= 0 || !self.works_village(node) {
            return false;
        }
        let Some(g) = self.nodes[node]
            .kind
            .culture()
            .and_then(|c| self.goods.currency(c))
        else {
            return false;
        };
        add(&mut self.works.entry(node).or_default().granted, g, amount);
        self.works_touch(node, at);
        true
    }

    /// Village `node` pays no tribute any more (its purse ran dry, or the lair fell vacant):
    /// it buys again, golems first for a season (SINK.md §6 walkthrough B).
    pub fn tribute_lapsed(&mut self, node: usize, at: Tick) {
        let Some(s) = self.works.get_mut(&node) else {
            return;
        };
        if !s.tributary {
            return;
        }
        s.tributary = false;
        s.threat_at = Some(at);
        self.works_touch(node, at);
    }
}

/// Boundaries past which a work is never booked: some 140,000 years of quanta, so a booking's
/// tick never overflows and a search never runs long.
const FAR: u64 = 1 << 36;

/// The fewest boundaries after which the currency lines' sinks add `want` home thousandths:
/// one division when one line sinks (the common case), else a search over their plans.
fn steps_to_value(lines: &[CoinLine], want: Amount) -> Option<u64> {
    let sinking: Vec<&CoinLine> = lines
        .iter()
        .filter(|c| c.par > 0 && c.line.will_sink())
        .collect();
    match sinking.as_slice() {
        [] => None,
        [c] => {
            // Coin thousandths worth `want`: value(a) = floor(a·par/1e6) ≥ want.
            let a = (i128::from(want) * i128::from(PRICE_ONE) + i128::from(c.par) - 1)
                / i128::from(c.par);
            c.line
                .steps_to_sink(Amount::try_from(a).ok()?)
                .filter(|&k| k <= FAR)
        }
        _ => {
            let worth = |k: u64| -> Amount {
                sinking
                    .iter()
                    .map(|c| value(c.line.sunk_over(k), c.par))
                    .sum()
            };
            let mut hi: u64 = 1;
            while worth(hi) < want {
                if hi >= FAR {
                    return None;
                }
                hi *= 2;
            }
            let mut lo = hi / 2;
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if worth(mid) >= want {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            Some(if worth(lo) >= want { lo } else { hi })
        }
    }
}

/// Place a bought work (PLACE_HOUSE with the work's template, SEARCH from the meeting point).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceWork {
    pub op: u64,
    pub village: u64,
    pub frame: u16,
    pub template: String,
    pub processors: String,
    pub x: i32,
    pub z: i32,
    pub radius: u32,
    pub rotation: u8,
    pub seed: u64,
}

/// Spawn a bought work's body at the village's meeting point (SPAWN_RESIDENT, a golem).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnWork {
    pub op: u64,
    pub village: u64,
    pub body: String,
}

/// The first catalogue as data for tests and the CLI's synthetic worlds (the game reads
/// `data/<ns>/mic_rtsim/work/*.json`): a lamp for every human village, a lamp for every ribbit
/// village, a decor set and an iron golem for human villages. Block and item counts are the
/// templates' own (SINK.md §4.2), rounded.
pub fn synthetic_catalog(goods: &rtsim_trade::Goods) -> Vec<WorkDef> {
    let iron = goods.find("iron_ingot");
    let tpl = |t: &str, p: &str, other: i64, blocks: i64| WorkVariant {
        place: WorkPlace::Template {
            template: t.into(),
            processors: p.into(),
        },
        goods: Vec::new(),
        other_items: other,
        blocks,
    };
    let mut golem = WorkVariant {
        place: WorkPlace::Spawn {
            body: "minecraft:iron_golem".into(),
        },
        goods: Vec::new(),
        other_items: 1,
        blocks: 1,
    };
    match iron {
        Some(g) => golem.goods.push((g, 36)),
        None => golem.other_items += 36,
    }
    vec![
        WorkDef {
            id: "mic_rtsim:human_lamp".into(),
            group: WorkGroup::Lamp,
            culture: Culture::Human,
            style: 0,
            cap: WorkCap::PerResidents(3),
            variants: vec![tpl("minecraft:village/plains/plains_lamp_1", "", 8, 8)],
        },
        WorkDef {
            id: "mic_rtsim:human_decor".into(),
            group: WorkGroup::Decor,
            culture: Culture::Human,
            style: 0,
            cap: WorkCap::Fixed(4),
            variants: vec![
                tpl("minecraft:village/taiga/taiga_decoration_1", "", 1, 1),
                tpl("minecraft:village/taiga/taiga_decoration_2", "", 6, 6),
                tpl("minecraft:village/taiga/taiga_decoration_3", "", 12, 12),
            ],
        },
        WorkDef {
            id: "mic_rtsim:iron_golem".into(),
            group: WorkGroup::Golem,
            culture: Culture::Human,
            style: 0,
            cap: WorkCap::PerResidents(5),
            variants: vec![golem],
        },
        WorkDef {
            id: "mic_rtsim:ribbit_lamp".into(),
            group: WorkGroup::Lamp,
            culture: Culture::Ribbit,
            style: 0,
            cap: WorkCap::PerResidents(2),
            variants: vec![tpl("ribbits:decor/lamp_post", "ribbits:main", 20, 20)],
        },
    ]
}
