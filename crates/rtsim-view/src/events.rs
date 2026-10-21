// SPDX-License-Identifier: AGPL-3.0-or-later
//! The event archive as the page's timeline (VIEWER.md §10.6): `/api/events`, the histogram and the
//! kinds, over a read-only [`ArchiveReader`] of the save's own archive. Chunks tagged above the
//! generation being viewed are ignored (§10.5), so the timeline ends where the save does.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use mic_viewer::api::{EventKind, EventRow, EventsAnswer, Histogram, LegRow, Tick};
use mic_viewer::viewer::{EventQuery, MAX_BUCKETS};
use rtsim_archive::row::{Event, Handle, Leg, Row, Severity};
use rtsim_archive::{ArchiveReader, Record, dir_for, kinds};

/// Save-to-tick pairs the histogram sends at most beyond its buckets.
const MAX_WALL_PAIRS: usize = 64;

/// A world's archive, read as of one generation.
pub struct Events {
    reader: ArchiveReader,
    /// The page's frame id for each archive frame number.
    frames: Vec<String>,
    kinds: Vec<EventKind>,
}

impl Events {
    /// The archive of the world at `world`, as of the generation with `obs_seq`; `None` when the
    /// world has none. `frame_id` turns an archive frame name into the page's frame id.
    pub fn open(
        world: &Path,
        obs_seq: u64,
        frame_id: impl Fn(&str) -> String,
    ) -> io::Result<Option<Events>> {
        let Some(reader) = ArchiveReader::open(dir_for(world), Some(obs_seq))? else {
            return Ok(None);
        };
        let dict = reader.dictionary();
        let frames = dict.frames().iter().map(|f| frame_id(f)).collect();
        let mut kinds: Vec<EventKind> = dict.kinds().iter().map(|n| kind_info(n)).collect();
        kinds.sort_by(|a, b| (&a.group, &a.id).cmp(&(&b.group, &b.id)));
        Ok(Some(Events {
            reader,
            frames,
            kinds,
        }))
    }

    /// `/api/world`'s `event_kinds`.
    pub fn kinds(&self) -> &[EventKind] {
        &self.kinds
    }

    /// Whether there is anything to show.
    pub fn any(&self) -> bool {
        !self.reader.entries().is_empty()
    }

    /// The archive's own frame names (`dim0`), in frame number order.
    pub fn frame_numbers(&self) -> impl Iterator<Item = &str> + '_ {
        self.reader.dictionary().frames().iter().map(String::as_str)
    }

    /// Chunks indexed (as of the generation viewed).
    pub fn chunks(&self) -> usize {
        self.reader.entries().len()
    }

    /// The first and last archived tick.
    pub fn span(&self) -> Option<(u64, u64)> {
        self.reader.span()
    }

    /// Rows the engine dropped (the buffer's cap, an unnamed frame), and damaged chunks skipped.
    pub fn losses(&self) -> (u64, u32) {
        let d = self.reader.dropped();
        (
            d.by_severity.iter().sum::<u64>() + d.no_frame,
            self.reader.damaged(),
        )
    }

    /// `/api/events`: the newest rows in `from..to` in the frame and area, at most `limit` events
    /// and `limit` legs.
    pub fn events(&self, q: &EventQuery) -> EventsAnswer {
        let Some((t0, t1)) = span(q.from, q.to) else {
            return EventsAnswer::default();
        };
        let frame = match self.frame_filter(q.frame.as_deref()) {
            Ok(f) => f,
            Err(()) => return EventsAnswer::default(),
        };
        let wanted: Vec<bool> = self
            .reader
            .dictionary()
            .kinds()
            .iter()
            .map(|k| q.wants(k))
            .collect();
        let kind_ok = |k: u16| wanted.get(k as usize).copied().unwrap_or(false);
        let frame_ok = |f: u8| frame.is_none_or(|want| want == f);
        let in_area = |x0: i32, z0: i32, x1: i32, z1: i32| {
            q.area.is_none_or(|b| {
                let (lx, hx) = (f64::from(x0.min(x1)), f64::from(x0.max(x1)));
                let (lz, hz) = (f64::from(z0.min(z1)), f64::from(z0.max(z1)));
                hx >= b[0] && lx <= b[2] && hz >= b[1] && lz <= b[3]
            })
        };
        let events = self.reader.newest(t0, t1, q.limit, |r| match r {
            Row::Event(e) => kind_ok(e.kind) && frame_ok(e.frame) && in_area(e.x, e.z, e.x, e.z),
            Row::Leg(_) => false,
        });
        let legs = self.reader.newest(t0, t1, q.limit, |r| match r {
            Row::Leg(l) => kind_ok(l.kind) && frame_ok(l.frame) && in_area(l.x0, l.z0, l.x1, l.z1),
            Row::Event(_) => false,
        });
        // An unreadable folder (permissions, a disk error) answers empty; damaged chunks were
        // skipped already.
        let (events, more_events) = events.unwrap_or_default();
        let (legs, more_legs) = legs.unwrap_or_default();
        EventsAnswer {
            rows: events.iter().filter_map(|r| self.event_row(r)).collect(),
            legs: legs.iter().filter_map(|r| self.leg_row(r)).collect(),
            truncated: more_events || more_legs,
        }
    }

    /// `/api/events/histogram`: rows per kind per bucket from the chunk headers (no area: headers
    /// only know a chunk's box). An open `from` starts at the first archived tick; an open `to`
    /// ends after `now`.
    pub fn histogram(&self, q: &EventQuery, now: Tick) -> Histogram {
        let first = self.reader.span().map_or(0, |s| s.0 as Tick);
        let last = self.reader.span().map_or(0, |s| s.1 as Tick);
        let from = if q.from == Tick::MIN { first } else { q.from };
        let to = if q.to == Tick::MAX {
            now.max(last).max(from) + 1
        } else {
            q.to.max(from + 1)
        };
        let buckets = q.limit.clamp(1, MAX_BUCKETS);
        let mut h = Histogram {
            from,
            to,
            buckets,
            series: BTreeMap::new(),
            wall: self.wall(from, to),
        };
        let Ok(frame) = self.frame_filter(q.frame.as_deref()) else {
            return h;
        };
        let dict = self.reader.dictionary();
        let raw =
            self.reader
                .histogram(from.max(0) as u64, to.max(0) as u64, buckets, frame, |k| {
                    dict.kind_name(k).is_some_and(|n| q.wants(n))
                });
        for (k, v) in raw {
            if let Some(name) = dict.kind_name(k) {
                h.series.insert(name.to_owned(), whole(&v));
            }
        }
        h
    }

    /// The archive frame a query's frame names: `Ok(None)` for every frame, `Err` for a frame
    /// with no archived rows.
    fn frame_filter(&self, frame: Option<&str>) -> Result<Option<u8>, ()> {
        match frame {
            None => Ok(None),
            Some(f) => self
                .frames
                .iter()
                .position(|id| id == f)
                .map(|i| Some(i as u8))
                .ok_or(()),
        }
    }

    /// The wall clock against game ticks over `from..to`, thinned to [`MAX_WALL_PAIRS`] with the
    /// pairs either side kept, so the page can interpolate its edges.
    fn wall(&self, from: Tick, to: Tick) -> Vec<[i64; 2]> {
        let all: Vec<[i64; 2]> = self
            .reader
            .wall()
            .into_iter()
            .map(|(t, ms)| [t as i64, ms as i64])
            .collect();
        let lo = all.partition_point(|p| p[0] < from).saturating_sub(1);
        let hi = (all.partition_point(|p| p[0] <= to) + 1).min(all.len());
        let inside = &all[lo..hi];
        if inside.len() <= MAX_WALL_PAIRS {
            return inside.to_vec();
        }
        let n = inside.len() - 1;
        (0..MAX_WALL_PAIRS)
            .map(|i| inside[i * n / (MAX_WALL_PAIRS - 1)])
            .collect()
    }

    fn kind_name(&self, k: u16) -> String {
        self.reader
            .dictionary()
            .kind_name(k)
            .map_or_else(|| format!("kind{k}"), str::to_owned)
    }

    fn frame_name(&self, f: u8) -> Option<String> {
        self.frames.get(f as usize).cloned()
    }

    /// An entity as the page names it: decimal, a UUID as a UUID; none for id 0.
    fn id(&self, h: Handle) -> Option<String> {
        if !h.dict && h.value == 0 {
            return None;
        }
        let v = self.reader.dictionary().resolve(h)?;
        Some(if v > u128::from(u64::MAX) {
            format!(
                "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                v >> 96,
                (v >> 80) & 0xFFFF,
                (v >> 64) & 0xFFFF,
                (v >> 48) & 0xFFFF,
                v & 0xFFFF_FFFF_FFFF
            )
        } else {
            v.to_string()
        })
    }

    fn event_row(&self, r: &Record) -> Option<EventRow> {
        let Row::Event(e) = r.row else { return None };
        let Event {
            tick,
            kind,
            frame,
            severity,
            x,
            z,
            subject,
            object,
            magnitude,
            ..
        } = e;
        Some(EventRow {
            t: tick as Tick,
            kind: self.kind_name(kind),
            sev: sev(severity),
            x: f64::from(x),
            z: f64::from(z),
            subject: self.id(subject),
            object: self.id(object),
            mag: i64::from(magnitude),
            text: r
                .payload
                .as_deref()
                .and_then(|p| std::str::from_utf8(p).ok())
                .unwrap_or_default()
                .to_owned(),
            frame: self.frame_name(frame),
        })
    }

    fn leg_row(&self, r: &Record) -> Option<LegRow> {
        let Row::Leg(l) = r.row else { return None };
        let Leg {
            t0,
            kind,
            x0,
            z0,
            x1,
            z1,
            subject,
            ..
        } = l;
        Some(LegRow {
            t0: t0 as Tick,
            t1: l.end() as Tick,
            kind: self.kind_name(kind),
            subject: self.id(subject),
            p: vec![[f64::from(x0), f64::from(z0), f64::from(x1), f64::from(z1)]],
        })
    }
}

/// A query's half-open `from..to` as the archive's inclusive ticks; `None` when empty.
fn span(from: Tick, to: Tick) -> Option<(u64, u64)> {
    let t0 = from.max(0) as u64;
    let t1 = if to == Tick::MAX {
        u64::MAX
    } else {
        u64::try_from(to.checked_sub(1)?).ok()?
    };
    (t0 <= t1).then_some((t0, t1))
}

/// The page's three levels: routine and minor 0, notable 1, major 2.
fn sev(s: Severity) -> u8 {
    match s {
        Severity::Routine | Severity::Minor => 0,
        Severity::Notable => 1,
        Severity::Major => 2,
    }
}

/// Spread counts as whole numbers that keep the total: each bucket is the rounded running sum
/// less the one before, so a small chunk spread over many buckets still shows.
fn whole(v: &[f64]) -> Vec<u64> {
    let mut sum = 0.0;
    let mut prev = 0u64;
    v.iter()
        .map(|x| {
            sum += x;
            let now = sum.round() as u64;
            let d = now.saturating_sub(prev);
            prev = now.max(prev);
            d
        })
        .collect()
}

/// A kind's label, group, colour and usual severity: the registry's, else from its name.
fn kind_info(name: &str) -> EventKind {
    let group = kinds::group(name).to_owned();
    let (label, severity) = match kinds::info(name) {
        Some(k) => (k.label.to_owned(), sev(k.severity)),
        None => (kinds::fallback_label(name), 0),
    };
    EventKind {
        id: name.to_owned(),
        label,
        color: group_color(&group).to_owned(),
        group,
        severity,
    }
}

/// One colour a group, readable on both themes.
fn group_color(group: &str) -> &'static str {
    match group {
        "conflict" => "#e5484d",
        "dragons" => "#f76b15",
        "herds" => "#46a758",
        "ships" => "#0090ff",
        "trade" => "#d6a10f",
        "settle" | "world" => "#12a594",
        "travel" => "#00a2c7",
        "knowledge" => "#8e4ec6",
        "industry" => "#ad7f58",
        other => {
            const MORE: [&str; 4] = ["#d6409f", "#7c66dc", "#978365", "#5eb1ef"];
            let h = other
                .bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
            MORE[h as usize % MORE.len()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_archive::{Archive, Dictionary, Pending, Stamp};

    struct Tmp(std::path::PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn world(tag: &str) -> Tmp {
        let d =
            std::env::temp_dir().join(format!("rtsim-view-events-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Tmp(d)
    }

    fn ev(d: &mut Dictionary, tick: u64, kind: &str, frame: &str, sev: Severity, x: i32) -> Row {
        Row::Event(Event {
            tick,
            kind: d.kind(kind).unwrap(),
            frame: d.frame(frame).unwrap(),
            severity: sev,
            x,
            z: -x,
            subject: d.handle(if tick.is_multiple_of(2) { 7 } else { u128::MAX }),
            object: Handle::direct(0),
            magnitude: 3,
            ..Event::default()
        })
    }

    /// Two saves: obs 10 (ticks 1,000 to 1,099) and obs 20 (2,000 to 2,099), overworld raids and
    /// nether deals, one dragon leg a save.
    fn archive(w: &Path) {
        let mut a = Archive::open(dir_for(w)).unwrap();
        let mut d = Dictionary::new();
        for (obs, base) in [(10u64, 1_000u64), (20, 2_000)] {
            let mut p = Pending::new();
            for t in base..base + 100 {
                let r = if t % 4 == 0 {
                    ev(&mut d, t, "trade.deal", "dim1", Severity::Routine, 50)
                } else {
                    ev(
                        &mut d,
                        t,
                        "conflict.raid",
                        "dim0",
                        Severity::Notable,
                        t as i32 - 1_500,
                    )
                };
                if t % 10 == 1 {
                    p.push_with_payload(&r, format!("loot at {t}").into_bytes());
                } else {
                    p.push(&r);
                }
            }
            p.push(&Row::Leg(Leg {
                t0: base,
                duration: 64,
                kind: d.kind("dragons.flight").unwrap(),
                frame: d.frame("dim0").unwrap(),
                severity: Severity::Notable,
                x0: 0,
                z0: 0,
                x1: 640,
                z1: 0,
                subject: Handle::direct(99),
                ..Leg::default()
            }));
            a.flush(
                p.take(),
                Stamp {
                    obs_seq: obs,
                    tick: base + 100,
                    wall_ms: 1_000_000 + base,
                },
                Some(&d),
            )
            .unwrap();
        }
    }

    fn names(f: &str) -> String {
        match f {
            "dim0" => "minecraft:overworld".into(),
            other => other.into(),
        }
    }

    fn q(frame: &str, from: Tick, to: Tick, limit: usize) -> EventQuery {
        EventQuery {
            frame: Some(frame.into()),
            from,
            to,
            limit,
            ..EventQuery::default()
        }
    }

    #[test]
    fn no_archive_is_none() {
        let w = world("none");
        assert!(Events::open(&w.0, 5, names).unwrap().is_none());
    }

    #[test]
    fn events_are_newest_first_in_frame_kind_and_area() {
        let w = world("events");
        archive(&w.0);
        let e = Events::open(&w.0, 20, names).unwrap().unwrap();
        assert!(e.any());
        let ids: Vec<&str> = e.kinds().iter().map(|k| k.id.as_str()).collect();
        assert_eq!(ids, ["conflict.raid", "dragons.flight", "trade.deal"]);
        assert_eq!(e.kinds()[0].label, "Raid");
        assert_eq!(e.kinds()[0].color, "#e5484d");
        let a = e.events(&q("minecraft:overworld", Tick::MIN, Tick::MAX, 5));
        assert!(a.truncated);
        assert_eq!(
            a.rows.iter().map(|r| r.t).collect::<Vec<_>>(),
            [2_099, 2_098, 2_097, 2_095, 2_094]
        );
        let r = &a.rows[0];
        assert_eq!(
            (r.kind.as_str(), r.sev, r.x, r.z, r.mag),
            ("conflict.raid", 1, 599.0, -599.0, 3)
        );
        assert_eq!(r.frame.as_deref(), Some("minecraft:overworld"));
        assert_eq!(
            r.subject.as_deref(),
            Some("ffffffff-ffff-ffff-ffff-ffffffffffff")
        );
        assert_eq!(r.object, None);
        assert_eq!(a.rows[1].subject.as_deref(), Some("7"));
        assert_eq!(a.legs.len(), 2);
        assert_eq!(
            (a.legs[0].t0, a.legs[0].t1, a.legs[0].subject.as_deref()),
            (2_000, 2_064, Some("99"))
        );
        assert_eq!(a.legs[0].p, [[0.0, 0.0, 640.0, 0.0]]);
        // Payloads are the row's text; to is exclusive.
        let a = e.events(&q("minecraft:overworld", 1_000, 1_012, 100));
        assert!(!a.truncated);
        assert_eq!(a.rows.len(), 9);
        assert_eq!(
            a.rows.iter().find(|r| r.t == 1_001).unwrap().text,
            "loot at 1001"
        );
        assert_eq!(a.rows.iter().find(|r| r.t == 1_002).unwrap().text, "");
        // The nether's deals; a kind filter; an area that holds a few raids.
        assert_eq!(e.events(&q("dim1", 2_000, 2_100, 100)).rows.len(), 25);
        let mut kq = q("minecraft:overworld", 0, 3_000, 1_000);
        kq.kinds = vec!["dragons.flight".into()];
        let a = e.events(&kq);
        assert!(a.rows.is_empty() && a.legs.len() == 2);
        let mut aq = q("minecraft:overworld", 0, 3_000, 1_000);
        aq.area = Some([500.0, -600.0, 600.0, -500.0]);
        let a = e.events(&aq);
        assert!(a.rows.iter().all(|r| (500.0..=600.0).contains(&r.x)));
        assert_eq!(a.rows.len(), 75);
        assert!(a.legs.is_empty(), "the dragon flew along z = 0");
        // A frame with nothing archived, an empty span.
        assert!(
            e.events(&q("minecraft:the_end", 0, 3_000, 10))
                .rows
                .is_empty()
        );
        assert!(e.events(&q("dim1", 5, 5, 10)).rows.is_empty());
        assert!(e.events(&q("dim1", Tick::MIN, 0, 10)).rows.is_empty());
    }

    #[test]
    fn an_older_generation_ends_the_timeline_there() {
        let w = world("older");
        archive(&w.0);
        let e = Events::open(&w.0, 10, names).unwrap().unwrap();
        let a = e.events(&q("minecraft:overworld", Tick::MIN, Tick::MAX, 1));
        assert_eq!(a.rows[0].t, 1_099);
    }

    #[test]
    fn the_histogram_keeps_totals_and_the_wall_clock() {
        let w = world("hist");
        archive(&w.0);
        let e = Events::open(&w.0, 20, names).unwrap().unwrap();
        let h = e.histogram(&q("minecraft:overworld", Tick::MIN, Tick::MAX, 30), 2_100);
        assert_eq!((h.from, h.to, h.buckets), (1_000, 2_101, 30));
        assert_eq!(h.series["conflict.raid"].iter().sum::<u64>(), 150);
        assert_eq!(h.series["dragons.flight"].iter().sum::<u64>(), 2);
        assert!(!h.series.contains_key("trade.deal"));
        assert_eq!(h.wall, [[1_100, 1_001_000], [2_100, 1_002_000]]);
        let h = e.histogram(&q("dim1", 2_000, 2_100, 4), 2_100);
        assert_eq!(h.series["trade.deal"], [6, 7, 6, 6]);
        assert!(
            e.histogram(&q("minecraft:the_end", 0, 10, 4), 0)
                .series
                .is_empty()
        );
    }

    #[test]
    fn whole_numbers_keep_the_sum() {
        assert_eq!(whole(&[0.3; 10]), [0, 1, 0, 0, 1, 0, 0, 0, 1, 0]);
        assert_eq!(whole(&[0.3; 10]).iter().sum::<u64>(), 3);
        assert_eq!(whole(&[2.0, 0.0, 5.0]), [2, 0, 5]);
    }
}
