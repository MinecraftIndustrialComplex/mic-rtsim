// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exit test (b) of VIEWER.md §16 V3: 10,000 game days through the engine's path (module tapes,
//! the recorder, a flush at each save and compaction after it) under the default 256 MiB budget.
//! Bytes on disk stay under the budget and every tier within its share at every save, every row
//! is accounted for (held, counted, expired with the oldest counts, or dropped and counted), and
//! the index matches the folder and reopens to the same archive.
//!
//! A synthetic world stands in for the modules (a real one runs far slower than 10,000 days in
//! twenty minutes): rows at a few thousand places of a Deep Time 32k world, by the registered
//! kinds at their usual severities, legs for movers, payloads on notable rows, players' UUIDs
//! through the id dictionary. Four saves a game day (Minecraft's five-minute autosave).
//!
//! `cargo test -p rtsim-archive --release --test soak -- --ignored --nocapture`; the environment
//! sets the size: `RTSIM_SOAK_DAYS` (10,000), `RTSIM_SOAK_ROWS` (rows a game day, 20,000),
//! `RTSIM_SOAK_SAVES` (saves a game day, 4), `RTSIM_SOAK_DIR` (the folder to write in: a tmpfs such
//! as /dev/shm takes a busy disk out of the timing). The quick test below runs the same loop for 300 days
//! under a 4 MiB budget.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rtsim_archive::kinds::{KINDS, Kind};
use rtsim_archive::{
    Archive, ArchiveReader, At, Budget, Compaction, Recorder, Severity, Stamp, TICKS_PER_DAY, Tape,
};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let root =
            std::env::var_os("RTSIM_SOAK_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let d = root.join(format!("rtsim-archive-soak-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        Dir(d)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.replace(['_', ','], "").parse().ok())
        .unwrap_or(default)
}

/// splitmix64.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The synthetic world's places: villages, districts, lairs and havens on a 32k Deep Time map
/// (26,112 by 13,056 blocks, centred), most of them in the overworld.
struct World {
    places: Vec<(u32, i32, i32)>,
    kinds: Vec<Kind>,
    /// Kinds by usual severity: most rows are routine and minor, as in the scenarios.
    weights: Vec<u64>,
    players: Vec<u128>,
}

impl World {
    fn new() -> World {
        let places = (0..3_000u64)
            .map(|i| {
                let h = mix(i);
                let dim = if h.is_multiple_of(50) { 1 } else { 0 };
                let x = (h >> 8) % 26_112;
                let z = (h >> 32) % 13_056;
                (dim, x as i32 - 13_056, z as i32 - 6_528)
            })
            .collect();
        let kinds: Vec<Kind> = (0..KINDS.len()).filter_map(Kind::from_index).collect();
        let weights = kinds
            .iter()
            .map(|k| match k.severity() {
                Severity::Routine => 40,
                Severity::Minor => 12,
                Severity::Notable => 2,
                Severity::Major => 1,
            })
            .collect();
        let players = (0..48u128)
            .map(|i| (u128::from(mix(i as u64 + 7)) << 64) | u128::from(mix(i as u64)))
            .collect();
        World {
            places,
            kinds,
            weights,
            players,
        }
    }

    fn kind(&self, h: u64) -> Kind {
        let total: u64 = self.weights.iter().sum();
        let mut r = h % total;
        for (k, w) in self.kinds.iter().zip(&self.weights) {
            if r < *w {
                return *k;
            }
            r -= w;
        }
        self.kinds[0]
    }

    /// `n` rows between ticks `t0` and `t1` onto `tape`.
    fn rows(&self, tape: &mut Tape, t0: u64, t1: u64, n: u64, seed: u64) {
        for i in 0..n {
            let h = mix(seed.wrapping_mul(0x1_0000_0001) ^ i);
            let tick = t0 + (t1 - t0) * i / n.max(1);
            let kind = self.kind(h);
            let (dim, px, pz) = self.places[(h >> 20) as usize % self.places.len()];
            let (x, z) = (px + (h >> 40) as i32 % 64, pz + (h >> 48) as i32 % 64);
            let subject = if h.is_multiple_of(9) {
                self.players[(h >> 12) as usize % self.players.len()]
            } else {
                u128::from((h >> 16) % 20_000 + 1)
            };
            if kind.name().ends_with("_leg") || kind.name() == "dragons.flight" {
                let (_, qx, qz) = self.places[(h >> 28) as usize % self.places.len()];
                let duration = 1_200 + (h >> 36) % 12_000;
                tape.leg(kind, dim, (tick, x, z), (tick + duration, qx, qz), subject);
            } else {
                let object = if h.is_multiple_of(3) {
                    u128::from((h >> 24) % 5_000)
                } else {
                    0
                };
                let mark = tape.event(
                    kind,
                    At::new(tick, dim, x, z),
                    subject,
                    object,
                    (h % 200) as i64,
                );
                // Payloads only at notable and above, as the modules write them.
                if kind.severity() >= Severity::Notable {
                    mark.text(|| format!("loot: {} iron ingot, {} bread", h % 64, (h >> 6) % 16));
                }
            }
        }
    }
}

/// Rows the archive accounts for: held, counted, expired or dropped.
fn accounted(a: &Archive, expired: u64) -> u64 {
    let held: u64 = a
        .entries()
        .iter()
        .map(|e| {
            if e.header.tier.is_counts() {
                e.header.counted()
            } else {
                u64::from(e.header.rows)
            }
        })
        .sum();
    held + expired + a.dropped().rows()
}

fn folder_bytes(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".rtac"))
        .map(|e| e.metadata().unwrap().len())
        .sum()
}

struct Soak {
    days: u64,
    per_day: u64,
    saves: u64,
    budget: Budget,
}

fn soak(tag: &str, s: Soak) {
    let dir = Dir::new(tag);
    let w = World::new();
    let mut a = Archive::open(&dir.0).unwrap().with_budget(s.budget);
    let mut rec = Recorder::new(a.dictionary().clone());
    let mut tape = Tape::new();
    tape.set_on(true);
    let (mut pushed, mut expired) = (0u64, 0u64);
    let mut all = Compaction::default();
    let mut peak = [0u64; 3];
    let mut peak_total = 0;
    let start = Instant::now();
    let per_save = s.per_day / s.saves;
    let span = TICKS_PER_DAY / s.saves;
    let mut obs = 0;
    for day in 0..s.days {
        for k in 0..s.saves {
            let t0 = day * TICKS_PER_DAY + k * span;
            // A burst one day in fifty, five times the rows; one day in a thousand, ten times, past
            // the pending buffer's cap (the drops are counted).
            let n = match day {
                d if d % 1_000 == 500 => per_save * 10,
                d if d % 50 == 25 => per_save * 5,
                _ => per_save,
            };
            w.rows(&mut tape, t0, t0 + span, n, day * s.saves + k);
            pushed += n;
            rec.absorb(&mut tape);
            let (batch, dict) = rec.take();
            obs += 1;
            let stamp = Stamp {
                obs_seq: obs,
                serial: obs,
                tick: t0 + span,
                wall_ms: obs * 300_000,
            };
            a.flush(batch, stamp, dict.as_ref()).unwrap();
            let c = a.compact(obs).unwrap();
            expired += c.expired_rows;
            all.read += c.read;
            all.written += c.written;
            all.demoted[0] += c.demoted[0];
            all.demoted[1] += c.demoted[1];
            all.coarsened += c.coarsened;
            all.expired += c.expired;
            all.merged += c.merged;
            let bytes = a.tier_bytes();
            for t in 0..3 {
                assert!(
                    bytes[t] <= a.budget().share(t),
                    "day {day}: tier {t} at {} bytes, over its share {}",
                    bytes[t],
                    a.budget().share(t)
                );
                peak[t] = peak[t].max(bytes[t]);
            }
            peak_total = peak_total.max(a.bytes());
            assert!(
                a.bytes() <= s.budget.total,
                "day {day}: {} bytes",
                a.bytes()
            );
        }
        // Summing every count chunk is the slow part: every 25 days.
        if day % 25 == 24 || day + 1 == s.days {
            assert_eq!(
                accounted(&a, expired),
                pushed,
                "day {day}: a row unaccounted"
            );
        }
        if day % 1_000 == 999 {
            assert_eq!(a.verify().unwrap(), Vec::<String>::new(), "day {day}");
            eprintln!(
                "{tag}: day {} at {:.0} s, tiers {:?} bytes, dropped {:?}",
                day + 1,
                start.elapsed().as_secs_f64(),
                a.tier_bytes(),
                a.dropped().by_severity
            );
        }
    }
    let wall = start.elapsed().as_secs_f64();
    assert_eq!(a.verify().unwrap(), Vec::<String>::new());
    assert_eq!(
        folder_bytes(&dir.0),
        a.bytes(),
        "bytes on disk are the index's"
    );
    let size = |f: &str| fs::metadata(dir.0.join(f)).map_or(0, |m| m.len());
    let (index, dictionary) = (
        size(rtsim_archive::store::INDEX_FILE),
        size(rtsim_archive::store::DICT_FILE),
    );
    let entries = a.entries().to_vec();
    let (bytes, dropped) = (a.tier_bytes(), *a.dropped());
    drop(a);
    let a = Archive::open(&dir.0).unwrap().with_budget(s.budget);
    assert_eq!(a.entries(), &entries[..], "reopened, the same index");
    let r = ArchiveReader::open(&dir.0, None).unwrap().unwrap();
    let (first, last) = r.span().unwrap();
    let held: u64 = entries
        .iter()
        .filter(|e| !e.header.tier.is_counts())
        .map(|e| u64::from(e.header.rows))
        .sum();
    let tiers = |t: &dyn Fn(&rtsim_archive::Entry) -> bool| {
        let es: Vec<_> = entries.iter().filter(|e| t(e)).collect();
        let first = es.iter().map(|e| e.header.first_tick).min().unwrap_or(0) / TICKS_PER_DAY;
        let last = es.iter().map(|e| e.header.last_tick).max().unwrap_or(0) / TICKS_PER_DAY;
        (es.len(), first, last)
    };
    let full = tiers(&|e| e.header.tier == rtsim_archive::Tier::Full);
    let notable = tiers(&|e| e.header.tier == rtsim_archive::Tier::Notable);
    let counts = tiers(&|e| e.header.tier.is_counts());
    eprintln!(
        "{tag}: {} game days, {} rows a day ({} saves a day), {pushed} rows in {wall:.0} s\n  \
         bytes per tier at the end: full {}, notable {}, counts {} (shares {}, {}, {}); peak {:?}, \
         peak total {peak_total} of {}\n  \
         chunks (count, first day, last day): full {full:?}, notable {notable:?}, counts {counts:?}\n  \
         span {} to {} days; {held} rows held, {expired} expired with the oldest counts, dropped {:?} \
         (by severity routine, minor, notable, major; {} without a frame)\n  \
         compaction: {} MiB read, {} MiB written, {:?} demoted, {} coarsened, {} expired, {} merged\n  \
         {} chunk files, the index {index} bytes, the dictionary {dictionary} bytes",
        s.days,
        s.per_day,
        s.saves,
        bytes[0],
        bytes[1],
        bytes[2],
        s.budget.share(0),
        s.budget.share(1),
        s.budget.share(2),
        peak,
        s.budget.total,
        first / TICKS_PER_DAY,
        last / TICKS_PER_DAY,
        dropped.by_severity,
        dropped.no_frame,
        all.read >> 20,
        all.written >> 20,
        all.demoted,
        all.coarsened,
        all.expired,
        all.merged,
        entries.len(),
    );
    assert!(all.demoted[0] > 0, "the full tier moved down: {all:?}");
}

#[test]
#[ignore = "the 10,000-day soak: run in release with --ignored"]
fn ten_thousand_game_days_stay_under_the_budget() {
    soak(
        "full",
        Soak {
            days: env("RTSIM_SOAK_DAYS", 10_000),
            per_day: env("RTSIM_SOAK_ROWS", 20_000),
            saves: env("RTSIM_SOAK_SAVES", 4).max(1),
            budget: Budget::default(),
        },
    );
}

#[test]
fn a_short_soak_under_a_small_budget() {
    soak(
        "quick",
        Soak {
            days: 300,
            per_day: 4_000,
            saves: 4,
            budget: Budget {
                total: 4 << 20,
                work: 512 << 10,
                full_days: 10,
                notable_days: 60,
                ..Budget::default()
            },
        },
    );
}
