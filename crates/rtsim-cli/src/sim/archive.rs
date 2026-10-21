// SPDX-License-Identifier: AGPL-3.0-or-later
//! The event archive's rate in a scenario's run (VIEWER.md §10.4, exit test (a) of stage V3): R, the
//! archive rows the run's modules record per game day.
//!
//! Every `rtsim-cli sim` scenario turns on the tape of each module engine its main run builds (the
//! world's two tapes too: in the game every module runs together), drains them all into one
//! [`Meter`] at least once a game day (a tape drops what it holds past 65,536 rows), and ends with
//! [`Rate::new`], one printed line ([`Rate::print`]) and an `"archive"` object in its JSON. A side
//! run beside the main one (a restored twin, a restart copy compared by hash) leaves its tapes off
//! and is not counted.

use std::collections::BTreeMap;

use rtsim_archive::record::Meter;
use rtsim_world::World;
use serde::Serialize;

/// What a run recorded, per game day: the scenario's `"archive"` JSON object.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Rate {
    /// R: rows (taken or dropped) per game day.
    pub rows_per_day: f64,
    /// Game days the 128 MiB full tier holds at R (null when nothing was recorded).
    pub full_days: Option<f64>,
    /// Game days the main run covered.
    pub days: f64,
    /// Rows in all, taken or dropped.
    pub rows: u64,
    /// Rows per game day by severity: routine, minor, notable, major.
    pub by_severity_per_day: [f64; 4],
    pub legs_per_day: f64,
    pub payload_bytes_per_day: f64,
    /// Rows a tape dropped past its cap, in all.
    pub dropped: u64,
    /// Rows taken, in all, by kind name.
    pub by_kind: BTreeMap<&'static str, u64>,
}

impl Rate {
    /// R and its parts for a run of `days` game days.
    pub fn new(m: &Meter, days: f64) -> Rate {
        let per_day = |n: u64| if days > 0.0 { n as f64 / days } else { 0.0 };
        let rows_per_day = m.per_day(days);
        Rate {
            rows_per_day,
            full_days: Meter::full_days(rows_per_day),
            days,
            rows: m.rows(),
            by_severity_per_day: m.by_severity.map(per_day),
            legs_per_day: per_day(m.legs),
            payload_bytes_per_day: per_day(m.payload_bytes),
            dropped: m.dropped,
            by_kind: m.by_kind.clone(),
        }
    }

    /// The one line every scenario prints, `label` naming a variant when a scenario has several.
    pub fn line(&self, label: Option<&str>) -> String {
        let s = self.by_severity_per_day;
        let full = self.full_days.map_or_else(
            || "the 128 MiB full tier never fills".to_string(),
            |d| format!("the 128 MiB full tier holds {} game days", count(d)),
        );
        format!(
            "archive{}: R = {} rows a game day (routine {}, minor {}, notable {}, major {} a day; {} legs a day; {} payload bytes a day; {} dropped); {full}",
            label.map_or(String::new(), |l| format!(" ({l})")),
            count(self.rows_per_day),
            count(s[0]),
            count(s[1]),
            count(s[2]),
            count(s[3]),
            count(self.legs_per_day),
            count(self.payload_bytes_per_day),
            self.dropped,
        )
    }

    pub fn print(&self, label: Option<&str>) {
        println!("{}", self.line(label));
    }

    /// The `n` kinds with the most rows, biggest first (ties by name).
    pub fn top_kinds(&self, n: usize) -> Vec<(&'static str, u64)> {
        let mut v: Vec<(&'static str, u64)> = self.by_kind.iter().map(|(k, c)| (*k, *c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        v.truncate(n);
        v
    }

    /// A second line for a scenario with no JSON to carry `by_kind`: its top five kinds.
    pub fn print_kinds(&self) {
        let kinds: Vec<String> = self
            .top_kinds(5)
            .iter()
            .map(|(k, c)| format!("{k} {}", commas(&c.to_string())))
            .collect();
        println!("  top kinds by rows: {}", kinds.join(", "));
    }
}

/// A count of something per day: whole numbers from 100 up, then one decimal from 10, then two
/// (a rate under one a day is not zero), with thousands separated by commas.
fn count(v: f64) -> String {
    let places = if v >= 100.0 {
        0
    } else if v >= 10.0 {
        1
    } else {
        2
    };
    let s = format!("{v:.places$}");
    match s.split_once('.') {
        Some((whole, frac)) => format!("{}.{frac}", commas(whole)),
        None => commas(&s),
    }
}

/// Digits with a comma between thousands.
fn commas(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Turns on both of a world's tapes (its own and its knowledge's).
pub fn world_on(w: &mut World) {
    w.tape.set_on(true);
    w.knowledge.tape.set_on(true);
}

/// Counts and empties both of a world's tapes.
pub fn drain_world(m: &mut Meter, w: &mut World) {
    m.drain(&mut w.tape);
    m.drain(&mut w.knowledge.tape);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_archive::kinds::{CONFLICT_RAID, TRAVEL_CARAVAN};
    use rtsim_archive::{At, Tape};

    #[test]
    fn a_rate_is_rows_per_day_and_the_days_the_full_tier_holds() {
        let mut t = Tape::new();
        t.set_on(true);
        for i in 0..1_000 {
            t.event(TRAVEL_CARAVAN, At::new(i, 0, 1, 2), 5, 6, 1);
        }
        t.leg(TRAVEL_CARAVAN, 0, (0, 0, 0), (100, 10, 10), 5);
        t.event(CONFLICT_RAID, At::new(7, 0, 3, 4), 8, 9, 12)
            .text(|| "loot: 12 iron ingot".into());
        let mut m = Meter::new();
        m.drain(&mut t);
        assert!(t.is_empty() && t.is_on(), "drained, still on");
        let r = Rate::new(&m, 3.0);
        assert_eq!(r.rows, 1_002);
        assert_eq!(r.rows_per_day, 334.0);
        let full = r.full_days.expect("rows were recorded");
        assert!((full - 4_194_304.0 / 334.0).abs() < 1e-9, "{full}");
        // The day's rows by severity (a caravan is routine, a raid major) and by kind.
        assert_eq!(r.by_severity_per_day, [1_001.0 / 3.0, 0.0, 0.0, 1.0 / 3.0]);
        assert_eq!(r.legs_per_day, 1.0 / 3.0);
        assert_eq!(r.payload_bytes_per_day, 19.0 / 3.0);
        assert_eq!(r.by_kind["travel.caravan"], 1_001);
        assert_eq!(r.by_kind["conflict.raid"], 1);
        assert_eq!(r.top_kinds(1), vec![("travel.caravan", 1_001)]);
        assert_eq!(
            r.line(None),
            "archive: R = 334 rows a game day (routine 334, minor 0.00, notable 0.00, major 0.33 a day; 0.33 legs a day; 6.33 payload bytes a day; 0 dropped); the 128 MiB full tier holds 12,558 game days"
        );
        assert!(
            r.line(Some("ignore"))
                .starts_with("archive (ignore): R = 334 ")
        );
        // A run with no rows has an R of nothing and a full tier that never fills.
        let none = Rate::new(&Meter::new(), 10.0);
        assert_eq!((none.rows_per_day, none.full_days), (0.0, None));
        assert!(none.line(None).ends_with("full tier never fills"));
        // The JSON object carries the figures under their names.
        let j = serde_json::to_value(&r).expect("json");
        assert_eq!(j["rows_per_day"], 334.0);
        assert_eq!(j["rows"], 1_002);
        assert_eq!(j["by_kind"]["conflict.raid"], 1);
        assert!(serde_json::to_value(&none).expect("json")["full_days"].is_null());
    }

    #[test]
    fn counts_group_thousands_and_keep_small_rates() {
        assert_eq!(count(0.0), "0.00");
        assert_eq!(count(0.254), "0.25");
        assert_eq!(count(12.34), "12.3");
        assert_eq!(count(123.4), "123");
        assert_eq!(count(1_234.6), "1,235");
        assert_eq!(count(4_194_304.0), "4,194,304");
        assert_eq!(count(1_000_000.4), "1,000,000");
    }
}
