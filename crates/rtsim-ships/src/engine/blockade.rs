// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ports that close under blockade (EQUILIBRIUM.md §2.4.6a rule 7, slice S8b-4; choices PC20 to
//! PC25 and PC80 to PC89). The ships engine's half: the world's lanes close and open with
//! `World::close_lane` and `World::open_lane` (S8b-4a).
//!
//! - **Heat** (PC20). Each lane's threat row keeps an exponential average of its applied hazard
//!   share, time constant `blockade_days`, brought up to date in closed form whenever the share
//!   changes ([`LaneThreat::heat`]).
//! - **Checks** (PC25). A trade port (a village's dock; a haven's never closes) is weighed every
//!   `blockade_check_days` while one of its lanes is warm (a share above 0, or a heat above
//!   [`HEAT_FLOOR_MILLI`]) or it is blockaded or closed. A share that becomes positive books a
//!   check at each trade end with none, so the work follows the docks pirates hunt near.
//! - **Thresholds** (PC21). "Full" is one full-crew pirate of the dock's medium hunting
//!   unopposed (sea 120‰, sky 100‰ by default). A port is blockaded once every lane is at least
//!   a quarter of full, closes when every lane is at least half, reopens when no lane is above a
//!   quarter once it has been closed `blockade_min_closed_days`, and is no longer blockaded when
//!   open with no lane above an eighth (PC82). [`PortState::step`] is the rule.
//! - **Derived state** (PC22, PC80). Only the ports' states are stored. A merchant service is
//!   laid up when any dock it calls at is closed; a lane is closed when either end is closed or
//!   it has services and every one is laid up (a patrol never is). Whatever order ports close and
//!   reopen in, the world's lanes and the ships agree with that rule.
//! - **Ships** (PC23, PC24, PC50). A laid-up service's ship out of view and holding no promoted
//!   cargo is laid up at once (`Status::Lost` with `back` [`LAID_UP`]). One in view, or with a
//!   player aboard, keeps its timetable until it leaves view: the voyage under way at the closure
//!   keeps its cargo (those consignments still land), and every later one sails with empty holds
//!   (PC81). One holding promoted cargo lays up once the cargo lands. Navy ships keep sailing. A
//!   reopening relaunches each laid-up slot at its home dock (`Ev::Respawn` at
//!   `relaunch_time`).

use rtsim_settle::NodeKind;

use super::*;

/// The heat under which a lane no longer keeps its ports' checks going, thousandths (PC25): the
/// smallest heat the rounding keeps above 0.
pub const HEAT_FLOOR_MILLI: i64 = 1;

/// A trade port's blockade state (§2.4.6a rule 7), saved. What follows from it (services laid
/// up, lanes closed, ships waiting to lay up) is derived, not stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortState {
    /// Since when every lane has been at least a quarter of full; `None` when not blockaded.
    pub blockaded: Option<Tick>,
    /// Since when the port has been closed; `None` when open.
    pub closed: Option<Tick>,
    /// When its one live check is due; `None` when none is booked.
    pub check: Option<Tick>,
    /// Stamps the live check: booking one bumps it, so an older check is stale.
    pub epoch: u32,
}

/// What a port's news would tell (rule 8 of the slice): knowledge has no kind for it yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortNews {
    Blockade,
    Closed,
    Reopened,
}

/// A change a check makes to a port, in the order a check applies them; its value is the
/// magnitude of the `ships.port` tape row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortChange {
    /// Every lane reached a quarter of full.
    Blockaded = 1,
    /// Every lane reached half of full.
    Closed = 2,
    /// Closed for the minimum, and no lane is above a quarter.
    Reopened = 3,
    /// Open, and no lane is above an eighth.
    Lifted = 4,
}

impl PortState {
    /// One check's rule (PC21, PC82), pure: from the port's lanes' heats at `at` (thousandths)
    /// and `full`, the share of one full-crew pirate hunting unopposed. In order: a port closed
    /// for at least `min_closed` ticks with no lane above a quarter reopens; an open blockaded
    /// port with no lane above an eighth is no longer blockaded; a port not blockaded whose every
    /// lane is at least a quarter is blockaded; an open port whose every lane is at least half
    /// closes. Each threshold that ends a state lies below the one that began it (half and a
    /// quarter, a quarter and an eighth), so a heat hovering at a threshold cannot flap the
    /// state from check to check. A port with no lanes is never blockaded.
    pub fn step(&mut self, heats: &[i64], full: i64, min_closed: u64, at: Tick) -> Vec<PortChange> {
        let (eighth, quarter, half) = (full / 8, full / 4, full / 2);
        let any = !heats.is_empty() && quarter > 0;
        let all_at_least = |x: i64| any && heats.iter().all(|&h| h >= x);
        let none_above = |x: i64| heats.iter().all(|&h| h <= x);
        let mut out = Vec::new();
        if let Some(since) = self.closed
            && none_above(quarter)
            && at.0 >= since.0.saturating_add(min_closed)
        {
            self.closed = None;
            out.push(PortChange::Reopened);
        }
        if self.closed.is_none() && self.blockaded.is_some() && none_above(eighth) {
            self.blockaded = None;
            out.push(PortChange::Lifted);
        }
        if self.blockaded.is_none() && all_at_least(quarter) {
            self.blockaded = Some(at);
            out.push(PortChange::Blockaded);
        }
        if self.closed.is_none() && all_at_least(half) {
            self.closed = Some(at);
            out.push(PortChange::Closed);
        }
        out
    }
}

fn ticks_of_days(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64).round().max(1.0) as u64
}

impl Ships {
    /// Whether ports close under blockade: the lifecycle on (closures are its rule 7) and a
    /// heat time constant above 0.
    pub fn blockades_on(&self) -> bool {
        self.tun.lifecycle && self.tun.blockade_days > 0.0
    }

    /// The heat's time constant, ticks; 0 with closures off.
    pub(crate) fn heat_tau(&self) -> f64 {
        if self.blockades_on() {
            self.tun.blockade_days * TICKS_PER_DAY as f64
        } else {
            0.0
        }
    }

    fn check_ticks(&self) -> u64 {
        ticks_of_days(self.tun.blockade_check_days)
    }

    fn min_closed_ticks(&self) -> u64 {
        ticks_of_days(self.tun.blockade_min_closed_days)
    }

    /// Lane `l`'s heat at `t` (no earlier than its last share change), thousandths: a pure read.
    pub fn heat_at(&self, l: u32, t: Tick) -> i64 {
        self.threat(l).map_or(0, |r| r.heat(t, self.heat_tau()))
    }

    /// One full-crew pirate of `m` hunting unopposed, thousandths (PC21's "full").
    pub fn full_share(&self, m: Medium) -> i64 {
        let crew = f64::from(self.tun.medium(m).pirate_crew);
        fight::hazard_milli(crew * self.tun.pirate_strength, 0.0, &self.tun)
    }

    /// Whether dock `d` is a trade port: a village's dock. Only these close (a haven's never
    /// does).
    pub fn is_trade_port(&self, w: &World, d: u32) -> bool {
        self.docks
            .get(d as usize)
            .and_then(|k| w.nodes.get(k.node))
            .is_some_and(|n| matches!(n.kind, NodeKind::Village(_)))
    }

    /// Dock `d`'s blockade state, if a hunted lane has touched it.
    pub fn port(&self, d: u32) -> Option<&PortState> {
        self.ports.get(&d)
    }

    pub fn port_closed(&self, d: u32) -> bool {
        self.ports.get(&d).is_some_and(|p| p.closed.is_some())
    }

    /// The lanes with dock `d` at an end, in lane order.
    pub fn dock_lanes(&self, d: u32) -> &[u32] {
        self.dock_lanes.get(d as usize).map_or(&[], Vec::as_slice)
    }

    /// Whether service `s` is laid up: a merchant service calling at a closed port (PC22).
    /// Patrols never are.
    pub fn service_laid_up(&self, s: u32) -> bool {
        if self.ports.is_empty() {
            return false;
        }
        let Some(sv) = self.services.get(s as usize) else {
            return false;
        };
        sv.kind == ServiceKind::Merchant
            && sv
                .legs
                .iter()
                .any(|g| self.port_closed(g.from_dock) || self.port_closed(g.to_dock))
    }

    /// Since when service `s` has been laid up: the earliest closure among the closed ports it
    /// calls at (each has been closed without a break since its own). `None` when it sails.
    pub fn laid_up_since(&self, s: u32) -> Option<Tick> {
        if self.ports.is_empty() {
            return None;
        }
        let sv = self.services.get(s as usize)?;
        if sv.kind != ServiceKind::Merchant {
            return None;
        }
        sv.legs
            .iter()
            .flat_map(|g| [g.from_dock, g.to_dock])
            .filter_map(|d| self.ports.get(&d).and_then(|p| p.closed))
            .min()
    }

    /// Whether ship `id`'s voyage `voyage` carries nothing because its service is laid up: it
    /// departs no earlier than the lay-up (PC23, PC81). A voyage already under way when the port
    /// closed keeps its window of the flow, whose consignments still land: a body in view shows
    /// them and a loot or a raid can promote them.
    pub fn voyage_laid_up(&self, id: ShipId, voyage: u64) -> bool {
        let Some(since) = self
            .ships
            .get(id)
            .and_then(|x| x.service)
            .and_then(|s| self.laid_up_since(s))
        else {
            return false;
        };
        self.voyage_dir(id, voyage)
            .is_none_or(|(_, _, t, _)| t.depart >= since.0 as i64)
    }

    /// Whether lane `l` is closed by the rule (PC22): either end is a closed port, or it has
    /// services and every one is laid up.
    pub fn lane_shut(&self, l: u32) -> bool {
        let ln = &self.lanes[l as usize];
        if self.port_closed(ln.a) || self.port_closed(ln.b) {
            return true;
        }
        let hops = &self.lane_hops[l as usize];
        !hops.is_empty() && hops.iter().all(|&(s, _)| self.service_laid_up(s))
    }

    /// Whether ship `s` is a merchant still sailing for a laid-up service: in view, or holding a
    /// promoted voyage's cargo (PC23).
    pub fn waiting_to_lay_up(&self, s: ShipId) -> bool {
        self.ships.get(s).is_some_and(|x| {
            matches!(x.role, Role::Merchant)
                && x.is_active()
                && x.service.is_some_and(|v| self.service_laid_up(v))
        })
    }

    // ---- checks --------------------------------------------------------------------------------

    /// After lane `l`'s share became positive: a check for each trade end that has none.
    pub(crate) fn book_port_checks(&mut self, w: &World, l: u32, at: Tick) {
        let ln = &self.lanes[l as usize];
        for d in [ln.a, ln.b] {
            if self.is_trade_port(w, d) && self.ports.get(&d).is_none_or(|p| p.check.is_none()) {
                let when = at + self.check_ticks();
                self.book_check(d, when);
            }
        }
    }

    fn book_check(&mut self, d: u32, when: Tick) {
        let when = when.max(self.wheel.now());
        let p = self.ports.entry(d).or_default();
        p.epoch = p.epoch.wrapping_add(1);
        p.check = Some(when);
        let epoch = p.epoch;
        self.schedule(when, Ev::DockCheck { dock: d, epoch });
    }

    /// Whether lane `l` keeps its ports' checks going at `t`.
    fn lane_warm(&self, l: u32, t: Tick) -> bool {
        self.threat(l).is_some_and(|r| r.share > 0) || self.heat_at(l, t) > HEAT_FLOOR_MILLI
    }

    /// A port's check (PC21, PC25): its lanes' heats move it between open, blockaded and closed,
    /// and it books the next while a lane is warm or it is blockaded or closed.
    pub(crate) fn dock_check(&mut self, w: &mut World, d: u32, epoch: u32, at: Tick) {
        let Some(mut p) = self.ports.get(&d).copied() else {
            self.stats.stale += 1;
            return;
        };
        if p.epoch != epoch || p.check.is_none() {
            self.stats.stale += 1;
            return;
        }
        p.check = None;
        self.stats.port_checks += 1;
        let full = self.full_share(self.docks[d as usize].medium);
        let lanes: Vec<u32> = self
            .dock_lanes(d)
            .iter()
            .copied()
            .filter(|&l| self.lanes[l as usize].edge != u32::MAX)
            .collect();
        let heats: Vec<i64> = lanes.iter().map(|&l| self.heat_at(l, at)).collect();
        let hottest = lanes
            .iter()
            .zip(&heats)
            .max_by_key(|(l, h)| (**h, std::cmp::Reverse(**l)))
            .and_then(|(&l, _)| self.threat(l).and_then(|t| t.haven));
        let was = p.closed;
        let changes = p.step(&heats, full, self.min_closed_ticks(), at);
        let mut moved = false;
        for c in &changes {
            match c {
                PortChange::Blockaded => self.stats.blockades += 1,
                PortChange::Closed => {
                    self.stats.closures += 1;
                    moved = true;
                }
                PortChange::Reopened => {
                    self.stats.reopenings += 1;
                    let since = was.expect("a reopened port was closed");
                    self.stats.closed_dock_days += (at.0 - since.0) / TICKS_PER_DAY;
                    moved = true;
                }
                PortChange::Lifted => {}
            }
        }
        self.ports.insert(d, p);
        if moved {
            self.resettle(w, d, at);
        }
        for &c in &changes {
            self.port_event(w, d, c, hottest, &heats, at);
        }
        let warm = lanes.iter().any(|&l| self.lane_warm(l, at));
        if warm || p.closed.is_some() || p.blockaded.is_some() {
            let when = at + self.check_ticks();
            self.book_check(d, when);
        }
    }

    /// The tape row, note and news of a port's change.
    fn port_event(
        &mut self,
        w: &mut World,
        d: u32,
        what: PortChange,
        haven: Option<u32>,
        heats: &[i64],
        at: Tick,
    ) {
        let dk = &self.docks[d as usize];
        let (node, frame, x, z) = (dk.node, dk.frame, dk.x, dk.z);
        let by = haven
            .and_then(|h| self.havens.get(h as usize))
            .map_or(0, |hv| nid(w, hv.node));
        let (text, news) = match what {
            PortChange::Blockaded => ("blockaded", Some(PortNews::Blockade)),
            PortChange::Closed => ("closed", Some(PortNews::Closed)),
            PortChange::Reopened => ("reopened", Some(PortNews::Reopened)),
            PortChange::Lifted => ("no longer blockaded", None),
        };
        self.tape
            .event(
                kinds::SHIPS_PORT,
                spot(at, frame, x, z),
                nid(w, node),
                by,
                what as i64,
            )
            .text(|| format!("the port of node {node} is {text}"));
        self.note(
            at,
            "port",
            None,
            format!("dock {d} (node {node}) is {text}: its lanes' heats {heats:?}‰"),
        );
        match news {
            // ReportKind Blockade (17): the port's people learn pirates hold its lanes.
            Some(PortNews::Blockade) => self.port_news(w, d, PortNews::Blockade, at),
            // ReportKind PortClosed (18): the port shuts its harbour; its trade stops.
            Some(PortNews::Closed) => self.port_news(w, d, PortNews::Closed, at),
            // ReportKind PortReopened (19): ships sail from the port again.
            Some(PortNews::Reopened) => self.port_news(w, d, PortNews::Reopened, at),
            None => {}
        }
    }

    /// A port's news for knowledge (rule 8 of the slice): a stub until knowledge defines the
    /// report kinds Blockade (17), PortClosed (18) and PortReopened (19). Nothing is reported.
    fn port_news(&mut self, _w: &mut World, _dock: u32, _news: PortNews, _at: Tick) {}

    // ---- what a closure does -------------------------------------------------------------------

    /// After dock `d` closed or reopened: every merchant service calling there lays up its
    /// ships or relaunches them by the rule, and every lane whose rule may have changed (the
    /// dock's, and those services') is closed or opened to match. Order-independent: it reads
    /// only the ports' states.
    pub(crate) fn resettle(&mut self, w: &mut World, d: u32, at: Tick) {
        let lanes = self.dock_lanes(d).to_vec();
        let services: BTreeSet<u32> = lanes
            .iter()
            .flat_map(|&l| self.lane_hops[l as usize].iter().map(|&(s, _)| s))
            .collect();
        let mut touched: BTreeSet<u32> = lanes.into_iter().collect();
        for s in services {
            let sv = &self.services[s as usize];
            if sv.kind != ServiceKind::Merchant {
                continue;
            }
            touched.extend(sv.legs.iter().map(|g| g.hop.lane));
            let laid = self.service_laid_up(s);
            let ids: Vec<ShipId> = self.service_ships[s as usize].ids().collect();
            for id in ids {
                if laid {
                    self.lay_up_if_waiting(id, at);
                } else {
                    self.relaunch_laid_up(id, at);
                }
            }
        }
        for l in touched {
            self.settle_lane(w, l, at);
        }
    }

    /// Closes or opens lane `l`'s world edge to match the rule.
    pub(crate) fn settle_lane(&mut self, w: &mut World, l: u32, at: Tick) {
        let edge = self.lanes[l as usize].edge;
        if edge == u32::MAX {
            return;
        }
        let want = self.lane_shut(l);
        if w.lane_closed(edge) == want {
            return;
        }
        let ok = if want {
            w.close_lane(edge, at)
        } else {
            w.open_lane(edge, at)
        };
        debug_assert!(ok, "lane {l}'s edge {edge} is a ships' lane");
        self.note(
            at,
            "lane",
            None,
            format!("lane {l} is {}", if want { "closed" } else { "open again" }),
        );
    }

    /// Lays up ship `s` if it is a merchant of a laid-up service that no longer has a reason to
    /// sail: out of view and holding no promoted cargo.
    pub(crate) fn lay_up_if_waiting(&mut self, s: ShipId, at: Tick) {
        let idle = self
            .ships
            .get(s)
            .is_some_and(|x| !x.embodied && x.promoted.is_none() && x.cargo.is_empty());
        if idle && self.waiting_to_lay_up(s) {
            self.lay_up_merchant(s, at);
        }
    }

    /// A merchant slot laid up in port (PC15's `LAID_UP`): no relaunch is due until its service
    /// sails again. Its crew stays with it; nothing is held, so nothing is lost.
    pub(crate) fn lay_up_merchant(&mut self, s: ShipId, at: Tick) {
        let x = &mut self.ships[s];
        x.status = Status::Lost {
            since: at,
            back: LAID_UP,
        };
        x.quotes.clear();
        self.stats.ships_laid_up_by_closure += 1;
        self.note(
            at,
            "laid_up",
            Some(s),
            "its service calls at a closed port: laid up".into(),
        );
    }

    /// Relaunches laid-up merchant `s` at its home dock (PC24), its service sailing again.
    fn relaunch_laid_up(&mut self, s: ShipId, at: Tick) {
        if !self
            .ships
            .get(s)
            .is_some_and(|x| matches!(x.role, Role::Merchant) && x.is_laid_up())
        {
            return;
        }
        let back = self.relaunch_time(s, at);
        if let Status::Lost { back: b, .. } = &mut self.ships[s].status {
            *b = back;
        }
        self.schedule(back, Ev::Respawn { ship: s });
    }

    // ---- invariants ----------------------------------------------------------------------------

    /// Rule 7's invariants: the world's lanes match the rule, a laid-up merchant slot belongs to
    /// a laid-up service, a ship still sailing for one is in view or holds promoted cargo, only
    /// trade ports have a state, every warm, blockaded or closed port has exactly one live check, and each
    /// lane's heat is consistent with its stored tick. A whole-table pass.
    pub(crate) fn check_ports(&self, w: &World) -> Result<(), String> {
        let now = self.now();
        if !self.blockades_on() {
            if !self.ports.is_empty() {
                return Err(format!(
                    "closures are off but {} ports have a state",
                    self.ports.len()
                ));
            }
            for (l, t) in self.lane_threats() {
                if t.heat_nano != 0 || t.heat_at != Tick(0) {
                    return Err(format!("closures are off but lane {l} keeps a heat"));
                }
            }
            return Ok(());
        }
        let tau = self.heat_tau();
        for (l, t) in self.lane_threats() {
            if t.heat_at > now {
                return Err(format!(
                    "lane {l}'s heat is stamped {:?}, after now",
                    t.heat_at
                ));
            }
            if !(0..=self.tun.max_share_milli * HEAT_PER_MILLI).contains(&t.heat_nano) {
                return Err(format!("lane {l}'s heat {} is out of range", t.heat_nano));
            }
            // Between changes the heat relaxes monotonically from its stored value to the share.
            let h = t.heat_nano_at(now, tau);
            let target = t.share * HEAT_PER_MILLI;
            let (lo, hi) = (t.heat_nano.min(target), t.heat_nano.max(target));
            if h < lo || h > hi {
                return Err(format!(
                    "lane {l}'s heat {h} now is not between its stored {} and its share {}‰",
                    t.heat_nano, t.share
                ));
            }
        }
        let mut live: BTreeMap<u32, Vec<Tick>> = BTreeMap::new();
        for (when, e) in self.wheel.iter() {
            if let Ev::DockCheck { dock, epoch } = *e
                && self.ports.get(&dock).is_some_and(|p| p.epoch == epoch)
            {
                live.entry(dock).or_default().push(when);
            }
        }
        for (&d, p) in &self.ports {
            if !self.is_trade_port(w, d) {
                return Err(format!(
                    "dock {d} has a blockade state but is not a trade port"
                ));
            }
            if p.closed.is_some() && (p.blockaded.is_none() || p.blockaded > p.closed) {
                return Err(format!(
                    "port {d} is closed but was not blockaded first: {p:?}"
                ));
            }
            let checks = live.get(&d).map_or(&[][..], Vec::as_slice);
            match p.check {
                Some(t) if checks != [t] => {
                    return Err(format!("port {d}'s check at {t:?}, live checks {checks:?}"));
                }
                None if !checks.is_empty() => {
                    return Err(format!(
                        "port {d} has no check booked, live checks {checks:?}"
                    ));
                }
                _ => {}
            }
            if p.closed.is_some() {
                for &l in self.dock_lanes(d) {
                    let e = self.lanes[l as usize].edge;
                    if e != u32::MAX && !w.lane_closed(e) {
                        return Err(format!("port {d} is closed but its lane {l} is open"));
                    }
                }
            }
            let warm = self.dock_lanes(d).iter().any(|&l| self.lane_warm(l, now));
            if (warm || p.closed.is_some() || p.blockaded.is_some()) && p.check.is_none() {
                return Err(format!(
                    "port {d} is warm, blockaded or closed with no check: {p:?}"
                ));
            }
        }
        // A warm lane's trade ends have a check, whether or not they have a state yet.
        for (l, _) in self.lane_threats() {
            if !self.lane_warm(l, now) {
                continue;
            }
            let ln = &self.lanes[l as usize];
            for d in [ln.a, ln.b] {
                if self.is_trade_port(w, d) && self.ports.get(&d).is_none_or(|p| p.check.is_none())
                {
                    return Err(format!("lane {l} is warm but its port {d} has no check"));
                }
            }
        }
        for l in 0..self.lanes.len() as u32 {
            let e = self.lanes[l as usize].edge;
            if e != u32::MAX && w.lane_closed(e) != self.lane_shut(l) {
                return Err(format!(
                    "lane {l}'s edge {e} is {} but the rule has it {}",
                    if w.lane_closed(e) { "closed" } else { "open" },
                    if self.lane_shut(l) { "closed" } else { "open" }
                ));
            }
        }
        for (id, x) in self.ships.iter() {
            if !matches!(x.role, Role::Merchant) {
                continue;
            }
            let laid = x.service.is_some_and(|s| self.service_laid_up(s));
            if x.is_laid_up() && !laid {
                return Err(format!(
                    "merchant {id:?} is laid up but its service sails: {:?}",
                    x.status
                ));
            }
            if x.is_laid_up() && (x.promoted.is_some() || !x.cargo.is_empty()) {
                return Err(format!(
                    "merchant {id:?} laid up mid-voyage: promoted {:?}, holding {:?}",
                    x.promoted, x.cargo
                ));
            }
            if laid && x.is_active() && !x.embodied && x.promoted.is_none() {
                return Err(format!(
                    "merchant {id:?} sails for a laid-up service, out of view with nothing promoted"
                ));
            }
        }
        Ok(())
    }
}
