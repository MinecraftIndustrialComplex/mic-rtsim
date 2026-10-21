// SPDX-License-Identifier: AGPL-3.0-or-later
//! The part format round trips: every mark kind, bucketing, counts, unknown kinds, the header and
//! the lease.

use std::collections::BTreeMap;

use rtsim_kernel::codec::Writer;
use rtsim_parts::mark::kind;
use rtsim_parts::part::kind as part_kind;
use rtsim_parts::store::{self, MAX_FOCUS};
use rtsim_parts::*;

fn every_kind() -> Vec<Mark> {
    vec![
        Mark::point(7, 10.5, -3.0)
            .with_ci(2)
            .with_label("Seven")
            .with_size(1.5)
            .with_glyph("star")
            .with_ring(),
        Mark::leg("l", [0.0, 0.0], [600.0, 10.0], 100, 200)
            .with_ci(1)
            .with_color("#123456"),
        Mark::Path {
            id: Some("p".into()),
            p: vec![[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]],
            ci: None,
            c: Some("#fff".into()),
            w: Some(1.0),
        },
        Mark::edge([0.0, 0.0], [1024.0, 0.0], 2.5),
        Mark::disc(Some("d".into()), -700.0, 30.0, 16.0).with_label("Disc"),
        Mark::rect(None, [0.0, 0.0, 16.0, 32.0]).with_ci(4),
        Mark::Cell {
            x0: 0.0,
            z0: 0.0,
            size: 16.0,
            w: 2,
            h: 1,
            v: vec![Some(1.0), None],
            ramp: Some("heat".into()),
            lo: Some(0.0),
            hi: None,
        },
        Mark::Cluster {
            x: 5.0,
            z: 5.0,
            n: 12,
            by: BTreeMap::from([("a".into(), 5), ("b".into(), 7)]),
        },
        Mark::Label {
            x: 1.0,
            z: -1.0,
            text: "a label".into(),
        },
        Mark::rings(
            Some("o".into()),
            vec![
                vec![[0.0, 0.0], [32.0, 0.0], [32.0, 16.0], [0.0, 16.0]],
                vec![[64.0, 0.0], [80.0, 0.0], [80.0, 16.0], [64.0, 16.0]],
            ],
        )
        .with_st(Some("dash"))
        .with_label("Outlined"),
    ]
}

#[test]
fn every_mark_kind_round_trips_with_its_fields() {
    let marks = every_kind();
    let mut w = PartWriter::marks(3, 100, false);
    for (i, m) in marks.iter().enumerate() {
        w.push_with(
            i as u64,
            u32::from(i == 4),
            m,
            &[
                (0, Value::I64(-(i as i64))),
                (1, Value::Str(format!("f{i}"))),
                (2, Value::F64(0.25)),
                (3, Value::Bool(true)),
            ],
        );
    }
    let parts = w.finish();
    assert_eq!(parts.len(), 1, "one unit, no regions");
    let (key, part) = &parts[0];
    assert_eq!(*key, PartKey::unit(3, 0));
    assert_eq!(part.rows as usize, marks.len());
    assert_eq!(part.crc, crc32fast::hash(&part.bytes));
    let v = read_part(&part.bytes).unwrap();
    assert_eq!(v.key, *key);
    let back: Vec<Mark> = v.rows.iter().map(|r| r.mark.clone().unwrap()).collect();
    assert_eq!(back, marks);
    assert_eq!(v.rows[4].frame, 1);
    assert_eq!(v.rows[2].fields[1], (1, Value::Str("f2".into())));
    // The counts add up to the rows, per region of each mark's anchor.
    let total: u32 = v.counts.iter().map(|c| c.2).sum();
    assert_eq!(total as usize, marks.len());
    assert!(
        v.counts
            .contains(&(RegionKey::of(1, -700.0, 30.0), kind::AREA, 1))
    );
    // The JSON is mic-viewer's: tagged by `k`.
    let j = serde_json::to_value(&marks[0]).unwrap();
    assert_eq!(j["k"], "point");
    assert_eq!(j["ring"], true);
}

#[test]
fn units_bucket_by_key_and_stay_put_layers_by_region_too() {
    let mut movers = PartWriter::marks(1, 10, false);
    let mut fixed = PartWriter::marks(2, 10, true);
    for k in 0..25u64 {
        let m = Mark::point(k, k as f64 * 100.0, 0.0);
        movers.push(k, 0, &m);
        fixed.push(k, 0, &m);
    }
    let movers = movers.finish();
    let units: Vec<u64> = movers.iter().map(|(k, _)| k.unit).collect();
    assert_eq!(units, vec![0, 1, 2]);
    assert!(movers.iter().all(|(k, _)| k.region.is_none()));
    let fixed = fixed.finish();
    // Unit 0 holds x 0..900: regions 0 (0..500) and 1 (600..900).
    let u0: Vec<(i32, u32)> = fixed
        .iter()
        .filter(|(k, _)| k.unit == 0)
        .map(|(k, p)| (k.region.unwrap().rx, p.rows))
        .collect();
    assert_eq!(u0, vec![(0, 6), (1, 4)]);
    let rows: u32 = fixed.iter().map(|(_, p)| p.rows).sum();
    assert_eq!(rows, 25);
    for (k, p) in &fixed {
        assert_eq!(read_part(&p.bytes).unwrap().key, *k);
        assert_eq!(k.unit_key(), PartKey::unit(2, k.unit));
    }
}

#[test]
fn unknown_row_kinds_are_skipped_by_their_length() {
    // A part with a V6 graph row between two marks, written by hand as a newer writer would.
    let mut w = PartWriter::new(part_kind::GRAPH, 0, 1, false);
    w.push(0, 0, &Mark::point(1, 0.0, 0.0));
    let (key, part) = w.finish().remove(0);
    let v = read_part(&part.bytes).unwrap();
    assert_eq!(v.key.kind, part_kind::GRAPH);
    // Splice in a row of kind GRAPH_EDGE with an arbitrary body before the point.
    let mut b = Writer::new();
    b.u32(u32::from_le_bytes(*b"MICP"));
    // The format's version (2: areas with rings).
    b.u8(2);
    key.encode(&mut b);
    b.var_u64(0);
    b.var_u64(2);
    b.u8(kind::GRAPH_EDGE);
    let mut body = Writer::new();
    body.var_u64(0);
    body.f64(1.0);
    body.raw(b"anything a later version writes");
    b.bytes(body.as_bytes());
    let mut point = Writer::new();
    point.var_u64(0);
    Mark::point(1, 0.0, 0.0).encode_body(&mut point);
    point.var_u64(0);
    b.u8(kind::POINT);
    b.bytes(point.as_bytes());
    let v = read_part(b.as_bytes()).unwrap();
    assert_eq!(v.rows.len(), 2);
    assert_eq!(v.rows[0].kind, kind::GRAPH_EDGE);
    assert!(v.rows[0].mark.is_none());
    assert!(v.rows[0].body.ends_with(b"anything a later version writes"));
    assert_eq!(v.rows[1].mark, Some(Mark::point(1, 0.0, 0.0)));
    assert_eq!(kind::name(kind::GRAPH_NODE), "graph_node");
}

#[test]
fn raw_rows_round_trip_with_their_bodies_and_regions() {
    let mut w = PartWriter::new(part_kind::ARCHIVE_ROWS, 3, 1 << 20, false);
    w.push_raw(0, 0, [10.0, -600.0], kind::ARCHIVE_ROW, &[7u8; 32]);
    w.push_raw(1, 2, [1e9, 0.0], kind::ARCHIVE_NAME, b"\x00\x05trade.sold");
    w.push(2, 0, &Mark::point(3, 513.0, 0.0));
    let (key, part) = w.finish().remove(0);
    assert_eq!(key.kind, part_kind::ARCHIVE_ROWS);
    assert_eq!(part.rows, 3);
    assert_eq!(
        part.regions,
        vec![
            RegionKey::of(0, 10.0, -600.0),
            RegionKey::of(0, 513.0, 0.0),
            RegionKey::of(2, 1e9, 0.0),
        ]
    );
    let v = read_part(&part.bytes).unwrap();
    assert_eq!(v.rows[0].kind, kind::ARCHIVE_ROW);
    assert_eq!(v.rows[0].body, vec![7u8; 32]);
    assert_eq!(v.rows[1].frame, 2);
    assert_eq!(v.rows[1].body, b"\x00\x05trade.sold".to_vec());
    assert!(v.rows[2].mark.is_some() && v.rows[2].body.is_empty());
    assert_eq!(kind::name(kind::ARCHIVE_ROW), "archive_row");
}

#[test]
fn damaged_parts_fail_to_read() {
    let mut w = PartWriter::marks(0, 1, false);
    w.push(0, 0, &Mark::point(1, 0.0, 0.0));
    let (_, p) = w.finish().remove(0);
    assert!(read_part(&p.bytes[..p.bytes.len() - 1]).is_err());
    let mut b = p.bytes.clone();
    b[0] ^= 1;
    assert!(read_part(&b).is_err());
}

#[test]
fn regions_pack_into_the_lease_form_and_back() {
    for (f, x, z) in [
        (0u32, 0.0, 0.0),
        (3, -1.0, 511.9),
        (65_535, -4.0e9, 4.0e9),
        (7, 1.0e6, -1.0e6),
    ] {
        let r = RegionKey::of(f, x, z);
        assert_eq!(RegionKey::unpack(r.pack()), r);
    }
    assert_eq!(
        RegionKey::of(0, -0.1, 512.0),
        RegionKey {
            frame: 0,
            rx: -1,
            rz: 1
        }
    );
    assert_eq!(RegionKey::of(0, f64::NAN, 0.0).rx, 0);
    assert_eq!(
        RegionKey {
            frame: 0,
            rx: 1,
            rz: -1
        }
        .bounds(),
        [512.0, -512.0, 1024.0, 0.0]
    );
}

#[test]
fn the_header_round_trips() {
    let h = Header {
        epoch: 9,
        commit: 3,
        game_time: 123_456,
        wall_ms: 1_700_000_000_000,
        tps: 19.5,
        build: "abc123".into(),
        cycle_ms: 850,
        cycles: 2,
        layers: vec![
            LayerState {
                version: 4,
                rows: 10,
                per_unit: 500,
                status: None,
                empty: Some("nothing".into()),
            },
            LayerState {
                version: 0,
                rows: 0,
                per_unit: 0,
                status: Some("the ships module is off".into()),
                empty: None,
            },
        ],
        units: vec![UnitEntry {
            key: PartKey::unit(0, 5),
            resident: false,
            at: 120_000,
            parts: vec![
                PartEntry {
                    region: Some(RegionKey {
                        frame: 1,
                        rx: -3,
                        rz: 4,
                    }),
                    page: 77,
                    crc: 0xdead_beef,
                    rows: 12,
                },
                PartEntry {
                    region: None,
                    page: 78,
                    crc: 1,
                    rows: 0,
                },
            ],
        }],
        stats: vec![("tap_ns".into(), 99)],
    };
    let b = h.encode();
    assert_eq!(Header::decode(&b).unwrap(), h);
    assert_eq!(h.stat("tap_ns"), Some(99));
    assert!(Header::decode(&b[..b.len() - 1]).is_err());
}

#[test]
fn the_lease_round_trips_and_keeps_at_most_64_regions() {
    let focus: Vec<RegionKey> = (0..100)
        .map(|i| RegionKey {
            frame: 0,
            rx: i,
            rz: -i,
        })
        .collect();
    let l = Lease { pid: 4242, focus };
    let back = Lease::decode(&l.encode()).unwrap();
    assert_eq!(back.pid, 4242);
    assert_eq!(back.focus.len(), MAX_FOCUS);
    assert_eq!(back.focus[..], l.focus[..MAX_FOCUS]);
    let dir = std::env::temp_dir().join(format!("rtsim-parts-lease-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    l.write(&dir).unwrap();
    assert_eq!(Lease::read(&dir).unwrap(), back);
    // Epoch folders sort by epoch and nothing else in view/ counts.
    for e in [3u64, 1, 0x10] {
        std::fs::create_dir_all(store::epoch_dir(&dir, e)).unwrap();
    }
    std::fs::create_dir_all(dir.join("view").join("junk")).unwrap();
    assert_eq!(store::epochs(&dir), vec![1, 3, 0x10]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn descriptors_carry_mic_viewers_json() {
    let d = LayerDesc::new("a.b", "a", "B", "leg", "#fff")
        .glyph("star", 3.0)
        .legend(&[("x", "#000")])
        .fields(&["Speed"]);
    let j = serde_json::to_value(&d).unwrap();
    for k in [
        "id",
        "group",
        "label",
        "description",
        "kinds",
        "priority",
        "default_on",
        "min_bpp",
        "max_bpp",
        "style",
        "legend",
        "has_cards",
        "has_past",
        "animated",
        "fields",
    ] {
        assert!(j.get(k).is_some(), "{k}");
    }
    assert_eq!(j["animated"], true);
    assert_eq!(j["style"]["glyph"], "star");
}
