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
//! - `ships`: the synthetic coast's pirate haven (`rtsim_ships::synth::coastal`) through the fake
//!   ships plugin. Kind `ship.haven`.
//! - `conflict`: the goblin camp of the conflict's synthetic world. Kind `conflict.camp`.
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
    Ships,
    Conflict,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Ecology, Kind::Ships, Kind::Conflict];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Ecology => "ecology",
            Kind::Ships => "ships",
            Kind::Conflict => "conflict",
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
}

impl Scenario {
    pub fn new(kind: Kind, seed: u64) -> Self {
        match kind {
            Kind::Ecology => Scenario::Ecology(Box::new(Ecology::new(seed))),
            Kind::Ships => {
                let tun = rtsim_ships::ShipTunables::default();
                let fake = rtsim_ships::synth::FakeShips::new(f64::from(tun.view_radius));
                Scenario::Ships(Box::new(Sea {
                    c: rtsim_ships::synth::coastal(seed, tun),
                    fake,
                }))
            }
            Kind::Conflict => {
                use rtsim_conflict::synth;
                let (mut w, _) = synth::world(seed);
                let mut c = synth::conflict(seed, &w);
                c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
                    .expect("the goblin culture");
                Scenario::Conflict(Box::new(Camp { w, c }))
            }
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
                out.extend(s.c.ships.census());
                out
            }
            Scenario::Conflict(c) => {
                let mut out = c.w.census();
                out.extend(c.c.census());
                out
            }
        }
    }
}

impl Ecology {
    pub fn new(seed: u64) -> Self {
        use rtsim_ecology::boundary::{self, Obs, OilChunkRec, OilChunks};
        use rtsim_ecology::synth;
        let (mut w, _) = synth::world(seed);
        let tun = DragonTunables {
            calendar: Calendar::default(),
            ..DragonTunables::default()
        };
        let mut d = Dragons::new(seed, tun, Tick::ZERO);
        synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
        let htun = HerdTunables {
            calendar: Calendar::default(),
            ..HerdTunables::default()
        };
        let h = rtsim_ecology::herds::synth::valley(seed, htun, Tick::ZERO);
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
        let oil = Obs::OilChunks(OilChunks {
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
        e.d.ingest(&mut e.w, &mut e.wells, &oil, Tick::ZERO);
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
    let mut s = Scenario::new(kind, seed);
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
        "{why}\nusage: rtsim-cli sim census [--scenario all|ecology,ships,conflict] [--days N] \
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

    /// With nobody watching nothing presses: every kind keeps its sites, no site breaks, every
    /// pressure is 0, and from the first year on each kind's members stay between a quarter of
    /// its capacity and twice its start (herds grow past their founding count until slice S2's
    /// density term).
    #[test]
    fn census_is_flat_with_no_player() {
        for k in Kind::ALL {
            let rows = run(k, 5, 480, 24);
            let first = &rows[0].1;
            for (day, cs) in &rows {
                assert_eq!(kinds(cs), kinds(first), "{} day {day}", k.name());
                for (c, c0) in cs.iter().zip(first) {
                    let at = format!("{} day {day}: {c:?}", k.name());
                    assert_eq!(c.sites, c0.sites, "{at}");
                    assert_eq!(c.broken, 0, "{at}");
                    assert_eq!(c.mean_p(), 0.0, "{at}");
                    assert!(c.members <= 2 * c0.members.max(c.capacity) + 4, "{at}");
                    if *day >= 96 {
                        assert!(4 * c.members >= c.capacity, "{at}");
                    }
                }
            }
        }
    }
}
