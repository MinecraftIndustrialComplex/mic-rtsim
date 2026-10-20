// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli bench scale` (SCALE.md §10): sweeps that vary one count at a time across orders of
//! magnitude, measure cost, fit growth slopes, and check them against declared contracts.
//!
//! **One process per configuration.** Each point runs in a fresh child process (`bench
//! scale-one`), so resident memory and heap peaks belong to that configuration alone, and one
//! configuration's allocator state never leaks into the next.
//!
//! **Slopes.** For each sweep and metric, the slope is the least-squares fit of log10(metric)
//! against log10(count). A slope of 0 means flat; 1 means linear.
//!
//! **Contracts.** Each sweep declares the slope it must not exceed (`declared`), next to what
//! SCALE.md §10 asks for (`scale_md`). Where the two differ, the note says why, and DECISIONS.md
//! records it as an overnight choice for Ben. `--check` fails when a measured slope exceeds its
//! declared one. Declared slopes are design contracts, not measurements; the tolerances on them
//! are PLACEHOLDERS until Ben sets targets (P24).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::lazy::LazyTable;
use rtsim_kernel::rng::SystemId;
use rtsim_kernel::synth::{DyadicRegion, Stock, SynthRegion};
use serde::{Deserialize, Serialize};

use super::paged::{PagedSpec, PagedWorld};
use super::world::{BenchModel, ModelKind, World, WorldConfig, scratch_store};
use crate::sys;

/// What one child process runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Spec {
    /// Run a world for some days, then save it.
    World(WorldConfig),
    /// Build `regions` regions, leave them unread for `elapsed_days`, then read each once.
    CatchUp(CatchUpConfig),
    /// Run a world on paged regions under a memory budget (A16), saving as Minecraft does.
    Paged(PagedSpec),
    /// A phase 1 market world (`bench::market`).
    Market(super::market::MarketConfig),
    /// Dragons over a fixed village grid, nobody watching (`bench::dragons`).
    Dragons(super::dragons::DragonConfig),
    /// Merchant ships over a fixed ocean grid of ports, nobody watching (`bench::ships`).
    Ships(super::ships::ShipsConfig),
    /// Migrating herds in the synthetic valley, nobody watching (`bench::herds`).
    Herds(super::herds::HerdsConfig),
    /// Conflict factions, one village per camp, nobody watching (`bench::conflict`).
    Factions(super::conflict::FactionConfig),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatchUpConfig {
    pub model: ModelKind,
    pub regions: u64,
    pub latent_per_region: u64,
    pub elapsed_days: u64,
    pub seed: u64,
}

/// What one child process measured.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Measured {
    pub cpu_ns_per_day: Option<f64>,
    pub wall_ns_per_day: Option<f64>,
    pub catch_up_cpu_ns_per_read: Option<f64>,
    pub catch_up_wall_ns_per_read: Option<f64>,
    pub commits_per_read: Option<f64>,
    /// Live heap bytes at the end of the run, from the counting allocator.
    pub heap_live_bytes: u64,
    pub heap_peak_bytes: u64,
    /// What the engine's own structures say they hold.
    pub engine_heap_bytes: u64,
    pub rss_bytes: Option<u64>,
    pub rss_peak_bytes: Option<u64>,
    pub save_full_bytes: Option<u64>,
    pub save_full_ms: Option<f64>,
    pub save_incremental_bytes: Option<u64>,
    pub save_incremental_pages: Option<u64>,
    pub save_incremental_ms: Option<f64>,
    pub reads: u64,
    pub events: u64,
    pub slot_commits: u64,
    pub caravans: u64,
    pub conservation: String,
    /// Invariants this run broke: conservation, model invariants, the eager bound on slot
    /// commits. Any entry fails `--check`.
    pub failures: Vec<String>,
    pub state_hash: Option<u32>,
    /// Paged runs only (A16): the table's resident page bytes at the end and at their peak in
    /// the measured window, the budget, and the page store's always-resident index.
    pub resident_bytes: Option<u64>,
    pub resident_peak_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub store_index_bytes: Option<u64>,
    /// Paged runs: per sim day, pages loaded on demand, evicted, read ahead (and of those, used).
    pub loads_per_day: Option<f64>,
    pub evictions_per_day: Option<f64>,
    pub prefetches_per_day: Option<f64>,
    pub prefetch_hits_per_day: Option<f64>,
    /// Paged runs: budget checks that ended over budget (every remaining page dirty).
    pub over_budget_checks: Option<u64>,
    /// Paged runs: the periodic saves in the measured window: wall time per sim day, and bytes
    /// (segment plus log record) per save.
    pub save_ms_per_day: Option<f64>,
    /// Paged runs: thread CPU inside those saves, per sim day. `cpu_ns_per_day` excludes it.
    pub save_cpu_ms_per_day: Option<f64>,
    pub save_bytes_per_save: Option<f64>,
    pub save_log_bytes_per_save: Option<f64>,
    pub checkpoints: Option<u64>,
    /// Edges in a market world.
    #[serde(default)]
    pub edges: Option<u64>,
    /// CPU per sim day per unit of the swept count, beyond what the world costs without it (the
    /// dragon sweep: per dragon).
    #[serde(default)]
    pub cpu_ns_per_unit_day: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vary {
    Latent,
    Regions,
    Events,
    Elapsed,
    /// World age: days the world has run before the measured window.
    Age,
    /// Markets in a phase 1 world.
    Markets,
    /// The edge rule's nearest-k, with markets fixed.
    Edges,
    /// Dragons over a fixed world (PLAN.md §9).
    Dragons,
    /// Merchant ships per service over a fixed ocean grid (PLAN.md §10.1).
    Ships,
    /// Herds in the valley, members per herd fixed (PLAN.md §10.2).
    Herds,
    /// Members per herd, herd count fixed (PLAN.md §10.2).
    Members,
    /// Conflict factions, one village per camp (PLAN.md §10.3).
    Factions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// CPU per sim day.
    Cpu,
    /// Live heap at the end of the run.
    Heap,
    /// Bytes of a full save.
    Save,
    /// CPU per catch-up read.
    CatchUp,
    /// Paged runs: peak resident page bytes in the measured window.
    Resident,
    /// Paged runs: bytes per periodic save (segment plus log record).
    SaveIncr,
    /// CPU per sim day per unit of the swept count (`Measured::cpu_ns_per_unit_day`).
    CpuPerUnit,
    /// Events handled per unit of the swept count over the window (`events / reads`; the
    /// faction sweep sets `reads` to the faction count): the algorithmic part of a per-unit cost,
    /// free of cache effects.
    EventsPerUnit,
}

impl Metric {
    fn of(self, m: &Measured) -> Option<f64> {
        match self {
            Metric::Cpu => m.cpu_ns_per_day,
            Metric::Heap => Some(m.heap_live_bytes as f64),
            Metric::Save => m.save_full_bytes.map(|b| b as f64),
            Metric::CatchUp => m.catch_up_cpu_ns_per_read,
            Metric::Resident => m.resident_peak_bytes.map(|b| b as f64),
            Metric::SaveIncr => m.save_bytes_per_save,
            Metric::CpuPerUnit => m.cpu_ns_per_unit_day,
            Metric::EventsPerUnit => (m.reads > 0).then(|| m.events as f64 / m.reads as f64),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contract {
    pub metric: Metric,
    /// The slope this code claims and `--check` enforces.
    pub declared: f64,
    /// What SCALE.md §10 asks for.
    pub scale_md: f64,
    /// For flat contracts: the most the metric may vary across the sweep (max / min). A slope of
    /// 0.1 alone would allow a 6.3x rise over eight decades.
    pub max_ratio: Option<f64>,
    pub note: String,
}

/// The max / min bound on flat contracts. PLACEHOLDER: the latent sweep's CPU varies about 3x
/// where the binomial sampler moves from inversion to BTRS (bench/results/README.md).
const FLAT_RATIO: f64 = 4.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sweep {
    pub name: String,
    pub model: ModelKind,
    pub vary: Vary,
    pub values: Vec<u64>,
    /// A region count that replaces the baseline's for this sweep, with the reason in the
    /// contracts' notes.
    pub regions: Option<u64>,
    /// Run on paged regions at the baseline's budget (A16).
    #[serde(default)]
    pub paged: bool,
    pub contracts: Vec<Contract>,
}

/// Counts held fixed while another varies. PLACEHOLDER: chosen to run in seconds on a laptop, not
/// to model a target world (P24).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Baseline {
    pub regions: u64,
    pub latent: u64,
    pub events_per_day: u64,
    pub observers: u64,
    pub regions_per_page: u64,
    pub warmup_days: u64,
    pub days: u64,
    /// Regions read per catch-up point. Closed-form reads cost tens of nanoseconds, so they need
    /// many to time; slot reads at 10,000 days cost milliseconds, so they get few.
    pub catch_up_regions_closed: u64,
    pub catch_up_regions_herd: u64,
    pub catch_up_latent_per_region: u64,
    pub catch_up_elapsed_days: u64,
    pub seed: u64,
    /// Paged sweeps: the resident budget. PLACEHOLDER: small enough that every point but the
    /// first pages, not a target (A16, P24).
    pub paged_budget_bytes: u64,
    /// Paged sweeps: ticks between saves, Minecraft's autosave interval.
    pub save_every_ticks: u64,
}

impl Baseline {
    fn new(quick: bool) -> Self {
        Baseline {
            regions: 10_000,
            latent: 10_000_000,
            events_per_day: 1_000,
            observers: 1,
            regions_per_page: 64,
            warmup_days: 1,
            days: if quick { 3 } else { 10 },
            catch_up_regions_closed: if quick { 16_384 } else { 65_536 },
            catch_up_regions_herd: if quick { 64 } else { 256 },
            catch_up_latent_per_region: 1_000,
            catch_up_elapsed_days: 100,
            seed: 0x5CA1_E000,
            paged_budget_bytes: 4 << 20,
            save_every_ticks: 6_000,
        }
    }

    fn spec(&self, sweep: &Sweep, value: u64) -> Spec {
        if sweep.vary == Vary::Factions {
            return Spec::Factions(super::conflict::FactionConfig {
                factions: value,
                // One village per camp: the sweep measures cost per faction at a fixed density.
                villages: value.max(FACTION_VILLAGES_MIN),
                warmup_days: 2,
                // A faction's events are days apart: four times the baseline's days.
                days: self.days * 4,
                seed: self.seed,
            });
        }
        if matches!(sweep.vary, Vary::Herds | Vary::Members) {
            let members = sweep.vary == Vary::Members;
            return Spec::Herds(super::herds::HerdsConfig {
                herds: if members { HERD_BASE } else { value },
                members: if members { value } else { MEMBER_BASE },
                warmup_days: 8,
                // A decision per herd per sub-season (8 days): three sub-seasons' worth per point.
                days: self.days.max(3) * 8,
                gap_days: 10_000,
                seed: self.seed,
            });
        }
        if sweep.vary == Vary::Ships {
            return Spec::Ships(super::ships::ShipsConfig {
                ships_per_service: value,
                side: SHIP_GRID_SIDE,
                warmup_days: 2,
                // A few pirate events a day are all there is to time: five times the baseline's
                // days, so each point times enough of them to be read above noise.
                days: self.days * 5,
                seed: self.seed,
            });
        }
        if sweep.vary == Vary::Dragons {
            return Spec::Dragons(super::dragons::DragonConfig {
                dragons: value,
                villages: DRAGON_VILLAGES,
                warmup_days: 2,
                days: self.days,
                seed: self.seed,
            });
        }
        if matches!(sweep.vary, Vary::Markets | Vary::Edges) {
            return Spec::Market(super::market::MarketConfig {
                markets: if sweep.vary == Vary::Markets {
                    value
                } else {
                    MARKET_BASE
                },
                nearest: if sweep.vary == Vary::Edges { value } else { 4 },
                warmup_days: 10,
                days: self.days,
                seed: self.seed,
            });
        }
        if sweep.vary == Vary::Elapsed {
            return Spec::CatchUp(CatchUpConfig {
                model: sweep.model,
                regions: match sweep.model {
                    ModelKind::Closed => self.catch_up_regions_closed,
                    ModelKind::Herd | ModelKind::Dyadic => self.catch_up_regions_herd,
                },
                latent_per_region: self.catch_up_latent_per_region,
                elapsed_days: value,
                seed: self.seed,
            });
        }
        let mut c = WorldConfig {
            model: sweep.model,
            regions: sweep.regions.unwrap_or(self.regions),
            latent: self.latent,
            events_per_day: self.events_per_day,
            observers: self.observers,
            regions_per_page: self.regions_per_page,
            warmup_days: self.warmup_days,
            days: self.days,
            seed: self.seed,
        };
        match sweep.vary {
            Vary::Latent => c.latent = value,
            Vary::Regions => c.regions = value,
            Vary::Events => c.events_per_day = value,
            Vary::Age => c.warmup_days = value,
            Vary::Elapsed
            | Vary::Markets
            | Vary::Edges
            | Vary::Dragons
            | Vary::Ships
            | Vary::Herds
            | Vary::Members
            | Vary::Factions => {
                unreachable!()
            }
        }
        if sweep.paged {
            return Spec::Paged(PagedSpec {
                world: c,
                budget_bytes: self.paged_budget_bytes,
                save_every: self.save_every_ticks,
            });
        }
        Spec::World(c)
    }
}

/// A contract; ones that declare flat growth (slope 0.15 or less) also get the ratio bound.
fn contract(metric: Metric, declared: f64, scale_md: f64, note: &str) -> Contract {
    Contract {
        metric,
        declared,
        scale_md,
        max_ratio: (declared <= 0.15).then_some(FLAT_RATIO),
        note: note.to_string(),
    }
}

/// The sweeps and their contracts. `quick` keeps the ends of each range and fewer points between.
pub fn sweeps(quick: bool) -> Vec<Sweep> {
    let pick = |full: &[u64], q: &[u64]| if quick { q.to_vec() } else { full.to_vec() };
    // Tolerance on a flat contract: noise from a loaded machine, not growth. PLACEHOLDER.
    let flat = 0.1;
    vec![
        Sweep {
            name: "latent".into(),
            model: ModelKind::Herd,
            vary: Vary::Latent,
            regions: None,
            paged: false,
            values: pick(
                &[
                    10_000,
                    1_000_000,
                    100_000_000,
                    10_000_000_000,
                    1_000_000_000_000,
                ],
                &[10_000, 100_000_000, 1_000_000_000_000],
            ),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    flat,
                    0.0,
                    "latent members are counts; each cohort step is one O(1) binomial draw",
                ),
                contract(Metric::Heap, flat, 0.0, "latent members cost zero bytes"),
                contract(
                    Metric::Save,
                    flat,
                    0.0,
                    "counts grow by varint bytes only, log of the count",
                ),
            ],
        },
        Sweep {
            name: "regions".into(),
            model: ModelKind::Closed,
            vary: Vary::Regions,
            regions: None,
            paged: false,
            values: pick(
                &[100, 1_000, 10_000, 100_000, 1_000_000],
                &[100, 10_000, 1_000_000],
            ),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    0.15,
                    0.0,
                    "closed-form regions cost nothing unless touched",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "this sweep keeps every region resident (LazyTable); the paged sweeps measure A16's budget",
                ),
                contract(Metric::Save, 1.1, 1.0, "a full save writes every region"),
            ],
        },
        Sweep {
            name: "regions".into(),
            model: ModelKind::Herd,
            vary: Vary::Regions,
            regions: None,
            paged: false,
            values: pick(
                &[100, 1_000, 10_000, 100_000, 1_000_000],
                &[100, 10_000, 1_000_000],
            ),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    1.0,
                    0.0,
                    "slot commits are O(elapsed slots) per touch (SCALE §4), so spreading the same events over more regions pays more slots; bounded by eager stepping. DECISIONS O5",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "this sweep keeps every region resident (LazyTable); the paged sweeps measure A16's budget",
                ),
                contract(Metric::Save, 1.1, 1.0, "a full save writes every region"),
            ],
        },
        Sweep {
            name: "events".into(),
            model: ModelKind::Herd,
            vary: Vary::Events,
            regions: None,
            paged: false,
            values: pick(&[10, 100, 1_000, 10_000, 100_000], &[10, 1_000, 100_000]),
            contracts: vec![contract(
                Metric::Cpu,
                1.1,
                1.0,
                "cost grows with events, which SCALE §0 allows",
            )],
        },
        Sweep {
            name: "elapsed".into(),
            model: ModelKind::Closed,
            vary: Vary::Elapsed,
            regions: None,
            paged: false,
            values: pick(&[1, 10, 100, 1_000, 10_000], &[1, 100, 10_000]),
            contracts: vec![contract(
                Metric::CatchUp,
                flat,
                0.0,
                "closed-form catch-up is O(1)",
            )],
        },
        Sweep {
            name: "elapsed".into(),
            model: ModelKind::Herd,
            vary: Vary::Elapsed,
            regions: None,
            paged: false,
            values: pick(&[1, 10, 100, 1_000, 10_000], &[1, 100, 10_000]),
            contracts: vec![contract(
                Metric::CatchUp,
                1.1,
                0.0,
                "one commit per elapsed slot, as SCALE §4 specifies; SCALE §10 forbids it. DECISIONS O5",
            )],
        },
        Sweep {
            name: "age".into(),
            model: ModelKind::Closed,
            vary: Vary::Age,
            regions: Some(AGE_REGIONS),
            paged: false,
            values: pick(&[1, 10, 100, 1_000], &[1, 30, 300]),
            contracts: vec![contract(
                Metric::Cpu,
                0.15,
                0.0,
                "closed-form regions owe nothing for time unread; 100,000 regions so a slot model would saturate inside the range",
            )],
        },
        Sweep {
            name: "age".into(),
            model: ModelKind::Herd,
            vary: Vary::Age,
            regions: Some(AGE_REGIONS),
            paged: false,
            values: pick(&[1, 10, 100, 1_000], &[1, 30, 300]),
            contracts: vec![contract(
                Metric::Cpu,
                1.0,
                0.0,
                "a region first touched at age A pays A slots, so cost per day grows with age until every region is touched within its debt, then holds at regions/slot (eager stepping). 100,000 regions put that point (regions / 2 touches per event / events per day = 50 days) inside the range. DECISIONS O5",
            )],
        },
        // The dyadic model: the same herd shape with randomness from dyadic counts (O17). It must
        // be flat where the slot model grows.
        Sweep {
            name: "latent".into(),
            model: ModelKind::Dyadic,
            vary: Vary::Latent,
            regions: None,
            paged: false,
            values: pick(
                &[
                    10_000,
                    1_000_000,
                    100_000_000,
                    10_000_000_000,
                    1_000_000_000_000,
                ],
                &[10_000, 100_000_000, 1_000_000_000_000],
            ),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    flat,
                    0.0,
                    "dyadic counts cost O(levels) binomial draws whatever the population",
                ),
                contract(Metric::Heap, flat, 0.0, "latent members cost zero bytes"),
            ],
        },
        Sweep {
            name: "regions".into(),
            model: ModelKind::Dyadic,
            vary: Vary::Regions,
            regions: None,
            paged: false,
            values: pick(
                &[100, 1_000, 10_000, 100_000, 1_000_000],
                &[100, 10_000, 1_000_000],
            ),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    0.15,
                    0.0,
                    "no slots to commit: untouched regions cost nothing (O17)",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "this sweep keeps every region resident (LazyTable); the paged sweeps measure A16's budget",
                ),
                contract(Metric::Save, 1.1, 1.0, "a full save writes every region"),
            ],
        },
        Sweep {
            name: "elapsed".into(),
            model: ModelKind::Dyadic,
            vary: Vary::Elapsed,
            regions: None,
            paged: false,
            values: pick(&[1, 10, 100, 1_000, 10_000], &[1, 100, 10_000]),
            contracts: vec![contract(
                Metric::CatchUp,
                0.15,
                0.0,
                "a read walks four trees of 32 levels whatever the elapsed time (O17)",
            )],
        },
        Sweep {
            name: "age".into(),
            model: ModelKind::Dyadic,
            vary: Vary::Age,
            regions: Some(AGE_REGIONS),
            paged: false,
            values: pick(&[1, 10, 100, 1_000], &[1, 30, 300]),
            contracts: vec![contract(
                Metric::Cpu,
                0.15,
                0.0,
                "no slot debt: a region first touched at any age costs one read (O17)",
            )],
        },
        // Paged regions under a fixed budget (A16, SCALE.md §8): resident memory must not grow
        // with the world.
        paged_sweep(ModelKind::Closed, quick),
        paged_sweep(ModelKind::Herd, quick),
        paged_sweep(ModelKind::Dyadic, quick),
        // Phase 1 markets (rtsim-world). The model kind is a label only; the spec says what
        // runs.
        Sweep {
            name: "markets".into(),
            model: ModelKind::Closed,
            vary: Vary::Markets,
            regions: None,
            paged: false,
            values: pick(&[16, 64, 256, 1_024, 4_096], &[16, 256, 4_096]),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    1.3,
                    1.0,
                    "a world still settling (days 10 to 20): events per market per day are about flat across sizes, so the algorithm is linear in markets; cost per event rises as the working set (about 16 KB per market after round 2, against the research's 1 KB estimate) leaves the caches. Layout work, not an algorithmic slope. A settled world costs a tenth or less of this (sim quiet), and a world at rest nothing",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "markets and edges are resident: paging (A16) is not built yet",
                ),
            ],
        },
        Sweep {
            name: "edges".into(),
            model: ModelKind::Closed,
            vary: Vary::Edges,
            regions: None,
            paged: false,
            values: pick(&[1, 2, 4, 8, 16], &[1, 4, 16]),
            contracts: vec![contract(
                Metric::Cpu,
                2.1,
                1.0,
                "nearest-k at 512 markets: a market event costs O(deg × G) (allocation over its edges) and news reaches every neighbour, so events also grow with degree: up to deg squared",
            )],
        },
        // Phase 2 dragons (rtsim-ecology), nobody watching. The model kind is a label only.
        Sweep {
            name: "dragons".into(),
            model: ModelKind::Closed,
            vary: Vary::Dragons,
            regions: None,
            paged: false,
            values: pick(&[8, 32, 128, 512], &[8, 128, 512]),
            contracts: vec![
                contract(
                    Metric::CpuPerUnit,
                    flat,
                    0.0,
                    "a flat cost per unobserved dragon: about one event a day each (hunger is a closed form, a hunt is a leg there, a patrol, a leg home), decisions look up the cells around the lair, legs book and search only the cells they cover, roosting dragons book nothing; nothing is ticked. Measured as the dragons' own CPU (the world stepped apart) per dragon-day, on a fixed 256-village world up to about two lairs per region, Ice and Fire's own lair spacing. Denser (2,048 dragons, seven per region) prey runs out and each dragon hunts twice as often: behaviour, with the cost per event still flat (about 10 us)",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "dragons are resident rows (paging, A16, is not built yet)",
                ),
            ],
        },
        // Ships (rtsim-ships), nobody watching. The model kind is a label only; the value is the
        // service cap (merchant ships per loop direction), on a fixed 100-port ocean grid.
        Sweep {
            name: "ships".into(),
            model: ModelKind::Closed,
            vary: Vary::Ships,
            regions: None,
            paged: false,
            values: pick(&[1, 2, 4, 8, 16], &[1, 4, 16]),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    flat,
                    0.0,
                    "an unobserved merchant ship costs nothing: its position is a closed form of the clock (service timetable and saved clock offset), it books no voyage and schedules no event, so the ships engine's own CPU per sim day (the world stepped apart) stays at the pirates' few events a hunt whatever the ship count. Per ship (cpu_ns_per_unit_day) it falls as one over the count",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "ships are resident rows (paging, A16, is not built yet)",
                ),
            ],
        },
        // Herds (rtsim-ecology's herds), nobody watching: herd count at 5,000 members each.
        Sweep {
            name: "herds".into(),
            model: ModelKind::Dyadic,
            vary: Vary::Herds,
            regions: None,
            paged: false,
            values: pick(&[4, 16, 64, 256], &[4, 64, 256]),
            contracts: vec![
                contract(
                    Metric::CpuPerUnit,
                    flat,
                    0.0,
                    "a flat cost per herd-day: one decision per sub-season (a flow field bounded by its reach cap), births one draw a year, deaths dyadic counts read on demand; nothing per member or per day. Herds of different species are mixed evenly, so per-herd costs average over speeds (reach) and footprints",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "herds are resident rows with their grazed forage cells (paging, A16, is not built yet)",
                ),
            ],
        },
        // Members per herd, 16 herds: the latent members' contract.
        Sweep {
            name: "members".into(),
            model: ModelKind::Dyadic,
            vary: Vary::Members,
            regions: None,
            paged: false,
            values: pick(
                &[100, 1_000, 10_000, 100_000, 1_000_000],
                &[100, 10_000, 1_000_000],
            ),
            contracts: vec![
                contract(
                    Metric::Cpu,
                    flat,
                    0.0,
                    "members are latent: deaths are dyadic counts (O5), births one binomial a year, so a decision costs the same for a hundred animals or a million. The grazing footprint grows with members up to its cap (1,536 blocks), a bounded term",
                ),
                contract(
                    Metric::Heap,
                    0.3,
                    0.0,
                    "a latent member costs zero bytes (year-classes, not rows); what grows is the forage field's grazed cells, a row per cell a footprint grazed since the last yearly fold, and footprints grow with members up to their cap (1,536 blocks): bounded by the cap, not flat below it",
                ),
                contract(
                    Metric::CatchUp,
                    flat,
                    0.0,
                    "a read after 10,000 unread days walks at most three dyadic trees per year-class, whatever the members",
                ),
            ],
        },
        // Conflict (rtsim-conflict), nobody watching. The model kind is a label only; the value is
        // the faction count, with one village per camp.
        Sweep {
            name: "factions".into(),
            model: ModelKind::Closed,
            vary: Vary::Factions,
            regions: None,
            paged: false,
            values: pick(&[16, 64, 256, 1024], &[16, 256, 1024]),
            contracts: vec![
                contract(
                    Metric::EventsPerUnit,
                    flat,
                    0.0,
                    "a flat number of events per faction: a feud books one event per rung (its next threshold's closed-form crossing, its fall, its deadline) and a plunder is a muster, a march as legs, a raid and a homecoming; nothing is ticked. Measured about 1.04 a faction-day at every point",
                ),
                contract(
                    Metric::CpuPerUnit,
                    0.2,
                    0.0,
                    "the conflict engine's own CPU (the world stepped apart) per faction-day, one village per camp (a fixed density): flat events (above) at a cost per event that rises from about 5.4 to 9.1 us as the world grows 64 times (1,024 camps and villages, 80 MiB of heap): cache misses and log factors, not scans (a signal, an emission, a route or a plunder target looks up the cells around it, never every faction or node). Declared 0.2 from the 2026-10-20 runs (measured 0.09 to 0.155); 0.24 before routes and plunder targets used cells",
                ),
                contract(
                    Metric::Heap,
                    1.1,
                    1.0,
                    "factions, feuds and parties are resident rows (paging, A16, is not built yet)",
                ),
            ],
        },
    ]
}

/// The faction sweep grows the village grid with the camps (one village per camp, a camp in each
/// village cell), so it measures the cost per faction at a fixed density; at a fixed world,
/// denser camps feel more of each other's smog and plunder each other more (behaviour). At least
/// this many villages. PLACEHOLDER 16.
const FACTION_VILLAGES_MIN: u64 = 16;

/// Members per herd held fixed while the herd sweep varies herds. PLACEHOLDER 5,000.
const MEMBER_BASE: u64 = 5_000;

/// Herds held fixed while the members sweep varies members per herd. PLACEHOLDER 16.
const HERD_BASE: u64 = 16;

/// Ports on a side of the ship sweep's ocean grid. PLACEHOLDER 10: 100 ports, from about 70
/// merchant ships at one a service to about a thousand at sixteen.
const SHIP_GRID_SIDE: u64 = 10;

/// Markets held fixed while the edge sweep varies nearest-k.
const MARKET_BASE: u64 = 512;

/// Villages held fixed while the dragon sweep varies dragons. PLACEHOLDER 256 (a 16 × 16 grid,
/// about 8,700 blocks across): the densest point is a few lairs per region, about Ice and Fire's
/// own lair spacing (roost spacing 15 chunks, caves 18, verified in its structure sets).
const DRAGON_VILLAGES: u64 = 256;

fn paged_sweep(model: ModelKind, quick: bool) -> Sweep {
    Sweep {
        name: "paged".into(),
        model,
        vary: Vary::Regions,
        regions: None,
        paged: true,
        // A hundred million regions for the cheap models; the dyadic model's conservation check
        // (its invariants walk four trees per region) would take minutes per repeat there.
        values: match (quick, model) {
            (true, _) => vec![100_000, 1_000_000],
            (false, ModelKind::Dyadic) => vec![100_000, 1_000_000, 10_000_000],
            (false, _) => vec![100_000, 1_000_000, 10_000_000, 100_000_000],
        },
        contracts: vec![
            contract(
                Metric::Resident,
                0.1,
                0.0,
                "resident pages are held to the budget whatever the world's size (A16)",
            ),
            contract(
                Metric::Heap,
                PAGED_HEAP_SLOPE,
                0.0,
                "the budget, the store's index-block cache and its overlay, all bounded by constants; the heap rises only until the world's index outgrows the block cache (K15)",
            ),
            contract(
                Metric::Cpu,
                PAGED_CPU_SLOPE,
                0.0,
                "each event costs at most one page load, so cost rises with the miss rate until every event misses, then holds",
            ),
            contract(
                Metric::SaveIncr,
                PAGED_SAVE_SLOPE,
                0.0,
                "a save writes the pages dirtied since the last one and a log record of them, not the world (K11)",
            ),
        ],
    }
}

/// Declared slopes of the paged sweeps. PLACEHOLDERS set from the first measured runs (bench
/// results README): the heap holds constants only (the page index is paged, K15), the miss rate
/// rises until every event misses, and dirty pages per save rise until every event dirties its
/// own page.
const PAGED_HEAP_SLOPE: f64 = 0.1;
const PAGED_CPU_SLOPE: f64 = 0.35;
const PAGED_SAVE_SLOPE: f64 = 0.35;

/// The age sweeps' region count: see their notes.
const AGE_REGIONS: u64 = 100_000;

// ---- The child side ----

fn cpu_or_wall(cpu0: Option<u64>, wall0: Instant) -> (Option<f64>, f64) {
    let wall = wall0.elapsed().as_nanos() as f64;
    let cpu = match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    };
    (cpu, wall)
}

pub fn run_one(spec: &Spec) -> Measured {
    match spec {
        Spec::World(c) => match c.model {
            ModelKind::Herd => run_world::<SynthRegion>(c),
            ModelKind::Closed => run_world::<Stock>(c),
            ModelKind::Dyadic => run_world::<DyadicRegion>(c),
        },
        Spec::CatchUp(c) => match c.model {
            ModelKind::Herd => run_catch_up::<SynthRegion>(c),
            ModelKind::Closed => run_catch_up::<Stock>(c),
            ModelKind::Dyadic => run_catch_up::<DyadicRegion>(c),
        },
        Spec::Market(c) => super::market::run(c),
        Spec::Dragons(c) => super::dragons::run(c),
        Spec::Ships(c) => super::ships::run(c),
        Spec::Herds(c) => super::herds::run(c),
        Spec::Factions(c) => super::conflict::run(c),
        Spec::Paged(p) => match p.world.model {
            ModelKind::Herd => run_paged::<SynthRegion>(p),
            ModelKind::Closed => run_paged::<Stock>(p),
            ModelKind::Dyadic => run_paged::<DyadicRegion>(p),
        },
    }
}

/// Bench scratch folders live under the target folder (on disk, not a tmpfs), or the system
/// temp folder when that is unknown.
fn scratch_dir(tag: &str) -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.join("bench-scratch")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("{tag}-{}", std::process::id()))
}

fn run_paged<M: BenchModel + rtsim_kernel::paging::PagedModel>(p: &PagedSpec) -> Measured {
    let c = &p.world;
    let mut w: PagedWorld<M> = PagedWorld::build(p, scratch_dir("paged"));
    w.run_to(Tick::from_days(c.warmup_days));
    w.table.reset_stats();
    let saves0 = w.saves;
    let (dep0, arr0) = (w.departures, w.arrivals);
    sys::reset_heap_peak();
    // The build phase passed through the allocator; resident size and its peak should show the
    // measured window only.
    sys::trim_heap();
    sys::reset_rss_peak();

    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    w.run_to(Tick::from_days(c.warmup_days + c.days));
    let (cpu, wall) = cpu_or_wall(cpu0, wall0);
    let days = c.days as f64;
    let ps = w.stats();
    let lazy = w.table.lazy_stats();
    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    let (rss, rss_peak) = sys::rss();
    let resident = w.table.resident_bytes() as u64;
    let saves = w.saves;
    let n_saves = (saves.saves - saves0.saves).max(1) as f64;

    let mut failures = Vec::new();
    // At a step boundary after a save, the budget holds: nothing is dirty.
    w.save();
    w.table.enforce_budget();
    if w.table.resident_bytes() as u64 > p.budget_bytes {
        failures.push(format!(
            "resident {} bytes over the budget {} after a save",
            w.table.resident_bytes(),
            p.budget_bytes
        ));
    }
    let eager = c.regions * (c.warmup_days + c.days + 1);
    if lazy.slot_commits > eager {
        failures.push(format!(
            "{} slot commits exceed the eager bound {eager}",
            lazy.slot_commits
        ));
    }
    let store_index = w.store.heap_bytes() as u64;
    let engine = w.heap_bytes() as u64;
    // Conservation and the hash load the whole world, page by page: outside the window. The hash
    // catches every region up, which costs the world's size, so big worlds skip it.
    let conservation = match w.conservation() {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    let state_hash = (c.regions <= 100_000).then(|| w.state_hash());

    let save_cpu = (saves.cpu_ns - saves0.cpu_ns) as f64;
    Measured {
        cpu_ns_per_day: cpu.map(|c| (c - save_cpu).max(0.0) / days),
        wall_ns_per_day: Some(wall / days),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes: engine,
        save_cpu_ms_per_day: Some(save_cpu / 1e6 / days),
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        save_full_bytes: Some(w.store.disk_bytes()),
        reads: lazy.reads,
        events: lazy.events,
        slot_commits: lazy.slot_commits,
        caravans: (w.departures - dep0) + (w.arrivals - arr0),
        conservation,
        failures,
        state_hash,
        resident_bytes: Some(resident),
        resident_peak_bytes: Some(ps.peak_resident_bytes),
        budget_bytes: Some(p.budget_bytes),
        store_index_bytes: Some(store_index),
        loads_per_day: Some(ps.loads as f64 / days),
        evictions_per_day: Some(ps.evictions as f64 / days),
        prefetches_per_day: Some(ps.prefetches as f64 / days),
        prefetch_hits_per_day: Some(ps.prefetch_hits as f64 / days),
        over_budget_checks: Some(ps.over_budget),
        save_ms_per_day: Some((saves.wall_ns - saves0.wall_ns) as f64 / 1e6 / days),
        save_bytes_per_save: Some(
            ((saves.segment_bytes - saves0.segment_bytes) + (saves.log_bytes - saves0.log_bytes))
                as f64
                / n_saves,
        ),
        save_log_bytes_per_save: Some((saves.log_bytes - saves0.log_bytes) as f64 / n_saves),
        checkpoints: Some(saves.checkpoints - saves0.checkpoints),
        ..Measured::default()
    }
}

fn run_world<M: BenchModel>(c: &WorldConfig) -> Measured {
    let mut w: World<M> = World::build(c.clone());
    w.run_to(Tick::from_days(c.warmup_days));
    w.table.reset_stats();
    let (dep0, arr0) = (w.departures, w.arrivals);
    sys::reset_heap_peak();

    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    w.run_to(Tick::from_days(c.warmup_days + c.days));
    let (cpu, wall) = cpu_or_wall(cpu0, wall0);
    let stats = w.table.stats();
    let days = c.days as f64;

    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    let (rss, rss_peak) = sys::rss();
    let conservation = match w.conservation() {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };

    let mut failures = Vec::new();
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    // No region can commit more slots than have elapsed since tick 0: the eager bound.
    let eager = c.regions * (c.warmup_days + c.days + 1);
    if stats.slot_commits > eager {
        failures.push(format!(
            "{} slot commits exceed the eager bound {eager}",
            stats.slot_commits
        ));
    }

    let (dir, mut store) = scratch_store("world");
    let t = Instant::now();
    let full = w.save_full(&mut store);
    let full_ms = t.elapsed().as_secs_f64() * 1e3;
    let full_bytes = store.disk_bytes();
    // One more day, then an incremental save of what it touched.
    w.run_to(Tick::from_days(c.warmup_days + c.days + 1));
    let t = Instant::now();
    let (pages, incr) = w.save_dirty(&mut store);
    let incr_ms = t.elapsed().as_secs_f64() * 1e3;
    let _ = std::fs::remove_dir_all(&dir);
    debug_assert!(full.pages_written > 0);

    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes: w.heap_bytes() as u64,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        save_full_bytes: Some(full_bytes),
        save_full_ms: Some(full_ms),
        save_incremental_bytes: Some(incr.segment_bytes + incr.manifest_bytes),
        save_incremental_pages: Some(pages),
        save_incremental_ms: Some(incr_ms),
        reads: stats.reads,
        events: stats.events,
        slot_commits: stats.slot_commits,
        caravans: (w.departures - dep0) + (w.arrivals - arr0),
        conservation,
        failures,
        state_hash: Some(w.state_hash()),
        ..Measured::default()
    }
}

fn run_catch_up<M: BenchModel>(c: &CatchUpConfig) -> Measured {
    let cfg = WorldConfig {
        model: c.model,
        regions: c.regions,
        latent: c.latent_per_region * c.regions,
        events_per_day: 0,
        observers: 0,
        regions_per_page: 64,
        warmup_days: 0,
        days: 0,
        seed: c.seed,
    };
    let mut table: LazyTable<M> =
        LazyTable::new(c.seed, SystemId::named("bench.region"), Tick::ZERO);
    let setup = rtsim_kernel::rng::StreamKey::new(c.seed, SystemId::named("bench.setup"), 0);
    let ids: Vec<_> = (0..c.regions)
        .map(|i| table.insert(i, M::make(i, &cfg, &mut setup.slot(i)), Tick::ZERO))
        .collect();
    // Read at a mid-day tick, so the read also projects past the last boundary.
    let at = Tick::from_days(c.elapsed_days) + TICKS_PER_DAY / 2;
    // Drain breakpoints first, as a step would (none are booked here), outside the timed window.
    while table.next_due(at).is_some() {}
    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    for &h in &ids {
        std::hint::black_box(table.read(h, at));
    }
    let (cpu, wall) = cpu_or_wall(cpu0, wall0);
    let n = c.regions as f64;
    let stats = table.stats();
    let (rss, rss_peak) = sys::rss();
    Measured {
        catch_up_cpu_ns_per_read: cpu.map(|c| c / n),
        catch_up_wall_ns_per_read: Some(wall / n),
        commits_per_read: Some(stats.slot_commits as f64 / n),
        heap_live_bytes: sys::heap_live() as u64,
        heap_peak_bytes: sys::heap_peak() as u64,
        engine_heap_bytes: table.heap_bytes() as u64,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        reads: stats.reads,
        slot_commits: stats.slot_commits,
        conservation: "n/a".into(),
        ..Measured::default()
    }
}

// ---- The parent side ----

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub sweep: String,
    pub model: ModelKind,
    pub vary: Vary,
    pub value: u64,
    pub spec: Spec,
    /// The repeat with the median primary metric (the sweep's first contract).
    pub measured: Measured,
    /// That metric in every repeat, in run order.
    pub repeats: Vec<Option<f64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Slope {
    pub sweep: String,
    pub model: ModelKind,
    pub metric: Metric,
    pub measured: Option<f64>,
    pub declared: f64,
    pub scale_md: f64,
    /// Max / min of the metric across the sweep.
    pub ratio: Option<f64>,
    pub max_ratio: Option<f64>,
    pub pass: bool,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub date_utc: String,
    pub mode: String,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub rustc: Option<String>,
    pub build_profile: String,
    pub machine: sys::Machine,
    pub nice: Option<i32>,
    /// Logical CPUs this process may run on (a `taskset` shows here).
    #[serde(default)]
    pub allowed_cpus: Option<String>,
    /// The run's tag (`--tag`), e.g. `potato`.
    #[serde(default)]
    pub tag: Option<String>,
    pub loadavg_before: Option<String>,
    pub loadavg_after: Option<String>,
    pub baseline: Baseline,
    pub runs: Vec<Run>,
    pub slopes: Vec<Slope>,
}

fn run_child(spec: &Spec) -> Result<Measured, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let json = serde_json::to_string(spec).map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .args(["bench", "scale-one", "--spec", &json])
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "child failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("child output: {e}"))
}

/// Whether a measured slope keeps its contract. No slope (too few usable points) fails: a check
/// that cannot measure must not pass silently.
pub fn within(measured: Option<f64>, declared: f64) -> bool {
    measured.is_some_and(|m| m <= declared)
}

/// Max / min of the positive metric values.
pub fn spread(points: &[(f64, f64)]) -> Option<f64> {
    let ys: Vec<f64> = points.iter().map(|p| p.1).filter(|y| *y > 0.0).collect();
    if ys.len() < 2 {
        return None;
    }
    let max = ys.iter().copied().fold(f64::MIN, f64::max);
    let min = ys.iter().copied().fold(f64::MAX, f64::min);
    Some(max / min)
}

/// Least-squares slope of log10(y) on log10(x).
pub fn log_slope(points: &[(f64, f64)]) -> Option<f64> {
    let pts: Vec<(f64, f64)> = points
        .iter()
        .filter(|(x, y)| *x > 0.0 && *y > 0.0)
        .map(|(x, y)| (x.log10(), y.log10()))
        .collect();
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len() as f64;
    let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    (sxx > 0.0).then(|| sxy / sxx)
}

pub struct Options {
    pub quick: bool,
    pub check: bool,
    /// Runs per configuration; the run with the median primary metric is kept.
    pub repeats: u32,
    pub out: Option<PathBuf>,
    pub only: Vec<String>,
    /// Appended to the result file's name and recorded in it (e.g. `potato`).
    pub tag: Option<String>,
}

pub(super) fn command_line(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub(super) fn nice_level() -> Option<i32> {
    // The 19th field of /proc/self/stat is the nice value.
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(16)?.parse().ok()
}

/// Days since 1970-01-01 to (year, month, day): Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub(super) fn now_utc() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    (
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            s / 3600,
            s / 60 % 60,
            s % 60
        ),
        format!("{y:04}-{m:02}-{d:02}-{:02}{:02}", s / 3600, s / 60 % 60),
    )
}

fn fmt_si(x: f64) -> String {
    if x >= 1e9 {
        format!("{:.0}e9", x / 1e9)
    } else if x >= 1e6 {
        format!("{:.0}e6", x / 1e6)
    } else if x >= 1e3 {
        format!("{:.0}e3", x / 1e3)
    } else {
        format!("{x:.0}")
    }
}

fn opt(x: Option<f64>, scale: f64, prec: usize) -> String {
    x.map_or("-".into(), |v| format!("{:.*}", prec, v / scale))
}

pub fn run(opts: Options) -> i32 {
    let base = Baseline::new(opts.quick);
    let sweeps: Vec<Sweep> = sweeps(opts.quick)
        .into_iter()
        .filter(|s| opts.only.is_empty() || opts.only.contains(&s.name))
        .collect();
    let loadavg_before = sys::loadavg();
    eprintln!(
        "bench scale ({}): {} sweeps, load {}",
        if opts.quick { "quick" } else { "full" },
        sweeps.len(),
        loadavg_before.as_deref().unwrap_or("?")
    );

    let mut runs = Vec::new();
    for s in &sweeps {
        for &v in &s.values {
            let spec = base.spec(s, v);
            eprint!("  {} {:?} = {} ... ", s.name, s.model, fmt_si(v as f64));
            let t = Instant::now();
            let primary = s.contracts[0].metric;
            let mut got: Vec<Measured> = Vec::new();
            for _ in 0..opts.repeats.max(1) {
                match run_child(&spec) {
                    Ok(m) => got.push(m),
                    Err(e) => {
                        eprintln!("FAILED: {e}");
                        return 2;
                    }
                }
            }
            eprintln!("{:.1} s", t.elapsed().as_secs_f64());
            let repeats: Vec<Option<f64>> = got.iter().map(|m| primary.of(m)).collect();
            let mut order: Vec<usize> = (0..got.len()).collect();
            order.sort_by(|&a, &b| {
                let (x, y) = (
                    repeats[a].unwrap_or(f64::NAN),
                    repeats[b].unwrap_or(f64::NAN),
                );
                x.total_cmp(&y)
            });
            let measured = got.swap_remove(order[order.len() / 2]);
            runs.push(Run {
                sweep: s.name.clone(),
                model: s.model,
                vary: s.vary,
                value: v,
                spec,
                measured,
                repeats,
            });
        }
    }

    let mut slopes = Vec::new();
    for s in &sweeps {
        let pts: Vec<&Run> = runs
            .iter()
            .filter(|r| r.sweep == s.name && r.model == s.model)
            .collect();
        for c in &s.contracts {
            let points: Vec<(f64, f64)> = pts
                .iter()
                .filter_map(|r| c.metric.of(&r.measured).map(|y| (r.value as f64, y)))
                .collect();
            let measured = log_slope(&points);
            let ratio = spread(&points);
            let ratio_ok = match (c.max_ratio, ratio) {
                (Some(max), Some(r)) => r <= max,
                (Some(_), None) => false,
                (None, _) => true,
            };
            slopes.push(Slope {
                sweep: s.name.clone(),
                model: s.model,
                metric: c.metric,
                measured,
                declared: c.declared,
                scale_md: c.scale_md,
                ratio,
                max_ratio: c.max_ratio,
                pass: within(measured, c.declared) && ratio_ok,
                note: c.note.clone(),
            });
        }
    }

    print_tables(&runs, &slopes);

    let (date, stamp) = now_utc();
    let report = Report {
        schema: 1,
        date_utc: date,
        mode: if opts.quick { "quick" } else { "full" }.into(),
        git_commit: command_line("git", &["rev-parse", "--short=12", "HEAD"]),
        git_dirty: command_line("git", &["status", "--porcelain", "--untracked-files=no"])
            .map(|s| !s.is_empty()),
        rustc: command_line("rustc", &["--version"]),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        machine: sys::Machine::probe(),
        nice: nice_level(),
        allowed_cpus: sys::allowed_cpus(),
        tag: opts.tag.clone(),
        loadavg_before,
        loadavg_after: sys::loadavg(),
        baseline: base,
        runs,
        slopes,
    };
    if let Some(dir) = &opts.out {
        match write_report(dir, &stamp, &report) {
            Ok(p) => eprintln!("wrote {}", p.display()),
            Err(e) => {
                eprintln!("could not write the report: {e}");
                return 2;
            }
        }
    }
    let failed: Vec<&Slope> = report.slopes.iter().filter(|s| !s.pass).collect();
    let broken: Vec<&Run> = report
        .runs
        .iter()
        .filter(|r| !r.measured.failures.is_empty())
        .collect();
    for r in &broken {
        eprintln!(
            "invariant broken in {} {:?} = {}: {}",
            r.sweep,
            r.model,
            r.value,
            r.measured.failures.join("; ")
        );
    }
    if opts.check && (!failed.is_empty() || !broken.is_empty()) {
        eprintln!(
            "scale check FAILED: {} contract(s) exceeded, {} run(s) broke an invariant",
            failed.len(),
            broken.len()
        );
        for f in failed {
            eprintln!(
                "  {} {:?} {:?}: slope {} (declared {}), max/min {} (bound {})",
                f.sweep,
                f.model,
                f.metric,
                opt(f.measured, 1.0, 3),
                f.declared,
                opt(f.ratio, 1.0, 2),
                f.max_ratio.map_or("-".into(), |m| format!("{m}"))
            );
        }
        return 1;
    }
    if opts.check {
        eprintln!("scale check passed");
    }
    0
}

fn write_report(dir: &Path, stamp: &str, report: &Report) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let tag = report
        .tag
        .as_ref()
        .map_or(String::new(), |t| format!("-{t}"));
    let path = dir.join(format!("scale-{stamp}-{}{tag}.json", report.mode));
    let mut f = std::fs::File::create(&path)?;
    serde_json::to_writer_pretty(&mut f, report)?;
    writeln!(f)?;
    Ok(path)
}

fn print_tables(runs: &[Run], slopes: &[Slope]) {
    println!(
        "{:<8} {:<6} {:>7} | {:>10} {:>10} {:>8} | {:>9} {:>9} {:>9} | {:>10} {:>10} | {:>9} conserved",
        "sweep",
        "model",
        "value",
        "cpu ms/day",
        "catchup us",
        "commits",
        "heap MiB",
        "rss MiB",
        "peak MiB",
        "save KiB",
        "incr KiB",
        "slots/day"
    );
    for r in runs {
        let m = &r.measured;
        let days = match &r.spec {
            Spec::World(c) => c.days as f64,
            Spec::Paged(p) => p.world.days as f64,
            Spec::Market(c) => c.days as f64,
            Spec::Dragons(c) => c.days as f64,
            Spec::Ships(c) => c.days as f64,
            Spec::Herds(c) => c.days as f64,
            Spec::Factions(c) => c.days as f64,
            Spec::CatchUp(_) => f64::NAN,
        };
        println!(
            "{:<8} {:<6} {:>7} | {:>10} {:>10} {:>8} | {:>9.1} {:>9} {:>9} | {:>10} {:>10} | {:>9} {}",
            r.sweep,
            format!("{:?}", r.model).to_lowercase(),
            fmt_si(r.value as f64),
            opt(m.cpu_ns_per_day, 1e6, 2),
            opt(m.catch_up_cpu_ns_per_read, 1e3, 3),
            opt(m.commits_per_read, 1.0, 1),
            m.heap_live_bytes as f64 / 1048576.0,
            opt(m.rss_bytes.map(|b| b as f64), 1048576.0, 1),
            opt(m.rss_peak_bytes.map(|b| b as f64), 1048576.0, 1),
            opt(m.save_full_bytes.map(|b| b as f64), 1024.0, 1),
            opt(m.save_incremental_bytes.map(|b| b as f64), 1024.0, 1),
            if days.is_nan() {
                "-".into()
            } else {
                format!("{:.0}", m.slot_commits as f64 / days)
            },
            m.conservation
        );
    }
    let paged: Vec<&Run> = runs
        .iter()
        .filter(|r| matches!(r.spec, Spec::Paged(_)))
        .collect();
    if !paged.is_empty() {
        println!();
        println!(
            "{:<8} {:<6} {:>7} | {:>10} {:>8} {:>8} {:>8} {:>8} | {:>9} {:>9} {:>11} | {:>9} {:>9} {:>8}",
            "paged",
            "model",
            "regions",
            "step ms/d",
            "heap MiB",
            "res MiB",
            "budget",
            "idx KiB",
            "loads/day",
            "evict/day",
            "prefetch/hit",
            "save cpu/d",
            "KiB/save",
            "log B"
        );
        for r in paged {
            let m = &r.measured;
            let mib = |b: Option<u64>| opt(b.map(|b| b as f64), 1048576.0, 1);
            println!(
                "{:<8} {:<6} {:>7} | {:>10} {:>8.1} {:>8} {:>8} {:>8} | {:>9} {:>9} {:>11} | {:>9} {:>9} {:>8}",
                r.sweep,
                format!("{:?}", r.model).to_lowercase(),
                fmt_si(r.value as f64),
                opt(m.cpu_ns_per_day, 1e6, 2),
                m.heap_live_bytes as f64 / 1048576.0,
                mib(m.resident_peak_bytes),
                mib(m.budget_bytes),
                opt(m.store_index_bytes.map(|b| b as f64), 1024.0, 0),
                opt(m.loads_per_day, 1.0, 0),
                opt(m.evictions_per_day, 1.0, 0),
                format!(
                    "{}/{}",
                    opt(m.prefetches_per_day, 1.0, 0),
                    opt(m.prefetch_hits_per_day, 1.0, 0)
                ),
                opt(m.save_cpu_ms_per_day, 1.0, 1),
                opt(m.save_bytes_per_save, 1024.0, 1),
                opt(m.save_log_bytes_per_save, 1.0, 0),
            );
        }
    }
    println!();
    println!(
        "{:<8} {:<6} {:<8} {:>9} {:>9} {:>9} {:>8} {:>6}  verdict",
        "sweep", "model", "metric", "slope", "declared", "SCALE.md", "max/min", "bound"
    );
    for s in slopes {
        let over_scale = s.declared > s.scale_md + 0.15;
        println!(
            "{:<8} {:<6} {:<8} {:>9} {:>9.2} {:>9.2} {:>8} {:>6}  {}{}",
            s.sweep,
            format!("{:?}", s.model).to_lowercase(),
            format!("{:?}", s.metric).to_lowercase(),
            opt(s.measured, 1.0, 3),
            s.declared,
            s.scale_md,
            opt(s.ratio, 1.0, 2),
            s.max_ratio.map_or("-".into(), |m| format!("{m:.1}")),
            if s.pass { "ok" } else { "FAIL" },
            if over_scale {
                "  (declared above SCALE.md; see note)"
            } else {
                ""
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slopes_of_power_laws() {
        let lin: Vec<(f64, f64)> = (1..6)
            .map(|i| (10f64.powi(i), 3.0 * 10f64.powi(i)))
            .collect();
        assert!((log_slope(&lin).unwrap() - 1.0).abs() < 1e-12);
        let flat: Vec<(f64, f64)> = (1..6).map(|i| (10f64.powi(i), 7.0)).collect();
        assert!(log_slope(&flat).unwrap().abs() < 1e-12);
        assert_eq!(log_slope(&[(1.0, 1.0)]), None);
    }

    #[test]
    fn the_check_fails_on_growth_and_on_no_data() {
        let quad: Vec<(f64, f64)> = (1..5).map(|i| (10f64.powi(i), 10f64.powi(2 * i))).collect();
        assert!(!within(log_slope(&quad), 1.1));
        assert!(within(log_slope(&quad), 2.0 + 1e-9));
        assert!(!within(None, 1.0));
    }

    #[test]
    fn spread_is_max_over_min() {
        assert_eq!(spread(&[(1.0, 2.0), (10.0, 8.0), (100.0, 4.0)]), Some(4.0));
        assert_eq!(spread(&[(1.0, 2.0)]), None);
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_381), (2025, 10, 20));
        assert_eq!(civil(-1), (1969, 12, 31));
    }

    #[test]
    fn small_worlds_conserve() {
        for model in [ModelKind::Herd, ModelKind::Closed, ModelKind::Dyadic] {
            let spec = Spec::World(WorldConfig {
                model,
                regions: 50,
                latent: 5_000,
                events_per_day: 200,
                observers: 2,
                regions_per_page: 8,
                warmup_days: 1,
                days: 2,
                seed: 3,
            });
            let a = run_one(&spec);
            assert_eq!(a.conservation, "ok");
            assert!(a.failures.is_empty(), "{:?}", a.failures);
            assert!(a.caravans > 0 && a.reads > 0);
            assert!(a.save_full_bytes.unwrap() > 0 && a.save_incremental_pages.unwrap() > 0);
            // Same spec, same world.
            assert_eq!(run_one(&spec).state_hash, a.state_hash);
        }
    }
}
