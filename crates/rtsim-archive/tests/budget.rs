// SPDX-License-Identifier: AGPL-3.0-or-later
//! The budget and the tiers (VIEWER.md §10.4): the flush's cap on the full tier, compaction's
//! demotions, coarsening, expiry and merges, the confirmed generation it never passes, and the
//! reader over count chunks. Every row is accounted for: held in the full or notable tier, counted
//! in a count chunk, expired with the oldest counts, or dropped and counted.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rtsim_archive::chunk::Tier;
use rtsim_archive::row::{Event, Handle, Leg, Row, Severity};
use rtsim_archive::{
    Archive, ArchiveReader, Budget, Compaction, Dictionary, Pending, Stamp, TICKS_PER_DAY,
};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "rtsim-archive-budget-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&d);
        Dir(d)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A day of a busy world: `n` rows spread over the day and over a 200,000-block square, one in
/// eight minor, one in twenty notable with a payload, one in a hundred major; one in five a leg.
fn day(p: &mut Pending, d: &mut Dictionary, day: u64, n: u64, seed: u64) {
    let kinds = [
        "travel.caravan",
        "conflict.raid",
        "dragons.flight",
        "herds.migration",
    ];
    let frame = d.frame("dim0").unwrap();
    for i in 0..n {
        let h = (i + seed)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .rotate_left(17)
            ^ day;
        let tick = day * TICKS_PER_DAY + i * TICKS_PER_DAY / n;
        let sev = match h % 100 {
            0 => Severity::Major,
            1..=5 => Severity::Notable,
            6..=18 => Severity::Minor,
            _ => Severity::Routine,
        };
        let kind = d.kind(kinds[(h >> 8) as usize % kinds.len()]).unwrap();
        let x = ((h >> 12) % 200_000) as i32 - 100_000;
        let z = ((h >> 28) % 200_000) as i32 - 100_000;
        let row = if h.is_multiple_of(5) {
            Row::Leg(Leg {
                t0: tick,
                kind,
                frame,
                severity: sev,
                x0: x,
                z0: z,
                x1: x + 300,
                z1: z - 200,
                subject: Handle::direct(h & 0xFFFF),
                duration: 2_000,
                clamped: false,
            })
        } else {
            Row::Event(Event {
                tick,
                kind,
                frame,
                severity: sev,
                x,
                z,
                subject: Handle::direct(h & 0xFFFF),
                object: Handle::direct(i),
                magnitude: (h % 50) as i32,
                payload: false,
                clamped: false,
            })
        };
        if sev >= Severity::Notable && h.is_multiple_of(3) {
            p.push_with_payload(&row, format!("loot: {} iron ingot", h % 64).into_bytes());
        } else {
            p.push(&row);
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

fn check(a: &Archive) {
    let bytes = a.tier_bytes();
    for (t, b) in bytes.iter().enumerate() {
        assert!(
            *b <= a.budget().share(t),
            "tier {t}: {b} bytes over its share {}",
            a.budget().share(t)
        );
    }
    assert_eq!(a.verify().unwrap(), Vec::<String>::new());
}

#[test]
fn the_flush_drops_the_lowest_severity_rather_than_pass_the_full_share() {
    let dir = Dir::new("cap");
    let budget = Budget {
        total: 200 << 10,
        ..Budget::default()
    };
    let mut a = Archive::open(dir.path()).unwrap().with_budget(budget);
    let mut d = Dictionary::new();
    let mut p = Pending::with_caps(1 << 20, 1 << 30);
    day(&mut p, &mut d, 0, 40_000, 1);
    let pushed = p.len() as u64;
    let notable = p
        .rows()
        .iter()
        .filter(|r| rtsim_archive::row::severity(r) >= Severity::Notable)
        .count() as u64;
    a.flush(p.take(), Stamp::default(), Some(&d)).unwrap();
    let full = a.tier_bytes()[0];
    assert!(full <= budget.share(0), "{full}");
    assert!(
        full > budget.share(0) / 2,
        "the cap left most of the share empty: {full}"
    );
    let dropped = a.dropped().by_severity;
    // Routine rows went first; nothing notable went while routine or minor rows were held.
    assert!(dropped[0] > 0);
    assert_eq!(dropped[2] + dropped[3], 0, "{dropped:?}");
    let held: u64 = a.entries().iter().map(|e| u64::from(e.header.rows)).sum();
    assert_eq!(held + a.dropped().rows(), pushed);
    let kept_notable: u64 = a
        .entries()
        .iter()
        .map(|e| {
            let c = a.read(e.seq).unwrap();
            c.rows
                .iter()
                .filter(|r| rtsim_archive::row::severity(r) >= Severity::Notable)
                .count() as u64
        })
        .sum();
    assert_eq!(kept_notable, notable);
    check(&a);
}

#[test]
fn compaction_keeps_every_tier_in_its_share_and_accounts_for_every_row() {
    let dir = Dir::new("tiers");
    let budget = Budget {
        total: 4 << 20,
        work: 512 << 10,
        full_days: 6,
        notable_days: 40,
        ..Budget::default()
    };
    let mut a = Archive::open(dir.path()).unwrap().with_budget(budget);
    let mut d = Dictionary::new();
    let mut p = Pending::with_caps(1 << 20, 1 << 30);
    let (mut pushed, mut expired) = (0u64, 0u64);
    let mut all = Compaction::default();
    let mut seq = 0;
    for dd in 0..400u64 {
        // A burst every 50 days, otherwise a steady world.
        let n = if dd % 50 == 25 { 30_000 } else { 2_000 };
        day(&mut p, &mut d, dd, n, dd);
        pushed += p.len() as u64;
        seq += 1;
        let stamp = Stamp {
            obs_seq: seq,
            tick: (dd + 1) * TICKS_PER_DAY,
            wall_ms: dd,
        };
        let dict = d.is_dirty().then(|| d.clone());
        d.mark_saved();
        a.flush(p.take(), stamp, dict.as_ref()).unwrap();
        let c = a.compact(seq).unwrap();
        assert!(
            c.read + c.written <= budget.work + (256 << 10),
            "day {dd}: {c:?}"
        );
        expired += c.expired_rows;
        all.demoted[0] += c.demoted[0];
        all.demoted[1] += c.demoted[1];
        all.coarsened += c.coarsened;
        all.expired += c.expired;
        all.merged += c.merged;
        check(&a);
        assert_eq!(accounted(&a, expired), pushed, "day {dd}");
    }
    assert!(all.demoted[0] > 0 && all.demoted[1] > 0, "{all:?}");
    assert!(
        all.coarsened > 0 && all.expired > 0 && all.merged > 0,
        "{all:?}"
    );
    // The horizons hold: no full row older than 6 days before the last save, none notable
    // older than 40 (a chunk may straddle the line, so its end is what counts).
    let now = 400 * TICKS_PER_DAY;
    for e in a.entries() {
        match e.header.tier {
            Tier::Full => assert!(e.header.last_end + 6 * TICKS_PER_DAY + TICKS_PER_DAY >= now),
            Tier::Notable => {
                assert!(e.header.last_end + 41 * TICKS_PER_DAY >= now)
            }
            _ => {}
        }
    }
    // Reopened, it is the same archive.
    let entries = a.entries().to_vec();
    drop(a);
    let a = Archive::open(dir.path()).unwrap().with_budget(budget);
    assert_eq!(a.entries(), &entries[..]);
    check(&a);
}

#[test]
fn compaction_never_touches_a_chunk_above_the_confirmed_generation() {
    let dir = Dir::new("confirmed");
    let budget = Budget {
        total: 1 << 20,
        work: 1 << 20,
        full_days: 1,
        ..Budget::default()
    };
    let mut a = Archive::open(dir.path()).unwrap().with_budget(budget);
    let mut d = Dictionary::new();
    let mut p = Pending::with_caps(1 << 20, 1 << 30);
    for dd in 0..5u64 {
        day(&mut p, &mut d, dd, 500, dd);
        let dict = d.clone();
        a.flush(
            p.take(),
            Stamp {
                obs_seq: dd + 1,
                tick: (dd + 1) * TICKS_PER_DAY,
                wall_ms: 0,
            },
            Some(&dict),
        )
        .unwrap();
    }
    // Saves 4 and 5 were pressure saves: confirmed is 3.
    let above: Vec<_> = a
        .entries()
        .iter()
        .filter(|e| e.header.obs_seq > 3)
        .cloned()
        .collect();
    a.compact(3).unwrap();
    for e in &above {
        assert!(a.entries().contains(e), "{} was touched", e.seq);
    }
    assert!(
        a.entries()
            .iter()
            .all(|e| e.header.obs_seq <= 3 || above.contains(e))
    );
    // Whatever compaction made from older chunks keeps a tag at or below confirmed.
    assert!(
        a.entries()
            .iter()
            .filter(|e| e.header.tier != Tier::Full)
            .all(|e| e.header.obs_seq <= 3)
    );
    // A restart that selects generation 3 cuts exactly the pressure saves' chunks.
    assert_eq!(a.cut_above(3).unwrap(), above.len());
    check(&a);
}

#[test]
fn a_quiet_world_merges_its_tiny_chunks() {
    let dir = Dir::new("quiet");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut d = Dictionary::new();
    let mut p = Pending::new();
    for s in 1..=200u64 {
        day(&mut p, &mut d, s, 3, s);
        let dict = d.is_dirty().then(|| d.clone());
        d.mark_saved();
        a.flush(
            p.take(),
            Stamp {
                obs_seq: s,
                tick: s * TICKS_PER_DAY,
                wall_ms: s,
            },
            dict.as_ref(),
        )
        .unwrap();
        a.compact(s).unwrap();
    }
    // 600 rows in 200 saves: a chunk or two, not 200.
    assert!(a.entries().len() <= 2, "{} chunks", a.entries().len());
    let rows: u64 = a.entries().iter().map(|e| u64::from(e.header.rows)).sum();
    assert_eq!(rows, 600);
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    assert_eq!(r.rows(0, u64::MAX).unwrap().len(), 600);
    check(&a);
}

#[test]
fn the_reader_counts_count_chunks_and_reads_them_as_heat() {
    let dir = Dir::new("heat");
    let budget = Budget {
        full_days: 2,
        notable_days: 4,
        ..Budget::default()
    };
    let mut a = Archive::open(dir.path()).unwrap().with_budget(budget);
    let mut d = Dictionary::new();
    let mut p = Pending::new();
    let mut pushed = 0;
    for dd in 0..10u64 {
        day(&mut p, &mut d, dd, 1_000, dd);
        pushed += p.len() as u64;
        let dict = d.is_dirty().then(|| d.clone());
        d.mark_saved();
        a.flush(
            p.take(),
            Stamp {
                obs_seq: dd + 1,
                tick: (dd + 1) * TICKS_PER_DAY,
                wall_ms: dd,
            },
            dict.as_ref(),
        )
        .unwrap();
        a.compact(dd + 1).unwrap();
    }
    assert_eq!(accounted(&a, 0), pushed);
    let tiers: Vec<Tier> = a.entries().iter().map(|e| e.header.tier).collect();
    assert!(tiers.contains(&Tier::Counts) && tiers.contains(&Tier::Notable));
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    // Rows, the newest and legs skip count chunks.
    let rows = r.rows(0, u64::MAX).unwrap();
    let held: u64 = a
        .entries()
        .iter()
        .filter(|e| !e.header.tier.is_counts())
        .map(|e| u64::from(e.header.rows))
        .sum();
    assert_eq!(rows.len() as u64, held);
    let (newest, _) = r.newest(0, u64::MAX, 100_000, |_| true).unwrap();
    assert_eq!(newest.len() as u64, held);
    assert!(rows.iter().all(|x| x.tier != Tier::Counts));
    r.legs_at(TICKS_PER_DAY + 10).unwrap();
    // The first days are only counts now; the histogram, from headers, still holds every row.
    let h = r.histogram(0, 11 * TICKS_PER_DAY, 11, None, |_| true);
    let all: f64 = h.values().flatten().sum();
    assert!((all - pushed as f64).abs() < 1.0, "{all} of {pushed}");
    let heat = r.count_rows(0, TICKS_PER_DAY - 1, Some(0)).unwrap();
    let notable_first_day = rows
        .iter()
        .filter(|x| match x.row {
            Row::Event(e) => e.tick < TICKS_PER_DAY,
            Row::Leg(l) => l.t0 < TICKS_PER_DAY,
        })
        .count() as u64;
    let counted: u64 = heat.iter().map(|c| u64::from(c.count)).sum();
    assert_eq!(counted + notable_first_day, 1_000);
    assert!(heat.iter().all(|c| !c.coarse && c.day == 0));
    assert!(r.count_rows(0, u64::MAX, Some(9)).unwrap().is_empty());
    assert_eq!(r.damaged(), 0);
}

#[test]
fn a_failed_flush_counts_its_rows_and_keeps_the_dictionary_due() {
    let dir = Dir::new("fail");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut d = Dictionary::new();
    let mut p = Pending::new();
    day(&mut p, &mut d, 0, 100, 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        // Root writes anyway; the test means nothing there.
        if fs::write(dir.path().join("probe"), b"").is_ok() {
            return;
        }
        let dict = d.clone();
        d.mark_saved();
        assert!(a.flush(p.take(), Stamp::default(), Some(&dict)).is_err());
        assert_eq!(a.dropped().rows(), 100);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        // The next flush has no new names, but writes the dictionary the failed one could not.
        day(&mut p, &mut d, 1, 10, 1);
        assert!(!d.is_dirty());
        a.flush(p.take(), Stamp::default(), None).unwrap();
        let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
        assert_eq!(r.dictionary().kinds().len(), dict.kinds().len());
        assert_eq!(r.dropped().rows(), 100);
        assert_eq!(r.rows(0, u64::MAX).unwrap().len(), 10);
    }
}
