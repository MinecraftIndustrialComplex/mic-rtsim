// SPDX-License-Identifier: AGPL-3.0-or-later
//! The archive folder end to end: flush, reopen, rollback, crash leftovers, damage, the read-only
//! reader, the histogram, legs at a tick and compaction's replacement.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rtsim_archive::chunk::{Chunk, ROWS_PER_CHUNK, Stamp, Tier};
use rtsim_archive::row::{self, Event, Handle, Leg, Row, Severity};
use rtsim_archive::store::{DICT_FILE, INDEX_FILE, chunk_name};
use rtsim_archive::{Archive, ArchiveReader, Dictionary, Pending};

/// A save's stamp: its game tick a thousand times its observation sequence.
fn st(obs_seq: u64, wall_ms: u64) -> Stamp {
    Stamp {
        obs_seq,
        tick: obs_seq * 1_000,
        wall_ms,
    }
}

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "rtsim-archive-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&d);
        Dir(d.join("rtsim").join("archive"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let root = self
            .0
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_path_buf();
        let _ = make_writable(&root);
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(mode))
}

fn make_writable(root: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        set_mode(root, 0o755)?;
        for e in fs::read_dir(root)? {
            let p = e?.path();
            if p.is_dir() {
                make_writable(&p)?;
            } else {
                set_mode(&p, 0o644)?;
            }
        }
    }
    Ok(())
}

fn event(dict: &mut Dictionary, tick: u64, kind: &str, sev: Severity, subject: u128) -> Row {
    Row::Event(Event {
        tick,
        kind: dict.kind(kind).unwrap(),
        frame: dict.frame("minecraft:overworld").unwrap(),
        severity: sev,
        x: (tick % 1000) as i32 - 500,
        z: (tick % 777) as i32,
        subject: dict.handle(subject),
        object: Handle::direct(3),
        magnitude: tick as i32,
        ..Event::default()
    })
}

fn leg(dict: &mut Dictionary, t0: u64, duration: u32, subject: u64) -> Row {
    Row::Leg(Leg {
        t0,
        duration,
        kind: dict.kind("dragons.leg").unwrap(),
        frame: dict.frame("minecraft:overworld").unwrap(),
        severity: Severity::Notable,
        x0: 0,
        z0: 0,
        x1: 1024,
        z1: -1024,
        subject: Handle::direct(subject),
        ..Leg::default()
    })
}

/// Pushes `n` events from tick `from`, every tenth notable with a payload, and returns them.
fn fill(p: &mut Pending, dict: &mut Dictionary, from: u64, n: u64) -> Vec<Row> {
    let mut rows = Vec::new();
    for t in from..from + n {
        let notable = t % 10 == 0;
        let sev = if notable {
            Severity::Notable
        } else {
            Severity::Routine
        };
        let kind = if t % 3 == 0 {
            "trade.deal"
        } else {
            "caravan.arrived"
        };
        let r = event(
            dict,
            t,
            kind,
            sev,
            if t % 7 == 0 {
                u128::MAX - u128::from(t)
            } else {
                u128::from(t)
            },
        );
        if notable {
            p.push_with_payload(&r, t.to_le_bytes().to_vec());
        } else {
            p.push(&r);
        }
        rows.push(r);
    }
    rows
}

#[test]
fn a_flush_reads_back_after_reopening() {
    let dir = Dir::new("flush");
    let mut a = Archive::open(dir.path()).unwrap();
    assert!(a.entries().is_empty() && !a.was_reset());
    let mut dict = a.dictionary().clone();
    let mut p = Pending::new();
    let rows = fill(&mut p, &mut dict, 1_000, 10_000);
    let seqs = a
        .flush(p.take(), st(5, 1_700_000_000_000), Some(&dict))
        .unwrap();
    assert_eq!(seqs, [0, 1, 2]);
    assert_eq!(
        a.entries()
            .iter()
            .map(|e| e.header.rows as usize)
            .collect::<Vec<_>>(),
        [ROWS_PER_CHUNK, ROWS_PER_CHUNK, 10_000 - 2 * ROWS_PER_CHUNK]
    );
    drop(a);
    let a = Archive::open(dir.path()).unwrap();
    assert_eq!(a.entries().len(), 3);
    assert_eq!(a.dictionary().kinds(), dict.kinds());
    assert_eq!(
        dir.files(),
        [
            chunk_name(0),
            chunk_name(1),
            chunk_name(2),
            DICT_FILE.into(),
            INDEX_FILE.into()
        ]
    );
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    assert_eq!(r.span(), Some((1_000, 10_999)));
    let back = r.rows(0, u64::MAX).unwrap();
    assert_eq!(back.len(), 10_000);
    for (got, want) in back.iter().zip(&rows) {
        let Row::Event(w) = want else { unreachable!() };
        let Row::Event(g) = got.row else {
            unreachable!()
        };
        assert_eq!(g.tick, w.tick);
        assert_eq!(g.subject, w.subject);
        assert_eq!(r.dictionary().resolve(g.subject), dict.resolve(w.subject));
        assert_eq!(got.payload.is_some(), w.tick % 10 == 0);
        if let Some(pl) = &got.payload {
            assert_eq!(pl[..], w.tick.to_le_bytes());
            assert!(g.payload);
        }
    }
    // A span in the middle reads only the rows in it.
    let mid = r.rows(5_000, 5_009).unwrap();
    assert_eq!(mid.len(), 10);
}

#[test]
fn rollback_cuts_chunks_above_the_selected_generation() {
    let dir = Dir::new("rollback");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    for (i, obs) in [10, 20, 30].into_iter().enumerate() {
        let mut p = Pending::new();
        fill(&mut p, &mut dict, i as u64 * 100, 100);
        a.flush(p.take(), st(obs, 0), dict.is_dirty().then_some(&dict))
            .unwrap();
        dict.mark_saved();
    }
    assert_eq!(a.entries().len(), 3);
    // The sidecar reading generation 20 ignores the chunk of 30 and cuts nothing.
    let r = ArchiveReader::open(dir.path(), Some(20)).unwrap().unwrap();
    assert_eq!(r.entries().len(), 2);
    assert_eq!(r.rows(0, u64::MAX).unwrap().len(), 200);
    assert_eq!(dir.files().len(), 5);
    // A crash after the save of 30 wrote its chunks but before its page commit: the engine selects
    // generation 20 and cuts.
    drop(a);
    let mut a = Archive::open(dir.path()).unwrap();
    assert_eq!(a.cut_above(20).unwrap(), 1);
    assert_eq!(a.cut_above(20).unwrap(), 0);
    assert!(!dir.files().contains(&chunk_name(2)));
    drop(a);
    let mut a = Archive::open(dir.path()).unwrap();
    assert_eq!(
        a.entries()
            .iter()
            .map(|e| e.header.obs_seq)
            .collect::<Vec<_>>(),
        [10, 20]
    );
    // New chunks never reuse a cut number.
    let mut p = Pending::new();
    fill(&mut p, &mut dict, 200, 10);
    assert_eq!(a.flush(p.take(), st(25, 0), None).unwrap(), [3]);
}

#[test]
fn crash_leftovers_are_cleared_on_open() {
    let dir = Dir::new("leftovers");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    let mut p = Pending::new();
    fill(&mut p, &mut dict, 0, 50);
    a.flush(p.take(), st(1, 0), Some(&dict)).unwrap();
    let entries = a.entries().to_vec();
    drop(a);
    // A chunk the index does not name (a flush cut short before its index), a temporary file, and
    // a stray file that is not the archive's.
    let orphan = Chunk::new(
        Tier::Full,
        vec![event(&mut dict, 9, "x", Severity::Major, 1).encode()],
        vec![],
        st(2, 0),
    );
    fs::write(dir.path().join(chunk_name(7)), orphan.encode()).unwrap();
    fs::write(dir.path().join("index.rtai.tmp"), b"half").unwrap();
    fs::write(dir.path().join("notes.txt"), b"keep").unwrap();
    // The reader ignores the orphan.
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    assert_eq!(r.entries(), &entries[..]);
    let a = Archive::open(dir.path()).unwrap();
    assert_eq!(a.entries(), &entries[..]);
    assert_eq!(
        dir.files(),
        [
            chunk_name(0),
            DICT_FILE.into(),
            INDEX_FILE.into(),
            "notes.txt".into()
        ]
    );
    // Numbers keep going past the orphan's.
    drop(a);
    fs::write(dir.path().join(chunk_name(7)), orphan.encode()).unwrap();
    let mut a = Archive::open(dir.path()).unwrap();
    let mut p = Pending::new();
    fill(&mut p, &mut dict, 100, 5);
    assert_eq!(a.flush(p.take(), st(3, 0), None).unwrap(), [8]);
}

#[test]
fn a_lost_or_damaged_index_is_rebuilt_from_the_chunks() {
    let dir = Dir::new("index");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    for obs in 1..=3 {
        let mut p = Pending::new();
        fill(&mut p, &mut dict, obs * 1_000, 20);
        a.flush(p.take(), st(obs, obs * 7), Some(&dict)).unwrap();
    }
    let entries = a.entries().to_vec();
    drop(a);
    // Damage the index: the reader scans headers without writing, the writer rebuilds it.
    let index = dir.path().join(INDEX_FILE);
    let mut bytes = fs::read(&index).unwrap();
    bytes[10] ^= 1;
    fs::write(&index, &bytes).unwrap();
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    assert_eq!(r.entries(), &entries[..]);
    assert_eq!(fs::read(&index).unwrap(), bytes);
    let a = Archive::open(dir.path()).unwrap();
    assert_eq!(a.entries(), &entries[..]);
    drop(a);
    // A lost index and a damaged chunk: the chunk goes, the rest are found.
    fs::remove_file(&index).unwrap();
    let c1 = dir.path().join(chunk_name(1));
    let mut c = fs::read(&c1).unwrap();
    c[12] ^= 0xFF;
    fs::write(&c1, c).unwrap();
    let a = Archive::open(dir.path()).unwrap();
    assert_eq!(
        a.entries().iter().map(|e| e.seq).collect::<Vec<_>>(),
        [0, 2]
    );
    assert!(!c1.exists());
    // An index naming a file that is gone drops the entry.
    fs::remove_file(dir.path().join(chunk_name(2))).unwrap();
    drop(a);
    let a = Archive::open(dir.path()).unwrap();
    assert_eq!(a.entries().iter().map(|e| e.seq).collect::<Vec<_>>(), [0]);
}

#[test]
fn the_reader_skips_a_chunk_damaged_after_indexing() {
    let dir = Dir::new("skip");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    for obs in 1..=3 {
        let mut p = Pending::new();
        fill(&mut p, &mut dict, obs * 1_000, 20);
        p.push(&leg(&mut dict, obs * 1_000, 10, obs));
        a.flush(p.take(), st(obs, obs * 7), Some(&dict)).unwrap();
    }
    drop(a);
    // The index still names chunk 1, but its body no longer matches its CRC.
    let c1 = dir.path().join(chunk_name(1));
    let mut c = fs::read(&c1).unwrap();
    let last = c.len() - 1;
    c[last] ^= 0xFF;
    fs::write(&c1, c).unwrap();
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    assert_eq!(r.entries().len(), 3);
    assert!(r.read(&r.entries()[1]).is_err());
    assert_eq!(r.rows(0, u64::MAX).unwrap().len(), 2 * 21);
    assert_eq!(r.damaged(), 1);
    assert!(r.legs_at(2_005).unwrap().is_empty());
    assert_eq!(r.legs_at(3_005).unwrap().len(), 1);
    assert_eq!(r.damaged(), 2);
    // The viewer's backend shares the reader between request threads.
    fn sync<T: Send + Sync>(_: &T) {}
    sync(&r);
}

#[test]
fn the_newest_rows_read_only_the_chunks_they_need() {
    let dir = Dir::new("newest");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    for obs in 1..=3 {
        let mut p = Pending::new();
        fill(&mut p, &mut dict, obs * 1_000, 20);
        a.flush(p.take(), st(obs, obs * 7), Some(&dict)).unwrap();
    }
    drop(a);
    let tick = |r: &rtsim_archive::Record| match r.row {
        Row::Event(e) => e.tick,
        Row::Leg(l) => l.t0,
    };
    // Chunk 0 is damaged: a query the newer chunks answer never opens it.
    let c0 = dir.path().join(chunk_name(0));
    let mut c = fs::read(&c0).unwrap();
    let last = c.len() - 1;
    c[last] ^= 0xFF;
    fs::write(&c0, c).unwrap();
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    let (rows, more) = r.newest(0, u64::MAX, 5, |_| true).unwrap();
    assert_eq!(
        rows.iter().map(tick).collect::<Vec<_>>(),
        [3_019, 3_018, 3_017, 3_016, 3_015]
    );
    assert!(more);
    assert_eq!(r.damaged(), 0);
    // A filter and a span: trade deals (ticks divisible by 3) up to 2,010.
    let deal = r
        .dictionary()
        .kinds()
        .iter()
        .position(|k| k == "trade.deal")
        .unwrap() as u16;
    let (rows, more) = r
        .newest(
            0,
            2_010,
            4,
            |row| matches!(row, Row::Event(e) if e.kind == deal),
        )
        .unwrap();
    assert_eq!(
        rows.iter().map(tick).collect::<Vec<_>>(),
        [2_010, 2_007, 2_004, 2_001]
    );
    assert!(more);
    assert_eq!(r.damaged(), 0);
    // Everything fits: no more, and the damaged chunk is skipped and counted.
    let (rows, more) = r.newest(0, u64::MAX, 100, |_| true).unwrap();
    assert_eq!(rows.len(), 40);
    assert!(!more);
    assert_eq!(r.damaged(), 1);
}

#[test]
fn a_damaged_dictionary_clears_the_archive() {
    let dir = Dir::new("dict");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    let mut p = Pending::new();
    fill(&mut p, &mut dict, 0, 10);
    a.flush(p.take(), st(1, 0), Some(&dict)).unwrap();
    drop(a);
    fs::write(dir.path().join(DICT_FILE), b"RTAD broken").unwrap();
    // The reader still opens, naming kinds by number.
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    assert_eq!(r.rows(0, 100).unwrap().len(), 10);
    assert_eq!(r.dictionary().kind_name(0), None);
    let a = Archive::open(dir.path()).unwrap();
    assert!(a.was_reset());
    assert!(a.entries().is_empty() && a.dictionary().kinds().is_empty());
    assert_eq!(dir.files(), [INDEX_FILE.to_string()]);
}

#[cfg(unix)]
#[test]
fn the_reader_works_on_a_read_only_copy() {
    let dir = Dir::new("readonly");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    let mut p = Pending::new();
    fill(&mut p, &mut dict, 0, 5_000);
    a.flush(p.take(), st(1, 0), Some(&dict)).unwrap();
    drop(a);
    for f in dir.files() {
        set_mode(&dir.path().join(f), 0o444).unwrap();
    }
    set_mode(dir.path(), 0o555).unwrap();
    let before = dir.files();
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    assert_eq!(r.rows(0, u64::MAX).unwrap().len(), 5_000);
    assert_eq!(dir.files(), before);
    // No archive at all.
    assert!(
        ArchiveReader::open(dir.path().join("nope"), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_histogram_reads_headers_only() {
    let dir = Dir::new("histogram");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    let mut p = Pending::new();
    // 1,000 rows, one a tick from 0: kinds alternate by tick % 3.
    fill(&mut p, &mut dict, 0, 1_000);
    a.flush(p.take(), st(1, 0), Some(&dict)).unwrap();
    let mut p = Pending::new();
    fill(&mut p, &mut dict, 5_000, 100);
    a.flush(p.take(), st(2, 0), None).unwrap();
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    let sum = |m: &std::collections::BTreeMap<u16, Vec<f64>>| -> Vec<f64> {
        let bins = m.values().next().map_or(0, Vec::len);
        (0..bins).map(|i| m.values().map(|v| v[i]).sum()).collect()
    };
    let all = sum(&r.histogram(0, 10_000, 10, None, |_| true));
    assert!((all[0] - 1_000.0).abs() < 1e-9, "{all:?}");
    assert!((all[5] - 100.0).abs() < 1e-9, "{all:?}");
    assert!((all.iter().sum::<f64>() - 1_100.0).abs() < 1e-9);
    // Finer bins split a chunk by its span.
    let fine = sum(&r.histogram(0, 1_000, 4, None, |_| true));
    assert!(fine.iter().all(|&v| (v - 250.0).abs() < 1e-9), "{fine:?}");
    // By kind: 334 deals in ticks 0 to 999, 33 in 5,000 to 5,099.
    let deals = dict.kinds().iter().position(|k| k == "trade.deal").unwrap() as u16;
    let by_kind = r.histogram(0, 10_000, 1, None, |_| true);
    assert_eq!(by_kind.len(), 2);
    assert!(
        (by_kind[&deals][0] - (334.0 + 33.0)).abs() < 1e-9,
        "{by_kind:?}"
    );
    let only = r.histogram(0, 10_000, 1, None, |k| k == deals);
    assert_eq!(only.keys().copied().collect::<Vec<_>>(), [deals]);
    // By frame: every row is in the overworld, frame 0.
    assert_eq!(r.histogram(0, 10_000, 1, Some(0), |_| true), by_kind);
    assert!(r.histogram(0, 10_000, 1, Some(1), |_| true).is_empty());
    assert!(r.histogram(0, 0, 3, None, |_| true).is_empty());
    // One pair of game tick and wall clock a save.
    assert_eq!(r.wall(), [(1_000, 0), (2_000, 0)]);
}

#[test]
fn legs_place_movers_at_any_tick_across_chunks() {
    let dir = Dir::new("legs");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    let mut p = Pending::new();
    // Powers of two keep the positions exact.
    p.push(&leg(&mut dict, 100, 1_024, 1));
    p.push(&leg(&mut dict, 150, 8, 2));
    a.flush(p.take(), st(1, 0), Some(&dict)).unwrap();
    let mut p = Pending::new();
    // A busy later save whose chunks start after the first dragon's leg began.
    fill(&mut p, &mut dict, 500, 5_000);
    p.push(&leg(&mut dict, 900, 512, 3));
    a.flush(p.take(), st(2, 0), Some(&dict)).unwrap();
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    let at = |t| {
        let mut v: Vec<_> = r
            .legs_at(t)
            .unwrap()
            .into_iter()
            .map(|l| (l.subject.value, l.at(t)))
            .collect();
        v.sort_by_key(|x| x.0);
        v
    };
    assert_eq!(at(154), [(1, (54.0, -54.0)), (2, (512.0, -512.0))]);
    assert_eq!(at(1_000), [(1, (900.0, -900.0)), (3, (200.0, -200.0))]);
    assert_eq!(at(1_200), [(3, (600.0, -600.0))]);
    assert!(at(99).is_empty());
    // The first chunk's header says its legs reach past its last start.
    assert_eq!(
        (
            r.entries()[0].header.last_tick,
            r.entries()[0].header.last_end
        ),
        (150, 1_124)
    );
}

#[test]
fn a_replacement_swaps_chunks_in_one_index_write() {
    let dir = Dir::new("replace");
    let mut a = Archive::open(dir.path()).unwrap();
    let mut dict = Dictionary::new();
    for obs in 1..=2 {
        let mut p = Pending::new();
        fill(&mut p, &mut dict, obs * 100, 50);
        a.flush(p.take(), st(obs, 0), Some(&dict)).unwrap();
    }
    // Keep the notable rows of both, as the notable tier would.
    let mut rows = Vec::new();
    for seq in [0, 1] {
        rows.extend(
            a.read(seq)
                .unwrap()
                .rows
                .into_iter()
                .filter(|r| row::severity(r) >= Severity::Notable),
        );
    }
    let merged = Chunk::new(Tier::Notable, rows, vec![], st(2, 0));
    assert_eq!(a.replace(&[0, 1], vec![merged]).unwrap(), [2]);
    assert_eq!(
        dir.files(),
        [chunk_name(2), DICT_FILE.into(), INDEX_FILE.into()]
    );
    let r = ArchiveReader::open(dir.path(), None).unwrap().unwrap();
    let back = r.rows(0, u64::MAX).unwrap();
    assert_eq!(back.len(), 10);
    assert!(
        back.iter()
            .all(|b| b.tier == Tier::Notable && b.row.severity() == Severity::Notable)
    );
}
