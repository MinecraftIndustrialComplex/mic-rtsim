// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ports that close under blockade (EQUILIBRIUM.md §2.4.6a rule 7, slice S8b-4; choices PC20 to
//! PC25 and PC80 to PC99). The ships engine's half: the world's lanes close and open with
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
//!   open with no lane above an eighth (PC82). A reopened port does not close again for
//!   `blockade_min_open_days` (PC99). [`PortState::step`] is the rule.
//! - **Derived state** (PC22, PC80, PC93). The ports' states are stored, and since when each
//!   laid-up service has been laid up. A merchant service is laid up when any dock it calls at is
//!   closed; a lane is closed when either end is closed or it has merchant services and every one
//!   is laid up (a patrol sailing it does not keep it open: PC94). Whatever order ports close and
//!   reopen in, the world's lanes and the ships agree with that rule. A port cold, open, not
//!   blockaded and past its minimum open spell has no state (PC96).
//! - **Ships** (PC23, PC24, PC50). A laid-up service's ship out of view and holding no promoted
//!   cargo is laid up at once (`Status::Lost` with `back` [`LAID_UP`]). One in view, or with a
//!   player aboard, keeps its timetable until it leaves view: the voyage under way at the closure
//!   keeps its cargo (those consignments still land), and every later one sails with empty holds
//!   (PC81). One holding promoted cargo lays up once the cargo lands. Navy ships keep sailing. A
//!   reopening relaunches each laid-up slot at its home dock (`Ev::Relaunch` at
//!   `relaunch_time`), never in a player's view: a watched home dock puts it off to the slot's
//!   next stay there (PC90). Laying up and relaunching each make the slot's old body stale
//!   (PC95), and a slot laid up gives up its booked voyages (PC92).

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
    /// When it last reopened, while that is less than `blockade_min_open_days` ago (PC99): it
    /// cannot close again until then. `None` otherwise.
    pub reopened: Option<Tick>,
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
    /// One check's rule (PC21, PC82, PC99), pure: from the port's lanes' heats at `at`
    /// (thousandths) and `full`, the share of one full-crew pirate hunting unopposed. In order: a
    /// port closed for at least `min_closed` ticks with no lane above a quarter reopens; an open
    /// blockaded port with no lane above an eighth is no longer blockaded; a port not blockaded
    /// whose every lane is at least a quarter is blockaded; an open port whose every lane is at
    /// least half closes, unless it reopened less than `min_open` ticks ago. Each threshold that
    /// ends a state lies below the one that began it (half and a quarter, a quarter and an
    /// eighth), so a heat hovering at a threshold cannot flap the state from check to check. A
    /// port with no lanes is never blockaded.
    pub fn step(
        &mut self,
        heats: &[i64],
        full: i64,
        min_closed: u64,
        min_open: u64,
        at: Tick,
    ) -> Vec<PortChange> {
        let (eighth, quarter, half) = (full / 8, full / 4, full / 2);
        let any = !heats.is_empty() && quarter > 0;
        let all_at_least = |x: i64| any && heats.iter().all(|&h| h >= x);
        let none_above = |x: i64| heats.iter().all(|&h| h <= x);
        let nerve_back = |r: Option<Tick>| r.is_none_or(|r| at.0 >= r.0.saturating_add(min_open));
        let mut out = Vec::new();
        if let Some(since) = self.closed
            && none_above(quarter)
            && at.0 >= since.0.saturating_add(min_closed)
        {
            self.closed = None;
            self.reopened = Some(at);
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
        if self.closed.is_none() && nerve_back(self.reopened) && all_at_least(half) {
            self.closed = Some(at);
            out.push(PortChange::Closed);
        }
        if nerve_back(self.reopened) {
            self.reopened = None;
        }
        out
    }
}

fn ticks_of_days(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64).round().max(1.0) as u64
}

/// Days as ticks, 0 allowed.
fn ticks_or_none(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64).round().max(0.0) as u64
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

    fn min_open_ticks(&self) -> u64 {
        ticks_or_none(self.tun.blockade_min_open_days)
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

    /// Since when service `s` has been laid up without a break (PC93): kept from the closure that
    /// laid it up until the reopening that ends it, so another of its ports closing or reopening
    /// in between does not move it. `None` when it sails.
    pub fn laid_up_since(&self, s: u32) -> Option<Tick> {
        self.laid_up.get(&s).copied()
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

    /// Whether lane `l` is closed by the rule (PC22, PC94): either end is a closed port, or it
    /// has merchant services and every one is laid up. A patrol sailing it does not keep it
    /// open: trade needs merchants.
    pub fn lane_shut(&self, l: u32) -> bool {
        let ln = &self.lanes[l as usize];
        if self.port_closed(ln.a) || self.port_closed(ln.b) {
            return true;
        }
        if self.ports.is_empty() {
            return false;
        }
        let mut merchants = self.lane_hops[l as usize]
            .iter()
            .filter(|&&(s, _)| self.services[s as usize].kind == ServiceKind::Merchant)
            .peekable();
        merchants.peek().is_some() && merchants.all(|&(s, _)| self.service_laid_up(s))
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
        let changes = p.step(
            &heats,
            full,
            self.min_closed_ticks(),
            self.min_open_ticks(),
            at,
        );
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
        // Checks go on while a lane is warm, or the port is blockaded, closed or within its
        // minimum open spell. Otherwise the port has nothing to keep (PC96): its state goes, so
        // the map holds only live ports and is empty when no lane is hunted.
        let warm = lanes.iter().any(|&l| self.lane_warm(l, at));
        if warm || p.closed.is_some() || p.blockaded.is_some() || p.reopened.is_some() {
            let when = at + self.check_ticks();
            self.book_check(d, when);
        } else {
            self.ports.remove(&d);
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
            if laid {
                self.laid_up.entry(s).or_insert(at);
            } else {
                self.laid_up.remove(&s);
            }
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
    /// sails again. Its crew stays with it; nothing is held, so nothing is lost. Its epoch moves
    /// on, so a late report about its last body changes nothing (PC95), and its booked voyages
    /// are given up now rather than at their ends (PC92).
    pub(crate) fn lay_up_merchant(&mut self, s: ShipId, at: Tick) {
        let x = &mut self.ships[s];
        x.status = Status::Lost {
            since: at,
            back: LAID_UP,
        };
        x.quotes.clear();
        x.epoch = x.epoch.wrapping_add(1);
        let booked: Vec<u64> = self
            .booked
            .range((s, 0)..=(s, u64::MAX))
            .map(|(&(_, v), _)| v)
            .collect();
        for voyage in booked {
            self.schedule(at, Ev::Unbook { ship: s, voyage });
        }
        self.stats.ships_laid_up_by_closure += 1;
        self.note(
            at,
            "laid_up",
            Some(s),
            "its service calls at a closed port: laid up".into(),
        );
    }

    /// Books the relaunch of laid-up merchant `s` at its home dock (PC24), its service sailing
    /// again: at the slot's next stay there. Its epoch moves on (PC95).
    fn relaunch_laid_up(&mut self, s: ShipId, at: Tick) {
        if !self
            .ships
            .get(s)
            .is_some_and(|x| matches!(x.role, Role::Merchant) && x.is_laid_up())
        {
            return;
        }
        let back = self.relaunch_time(s, at);
        let x = &mut self.ships[s];
        if let Status::Lost { back: b, .. } = &mut x.status {
            *b = back;
        }
        x.epoch = x.epoch.wrapping_add(1);
        self.schedule(back, Ev::Relaunch { ship: s });
    }

    /// A laid-up slot's relaunch is due (PC90). Its service laid up again since: it stays in port
    /// ([`Self::respawn`] lays it up). A player watching where it would appear: it waits for the
    /// slot's next stay at its home dock, and again, until nobody watches. Otherwise the new ship
    /// takes the slot.
    pub(crate) fn relaunch(&mut self, w: &mut World, s: ShipId, at: Tick) {
        let Some(x) = self.ships.get(s) else {
            return;
        };
        if !matches!(x.status, Status::Lost { back, .. } if back <= at) {
            self.stats.stale += 1;
            return;
        }
        let laid = x.service.is_some_and(|v| self.service_laid_up(v));
        if !laid && self.relaunch_in_view(w, s, at) {
            let back = self.relaunch_time(s, at + 1);
            if let Status::Lost { back: b, .. } = &mut self.ships[s].status {
                *b = back;
            }
            self.stats.relaunches_put_off += 1;
            self.schedule(back, Ev::Relaunch { ship: s });
            self.note(
                at,
                "relaunch_put_off",
                Some(s),
                "a player watches its home dock: it waits for its next stay there".into(),
            );
            return;
        }
        self.respawn(w, s, at);
    }

    /// Whether a player's interest centre is within sight of where slot `s` would appear at `at`
    /// (its timetable's place then, its home dock's berth): the view radius and a body's radius,
    /// as a meeting counts it, on the same frame.
    fn relaunch_in_view(&self, w: &World, s: ShipId, at: Tick) -> bool {
        let frame = self.ships[s].frame;
        let here = self.position(w, s, at);
        let reach = u64::from(self.tun.view_radius) + u64::from(self.tun.body_radius);
        self.players.values().any(|v| {
            v.interest
                .and_then(|st| w.legs.get(st.leg).filter(|r| r.generation == st.generation))
                .is_some_and(|r| {
                    r.path.frame == frame
                        && r.path.t0 <= at
                        && at <= r.path.t1
                        && self.sdist(frame, (r.path.x0, r.path.z0), here) <= reach
                })
        })
    }

    // ---- pirates --------------------------------------------------------------------------------

    /// A pirate that reaches lane `lane` and finds it closed since it sailed (PC91): no hunt
    /// begins, so no share, no toll and no feeding; it turns for home at once, a fruitless
    /// sortie. Whether the lane was closed.
    pub(crate) fn reach_closed_lane(&mut self, w: &mut World, s: ShipId, lane: u32, at: Tick) -> bool {
        let edge = self.lanes[lane as usize].edge;
        if edge == u32::MAX || !w.lane_closed(edge) {
            return false;
        }
        self.stats.fruitless += 1;
        let h = self.haven_of(s).expect("a pirate");
        let dock = self.havens[h as usize].dock;
        let from = self.pirate_pos(w, s, at);
        let path = self.sail_path(s, from, (dock.0, dock.2), at);
        let arrive = path.t1;
        let plan = self.bump(s);
        self.book_pirate_leg(w, s, path);
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.state = PState::Homebound { arrive };
        }
        if self.ships[s].embodied {
            let epoch = self.ships[s].epoch;
            self.outbox.push(Cmd::Depart(boundary::Depart {
                agent: Ship::agent(s),
                epoch,
                reason: 1,
                timeout_ticks: 2_400,
                pos: (
                    f64::from(dock.0),
                    f64::from(self.cruise_y_of(s)),
                    f64::from(dock.2),
                ),
            }));
        }
        self.schedule(arrive, Ev::Home { ship: s, plan });
        self.note(
            at,
            "leave",
            Some(s),
            format!("finds lane {lane} closed and turns for haven {h}"),
        );
        true
    }

    // ---- invariants ----------------------------------------------------------------------------

    /// Rule 7's invariants: the world's lanes match the rule, a laid-up merchant slot belongs to
    /// a laid-up service and was laid up no earlier than it, the laid-up services are exactly
    /// those calling at a closed port, a ship still sailing for one is in view or holds promoted
    /// cargo, only trade ports have a state, every port with a state has exactly one live check
    /// (and has one while warm), every booked check is its port's live one, and each lane's heat
    /// is consistent with its stored tick. A whole-table pass.
    pub(crate) fn check_ports(&self, w: &World) -> Result<(), String> {
        let now = self.now();
        if !self.blockades_on() {
            if !self.ports.is_empty() || !self.laid_up.is_empty() {
                return Err(format!(
                    "closures are off but {} ports have a state and {} services are laid up",
                    self.ports.len(),
                    self.laid_up.len()
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
            if let Ev::DockCheck { dock, epoch } = *e {
                // Every booked check is live: a port's state can go and come back with its epoch
                // counted from 0 again, so no stale one may wait in the wheel (PC96).
                if !self
                    .ports
                    .get(&dock)
                    .is_some_and(|p| p.epoch == epoch && p.check == Some(when))
                {
                    return Err(format!(
                        "a stale check of dock {dock} (epoch {epoch}) at {when:?}: {:?}",
                        self.ports.get(&dock)
                    ));
                }
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
                // A port with nothing left to check has no state (PC96).
                None => return Err(format!("port {d} has a state but no check: {p:?}")),
                _ => {}
            }
            if let Some(r) = p.reopened
                && (p.closed.is_some() || r > now)
            {
                return Err(format!("port {d} reopened at {r:?} but: {p:?}"));
            }
            if p.closed.is_some() {
                for &l in self.dock_lanes(d) {
                    let e = self.lanes[l as usize].edge;
                    if e != u32::MAX && !w.lane_closed(e) {
                        return Err(format!("port {d} is closed but its lane {l} is open"));
                    }
                }
            }
        }
        for v in 0..self.services.len() as u32 {
            let laid = self.service_laid_up(v);
            match self.laid_up.get(&v) {
                Some(&t) if !laid || t > now => {
                    return Err(format!(
                        "service {v} is kept laid up since {t:?} but calls at no closed port"
                    ));
                }
                None if laid => {
                    return Err(format!(
                        "service {v} calls at a closed port but is not kept laid up"
                    ));
                }
                _ => {}
            }
        }
        if let Some((&v, _)) = self.laid_up.range(self.services.len() as u32..).next() {
            return Err(format!("laid-up service {v} does not exist"));
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
            if let Status::Lost { since, back } = x.status
                && back == LAID_UP
                && x.service
                    .and_then(|s| self.laid_up_since(s))
                    .is_some_and(|t| since < t)
            {
                return Err(format!(
                    "merchant {id:?} laid up at {since:?}, before its service was"
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

#[cfg(test)]
mod tests {
    use rtsim_ecology::infra::NoInfrastructure;

    use super::*;
    use crate::synth;

    /// Closes trade port `d` now as a check would (blockaded and closed at `at`, its next check
    /// booked) and lays up what the rule says.
    fn close(c: &mut synth::Coastal, d: u32, at: Tick) {
        let (w, s) = (&mut c.world, &mut c.ships);
        assert!(s.is_trade_port(w, d), "dock {d}");
        let p = s.ports.entry(d).or_default();
        p.blockaded = p.blockaded.or(Some(at));
        p.closed = Some(at);
        s.stats.closures += 1;
        let when = at + s.check_ticks();
        s.book_check(d, when);
        s.resettle(w, d, at);
        s.check(w).expect("checks after the closure");
    }

    fn run(c: &mut synth::Coastal, to: Tick) {
        let mut t = c.ships.now();
        while t < to {
            t = (t + TICKS_PER_DAY).min(to);
            c.ships.run_until(&mut c.world, &mut NoInfrastructure, t);
            c.ships
                .check(&c.world)
                .unwrap_or_else(|e| panic!("day {}: {e}", t.0 / TICKS_PER_DAY));
        }
    }

    /// The coast with no pirates (so no heat: the ports' own checks reopen them once their
    /// minimum is up) and the navy.
    fn quiet_coast(seed: u64) -> synth::Coastal {
        synth::coastal_with(seed, ShipTunables::default(), 0, true)
    }

    /// A merchant service calling at two trade ports, and the two (sea docks, in dock order).
    fn two_port_service(c: &synth::Coastal) -> (u32, u32, u32) {
        let s = &c.ships;
        (0..s.services.len() as u32)
            .find_map(|v| {
                let sv = &s.services[v as usize];
                if sv.kind != ServiceKind::Merchant {
                    return None;
                }
                let docks: BTreeSet<u32> = sv
                    .legs
                    .iter()
                    .flat_map(|g| [g.from_dock, g.to_dock])
                    .filter(|&d| s.is_trade_port(&c.world, d))
                    .collect();
                let mut it = docks.into_iter();
                Some((v, it.next()?, it.next()?))
            })
            .expect("a service with two trade ports")
    }

    /// PC93, the review's probe: a service laid up by port `a`'s closure (day 10) and then by
    /// `b`'s too (day 20) stays laid up since day 10 when `a` reopens (day 40, its 30 days up)
    /// and `b` is still closed. Laid-up since the earliest closure among the ports closed now,
    /// it moved to day 20, so a voyage that departed between the two closures, laid up when it
    /// sailed, became a voyage under way with cargo.
    #[test]
    fn laid_up_since_holds_when_the_earlier_port_reopens() {
        let mut c = quiet_coast(1);
        let (v, a, b) = two_port_service(&c);
        let (t1, t2) = (Tick::from_days(10), Tick::from_days(20));
        run(&mut c, t1);
        close(&mut c, a, t1);
        run(&mut c, t2);
        close(&mut c, b, t2);
        assert_eq!(c.ships.laid_up_since(v), Some(t1));
        let id = c.ships.service_ships[v as usize].ids().next().expect("a ship");
        let gap = (0..10_000u64)
            .find(|&n| {
                c.ships
                    .voyage_dir(id, n)
                    .is_some_and(|(_, _, t, _)| t.depart >= t1.0 as i64 && t.depart < t2.0 as i64)
            })
            .expect("a voyage between the closures");
        assert!(c.ships.voyage_laid_up(id, gap));
        run(&mut c, Tick::from_days(45));
        assert!(!c.ships.port_closed(a), "a reopened after its minimum");
        assert!(c.ships.port_closed(b));
        assert!(c.ships.service_laid_up(v), "b is still closed");
        assert_eq!(c.ships.laid_up_since(v), Some(t1));
        assert!(c.ships.voyage_laid_up(id, gap));
    }

    /// Two closed ports on one service, the PC93 bookkeeping end to end on the quiet coast: `a`
    /// closes on day 10 and `b` on day 20; `a` reopens at its first check after day 40 and the
    /// service stays laid up since day 10, its ships in port; `b` reopens at its first after day
    /// 50 and the service sails: its lay-up is forgotten and its ships are booked to relaunch.
    /// `a` closing again on day 70 lays it up since day 70. Once both ports are open, cold and
    /// past their blockade, neither keeps a state (PC96).
    #[test]
    fn a_service_calling_at_two_closed_ports() {
        let mut c = quiet_coast(2);
        let (v, a, b) = two_port_service(&c);
        let ids: Vec<ShipId> = c.ships.service_ships[v as usize].ids().collect();
        let (t1, t2) = (Tick::from_days(10), Tick::from_days(20));
        run(&mut c, t1);
        close(&mut c, a, t1);
        assert!(ids.iter().all(|&id| c.ships.ships[id].is_laid_up()));
        run(&mut c, t2);
        close(&mut c, b, t2);
        run(&mut c, Tick::from_days(49));
        assert!(!c.ships.port_closed(a) && c.ships.port_closed(b));
        assert_eq!(c.ships.laid_up_since(v), Some(t1));
        assert!(ids.iter().all(|&id| c.ships.ships[id].is_laid_up()));
        run(&mut c, Tick::from_days(60));
        assert!(!c.ships.port_closed(a) && !c.ships.port_closed(b));
        assert_eq!(c.ships.laid_up_since(v), None);
        assert!(!c.ships.laid_up.contains_key(&v));
        assert!(
            ids.iter().all(|&id| !c.ships.ships[id].is_laid_up()),
            "every slot relaunched or booked to"
        );
        let t5 = Tick::from_days(70);
        run(&mut c, t5);
        close(&mut c, a, t5);
        assert_eq!(c.ships.laid_up_since(v), Some(t5));
        run(&mut c, Tick::from_days(130));
        assert!(!c.ships.service_laid_up(v));
        assert!(
            c.ships.ports.is_empty(),
            "ports with nothing to check keep a state: {:?}",
            c.ships.ports
        );
        assert!(c.ships.laid_up.is_empty());
    }

    /// PC94 and PC85: the navy keeps sailing and keeps no lane open. On the quiet coast, seeds 1
    /// to 3, each trade port on the patrol's walk closes in turn: the patrol's ships stay active
    /// for 10 days, every lane at the port is closed although the patrol sails it, and so is any
    /// other lane whose merchant services are all laid up, patrol or not.
    #[test]
    fn a_patrol_sails_on_but_keeps_no_lane_open() {
        let mut beyond = 0;
        for seed in 1..=3u64 {
            let base = quiet_coast(seed);
            let nv = base.navy.expect("a navy");
            let ps = base.ships.navies[nv as usize].service;
            let walk: BTreeSet<u32> = base.ships.services[ps as usize]
                .legs
                .iter()
                .flat_map(|g| [g.from_dock, g.to_dock])
                .filter(|&d| base.ships.is_trade_port(&base.world, d))
                .collect();
            assert!(!walk.is_empty(), "seed {seed}: no trade port on the patrol");
            for d in walk {
                let mut c = quiet_coast(seed);
                let at = Tick::from_days(5);
                run(&mut c, at);
                close(&mut c, d, at);
                run(&mut c, Tick::from_days(15));
                let navy: Vec<ShipId> = c.ships.service_ships[ps as usize].ids().collect();
                assert!(
                    navy.iter().all(|&id| c.ships.ships[id].is_active()),
                    "seed {seed} dock {d}: a navy ship stopped"
                );
                let patrolled: BTreeSet<u32> = c.ships.services[ps as usize]
                    .legs
                    .iter()
                    .map(|g| g.hop.lane)
                    .collect();
                for &l in &patrolled {
                    let ln = &c.ships.lanes[l as usize];
                    let edge = ln.edge;
                    let merchants: Vec<u32> = c.ships.lane_hops[l as usize]
                        .iter()
                        .map(|&(v, _)| v)
                        .filter(|&v| c.ships.services[v as usize].kind == ServiceKind::Merchant)
                        .collect();
                    let all_laid =
                        !merchants.is_empty() && merchants.iter().all(|&v| c.ships.service_laid_up(v));
                    let at_port = ln.a == d || ln.b == d;
                    assert_eq!(
                        c.world.lane_closed(edge),
                        at_port || all_laid,
                        "seed {seed} dock {d}: patrolled lane {l}"
                    );
                    if all_laid && !at_port {
                        beyond += 1;
                    }
                }
            }
        }
        eprintln!("patrolled lanes away from the closed port closed by their laid-up merchants: {beyond}");
        assert!(beyond > 0, "no patrolled lane away from a closed port was judged");
    }
}
