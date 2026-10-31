// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim census`: the who-is-alive census (EQUILIBRIUM.md §4, slice S1). Each scenario
//! runs a module's synthetic world with nobody watching and prints one line per kind of site every
//! N days: sites, members, capacity (the sites' K), short, empty, broken, mean pressure, and the
//! renewals since the kind's previous line (`rtsim_kernel::census`). Later slices measure
//! themselves with these lines, so the format is fixed-width and two runs diff line by line.
//!
//! **Scenarios** (each also counts its world's villages):
//! - `ecology`: the phase 1 dragon world's lairs and dragons (`rtsim_ecology::synth::populate`),
//!   oil reported under two nest cells with a well drilling beside each (a nest wakes and renews
//!   only while drilled), and the herd valley's six herds hunted by the dragons, all through the
//!   fake Ice and Fire and herd plugins. Kinds `dragon.lair`, `oil.nest`, `herd.range`.
//! - `ranges`: the same dragon world over a grid of herd ranges, 8 x 7 regions of the valley, one
//!   herd each founded as the world model founds one (`rtsim_ecology::herds::synth::ranges`), so a
//!   range has neighbours (EQUILIBRIUM.md §2.4.2's dispersal). No oil. Kinds as `ecology`.
//! - `ships`: the synthetic coast's pirate haven (`rtsim_ships::synth::coastal`) through the fake
//!   ships plugin. Kind `ship.haven`.
//! - `conflict`: the goblin camp of the conflict's synthetic world. Kind `conflict.camp`.
//! - `camps`: the conflict's three pool cultures, one camp each (goblin, bandit, raider), between
//!   the six villages of a grid world ([`rtsim_conflict::synth::camps`], slice S7): quiet, nobody
//!   emitting or killing, so they only plunder. Kind `conflict.camp`, with pressure, breaks and
//!   refoundings once a player presses them.
//! - `villages`: phase 1's synthetic world (six human villages, three Ribbit villages) with the
//!   bridge faked at the world level ([`Villages`], slice S6): every village seen alive by one
//!   census, every house and resident answered the day it is ordered, nobody watching. Kinds are
//!   the villages' own, with K: a human village's beds, a Ribbit village's houses.
//!
//! The host soak prints the same lines from the native engine (`Engine::census`).

use std::process::ExitCode;

use rtsim_ecology::calendar::Calendar;
use rtsim_ecology::engine::Dragons;
use rtsim_ecology::herds::synth::FakeHerds;
use rtsim_ecology::herds::{HerdTunables, Herds};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, NoInfrastructure, RaidOutcome, Well};
use rtsim_ecology::synth::FakeIaf;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_industry::ids::{DistrictId, Good};
use rtsim_industry::threat::Threat;
use rtsim_kernel::census::{CensusLog, KindCensus};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_world::World;

/// Steps: a quarter day.
const STEP: u64 = TICKS_PER_DAY / 4;
/// The fake plugins' player (never present: the census runs with nobody watching).
const PLAYER: (u64, u64) = (0xCE5, 0x5005);
/// The chunks reported as oil in the ecology scenario: two nest cells, (12, 12) and (-19, 7).
pub const OIL_CHUNKS: [(i32, i32); 4] = [(100, 100), (101, 100), (100, 101), (-150, 60)];

/// A scenario's module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ecology,
    Ranges,
    Ships,
    Conflict,
    Camps,
    Villages,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Ecology,
        Kind::Ranges,
        Kind::Ships,
        Kind::Conflict,
        Kind::Camps,
        Kind::Villages,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Ecology => "ecology",
            Kind::Ranges => "ranges",
            Kind::Ships => "ships",
            Kind::Conflict => "conflict",
            Kind::Camps => "camps",
            Kind::Villages => "villages",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// The dragons, nests and herds on the phase 1 dragon world.
pub struct Ecology {
    pub w: World,
    pub d: Dragons,
    pub h: Herds,
    pub iaf: FakeIaf,
    pub herds: FakeHerds,
    pub wells: Wells,
}

/// The ecology scenario's only industry: an endless well beside each oil chunk's nest, so the
/// nests wake and keep their worms. No districts, so nothing is raided.
#[derive(Clone, Debug, Default)]
pub struct Wells(pub Vec<Well>);

impl Infrastructure for Wells {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        None
    }

    fn good_name(&self, _: Good) -> Option<String> {
        None
    }

    fn wells(&mut self) -> Vec<Well> {
        self.0.clone()
    }
}

/// The conflict's world and its one camp.
pub struct Camp {
    pub w: World,
    pub c: rtsim_conflict::Conflict,
}

/// The pirate haven's coast.
pub struct Sea {
    pub c: rtsim_ships::synth::Coastal,
    pub fake: rtsim_ships::synth::FakeShips,
}

/// One scenario's world and engines, run with nobody watching.
pub enum Scenario {
    Ecology(Box<Ecology>),
    Ships(Box<Sea>),
    Conflict(Box<Camp>),
    Villages(Box<Villages>),
}

impl Scenario {
    /// The `ships` scenario with tunables `tun`.
    pub fn ships(seed: u64, tun: rtsim_ships::ShipTunables) -> Self {
        let fake = rtsim_ships::synth::FakeShips::new(f64::from(tun.view_radius));
        Scenario::Ships(Box::new(Sea {
            c: rtsim_ships::synth::coastal(seed, tun),
            fake,
        }))
    }

    pub fn new(kind: Kind, seed: u64) -> Self {
        match kind {
            Kind::Ecology => Scenario::Ecology(Box::new(Ecology::new(seed))),
            Kind::Ranges => {
                Scenario::Ecology(Box::new(Ecology::ranges(seed, Ecology::herd_tunables())))
            }
            Kind::Ships => Self::ships(seed, rtsim_ships::ShipTunables::default()),
            Kind::Conflict => {
                use rtsim_conflict::synth;
                let (mut w, _) = synth::world(seed);
                let mut c = synth::conflict(seed, &w);
                c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
                    .expect("the goblin culture");
                Scenario::Conflict(Box::new(Camp { w, c }))
            }
            Kind::Camps => {
                let (w, c) = rtsim_conflict::synth::camps(
                    6,
                    &["goblin", "bandit", "raider"],
                    seed,
                    rtsim_conflict::Cultures::builtin(),
                );
                Scenario::Conflict(Box::new(Camp { w, c }))
            }
            Kind::Villages => Scenario::Villages(Box::new(Villages::new(seed))),
        }
    }

    /// Runs to `to` in quarter days, the fake plugins answering.
    pub fn run_to(&mut self, to: Tick) {
        match self {
            Scenario::Ecology(e) => e.run_to(to),
            Scenario::Ships(s) => rtsim_ships::synth::drive(
                &mut s.c.world,
                &mut s.c.ships,
                &mut s.fake,
                &mut NoInfrastructure,
                &s.c.mask,
                to,
                STEP,
            ),
            Scenario::Conflict(c) => {
                let mut t = c.c.now();
                while t < to {
                    t = (t + STEP).min(to);
                    c.c.run_until(&mut c.w, &mut NoInfrastructure, t);
                    c.w.run_until(t);
                }
            }
            Scenario::Villages(v) => v.run_to(to),
        }
    }

    /// Every kind's census now: the world's villages, then the module's sites.
    pub fn census(&self) -> Vec<KindCensus> {
        match self {
            Scenario::Ecology(e) => {
                let mut out = e.w.census();
                out.extend(e.d.census());
                out.extend(e.h.census());
                out
            }
            Scenario::Ships(s) => {
                let mut out = s.c.world.census();
                out.extend(s.c.ships.census(&s.c.world));
                out
            }
            Scenario::Conflict(c) => {
                let mut out = c.w.census();
                out.extend(c.c.census());
                out
            }
            Scenario::Villages(v) => v.w.census(),
        }
    }
}

/// The player who does the killing in the villages scenario (the tests' wipes).
#[cfg_attr(not(test), allow(dead_code))]
pub const VILLAGE_KILLER: (u64, u64) = (0xE1, 0x5A4);

/// The synthetic world's villages with the bridge faked at the world level (slice S6,
/// docs/overnight-choices-equilibrium.md "S6 tests"): the harness keeps the real residents of each
/// village, answers every order the world gives (a house stands, a resident exists, both the day
/// they are ordered), and feeds a village a census only when the script says so. Nobody holds any
/// village, so after the setup census the engine hears nothing and its growth steps run.
#[cfg_attr(not(test), allow(dead_code))]
pub struct Villages {
    pub w: World,
    nodes: std::collections::BTreeMap<u64, usize>,
    /// Real residents of each village node.
    pub real: std::collections::BTreeMap<usize, u32>,
    /// The beds each village's census reports (a Ribbit village's census reports none).
    beds: std::collections::BTreeMap<usize, u32>,
    /// The day each wiped village was first ordered settlers after its wipe.
    pub wiped_at: std::collections::BTreeMap<usize, u64>,
    pub first_pair: std::collections::BTreeMap<usize, u64>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl Villages {
    /// The world at day 1, every village counted once by a complete census (so the engine has seen
    /// each alive).
    pub fn new(seed: u64) -> Self {
        Self::with(seed, |_| {})
    }

    /// [`Villages::new`] with the world's configuration adjusted first.
    pub fn with(seed: u64, tweak: impl FnOnce(&mut rtsim_world::WorldConfig)) -> Self {
        use rtsim_trade::build::Genesis;
        use rtsim_world::{WorldConfig, synth};
        let mut cfg = WorldConfig::new(seed);
        cfg.check_every_event = false;
        cfg.genesis = Genesis::AtTarget;
        tweak(&mut cfg);
        let (mut w, _) = synth::build(cfg, true);
        let nodes = (0..w.nodes.len()).map(|i| (w.nodes[i].key, i)).collect();
        w.run_until(Tick::from_days(1));
        let mut real = std::collections::BTreeMap::new();
        let mut beds = std::collections::BTreeMap::new();
        for i in 0..w.nodes.len() {
            let rtsim_settle::NodeKind::Village(culture) = w.nodes[i].kind else {
                continue;
            };
            let pop = w.nodes[i].local.pop;
            let b = if culture == rtsim_settle::Culture::Ribbit {
                0
            } else {
                pop
            };
            real.insert(i, pop);
            beds.insert(i, b);
            w.census_village(i, pop, b, true, w.now());
        }
        Villages {
            w,
            nodes,
            real,
            beds,
            wiped_at: Default::default(),
            first_pair: Default::default(),
        }
    }

    /// Whether village node `i` is of `culture`.
    pub fn is(&self, i: usize, culture: rtsim_settle::Culture) -> bool {
        self.w.nodes[i].kind == rtsim_settle::NodeKind::Village(culture)
    }

    /// A player kills every resident of village `i` and the engine's next two complete censuses
    /// read it empty.
    pub fn wipe(&mut self, i: usize) {
        use rtsim_world::inputs::{Input, apply};
        let key = self.w.nodes[i].key;
        let now = self.w.now().0;
        for _ in 0..self.real[&i] {
            let report = Input::VillageReport {
                key,
                player: VILLAGE_KILLER,
                kind: 8,
            };
            apply(&mut self.w, &mut self.nodes, &report, now);
        }
        self.real.insert(i, 0);
        for _ in 0..2 {
            self.w
                .census_village(i, 0, self.beds[&i], true, self.w.now());
        }
        self.wiped_at.insert(i, now / TICKS_PER_DAY);
    }

    /// Runs a day at a time to `to`, answering what the world orders.
    pub fn run_to(&mut self, to: Tick) {
        let mut day = self.w.now().0 / TICKS_PER_DAY;
        while self.w.now() < to {
            day += 1;
            self.w.run_until(Tick::from_days(day).min(to));
            self.answer(day);
        }
    }

    /// The bridge: every house stands and every resident exists the day it is ordered.
    fn answer(&mut self, day: u64) {
        use rtsim_world::Command;
        for _ in 0..4 {
            let cmds = self.w.take_commands();
            if cmds.is_empty() {
                break;
            }
            for c in cmds {
                match c {
                    Command::PlaceHouse(h) => {
                        self.w.house_placed(h.op);
                    }
                    Command::SpawnResident(r) => {
                        let Some(i) = self
                            .w
                            .node_of_key(r.village)
                            .filter(|i| self.real.contains_key(i))
                        else {
                            continue;
                        };
                        let now = self.w.now();
                        self.w.resident_result(r.op, true, now);
                        *self.real.get_mut(&i).expect("a village") += 1;
                        if let (Some(at), rtsim_world::Arrive::Settler { .. }) =
                            (self.wiped_at.get(&i), r.arrive)
                        {
                            self.first_pair.entry(i).or_insert(day - at);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// The ranges scenario's grid: regions x -3 to 4 and z -3 to 3 (56 ranges), under the lairs.
pub const RANGES_X: std::ops::RangeInclusive<i32> = -3..=4;
pub const RANGES_Z: std::ops::RangeInclusive<i32> = -3..=3;

impl Ecology {
    /// The herds' tunables the scenarios use (the default calendar).
    pub fn herd_tunables() -> HerdTunables {
        HerdTunables {
            calendar: Calendar::default(),
            ..HerdTunables::default()
        }
    }

    /// The ecology scenario: the herd valley's six herds, oil under two nests.
    pub fn new(seed: u64) -> Self {
        let h = rtsim_ecology::herds::synth::valley(seed, Self::herd_tunables(), Tick::ZERO);
        Self::with_herds(seed, h, true)
    }

    /// The ranges scenario: [`RANGES_X`] x [`RANGES_Z`] herd ranges, no oil.
    pub fn ranges(seed: u64, htun: HerdTunables) -> Self {
        Self::ranges_in(seed, htun, RANGES_X, RANGES_Z)
    }

    /// The ranges scenario's world over the herd ranges of regions `rx` x `rz`.
    pub fn ranges_in(
        seed: u64,
        htun: HerdTunables,
        rx: std::ops::RangeInclusive<i32>,
        rz: std::ops::RangeInclusive<i32>,
    ) -> Self {
        let h = rtsim_ecology::herds::synth::ranges(seed, htun, Tick::ZERO, rx, rz);
        Self::with_herds(seed, h, false)
    }

    /// The phase 1 dragon world over the herds `h`, with oil under two nests if `oil`.
    pub fn with_herds(seed: u64, h: Herds, oil: bool) -> Self {
        use rtsim_ecology::boundary::{self, Obs, OilChunkRec, OilChunks};
        use rtsim_ecology::synth;
        let (mut w, _) = synth::world(seed);
        let tun = DragonTunables {
            calendar: Calendar::default(),
            ..DragonTunables::default()
        };
        let mut d = Dragons::new(seed, tun, Tick::ZERO);
        synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
        let mut e = Ecology {
            w,
            d,
            h,
            iaf: FakeIaf::new(PLAYER),
            herds: FakeHerds::default(),
            wells: Wells(
                [(100i64, 102i64), (-150, 62)]
                    .iter()
                    .enumerate()
                    .map(|(i, &(cx, cz))| Well {
                        district: DistrictId(9 + i as u64),
                        frame: 0,
                        x: cx * 16 + 8,
                        z: cz * 16 + 8,
                        remaining: 0,
                        infinite: true,
                    })
                    .collect(),
            ),
        };
        let oil_obs = Obs::OilChunks(OilChunks {
            dimension: 0,
            source: boundary::OIL_SOURCE_CDG,
            chunks: OIL_CHUNKS
                .iter()
                .map(|&(cx, cz)| OilChunkRec {
                    cx,
                    cz,
                    amount: 1,
                    flags: 0,
                })
                .collect(),
        });
        if oil {
            e.d.ingest(&mut e.w, &mut e.wells, &oil_obs, Tick::ZERO);
        } else {
            e.wells.0.clear();
        }
        e.pump(Tick::ZERO);
        e
    }

    /// The engines' commands to the fake plugins and their answers due by `at`, until quiet.
    pub fn pump(&mut self, at: Tick) {
        loop {
            let cmds = self.d.take_commands();
            let quiet = cmds.is_empty();
            self.iaf.apply(cmds, at);
            let due = self.iaf.due(at);
            if quiet && due.is_empty() {
                break;
            }
            for (t, o) in due {
                let t = t.max(self.d.now());
                self.d.ingest(&mut self.w, &mut self.wells, &o, t);
            }
        }
        let cmds = self.h.take_commands();
        self.herds.apply(cmds, at);
        self.herds.walk(&self.h, at);
        for o in self.herds.take_obs() {
            self.h.ingest(&mut self.w.legs, &o, at);
        }
    }

    pub fn run_to(&mut self, to: Tick) {
        let mut t = self.d.now();
        while t < to {
            t = (t + STEP).min(to);
            self.d
                .run_until_with_herds(&mut self.w, &mut self.wells, &mut self.h, t);
            self.pump(t);
        }
    }
}

/// A census every `every` days from day 0 to `days`, with nobody watching: (day, every kind).
pub fn run(kind: Kind, seed: u64, days: u64, every: u64) -> Vec<(u64, Vec<KindCensus>)> {
    run_on(Scenario::new(kind, seed), days, every)
}

/// [`run`] on a scenario already built.
pub fn run_on(mut s: Scenario, days: u64, every: u64) -> Vec<(u64, Vec<KindCensus>)> {
    let mut out = vec![(0, s.census())];
    let mut d = 0;
    while d < days {
        d = (d + every.max(1)).min(days);
        s.run_to(Tick::from_days(d));
        out.push((d, s.census()));
    }
    out
}

pub fn cli(rest: &[&str]) -> ExitCode {
    let (mut seed, mut days, mut every) = (7u64, 960u64, 24u64);
    let mut kinds: Vec<Kind> = Kind::ALL.to_vec();
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match a {
            "--seed" => match num(it.next()) {
                Some(n) => seed = n,
                None => return bad("--seed needs a number"),
            },
            "--days" => match num(it.next()) {
                Some(n) if n > 0 => days = n,
                _ => return bad("--days needs a number above 0"),
            },
            "--every" => match num(it.next()) {
                Some(n) if n > 0 => every = n,
                _ => return bad("--every needs a number above 0"),
            },
            "--scenario" => {
                let Some(v) = it.next() else {
                    return bad("--scenario needs a name");
                };
                if *v == "all" {
                    kinds = Kind::ALL.to_vec();
                    continue;
                }
                let mut ks = Vec::new();
                for n in v.split(',') {
                    match Kind::parse(n) {
                        Some(k) => ks.push(k),
                        None => return bad(&format!("unknown scenario {n}")),
                    }
                }
                kinds = ks;
            }
            other => return bad(&format!("unknown option {other}")),
        }
    }
    println!("# rtsim-cli sim census: seed {seed}, {days} days, every {every}, nobody watching");
    for k in kinds {
        println!("# scenario {}", k.name());
        println!("{}", CensusLog::HEADER);
        let mut log = CensusLog::new();
        for (day, cs) in run(k, seed, days, every) {
            for l in log.lines(day, &cs) {
                println!("{l}");
            }
        }
    }
    ExitCode::SUCCESS
}

fn bad(why: &str) -> ExitCode {
    eprintln!(
        "{why}\nusage: rtsim-cli sim census [--scenario all|ecology,ranges,ships,conflict,camps,villages] [--days N] \
         [--every N] [--seed S]"
    );
    ExitCode::from(64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind<'a>(cs: &'a [KindCensus], k: &str) -> &'a KindCensus {
        cs.iter()
            .find(|c| c.kind == k)
            .unwrap_or_else(|| panic!("no {k} in {cs:?}"))
    }

    fn kinds(cs: &[KindCensus]) -> Vec<&'static str> {
        cs.iter().map(|c| c.kind).collect()
    }

    /// Every module's kinds are counted, and each count agrees with the engine read another way:
    /// the scenario's lairs, nests, herds, haven slots, camp and villages. A player's losses show
    /// as short or empty sites with pressure (a nest's worms killed, a haven's pirates sunk).
    #[test]
    fn census_counts_every_kind() {
        // Ecology.
        let mut s = Scenario::new(Kind::Ecology, 3);
        s.run_to(Tick::from_days(6));
        let cs = s.census();
        assert_eq!(
            kinds(&cs),
            [
                "village.human",
                "village.ribbit",
                "dragon.lair",
                "oil.nest",
                "herd.range"
            ]
        );
        let Scenario::Ecology(e) = &mut s else {
            unreachable!()
        };
        let human = rtsim_ecology::synth::nodes()
            .iter()
            .filter(|n| n.kind == rtsim_settle::NodeKind::Village(rtsim_settle::Culture::Human))
            .map(|n| (1u32, u64::from(n.pop)))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        let v = kind(&cs, "village.human");
        assert_eq!((v.sites, v.members), human);
        let lairs = rtsim_ecology::synth::lairs();
        let l = kind(&cs, "dragon.lair");
        assert_eq!(l.sites as usize, lairs.len());
        assert_eq!(l.capacity as usize, lairs.len());
        let homed: usize = e.d.lairs.iter().map(|x| x.occupants.len()).sum();
        assert_eq!(l.members as usize, homed);
        assert!(l.members > 0);
        let n = kind(&cs, "oil.nest");
        assert_eq!((n.sites, n.capacity, n.broken), (2, 4, 0));
        assert_eq!(n.members, 4, "both nests hold their K: {n:?}");
        assert_eq!(n.mean_p(), 0.0);
        let h = kind(&cs, "herd.range");
        let alive: u64 = e.h.totals(e.h.now()).iter().sum();
        assert_eq!(
            h.sites as usize,
            rtsim_ecology::herds::synth::scenario_herds().len()
        );
        assert_eq!(h.members, alive);
        assert!(h.capacity > 0 && h.empty == 0);
        // A player kills both worms of one nest: it is empty, and its pressure is up.
        let id = e.d.nests.ids()[0];
        let at = e.d.now();
        e.iaf.kill_worms(id, 2, at);
        e.pump(at);
        let n = kind(&s.census(), "oil.nest").clone();
        assert_eq!((n.members, n.empty, n.short), (2, 1, 0), "{n:?}");
        assert!(n.mean_p() > 0.4, "{n:?}");

        // Ships: one haven of two slots, manned; a player sinks both.
        let mut s = Scenario::new(Kind::Ships, 3);
        s.run_to(Tick::from_days(2));
        let c = kind(&s.census(), "ship.haven").clone();
        assert_eq!((c.sites, c.capacity, c.members), (1, 2, 2), "{c:?}");
        assert_eq!(c.mean_p(), 0.0);
        let Scenario::Ships(sea) = &mut s else {
            unreachable!()
        };
        // Ports (slice S11): every trade port a site of K 1, open.
        let trade = (0..sea.c.ships.docks.len() as u32)
            .filter(|&d| sea.c.ships.is_trade_port(&sea.c.world, d))
            .count() as u32;
        let p = kind(&sea.c.ships.census(&sea.c.world), "ship.port").clone();
        assert!(trade > 0);
        assert_eq!(
            (p.sites, p.capacity, p.members, p.empty, p.broken),
            (trade, u64::from(trade), u64::from(trade), 0, 0),
            "{p:?}"
        );
        let at = sea.c.ships.now();
        let h = sea.c.haven as usize;
        let mut op = 0u64;
        for p in sea.c.ships.havens[h].ships.clone() {
            let Some(x) = sea.c.ships.ship(p).filter(|x| x.is_active()) else {
                continue;
            };
            op += 1;
            let o = rtsim_ships::boundary::Obs::Sunk(rtsim_ships::boundary::Sunk {
                agent: rtsim_ships::engine::Ship::agent(p),
                epoch: x.epoch,
                by: rtsim_ships::boundary::By::Player,
                dimension: 0,
                player: PLAYER,
                ship: 0,
                op: 1 << 62 | op,
                pos: (0.0, 63.0, 0.0),
            });
            sea.c.ships.ingest(&mut sea.c.world, &o, &sea.c.mask, at);
        }
        let c = kind(&s.census(), "ship.haven").clone();
        assert_eq!((c.members, c.empty), (0, 1), "{c:?}");
        assert!(
            (c.mean_p() - 1.0).abs() < 1e-3,
            "two of two slots sunk: {c:?}"
        );

        // Conflict: one camp, its pool at the culture's cap.
        let mut s = Scenario::new(Kind::Conflict, 3);
        s.run_to(Tick::from_days(2));
        let cs = s.census();
        assert_eq!(
            kinds(&cs),
            ["village.human", "village.ribbit", "conflict.camp"]
        );
        let c = kind(&cs, "conflict.camp");
        assert_eq!(c.sites, 1);
        assert!(c.capacity > 0 && c.members <= c.capacity, "{c:?}");
    }

    /// The villages scenario counts each village by its K (slice S6): a human village's beds, a
    /// Ribbit village's houses. A player wiping a village shows as an empty site with pressure
    /// 1.00 (twelve kills at K 12) and no change in its K, and its settlers, 32 days later, as a
    /// short one.
    #[test]
    fn census_counts_villages_by_k() {
        let mut s = Scenario::new(Kind::Villages, 3);
        s.run_to(Tick::from_days(6));
        let cs = s.census();
        assert_eq!(kinds(&cs), ["village.human", "village.ribbit"]);
        let h = kind(&cs, "village.human").clone();
        assert_eq!(
            (h.sites, h.members, h.capacity, h.empty, h.short),
            (6, 89, 89, 0, 0),
            "{h:?}"
        );
        let r = kind(&cs, "village.ribbit").clone();
        assert_eq!((r.sites, r.empty), (3, 0), "{r:?}");
        assert!(r.members >= 32 && r.members <= r.capacity, "{r:?}");
        let Scenario::Villages(v) = &mut s else {
            unreachable!()
        };
        v.wipe(2);
        let h = kind(&s.census(), "village.human").clone();
        assert_eq!((h.members, h.capacity, h.empty), (77, 89, 1), "{h:?}");
        assert!(
            (h.mean_p() - 1.0 / 6.0).abs() < 1e-3,
            "twelve kills at K 12 over six villages: {h:?}"
        );
        s.run_to(Tick::from_days(6 + 33));
        let h = kind(&s.census(), "village.human").clone();
        assert_eq!(
            (h.members, h.capacity, h.empty, h.short),
            (79, 89, 0, 1),
            "{h:?}"
        );
    }

    /// The `camps` scenario holds the three pool cultures' camps, and a player's kills show in the
    /// census as pressure (slice S7).
    #[test]
    fn the_camps_scenario_shows_a_players_pressure() {
        let mut s = Scenario::new(Kind::Camps, 4);
        s.run_to(Tick::from_days(10));
        let k = kind(&s.census(), "conflict.camp").clone();
        assert_eq!((k.sites, k.broken, k.renewals), (3, 0, 0), "{k:?}");
        assert_eq!(k.mean_p(), 0.0);
        let Scenario::Conflict(c) = &mut s else {
            panic!("the camps scenario is the conflict's");
        };
        let slain = c.c.slay(&mut c.w, 0, u32::MAX, Tick::from_days(10));
        assert!(slain >= 40, "{slain}");
        let k = kind(&s.census(), "conflict.camp").clone();
        assert!(k.mean_p() > 0.2, "one camp at P 1 of three: {}", k.mean_p());
        assert_eq!(k.sites, 3);
    }

    /// With nobody watching nothing presses: every kind keeps its sites, every pressure is 0, no
    /// site breaks, and from the first year on each kind's members stay between a quarter of its
    /// capacity and twice its start. Every kind runs at its defaults, the coast's navy assaults
    /// included: a navy breaks a haven with no player (§2.4.6a rule 3, Ben's exception for
    /// havens), so `ship.haven` may count a broken site, with no pressure, and while it is broken
    /// its slots may stand empty until its site refounds (choices-ports.md PC34, PC38).
    #[test]
    fn census_is_flat_with_no_player() {
        for k in Kind::ALL {
            let rows = run(k, 5, 480, 24);
            let first = &rows[0].1;
            for (day, cs) in &rows {
                assert_eq!(kinds(cs), kinds(first), "{} day {day}", k.name());
                for (c, c0) in cs.iter().zip(first) {
                    let at = format!("{} day {day}: {c:?}", k.name());
                    let navy = c.kind == "ship.haven" && c.broken > 0;
                    assert_eq!(c.sites, c0.sites, "{at}");
                    assert!(c.broken == 0 || navy, "{at}");
                    assert_eq!(c.mean_p(), 0.0, "{at}");
                    assert!(c.members <= 2 * c0.members.max(c.capacity) + 4, "{at}");
                    if *day >= 96 && !navy {
                        assert!(4 * c.members >= c.capacity, "{at}");
                    }
                }
            }
        }
    }
}
