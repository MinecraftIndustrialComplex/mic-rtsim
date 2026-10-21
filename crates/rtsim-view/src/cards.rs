// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cards (VIEWER.md §4.4, §7): one per thing a layer draws, built from the decoded save on
//! demand. Every value is read through `&self` (peeks, `Market` views); a district's card restores
//! an engine for that district alone ([`crate::industry::IndustryView::inspect`]).

use std::collections::BTreeMap;

use mic_viewer::api::{Card, Field, Section, TICKS_PER_DAY};
use rtsim_ecology::dragon::{LairKind, Purpose, State, Target};
use rtsim_ecology::herds::HerdId;
use rtsim_ecology::species::SpeciesData;
use rtsim_industry::cluster::PoolKey;
use rtsim_industry::engine::ProcessPeek;
use rtsim_industry::ids::Good;
use rtsim_kernel::handle::Handle;
use rtsim_settle::{Culture, NodeKind};
use rtsim_ships::engine::{Role, Status as ShipStatus};
use rtsim_trade::{GoodId, MILLI, PRICE_ONE};
use rtsim_world::{Via, World};

use crate::industry::IndustryView;
use crate::layers::{Ctx, hunt_target, party_target_pos, price_index, target_pos};
use crate::load::Saved;

/// The most rows a card's section lists.
const ROWS: usize = 40;

/// A game tick as "day 12 06:00" (tick 0 is 06:00 of day 0).
pub fn day(t: i64) -> String {
    let d = t.div_euclid(TICKS_PER_DAY);
    let r = t.rem_euclid(TICKS_PER_DAY);
    let h = (r / 1000 + 6) % 24;
    let m = (r % 1000) * 60 / 1000;
    format!("day {d} {h:02}:{m:02}")
}

/// Items from thousandths.
fn items(a: i64) -> f64 {
    a as f64 / MILLI as f64
}

fn good_name(w: &World, g: GoodId) -> String {
    if g.index() < w.goods.len() {
        w.goods.get(g).name.clone()
    } else {
        format!("good {}", g.0)
    }
}

fn kind_label(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Village(Culture::Human) => "Village",
        NodeKind::Village(Culture::Ribbit) => "Ribbit village",
        NodeKind::PlayerBase { .. } => "Player base",
        NodeKind::Haven(_) => "Haven",
    }
}

fn village_title(w: &World, i: usize) -> String {
    match w.nodes.get(i) {
        Some(n) => format!("{} {i}", kind_label(n.kind)),
        None => format!("node {i}"),
    }
}

/// A thing's card, or `None` when the save has no such thing. Industry districts are answered by
/// the view (they decode pages); everything else here.
pub fn card(cx: &Ctx, layer: &str, id: &str) -> Option<Card> {
    match layer {
        "settle.villages" | "trade.prices" => village(cx, layer, id.parse().ok()?),
        "travel.roads" => road(cx, id.parse().ok()?),
        "travel.caravans" => caravan(cx, id.parse().ok()?),
        "travel.messengers" => messenger(cx, id.parse().ok()?),
        "travel.bands" => band(cx, id.parse().ok()?),
        "ecology.lairs" => lair(cx, id.parse().ok()?),
        "ecology.dragons" | "ecology.hunts" => dragon(cx, layer, id.parse().ok()?),
        "ecology.herds" | "ecology.herd_routes" => herd(cx, layer, id.parse().ok()?),
        "ships.docks" => dock(cx, id.parse().ok()?),
        "ships.lanes" => lane(cx, id.parse().ok()?),
        "ships.ships" => ship(cx, id.parse().ok()?),
        "conflict.factions" => faction(cx, id.parse().ok()?),
        "conflict.feuds" => {
            let (f, a) = id.split_once(':')?;
            feud(cx, f.parse().ok()?, a.parse().ok()?)
        }
        "conflict.parties" => party(cx, id.parse().ok()?),
        "industry.stations" => station(cx, id.parse().ok()?),
        "industry.lines" => {
            let (a, b) = id.split_once('-')?;
            line(cx, a.parse().ok()?, b.parse().ok()?)
        }
        "industry.trains" => train(cx, id.parse().ok()?),
        _ => None,
    }
}

fn village(cx: &Ctx, layer: &str, i: usize) -> Option<Card> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let n = w.nodes.get(i)?;
    let s = w.settlements.get(n.settlement);
    let culture = match n.kind {
        NodeKind::Village(c) | NodeKind::Haven(c) => format!("{c:?}"),
        NodeKind::PlayerBase { .. } => "none".into(),
    };
    let mut c = Card::new(layer, i, village_title(w, i))
        .subtitle(culture.clone())
        .at(f64::from(n.pos.x), f64::from(n.pos.z))
        .field("Kind", kind_label(n.kind))
        .field("Culture", culture)
        .field("Key", format!("{:#x}", n.key));
    if let Some(s) = s {
        c = c
            .field("Population", s.pop)
            .field("Joined", day(cx.saved.game(s.joined)));
    }
    if let NodeKind::PlayerBase { owner } = n.kind {
        c = c.field("Owner", format!("{owner:#x}"));
    }
    c = c.field("Neighbours", n.neighbours.len());
    let mut near = Section::new("Neighbours");
    for &j in n.neighbours.iter().take(ROWS) {
        near.push(Field::new(&village_title(w, j), "").link("settle.villages", j));
    }
    c = c.section(near);
    let m = w.market(i);
    let mut market = Section::new("Market");
    if let Some(cur) = m.currency {
        market = market.field("Currency", good_name(w, cur)).field_unit(
            "Treasury",
            items(m.treasury()),
            "coins",
        );
    }
    if let Some(ix) = price_index(&m) {
        market = market.field("Price index", (ix * 1000.0).round() / 1000.0);
    }
    c = c.section(market);
    let mut goods = Section::new("Goods (stock, price over anchor)");
    for (g, _) in w.goods.iter() {
        if g.index() >= m.lines.len() || Some(g) == m.currency {
            continue;
        }
        let (stock, a) = (m.stock(g), m.anchor(g));
        if stock == 0 && a == 0 {
            continue;
        }
        let p = if a > 0 {
            format!("{:.2}", m.price(g) as f64 / a as f64)
        } else {
            format!("{:.3} coins", m.price(g) as f64 / PRICE_ONE as f64)
        };
        goods.push(Field::new(
            &good_name(w, g),
            format!("{:.1} items, {p}", items(stock)),
        ));
        if goods.fields.len() >= ROWS {
            break;
        }
    }
    c = c.section(goods);
    if let Some(s) = w.sentiment_snapshot(i) {
        let mut feel = Section::new("Feelings toward players (ops only)");
        for (hi, lo, v) in s.entries.iter().take(ROWS) {
            let tone = if *v >= s.friend_at_or_above {
                " (friend)"
            } else if *v <= s.dislike_at_or_below {
                " (disliked)"
            } else {
                ""
            };
            feel.push(Field::new(
                &format!("{hi:016x}{lo:016x}"),
                format!("{v}{tone}"),
            ));
        }
        c = c.section(feel);
    }
    Some(c)
}

fn road(cx: &Ctx, i: usize) -> Option<Card> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let e = w.edges.get(i)?;
    let q = w.cfg.trade.quantum;
    let (pa, pb) = (w.nodes[e.a].pos, w.nodes[e.b].pos);
    let mut c = Card::new("travel.roads", i, format!("{:?} {i}", e.medium))
        .subtitle(format!(
            "{} to {}",
            village_title(w, e.a),
            village_title(w, e.b)
        ))
        .at(f64::from(pa.x + pb.x) / 2.0, f64::from(pa.z + pb.z) / 2.0)
        .link("From", village_title(w, e.a), "settle.villages", e.a)
        .link("To", village_title(w, e.b), "settle.villages", e.b)
        .field_unit("Length", e.length_blocks, "blocks")
        .field_unit("Travel", e.d * q.len(), "ticks")
        .field_unit("Lag", e.lag() * q.len(), "ticks")
        .field("Border", e.border);
    if e.msg_period > 0 {
        c = c.field_unit("Messenger period", e.msg_period * q.len(), "ticks");
    }
    let b = q.index(cx.wnow);
    for (k, d) in e.dirs.iter().enumerate() {
        let mut t = BTreeMap::new();
        d.in_transit(b, &mut t);
        let mut s = Section::new(&format!(
            "In transit, {} to {}",
            village_title(w, d.from),
            village_title(w, d.to)
        ));
        for (g, a) in t.iter().filter(|(_, a)| **a != 0).take(ROWS) {
            s = s.field_unit(&good_name(w, *g), *a as f64 / MILLI as f64, "items");
        }
        if let Some((haven, share)) = w.lane_hazard(i as u32, k as u8) {
            s = s.field("Raided to", village_title(w, haven)).field_unit(
                "Raid share",
                share as f64 / 10.0,
                "%",
            );
        }
        c = c.section(s);
    }
    Some(c)
}

fn caravan(cx: &Ctx, bits: u64) -> Option<Card> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let h = Handle::from_bits(bits);
    let x = w.caravans.get(h)?;
    let place = w.caravan_place(x, cx.wnow);
    let alive = x.members.iter().filter(|m| m.alive && m.present).count();
    let mut c = Card::new("travel.caravans", bits, format!("Caravan {}", h.index()))
        .subtitle(format!("{:?} to {}", x.stage, village_title(w, x.leg.to)))
        .at(place.x as f64, place.z as f64)
        .link("Home", village_title(w, x.home), "settle.villages", x.home)
        .field("Stage", format!("{:?}", x.stage))
        .link(
            "From",
            village_title(w, x.leg.from),
            "settle.villages",
            x.leg.from,
        )
        .link(
            "To",
            village_title(w, x.leg.to),
            "settle.villages",
            x.leg.to,
        )
        .field("Left", day(cx.game(x.leg.start)))
        .field("Arrives", day(cx.game(x.leg.end)))
        .field_unit("Progress", f64::from(x.progress_milli(cx.wnow)) / 10.0, "%")
        .field("Members", format!("{alive} of {}", x.members.len()));
    let mut cargo: BTreeMap<GoodId, i64> = BTreeMap::new();
    for m in &x.members {
        for (g, a) in &m.cargo {
            *cargo.entry(*g).or_default() += a;
        }
    }
    let mut s = Section::new("Carried");
    for (g, a) in cargo.iter().take(ROWS) {
        s = s.field_unit(&good_name(w, *g), items(*a), "items");
    }
    c = c.section(s);
    let mut s = Section::new("Purse");
    for (g, a) in x.purse.iter().take(ROWS) {
        s = s.field_unit(&good_name(w, *g), items(*a), "");
    }
    Some(c.section(s))
}

fn messenger(cx: &Ctx, id: u32) -> Option<Card> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let p = w.packets.get(&id)?;
    let q = w.cfg.trade.quantum;
    let mut c = Card::new("travel.messengers", id, format!("News packet {id}"))
        .link("From", village_title(w, p.from), "settle.villages", p.from)
        .link("To", village_title(w, p.to), "settle.villages", p.to)
        .field("Leaves", day(cx.game(q.at(p.departs))));
    if let Via::Road(r) = p.via {
        c = c.link("Road", r.edge, "travel.roads", r.edge);
        if let Some(e) = w.edges.get(r.edge as usize) {
            c = c.field_unit("Walk", e.news_walk() * q.len(), "ticks");
        }
    }
    c = c.field("Relayed snapshots", p.relays.len());
    if let Some(own) = &p.own {
        c = c.field("Prices as of", day(cx.game(own.at)));
    }
    Some(c)
}

fn band(cx: &Ctx, i: usize) -> Option<Card> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let b = w.bands.get(i)?;
    let v = w.band_view(i, cx.wnow);
    let q = w.cfg.trade.quantum;
    let mut c = Card::new("travel.bands", i, format!("Touring band {i}"))
        .at(v.x as f64, v.z as f64)
        .link("Home", village_title(w, b.home), "settle.villages", b.home)
        .field("Members", b.members)
        .field_unit("Stay", b.stay * q.len(), "ticks");
    c = match (v.staying, v.walking) {
        (Some((n, until)), _) => c
            .subtitle(format!("playing at {}", village_title(w, n)))
            .field("Until", day(cx.game(until))),
        (_, Some((from, to, milli))) => c
            .subtitle(format!(
                "walking from {} to {}",
                village_title(w, from),
                village_title(w, to)
            ))
            .field_unit("Progress", f64::from(milli) / 10.0, "%"),
        _ => c,
    };
    let mut s = Section::new("Circuit");
    for &n in b.route.iter().take(ROWS) {
        s.push(Field::new(&village_title(w, n), "").link("settle.villages", n));
    }
    Some(c.section(s))
}

fn lair(cx: &Ctx, i: u32) -> Option<Card> {
    let d = cx.saved.dragons.as_ref()?;
    let l = d.lairs.get(i)?;
    let kind = match l.kind {
        LairKind::Roost => "roost",
        LairKind::Cave => "cave",
    };
    let mut c = Card::new("ecology.lairs", i, l.name.clone())
        .subtitle(format!("{:?} {kind}", l.species))
        .at(f64::from(l.pos.x), f64::from(l.pos.z))
        .field("Species", format!("{:?}", l.species))
        .field("Kind", kind)
        .field("Height", l.y)
        .field("Hoard", l.hoard.values().sum::<i64>());
    if let Some(t) = l.vacant_since {
        c = c.field("Vacant since", day(cx.game(t)));
    }
    let mut s = Section::new("Occupants");
    for h in l.occupants.iter().take(ROWS) {
        let name = d
            .dragons
            .get(*h)
            .map_or_else(|| format!("dragon {}", h.to_bits()), |x| x.name.clone());
        s.push(Field::new(&name, "").link("ecology.dragons", h.to_bits()));
    }
    c = c.section(s);
    let mut s = Section::new("Hoard");
    for (item, n) in l.hoard.iter().take(ROWS) {
        s = s.field(&format!("{item:?}"), *n);
    }
    Some(c.section(s))
}

fn purpose(saved: &Saved, p: &Purpose) -> String {
    match p {
        Purpose::Hunt(t) => format!("to hunt {}", target_text(saved, t)),
        Purpose::Home => "home".into(),
        Purpose::Claim(l) => format!("to claim lair {l}"),
        Purpose::Refuel { field, x, z, .. } => format!("to drink at oil field {field} ({x}, {z})"),
    }
}

fn target_text(saved: &Saved, t: &Target) -> String {
    match t {
        Target::Road { edge, .. } => format!("on road {edge}"),
        Target::Village { node } => saved.phase1.as_ref().map_or_else(
            || format!("node {node}"),
            |p| village_title(&p.world, *node as usize),
        ),
        Target::Home { player, .. } => format!("the home of player {}", player.to_bits()),
        Target::Herd { herd, .. } => format!("herd {herd}"),
        Target::District { district, .. } => format!("district {district}"),
    }
}

fn dragon(cx: &Ctx, layer: &str, bits: u64) -> Option<Card> {
    let d = cx.saved.dragons.as_ref()?;
    let h = Handle::from_bits(bits);
    let x = d.dragons.get(h)?;
    let (pos, y) = x.position(&d.topo, &d.lairs, cx.wnow);
    let sd = SpeciesData::of(x.species);
    let state = match &x.state {
        State::Roosting => "roosting".to_string(),
        State::Flying(p) => format!("flying {}", purpose(cx.saved, p)),
        State::Hunting { target, until } => format!(
            "hunting {} until {}",
            target_text(cx.saved, target),
            day(cx.game(*until))
        ),
        State::Embodied { purpose: p, .. } => format!("near a player, {}", purpose(cx.saved, p)),
        State::Leaving { purpose: p, .. } => format!("leaving, {}", purpose(cx.saved, p)),
    };
    let mut c = Card::new(layer, bits, x.name.clone())
        .subtitle(format!("{:?}, {state}", x.species))
        .at(f64::from(pos.x), f64::from(pos.z))
        .field("Species", format!("{:?}", x.species))
        .field("Gender", format!("{:?}", x.gender))
        .field_unit("Age", x.age(cx.wnow) as f64 / TICKS_PER_DAY as f64, "days")
        .field("Stage", format!("{:?}", x.stage(&sd, cx.wnow)))
        .field("Fullness", x.fullness_at(&d.tun, cx.wnow))
        .field("Health", f64::from(x.health_at(&sd, cx.wnow)))
        .field("State", state)
        .field("Height", y);
    if let Some(l) = x.lair {
        let name = d
            .lairs
            .get(l)
            .map_or_else(|| format!("lair {l}"), |l| l.name.clone());
        c = c.link("Lair", name, "ecology.lairs", l);
    }
    if let Some(t) = hunt_target(&x.state)
        && let Some((_, at, _)) = target_pos(cx.saved, &t)
    {
        c = c.field("Target at", format!("{:.0}, {:.0}", at[0], at[1]));
    }
    for (label, other) in [("Mate", x.mate), ("Parent", x.parent)] {
        if let Some(o) = other {
            let name = d
                .dragons
                .get(o)
                .map_or_else(|| "gone".into(), |o| o.name.clone());
            c = c.link(label, name, "ecology.dragons", o.to_bits());
        }
    }
    Some(c.field("Children", x.children.len()))
}

fn herd(cx: &Ctx, layer: &str, bits: u64) -> Option<Card> {
    let hs = cx.saved.herds.as_ref()?;
    let h: HerdId = Handle::from_bits(bits);
    let x = hs.herd(h)?;
    let (px, pz) = hs.position(h, cx.wnow);
    let n = hs.count(h, cx.wnow);
    let [young, adult, old] = hs.cohorts(h, cx.wnow);
    let l = &x.ledger;
    let ledger = Section::new("Ledger")
        .field("Founded", l.founded)
        .field("Born", l.born)
        .field("Died", l.died)
        .field("Starved", l.starved)
        .field("Killed", l.killed)
        .field("Eaten", l.eaten)
        .field("Taken", l.taken)
        .field("Lost", l.lost);
    Some(
        Card::new(layer, bits, x.name.clone())
            .subtitle(format!("{n} {:?}", x.species))
            .at(px as f64, pz as f64)
            .field("Species", x.species.entity_id())
            .field("Count", n)
            .field("Young", young)
            .field("Adults", adult)
            .field("Old", old)
            .field_unit("Footprint", hs.footprint(h, cx.wnow), "blocks")
            .field(
                "Condition",
                format!("{:?}", x.condition.map(|v| f64::from(v) / 1000.0)),
            )
            .field("Satisfaction", f64::from(x.satisfaction) / 1000.0)
            .field("Route points", x.route.points.len())
            .field("Route ends", day(cx.game(x.route.until)))
            .section(ledger),
    )
}

fn dock(cx: &Ctx, i: usize) -> Option<Card> {
    let s = cx.saved.ships.as_ref()?;
    let d = s.docks.get(i)?;
    let mut c = Card::new("ships.docks", i, format!("{:?} dock {i}", d.medium))
        .at(f64::from(d.x), f64::from(d.z))
        .field("Medium", format!("{:?}", d.medium))
        .field("Height", d.y)
        .field("Berths", d.berths);
    if let Some(p) = &cx.saved.phase1 {
        c = c.link(
            "Market",
            village_title(&p.world, d.node),
            "settle.villages",
            d.node,
        );
    }
    let lanes: Vec<usize> = s
        .lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| l.a as usize == i || l.b as usize == i)
        .map(|(k, _)| k)
        .collect();
    let mut sec = Section::new("Lanes");
    for k in lanes.iter().take(ROWS) {
        sec.push(Field::new(&format!("Lane {k}"), "").link("ships.lanes", k));
    }
    Some(c.section(sec))
}

fn lane(cx: &Ctx, i: usize) -> Option<Card> {
    let s = cx.saved.ships.as_ref()?;
    let l = s.lanes.get(i)?;
    let mid = l
        .path
        .pts
        .get(l.path.pts.len() / 2)
        .copied()
        .unwrap_or((0, 0));
    let mut c = Card::new("ships.lanes", i, format!("{:?} lane {i}", l.medium))
        .at(f64::from(mid.0), f64::from(mid.1))
        .link("From", format!("dock {}", l.a), "ships.docks", l.a)
        .link("To", format!("dock {}", l.b), "ships.docks", l.b)
        .field("Medium", format!("{:?}", l.medium))
        .field_unit("Length", l.path.length(), "blocks")
        .link("Edge", l.edge, "travel.roads", l.edge)
        .field_unit("Pirates' share", s.lane_share(i as u32) as f64 / 10.0, "%");
    if let Some(t) = s.threat(i as u32) {
        c = c.field("Hunters", t.hunters.len()).field_unit(
            "Patrol",
            t.patrol_milli as f64 / 10.0,
            "%",
        );
        if let Some(h) = t.haven {
            c = c.field("Haven", h);
        }
    }
    Some(c)
}

fn ship(cx: &Ctx, bits: u64) -> Option<Card> {
    let s = cx.saved.ships.as_ref()?;
    let w = &cx.saved.phase1.as_ref()?.world;
    let h = Handle::from_bits(bits);
    let x = s.ships.get(h)?;
    let (px, pz) = s.position(w, h, cx.wnow);
    let role = match x.role {
        Role::Merchant => "merchant".to_string(),
        Role::Pirate { haven } => format!("pirate of haven {haven}"),
        Role::Navy { navy } => format!("navy {navy}"),
    };
    let status = match x.status {
        ShipStatus::Active => "active".to_string(),
        ref other => format!("{other:?}"),
    };
    let mut c = Card::new("ships.ships", bits, s.ship_name(h))
        .subtitle(role.clone())
        .at(f64::from(px), f64::from(pz))
        .field("Role", role)
        .field("Medium", format!("{:?}", x.medium))
        .field("Status", status)
        .field("Crew", x.crew)
        .field_unit("Hull", f64::from(x.hull_milli) / 10.0, "%")
        .field("Embodied", x.embodied);
    if let Some(sv) = x.service {
        c = c.field("Service", sv);
    }
    let mut sec = Section::new("Cargo");
    for (g, a) in x.cargo.iter().take(ROWS) {
        sec = sec.field_unit(&good_name(w, *g), items(*a), "items");
    }
    Some(c.section(sec))
}

fn faction(cx: &Ctx, f: u32) -> Option<Card> {
    let k = cx.saved.conflict.as_ref()?;
    let x = k.factions.get(f)?;
    let culture = k.cultures.get(x.culture);
    let mut c = Card::new("conflict.factions", f, k.faction_name(f))
        .subtitle(culture.name().to_string())
        .at(f64::from(x.pos.x), f64::from(x.pos.z))
        .field("Culture", culture.name())
        .field("Warriors", (k.pool_at(f, cx.wnow) * 10.0).round() / 10.0)
        .field_unit("Reach", x.radius, "blocks")
        .field("Chiefs so far", x.chiefs)
        .field("Plunders", x.plunders);
    let w = cx.saved.phase1.as_ref().map(|p| &p.world);
    let mut sec = Section::new("Feuds");
    for (a, feud) in k.feuds_of(f).iter().take(ROWS) {
        let name = w.map_or_else(|| format!("actor {}", a.to_bits()), |w| k.actor_name(w, *a));
        let g = feud.g.total_at(cx.wnow, culture.lambda);
        sec.push(
            Field::new(&name, format!("{:?}, grievance {g:.1}", feud.step))
                .link("conflict.feuds", format!("{f}:{}", a.to_bits())),
        );
    }
    c = c.section(sec);
    let mut sec = Section::new("Parties");
    for (h, p) in k.parties.iter().filter(|(_, p)| p.faction == f).take(ROWS) {
        sec.push(
            Field::new(&format!("{:?}", p.kind), format!("{:?}", p.phase))
                .link("conflict.parties", h.to_bits()),
        );
    }
    Some(c.section(sec))
}

fn feud(cx: &Ctx, f: u32, actor: u64) -> Option<Card> {
    let k = cx.saved.conflict.as_ref()?;
    let x = k.factions.get(f)?;
    let a = Handle::from_bits(actor);
    let feud = k.feud(f, a)?;
    let w = cx.saved.phase1.as_ref().map(|p| &p.world);
    let name = w.map_or_else(|| format!("actor {actor}"), |w| k.actor_name(w, a));
    let mut c = Card::new(
        "conflict.feuds",
        format!("{f}:{actor}"),
        format!("{} against {name}", k.faction_name(f)),
    )
    .at(f64::from(x.pos.x), f64::from(x.pos.z))
    .link("Faction", k.faction_name(f), "conflict.factions", f)
    .field("Step", format!("{:?}", feud.step))
    .field("Since", day(cx.game(feud.since)))
    .field("Raids", feud.raids);
    if let Some(v) = k.feud_view(x.key, a) {
        c = c.field("Grievance", (v.grievance * 10.0).round() / 10.0);
        let mut sec = Section::new("Grievance by kind");
        for (kind, g) in &v.by_kind {
            sec = sec.field(kind, (g * 10.0).round() / 10.0);
        }
        c = c.section(sec);
    }
    if let Some(d) = &feud.demand {
        let good = w.map_or_else(|| format!("good {}", d.good.0), |w| good_name(w, d.good));
        c = c.section(
            Section::new("Demand")
                .field("For", format!("{:?}", d.kind))
                .field("Tribute", format!("{} {good}", d.tribute))
                .field("Paid", d.paid)
                .field("Deadline", day(cx.game(d.deadline))),
        );
    }
    if let Some(p) = feud.party {
        c = c.link("Party", p.to_bits(), "conflict.parties", p.to_bits());
    }
    Some(c)
}

fn party(cx: &Ctx, bits: u64) -> Option<Card> {
    let k = cx.saved.conflict.as_ref()?;
    let h = Handle::from_bits(bits);
    let p = k.parties.get(h)?;
    let (x, z) = k.party_pos(h, cx.wnow);
    let units: u32 = p.units.iter().map(|(_, n)| n).sum();
    let mut c = Card::new(
        "conflict.parties",
        bits,
        format!("{:?} of {}", p.kind, k.faction_name(p.faction)),
    )
    .subtitle(format!("{:?}", p.phase))
    .at(f64::from(x), f64::from(z))
    .link(
        "Faction",
        k.faction_name(p.faction),
        "conflict.factions",
        p.faction,
    )
    .field("Kind", format!("{:?}", p.kind))
    .field("Phase", format!("{:?}", p.phase))
    .field("Units", format!("{units} of {}", p.start_units))
    .field("Until", day(cx.game(p.until)));
    if let Some(t) = party_target_pos(cx.saved, &p.target) {
        c = c.field("Target at", format!("{}, {}", t.x, t.z));
    }
    Some(c)
}

fn station(cx: &Ctx, id: u64) -> Option<Card> {
    let v = cx.saved.industry.as_ref()?;
    let s = v.stations.iter().find(|s| s.station.0 == id)?;
    let name = cx
        .saved
        .names
        .get(crate::keys::DICT_NAME, s.name)
        .map_or_else(|| format!("Station {id}"), str::to_owned);
    let mut c = Card::new("industry.stations", id, name)
        .at(f64::from(s.pos.x), f64::from(s.pos.z))
        .field("Position", format!("{}, {}, {}", s.pos.x, s.pos.y, s.pos.z));
    if let Some(d) = s.district {
        c = c.link("District", d.0, "industry.districts", d.0);
    }
    let mut sec = Section::new("Lines");
    for ((a, b), l) in v
        .lines
        .iter()
        .filter(|((a, b), _)| *a == id || *b == id)
        .take(ROWS)
    {
        sec.push(
            Field::new(&format!("{a} to {b}"), format!("{} trips", l.trips))
                .link("industry.lines", format!("{a}-{b}")),
        );
    }
    Some(c.section(sec))
}

fn line(cx: &Ctx, a: u64, b: u64) -> Option<Card> {
    let v = cx.saved.industry.as_ref()?;
    let (_, l) = v.lines.iter().find(|(k, _)| *k == (a, b))?;
    let mut sec = Section::new("Cargo seen");
    for (g, n) in l.cargo.iter().take(ROWS) {
        sec = sec.field(&v.good_name(g), n);
    }
    Some(
        Card::new(
            "industry.lines",
            format!("{a}-{b}"),
            format!("Line {a} to {b}"),
        )
        .link("From", a, "industry.stations", a)
        .link("To", b, "industry.stations", b)
        .field("Trips", l.trips)
        .field_unit("Travel", l.travel_ticks, "ticks")
        .field("First", day(l.first.0 as i64))
        .field("Last", day(l.last.0 as i64))
        .section(sec),
    )
}

fn train(cx: &Ctx, id: u64) -> Option<Card> {
    let v = cx.saved.industry.as_ref()?;
    let (_, l) = v.legs.iter().find(|(k, _)| *k == id)?;
    let mut c = Card::new("industry.trains", id, format!("Train {}", l.train.0))
        .link("From", l.from.0, "industry.stations", l.from.0)
        .field("Departs", day(l.depart.0 as i64))
        .field("Arrives", day(l.eta.0 as i64));
    if let Some(t) = l.to {
        c = c.link("To", t.0, "industry.stations", t.0);
    }
    let mut total: BTreeMap<rtsim_industry::ids::Good, i64> = BTreeMap::new();
    for (_, gv) in &l.cargo {
        for (g, n) in gv.iter() {
            *total.entry(g).or_default() += n;
        }
    }
    let mut sec = Section::new("Cargo");
    for (g, n) in total.iter().take(ROWS) {
        sec = sec.field(&v.good_name(*g), *n);
    }
    Some(c.field("Carriages", l.cargo.len()).section(sec))
}

/// A district's name from Java's dictionary, or its number.
fn district_name(saved: &Saved, v: &IndustryView, id: u64) -> String {
    v.district(id)
        .and_then(|d| saved.names.get(crate::keys::DICT_NAME, d.name))
        .map_or_else(|| format!("District {id}"), str::to_owned)
}

/// A replay process's line: a box process by number and the goods it touches, a packager by what
/// it packs and for whom.
fn process_label(saved: &Saved, v: &IndustryView, p: &ProcessPeek) -> String {
    let goods = |gs: &[Good]| {
        gs.iter()
            .map(|&g| v.good_name(g))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match p.fills {
        Some(PoolKey::Mail(_, inventory, address, g)) => {
            let to = saved
                .names
                .get(crate::keys::DICT_NAME, address)
                .map_or_else(|| format!("address {address}"), str::to_owned);
            format!("Packager: {} for {to}, postbox {inventory}", v.good_name(g))
        }
        Some(_) => format!("Link {}: {}", p.id, goods(&p.goods)),
        None => format!("Process {}: {}", p.id + 1, goods(&p.goods)),
    }
}

/// A district's card, and whether its pages read (a card that says they did not is not kept).
pub fn district(saved: &Saved, id: u64) -> Option<(Card, bool)> {
    let v = saved.industry.as_ref()?;
    let d = v.district(id)?;
    let name = district_name(saved, v, id);
    let (x, z) = d.centre();
    let mut c = Card::new("industry.districts", id, name)
        .subtitle(d.state)
        .at(x, z)
        .field("State", d.state)
        .field("Chunks", d.chunks)
        .field("Inventories", d.inventories)
        .field("Watched", d.watched);
    let mut sec = Section::new("Stations");
    for s in d.stations.iter().take(ROWS) {
        sec.push(Field::new(&format!("Station {s}"), "").link("industry.stations", s));
    }
    c = c.section(sec);
    match v.inspect(&*saved.source, id) {
        Ok(peek) => {
            let ins = &peek.inspection;
            if let Some(r) = &ins.reason {
                c = c.field("Reason", r.clone());
            }
            if let Some(t) = ins.since {
                c = c.field("Replaying since", day(t.0 as i64));
            }
            if let Some(r) = ins.running {
                c = c.field("Running", r);
            }
            if let Some(s) = &ins.stop {
                c = c.field("Stopped by", s.clone());
            }
            if let Some(t) = ins.learning_ticks_to_go {
                c = c.field_unit("Learning to go", t, "ticks");
            }
            let mut sec = Section::new("Rates");
            for (g, r) in ins.rates_per_hour.iter().take(ROWS) {
                sec = sec.field_unit(&v.good_name(*g), (r * 100.0).round() / 100.0, "per hour");
            }
            c = c.section(sec);
            if !peek.processes.is_empty() {
                let mut sec = Section::new("Processes");
                for p in peek.processes.iter().take(ROWS) {
                    let mut state = p.stop.clone().unwrap_or_else(|| "running".into());
                    if let Some(src) = p.requires {
                        state = format!("{state}, on {}'s power", district_name(saved, v, src.0));
                    }
                    sec = sec.field(&process_label(saved, v, p), state);
                }
                c = c.section(sec);
            }
            if peek.members.len() > 1 {
                let mut sec = Section::new("Replays with (one cluster)");
                for &m in peek.members.iter().filter(|&&m| m != id).take(ROWS) {
                    sec.push(
                        Field::new(&district_name(saved, v, m), "").link("industry.districts", m),
                    );
                }
                c = c.section(sec);
            }
            let mut sec = Section::new("Made since the replay began");
            for (g, n) in ins.made_since.iter().take(ROWS) {
                sec = sec.field(&v.good_name(g), n);
            }
            c = c.section(sec);
            if let Some(stock) = &peek.stock {
                let mut sec = Section::new("Replay stock");
                for (g, n) in stock.iter().take(ROWS) {
                    sec = sec.field(&v.good_name(g), n);
                }
                c = c.section(sec);
            }
            if let Some(r) = &ins.last_reconcile {
                c = c.section(
                    Section::new("Last reconcile")
                        .field("At", day(r.at.0 as i64))
                        .field("Chunks", r.chunks)
                        .field("Deltas", r.deltas)
                        .field("Goods changed", r.net.len()),
                );
            }
            let mut sec = Section::new("Attacks");
            for a in ins.attacks.iter().take(ROWS) {
                sec = sec.field(
                    &day(a.at.0 as i64),
                    format!("{:?} {:?} x{}: {:?}", a.source, a.class, a.count, a.outcome),
                );
            }
            c = c.section(sec);
        }
        Err(e) => {
            let c = c.note(format!(
                "its pages did not read ({}). The viewer shows the one save it opened: if the \
                 world has saved since, restart the viewer to see the new save",
                saved.scrub(&e)
            ));
            return Some((c, false));
        }
    }
    Some((c, true))
}
