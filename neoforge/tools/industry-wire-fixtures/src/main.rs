// SPDX-License-Identifier: AGPL-3.0-or-later
//! Prints the cross-language wire fixtures: one sample record of each industry kind the Java test
//! (`IndustryWireContractTest`) pins, encoded by the reference codec `rtsim_industry::wire`, plus the
//! enum code tables. The output is committed as
//! `neoforge/core/src/test/resources/industry-wire-fixtures.txt`; regenerate it with
//!
//! ```text
//! cd neoforge/tools/industry-wire-fixtures
//! nix develop ../../..# -c cargo run --release --quiet > ../../core/src/test/resources/industry-wire-fixtures.txt
//! ```
//!
//! Every field of a record gets a different, non-zero-byte value (derived from the record's name, so
//! a record's values do not move when another is added), which means a wrong offset on either side
//! shows up as a mismatch rather than hiding behind a repeated or zero value. Each record is decoded
//! again by `wire::decode_obs` or `wire::decode_cmd` before it is printed, so the fixtures are
//! also checked against the reference decoder.

mod glass;

use std::collections::HashMap;
use std::fmt::{Display, Write as _};

use rtsim_industry::cmd::{
    BlockDestroy, CarriageDelta, CarriageReason, ChunkReconcile, Cmd, DepositDelta, InvDelta,
    WritePackage,
};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodVec, StationId, TrainId};
use rtsim_industry::obs::{
    BlockFlags, CargoFrom, Carriage, CarriageResult, CarriageResultEntry, Conditions,
    DefenceSurvey, DepositPluginId, DepositSample, Direction, DiscardReason, DistrictDecl,
    EditKind, ExposedBlock, FightObserved, InvKind, InventoryDecl, LoadState, NotLearnable, Obs,
    PackageEvent, PortKind, PortTransfer, ReconcileResult, ResultEntry, SpawnSample, StationDecl,
    StationPorts, SweepEnd, Timed, TrainCall, TrainDeparture,
};
use rtsim_industry::threat::ThreatClass;
use rtsim_industry::wire;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::{JAVA_OP_BIT, OpId};

const HEADER: &str = "\
# Generated from crates/rtsim-industry/src/wire.rs (encode_obs, encode_cmd) by neoforge/tools/industry-wire-fixtures. Do not edit by hand.
# Regenerate: cd neoforge/tools/industry-wire-fixtures && nix develop ../../..# -c cargo run --release --quiet > ../../core/src/test/resources/industry-wire-fixtures.txt
# Lines: '<NAME> <hex>' is one encoded record (little-endian, header included, hex bytes without separators);
# '<NAME>.fields k=v ...' lists the values it was built from; 'CODES.<group> Variant=code ...' is an enum code table.
# Values: decimal integers (u32 and u64 unsigned, i32 and i64 signed), Rust variant names for enums, true/false, and
# 'none' for an absent Option. Journaled observations carry obs_seq at 8 and game_time at 16, latest-wins ones game_time at 8,
# commands cmd_seq at 8. NAME is a wire.rs kind, or a kind plus a suffix for a variant of the same kind.
";

// ---------------------------------------------------------------------------------------------
// Distinct, recognizable values.

/// A deterministic value source seeded by a record's name.
struct Gen(u64);

#[derive(Clone, Copy)]
enum Sign {
    Any,
    Pos,
    Neg,
}

impl Gen {
    fn new(name: &str) -> Gen {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for b in name.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Gen(h)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// `N` little-endian bytes, none of them zero; the top byte keeps the requested sign (for a
    /// signed value read as two's complement).
    fn bytes<const N: usize>(&mut self, sign: Sign) -> [u8; N] {
        let r = self.next().to_le_bytes();
        let mut out = [0u8; N];
        for i in 0..N {
            out[i] = if r[i] == 0 { 0xA5 } else { r[i] };
        }
        match sign {
            Sign::Any => {}
            Sign::Pos => out[N - 1] = 1 + out[N - 1] % 0x7F,
            Sign::Neg => out[N - 1] |= 0x80,
        }
        out
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.bytes::<4>(Sign::Any))
    }

    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.bytes::<8>(Sign::Any))
    }

    fn i32(&mut self, sign: Sign) -> i32 {
        i32::from_le_bytes(self.bytes::<4>(sign))
    }

    fn i64(&mut self, sign: Sign) -> i64 {
        i64::from_le_bytes(self.bytes::<8>(sign))
    }

    /// A float that prints and parses exactly: a whole number of eighths, not zero.
    fn f32(&mut self) -> f32 {
        let n = (self.next() % 1_000_000) as f32 + 1.0;
        n / 8.0
    }

    /// An op id the engine made: the top bit clear.
    fn engine_op(&mut self) -> OpId {
        OpId::engine(self.u64() & !JAVA_OP_BIT)
    }

    /// An op id the Java side made: the top bit set.
    fn bridge_op(&mut self) -> OpId {
        OpId::bridge(self.u64() & !JAVA_OP_BIT)
    }
}

/// The `name=value` pairs of one record. Numeric values must all differ within a record.
#[derive(Default)]
struct Fields {
    items: Vec<String>,
    seen: HashMap<String, String>,
}

impl Fields {
    fn num(&mut self, name: &str, v: impl Display) {
        let s = v.to_string();
        if let Some(prev) = self.seen.insert(s.clone(), name.to_string()) {
            panic!("fields {prev} and {name} both have the value {s}");
        }
        self.items.push(format!("{name}={s}"));
    }

    fn u32(&mut self, name: &str, v: u32) -> u32 {
        self.num(name, v);
        v
    }

    fn u64(&mut self, name: &str, v: u64) -> u64 {
        self.num(name, v);
        v
    }

    fn i32(&mut self, name: &str, v: i32) -> i32 {
        self.num(name, v);
        v
    }

    fn i64(&mut self, name: &str, v: i64) -> i64 {
        self.num(name, v);
        v
    }

    fn f32(&mut self, name: &str, v: f32) -> f32 {
        self.num(name, v);
        v
    }

    /// A value that may repeat: a name, a boolean, a count.
    fn sym(&mut self, name: &str, v: impl Display) {
        self.items.push(format!("{name}={v}"));
    }

    fn line(&self) -> String {
        self.items.join(" ")
    }
}

fn begin(name: &str) -> (Gen, Fields) {
    (Gen::new(name), Fields::default())
}

/// The game time: positive, so it reads the same as an i64 and a Tick.
fn game_time(f: &mut Fields, g: &mut Gen) -> Tick {
    Tick(f.i64("game_time", g.i64(Sign::Pos)) as u64)
}

fn district(f: &mut Fields, g: &mut Gen) -> DistrictId {
    DistrictId(f.u64("district", g.u64()))
}

/// A block position: x and z negative, y positive, so a swapped axis or a lost sign shows.
fn pos(f: &mut Fields, g: &mut Gen, prefix: &str) -> BlockPos {
    let x = f.i32(&format!("{prefix}x"), g.i32(Sign::Neg));
    let y = f.i32(&format!("{prefix}y"), g.i32(Sign::Pos));
    let z = f.i32(&format!("{prefix}z"), g.i32(Sign::Neg));
    BlockPos::new(x, y, z)
}

/// `n` goods entries, named `<prefix>0.good`, `<prefix>0.amount`, ...; amounts alternate in sign.
fn goods(f: &mut Fields, g: &mut Gen, prefix: &str, n: usize) -> GoodVec {
    let pairs: Vec<(Good, i64)> = (0..n)
        .map(|i| {
            let sign = if i % 2 == 0 { Sign::Pos } else { Sign::Neg };
            (Good(g.u32()), g.i64(sign))
        })
        .collect();
    let v = GoodVec::from_pairs(pairs);
    assert_eq!(v.len(), n, "goods collided");
    for (i, (good, amount)) in v.iter().enumerate() {
        f.u32(&format!("{prefix}{i}.good"), good.0);
        f.i64(&format!("{prefix}{i}.amount"), amount);
    }
    v
}

/// Carriage blocks with the given goods counts.
fn carriages(f: &mut Fields, g: &mut Gen, counts: &[usize]) -> Vec<Carriage> {
    f.sym("n_carriages", counts.len());
    counts
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let index = f.u32(&format!("carriage{i}.index"), g.u32());
            let item_slots = f.u32(&format!("carriage{i}.item_slots"), g.u32());
            let fluid_capacity = f.i64(&format!("carriage{i}.fluid_capacity"), g.i64(Sign::Pos));
            f.sym(&format!("carriage{i}.n_entries"), n);
            let goods = goods(f, g, &format!("carriage{i}.entry"), n);
            Carriage {
                index,
                item_slots,
                fluid_capacity,
                goods,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Output.

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}

fn check_header(name: &str, bytes: &[u8], kind: u16) {
    assert_eq!(
        u16::from_le_bytes([bytes[0], bytes[1]]),
        kind,
        "{name}: kind"
    );
    let size = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
    assert_eq!(size, bytes.len(), "{name}: size");
    assert_eq!(size % 8, 0, "{name}: size is a multiple of 8");
}

fn emit(out: &mut String, name: &str, bytes: &[u8], f: &Fields) {
    writeln!(out, "{name} {}", hex(bytes)).unwrap();
    writeln!(out, "{name}.fields {}", f.line()).unwrap();
}

/// Encodes an observation, decodes it again with the reference decoder, and prints it.
fn obs(out: &mut String, name: &str, kind: u16, seq: u64, t: Timed, f: &Fields) {
    let mut bytes = Vec::new();
    wire::encode_obs(seq, &t, &mut bytes).expect("encode_obs");
    check_header(name, &bytes, kind);
    let (got_seq, back) = wire::decode_obs(&bytes)
        .expect("decode_obs")
        .expect("a known kind");
    assert_eq!(back, t, "{name}: round trip");
    assert_eq!(
        got_seq,
        if t.obs.is_journaled() { seq } else { 0 },
        "{name}: obsSeq"
    );
    emit(out, name, &bytes, f);
}

/// Encodes a command, decodes it again with the reference decoder, and prints it.
fn cmd(out: &mut String, name: &str, kind: u16, seq: u64, c: Cmd, f: &Fields) {
    let mut bytes = Vec::new();
    wire::encode_cmd(seq, &c, &mut bytes).expect("encode_cmd");
    check_header(name, &bytes, kind);
    let (got_seq, back) = wire::decode_cmd(&bytes)
        .expect("decode_cmd")
        .expect("a known kind");
    assert_eq!(back, c, "{name}: round trip");
    assert_eq!(got_seq, seq, "{name}: cmdSeq");
    emit(out, name, &bytes, f);
}

// ---------------------------------------------------------------------------------------------
// Code tables: the number the reference codec writes for each enum variant.

fn obs_bytes(o: Obs) -> Vec<u8> {
    let mut bytes = Vec::new();
    let t = Timed {
        at: Tick(1),
        obs: o,
    };
    wire::encode_obs(7, &t, &mut bytes).expect("encode_obs");
    bytes
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn table(out: &mut String, group: &str, rows: &[(String, u32)]) {
    let cells: Vec<String> = rows.iter().map(|(n, c)| format!("{n}={c}")).collect();
    writeln!(out, "CODES.{group} {}", cells.join(" ")).unwrap();
}

fn code_tables(out: &mut String) {
    let d = DistrictId(1);
    let p = BlockPos::new(1, 2, 3);

    // IND_EDIT aux.
    let rows: Vec<_> = [
        EditKind::Placed,
        EditKind::Broken,
        EditKind::Configured,
        EditKind::Command,
    ]
    .into_iter()
    .map(|kind| {
        let b = obs_bytes(Obs::Edit {
            district: d,
            kind,
            player: (1, 2),
            pos: p,
        });
        (format!("{kind:?}"), u32_at(&b, 4))
    })
    .collect();
    table(out, "edit_kind", &rows);

    // IND_DISCARD aux.
    let rows: Vec<_> = [
        DiscardReason::UnattributedSession,
        DiscardReason::CreativeBlock,
        DiscardReason::Overstressed,
        DiscardReason::ProgressiveContraption,
        DiscardReason::Other,
    ]
    .into_iter()
    .map(|reason| {
        let b = obs_bytes(Obs::Discard {
            district: d,
            reason,
        });
        (format!("{reason:?}"), u32_at(&b, 4))
    })
    .collect();
    table(out, "discard_reason", &rows);

    // IND_LOAD_STATE aux.
    let rows: Vec<_> = [LoadState::Unloaded, LoadState::Partial, LoadState::Live]
        .into_iter()
        .map(|state| {
            let b = obs_bytes(Obs::LoadState {
                district: d,
                state,
                ticking_chunks: 1,
                total_chunks: 2,
            });
            (format!("{state:?}"), u32_at(&b, 4))
        })
        .collect();
    table(out, "load_state", &rows);

    // IND_PORT portKind, at 32. Reconcile (8) has no Java constant: Java never sends it.
    let rows: Vec<_> = [
        PortKind::TrainCargo,
        PortKind::Package,
        PortKind::PlayerSession,
        PortKind::PlayerToss,
        PortKind::PlayerPickup,
        PortKind::StockBroken,
        PortKind::Other,
    ]
    .into_iter()
    .map(|kind| {
        let b = obs_bytes(Obs::Port(PortTransfer {
            district: d,
            kind,
            inventory: None,
            counterpart: 1,
            op: None,
            goods: GoodVec::new(),
        }));
        (format!("{kind:?}"), u32_at(&b, 32))
    })
    .collect();
    table(out, "port_kind", &rows);

    // IND_TRAIN_ARRIVED flags, when the cargo was read (choice SG30): bits 1 and 2.
    let rows: Vec<_> = [
        CargoFrom::Arrival,
        CargoFrom::Approach,
        CargoFrom::Departure,
    ]
    .into_iter()
    .map(|cargo_from| {
        let b = obs_bytes(Obs::TrainArrived(TrainCall {
            train: TrainId(1),
            station: StationId(2),
            district: d,
            simulated: false,
            cargo_from,
            carriages: Vec::new(),
        }));
        (format!("{cargo_from:?}"), u32_at(&b, 48))
    })
    .collect();
    table(out, "cargo_from", &rows);

    // IND_PACKAGE direction, at 36.
    let rows: Vec<_> = [Direction::In, Direction::Out]
        .into_iter()
        .map(|direction| {
            let b = obs_bytes(Obs::Package(PackageEvent {
                district: d,
                inventory: 1,
                direction,
                address: 2,
                packages: 3,
                op: None,
                goods: GoodVec::new(),
            }));
            (format!("{direction:?}"), u32_at(&b, 36))
        })
        .collect();
    table(out, "package_direction", &rows);

    // IND_DISTRICT aux.
    let rows: Vec<_> = [
        NotLearnable::RotationCrossesBoundary,
        NotLearnable::ElectricLineCrossesBoundary,
        NotLearnable::MixtureNotSteady,
        NotLearnable::FiniteDepositUnknown,
        NotLearnable::MovingContraption,
        NotLearnable::SpawnFedFarm,
    ]
    .into_iter()
    .map(|n| {
        let b = obs_bytes(Obs::District(DistrictDecl {
            district: d,
            revision: 1,
            dim: 2,
            name: 3,
            not_learnable: Some(n),
            inventory_count: 4,
        }));
        (format!("{n:?}"), u32_at(&b, 4))
    })
    .collect();
    table(out, "not_learnable", &rows);

    // IND_SWEEP_END aux, one flag at a time.
    let rows: Vec<_> = ["live", "creative", "overstressed"]
        .into_iter()
        .map(|flag| {
            let b = obs_bytes(Obs::SweepEnd(SweepEnd {
                district: d,
                sweep: 1,
                records: 2,
                inventories: 3,
                live: flag == "live",
                creative: flag == "creative",
                overstressed: flag == "overstressed",
            }));
            (flag.to_string(), u32_at(&b, 4))
        })
        .collect();
    table(out, "sweep_flag", &rows);

    // InvKind bits (the IND_INVENTORY aux is their union).
    let rows = vec![
        ("ITEMS".to_string(), InvKind::ITEMS),
        ("FLUIDS".to_string(), InvKind::FLUIDS),
        ("ENERGY".to_string(), InvKind::ENERGY),
        ("POSTBOX".to_string(), InvKind::POSTBOX),
        ("PACKAGER_TARGET".to_string(), InvKind::PACKAGER_TARGET),
        ("PLUGIN".to_string(), InvKind::PLUGIN),
    ];
    table(out, "inv_kind", &rows);

    // IND_CARRIAGE_DELTA reason, at 32.
    let rows: Vec<_> = [CarriageReason::Exchange, CarriageReason::Loot]
        .into_iter()
        .map(|reason| {
            let mut bytes = Vec::new();
            let c = Cmd::CarriageDelta(CarriageDelta {
                op: OpId::engine(1),
                train: TrainId(1),
                reason,
                entries: Vec::new(),
            });
            wire::encode_cmd(1, &c, &mut bytes).expect("encode_cmd");
            (format!("{reason:?}"), u32_at(&bytes, 32))
        })
        .collect();
    table(out, "carriage_reason", &rows);

    // IND_DEPOSIT_SAMPLE aux (also IND_DEPOSIT_DELTA's).
    let rows: Vec<_> = [
        DepositPluginId::OreExcavation,
        DepositPluginId::DieselOil,
        DepositPluginId::DestroyCrude,
    ]
    .into_iter()
    .map(|plugin| {
        let b = obs_bytes(Obs::DepositSample(DepositSample {
            district: d,
            good: Good(1),
            plugin,
            chunk: ChunkPos { dim: 1, x: 2, z: 3 },
            remaining: 4,
            infinite: false,
            sweep: 5,
        }));
        (format!("{plugin:?}"), u32_at(&b, 4))
    })
    .collect();
    table(out, "deposit_plugin", &rows);

    // IND_FIGHT aux.
    let rows: Vec<_> = ThreatClass::ALL
        .into_iter()
        .map(|class| {
            let b = obs_bytes(Obs::Fight(FightObserved {
                district: d,
                class,
                attackers: 1,
                attacker_strength: 1.0,
                attackers_killed: 0,
                blocks_destroyed: 0,
                machines_destroyed: 0,
                guards_lost: 0,
                items_stolen: 0,
                duration_ticks: 1,
            }));
            (format!("{class:?}"), u32_at(&b, 4))
        })
        .collect();
    table(out, "threat_class", &rows);

    // IND_SURVEY_BLOCKS flag bits.
    let rows = vec![
        ("PERIMETER".to_string(), BlockFlags::PERIMETER),
        ("ROOFLESS".to_string(), BlockFlags::ROOFLESS),
        ("MACHINE".to_string(), BlockFlags::MACHINE),
        ("INVENTORY".to_string(), BlockFlags::INVENTORY),
        ("ROOF".to_string(), BlockFlags::ROOF),
        ("SUBTERRANEAN".to_string(), BlockFlags::SUBTERRANEAN),
        ("ARMOURED".to_string(), BlockFlags::ARMOURED),
    ];
    table(out, "block_flags", &rows);

    // IND_SURVEY aux: mobGriefing.
    let survey = |mob_griefing| {
        obs_bytes(Obs::Survey(DefenceSurvey {
            district: d,
            revision: 1,
            mob_griefing,
            perimeter_columns: 0,
            perimeter_gaps: 0,
            dark_spawnable: 0,
            machine_blocks: 0,
            roofed_machines: 0,
            kill_machines_at_gaps: 0,
            turret_strength: [0.0; 4],
            guards: [0; 4],
            ammo_good: None,
            ammo_per_engagement: 0,
        }))
    };
    let rows = vec![(
        "mob_griefing".to_string(),
        u32_at(&survey(true), 4) & !u32_at(&survey(false), 4),
    )];
    table(out, "survey_flag", &rows);
}

// ---------------------------------------------------------------------------------------------

fn main() {
    // `glass`: the glass-box fixtures (glass.rs) instead of the industry ones.
    if std::env::args().nth(1).as_deref() == Some("glass") {
        print!("{}", glass::fixtures());
        return;
    }
    let mut out = String::from(HEADER);

    // ---- latest-wins observations ----

    {
        let name = "IND_STOCK_SAMPLE";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let inventory = f.u32("inventory", g.u32());
        let sweep = f.u32("sweep", g.u32());
        f.sym("n_entries", 2);
        let goods = goods(&mut f, &mut g, "entry", 2);
        let o = Obs::StockSample {
            district,
            inventory,
            sweep,
            goods,
        };
        obs(
            &mut out,
            name,
            wire::IND_STOCK_SAMPLE,
            0,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_SWEEP_END";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let sweep = f.u32("sweep", g.u32());
        let records = f.u32("records", g.u32());
        let inventories = f.u32("inventories", g.u32());
        f.sym("live", true);
        f.sym("creative", false);
        f.sym("overstressed", true);
        let o = Obs::SweepEnd(SweepEnd {
            district,
            sweep,
            records,
            inventories,
            live: true,
            creative: false,
            overstressed: true,
        });
        obs(
            &mut out,
            name,
            wire::IND_SWEEP_END,
            0,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_WATCHED";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        f.sym("watched", true);
        let o = Obs::Watched {
            district,
            watched: true,
        };
        obs(
            &mut out,
            name,
            wire::IND_WATCHED,
            0,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_APPROACH";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let distance = f.u32("distance", g.u32());
        // Clear of 0xFFFFFFFF, which reads as "unknown".
        let eta = f.u32("eta_ticks", g.u32() & 0x7FFF_FFFF);
        f.sym("closing", true);
        let o = Obs::Approach {
            district,
            distance,
            eta_ticks: Some(eta),
            closing: true,
        };
        obs(
            &mut out,
            name,
            wire::IND_APPROACH,
            0,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_DEPOSIT_SAMPLE";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let good = Good(f.u32("good", g.u32()));
        let sweep = f.u32("sweep", g.u32());
        let remaining = f.i64("remaining", g.i64(Sign::Pos));
        let dim = f.u32("dim", g.u32());
        let x = f.i32("chunk_x", g.i32(Sign::Neg));
        let z = f.i32("chunk_z", g.i32(Sign::Pos));
        let plugin = DepositPluginId::DieselOil;
        f.sym("plugin", format!("{plugin:?}"));
        f.sym("infinite", true);
        let o = Obs::DepositSample(DepositSample {
            district,
            good,
            plugin,
            chunk: ChunkPos { dim, x, z },
            remaining,
            infinite: true,
            sweep,
        });
        obs(
            &mut out,
            name,
            wire::IND_DEPOSIT_SAMPLE,
            0,
            Timed { at, obs: o },
            &f,
        );
    }

    // ---- journaled observations ----

    {
        let name = "IND_DISTRICT";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let revision = f.u32("revision", g.u32());
        let dim = f.u32("dim", g.u32());
        let name_id = f.u32("name", g.u32());
        let inventory_count = f.u32("inventory_count", g.u32());
        let reason = NotLearnable::MixtureNotSteady;
        f.sym("not_learnable", format!("{reason:?}"));
        let o = Obs::District(DistrictDecl {
            district,
            revision,
            dim,
            name: name_id,
            not_learnable: Some(reason),
            inventory_count,
        });
        obs(
            &mut out,
            name,
            wire::IND_DISTRICT,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_DISTRICT_CHUNKS";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let revision = f.u32("revision", g.u32());
        let part = f.u32("part", g.u32());
        f.sym("n_chunks", 3);
        let chunks: Vec<(i32, i32)> = (0..3)
            .map(|i| {
                // x positive, z negative, so a swapped pair or a lost sign shows.
                let x = f.i32(&format!("chunk{i}.x"), g.i32(Sign::Pos));
                let z = f.i32(&format!("chunk{i}.z"), g.i32(Sign::Neg));
                (x, z)
            })
            .collect();
        let o = Obs::DistrictChunks {
            district,
            revision,
            part,
            chunks,
        };
        obs(
            &mut out,
            name,
            wire::IND_DISTRICT_CHUNKS,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_INVENTORY";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let revision = f.u32("revision", g.u32());
        let index = f.u32("index", g.u32());
        let p = pos(&mut f, &mut g, "");
        let item_slots = f.u32("item_slots", g.u32());
        let fluid_capacity = f.i64("fluid_capacity", g.i64(Sign::Pos));
        let energy_capacity = f.i64("energy_capacity", g.i64(Sign::Pos));
        f.sym("kind", "ITEMS|FLUIDS|POSTBOX");
        let o = Obs::Inventory(InventoryDecl {
            district,
            revision,
            index,
            pos: p,
            kind: InvKind(InvKind::ITEMS | InvKind::FLUIDS | InvKind::POSTBOX),
            item_slots,
            fluid_capacity,
            energy_capacity,
        });
        obs(
            &mut out,
            name,
            wire::IND_INVENTORY,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_DISTRICT_REMOVED";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let o = Obs::DistrictRemoved { district };
        obs(
            &mut out,
            name,
            wire::IND_DISTRICT_REMOVED,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_EDIT";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let most = f.u64("player_most", g.u64());
        let least = f.u64("player_least", g.u64());
        let p = pos(&mut f, &mut g, "");
        let kind = EditKind::Configured;
        f.sym("edit_kind", format!("{kind:?}"));
        let o = Obs::Edit {
            district,
            kind,
            player: (most, least),
            pos: p,
        };
        obs(
            &mut out,
            name,
            wire::IND_EDIT,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_DISCARD";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let reason = DiscardReason::ProgressiveContraption;
        f.sym("discard_reason", format!("{reason:?}"));
        let o = Obs::Discard { district, reason };
        obs(
            &mut out,
            name,
            wire::IND_DISCARD,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_LOAD_STATE";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let ticking_chunks = f.u32("ticking_chunks", g.u32());
        let total_chunks = f.u32("total_chunks", g.u32());
        let state = LoadState::Partial;
        f.sym("load_state", format!("{state:?}"));
        let o = Obs::LoadState {
            district,
            state,
            ticking_chunks,
            total_chunks,
        };
        obs(
            &mut out,
            name,
            wire::IND_LOAD_STATE,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_CHUNK_LOADED";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let dim = f.u32("dim", g.u32());
        let x = f.i32("chunk_x", g.i32(Sign::Neg));
        let z = f.i32("chunk_z", g.i32(Sign::Pos));
        let applied_epoch = f.u32("applied_epoch", g.u32());
        let o = Obs::ChunkLoaded {
            district,
            chunk: ChunkPos { dim, x, z },
            applied_epoch,
        };
        obs(
            &mut out,
            name,
            wire::IND_CHUNK_LOADED,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        // I7: a chunk of a district unloaded; the reserved field at 44 is zero.
        let name = "IND_CHUNK_UNLOADED";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let dim = f.u32("dim", g.u32());
        let x = f.i32("chunk_x", g.i32(Sign::Neg));
        let z = f.i32("chunk_z", g.i32(Sign::Pos));
        let o = Obs::ChunkUnloaded {
            district,
            chunk: ChunkPos { dim, x, z },
        };
        obs(
            &mut out,
            name,
            wire::IND_CHUNK_UNLOADED,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_PORT";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let kind = PortKind::PlayerToss;
        f.sym("port_kind", format!("{kind:?}"));
        let inventory = f.u32("inventory", g.u32());
        let counterpart = f.u64("counterpart", g.u64());
        let op = g.bridge_op();
        f.num("op", op.0);
        f.sym("n_entries", 2);
        let goods = goods(&mut f, &mut g, "entry", 2);
        let o = Obs::Port(PortTransfer {
            district,
            kind,
            inventory: Some(inventory),
            counterpart,
            op: Some(op),
            goods,
        });
        obs(
            &mut out,
            name,
            wire::IND_PORT,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        // The same kind with the two optional fields absent: 0xFFFFFFFF and 0.
        let name = "IND_PORT_NO_INVENTORY";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let kind = PortKind::Other;
        f.sym("port_kind", format!("{kind:?}"));
        f.sym("inventory", "none");
        let counterpart = f.u64("counterpart", g.u64());
        f.sym("op", "none");
        f.sym("n_entries", 1);
        let goods = goods(&mut f, &mut g, "entry", 1);
        let o = Obs::Port(PortTransfer {
            district,
            kind,
            inventory: None,
            counterpart,
            op: None,
            goods,
        });
        obs(
            &mut out,
            name,
            wire::IND_PORT,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_TRAIN_ARRIVED";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let train = TrainId(f.u64("train", g.u64()));
        let station = StationId(f.u64("station", g.u64()));
        let district = district(&mut f, &mut g);
        f.sym("simulated", true);
        f.sym("cargo_from", "Arrival");
        let carriages = carriages(&mut f, &mut g, &[1, 2]);
        let o = Obs::TrainArrived(TrainCall {
            train,
            station,
            district,
            simulated: true,
            cargo_from: CargoFrom::Arrival,
            carriages,
        });
        obs(
            &mut out,
            name,
            wire::IND_TRAIN_ARRIVED,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        // A loaded call whose cargo was read as the train approached (choice SG30).
        let name = "IND_TRAIN_ARRIVED_APPROACH";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let train = TrainId(f.u64("train", g.u64()));
        let station = StationId(f.u64("station", g.u64()));
        let district = district(&mut f, &mut g);
        f.sym("simulated", false);
        f.sym("cargo_from", "Approach");
        let carriages = carriages(&mut f, &mut g, &[2]);
        let o = Obs::TrainArrived(TrainCall {
            train,
            station,
            district,
            simulated: false,
            cargo_from: CargoFrom::Approach,
            carriages,
        });
        obs(
            &mut out,
            name,
            wire::IND_TRAIN_ARRIVED,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_TRAIN_DEPARTED";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let train = TrainId(f.u64("train", g.u64()));
        let station = StationId(f.u64("station", g.u64()));
        let district = district(&mut f, &mut g);
        let destination = StationId(f.u64("destination", g.u64()));
        let eta_ticks = f.i64("eta_ticks", g.i64(Sign::Pos));
        let carriages = carriages(&mut f, &mut g, &[2]);
        let o = Obs::TrainDeparted(TrainDeparture {
            train,
            station,
            district,
            destination: Some(destination),
            eta_ticks: eta_ticks as u64,
            carriages,
        });
        obs(
            &mut out,
            name,
            wire::IND_TRAIN_DEPARTED,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_STATION";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let station = StationId(f.u64("station", g.u64()));
        let district = district(&mut f, &mut g);
        let dim = f.u32("dim", g.u32());
        let p = pos(&mut f, &mut g, "");
        let name_id = f.u32("name", g.u32());
        let o = Obs::Station(StationDecl {
            station,
            district: Some(district),
            dim,
            pos: p,
            name: name_id,
        });
        obs(
            &mut out,
            name,
            wire::IND_STATION,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        // Three inventories: 52 bytes, padded to 56.
        let name = "IND_STATION_PORTS";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let station = StationId(f.u64("station", g.u64()));
        let district = district(&mut f, &mut g);
        let n = 3usize;
        f.sym("n_inventories", n);
        let inventories: Vec<u32> = (0..n)
            .map(|i| f.u32(&format!("inventory{i}"), g.u32()))
            .collect();
        let o = Obs::StationPorts(StationPorts {
            station,
            district,
            inventories,
        });
        obs(
            &mut out,
            name,
            wire::IND_STATION_PORTS,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_RECONCILE_RESULT";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let op = g.engine_op();
        f.num("op", op.0);
        let district = district(&mut f, &mut g);
        let epoch = f.u32("epoch", g.u32());
        let dim = f.u32("dim", g.u32());
        let x = f.i32("chunk_x", g.i32(Sign::Neg));
        let z = f.i32("chunk_z", g.i32(Sign::Pos));
        let blocks_skipped = f.u32("blocks_skipped", g.u32());
        f.sym("n_entries", 2);
        let entries: Vec<ResultEntry> = (0..2)
            .map(|i| ResultEntry {
                inventory: f.u32(&format!("entry{i}.inventory"), g.u32()),
                good: Good(f.u32(&format!("entry{i}.good"), g.u32())),
                requested: f.i64(&format!("entry{i}.requested"), g.i64(Sign::Neg)),
                applied: f.i64(&format!("entry{i}.applied"), g.i64(Sign::Pos)),
            })
            .collect();
        f.sym("n_destroyed", 2);
        let destroyed: Vec<BlockPos> = (0..2)
            .map(|i| pos(&mut f, &mut g, &format!("destroyed{i}.")))
            .collect();
        let o = Obs::ReconcileResult(ReconcileResult {
            op,
            district,
            epoch,
            chunk: ChunkPos { dim, x, z },
            entries,
            destroyed,
            blocks_skipped,
        });
        obs(
            &mut out,
            name,
            wire::IND_RECONCILE_RESULT,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_CARRIAGE_RESULT";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let op = g.engine_op();
        f.num("op", op.0);
        let train = TrainId(f.u64("train", g.u64()));
        f.sym("n_entries", 2);
        let entries: Vec<CarriageResultEntry> = (0..2)
            .map(|i| CarriageResultEntry {
                carriage: f.u32(&format!("entry{i}.carriage"), g.u32()),
                good: Good(f.u32(&format!("entry{i}.good"), g.u32())),
                requested: f.i64(&format!("entry{i}.requested"), g.i64(Sign::Pos)),
                applied: f.i64(&format!("entry{i}.applied"), g.i64(Sign::Neg)),
            })
            .collect();
        let o = Obs::CarriageResult(CarriageResult { op, train, entries });
        obs(
            &mut out,
            name,
            wire::IND_CARRIAGE_RESULT,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_CONDITIONS";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let season = f.u32("season", g.u32());
        let season_count = f.u32("season_count", g.u32());
        let season_length = f.i64("season_length", g.i64(Sign::Pos));
        let season_origin = f.i64("season_origin", g.i64(Sign::Pos));
        let o = Obs::Conditions(Conditions {
            season,
            season_count,
            season_length: season_length as u64,
            season_origin: Tick(season_origin as u64),
        });
        obs(
            &mut out,
            name,
            wire::IND_CONDITIONS,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_POWER_LINK";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let sink = DistrictId(f.u64("sink", g.u64()));
        let source = DistrictId(f.u64("source", g.u64()));
        let o = Obs::PowerLink {
            sink,
            source: Some(source),
        };
        obs(
            &mut out,
            name,
            wire::IND_POWER_LINK,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_PACKAGE";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let inventory = f.u32("inventory", g.u32());
        let direction = Direction::Out;
        f.sym("direction", format!("{direction:?}"));
        let address = f.u32("address", g.u32());
        let packages = f.u32("packages", g.u32());
        let op = g.bridge_op();
        f.num("op", op.0);
        f.sym("n_entries", 2);
        let goods = goods(&mut f, &mut g, "entry", 2);
        let o = Obs::Package(PackageEvent {
            district,
            inventory,
            direction,
            address,
            packages,
            op: Some(op),
            goods,
        });
        obs(
            &mut out,
            name,
            wire::IND_PACKAGE,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_SPAWN_SAMPLE";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let observed_ticks = f.i64("observed_ticks", g.i64(Sign::Pos));
        let dark_blocks = f.i64("dark_blocks", g.i64(Sign::Pos));
        let spawns: [u32; 4] = std::array::from_fn(|i| f.u32(&format!("spawns{i}"), g.u32()));
        let o = Obs::SpawnSample(SpawnSample {
            district,
            observed_ticks: observed_ticks as u64,
            dark_blocks: dark_blocks as u64,
            spawns,
        });
        obs(
            &mut out,
            name,
            wire::IND_SPAWN_SAMPLE,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_FIGHT";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let class = ThreatClass::Flying;
        f.sym("class", format!("{class:?}"));
        let attackers = f.u32("attackers", g.u32());
        let attacker_strength = f.f32("attacker_strength", g.f32());
        let attackers_killed = f.u32("attackers_killed", g.u32());
        let blocks_destroyed = f.u32("blocks_destroyed", g.u32());
        let machines_destroyed = f.u32("machines_destroyed", g.u32());
        let guards_lost = f.u32("guards_lost", g.u32());
        let items_stolen = f.i64("items_stolen", g.i64(Sign::Pos));
        let duration_ticks = f.i64("duration_ticks", g.i64(Sign::Pos));
        let o = Obs::Fight(FightObserved {
            district,
            class,
            attackers,
            attacker_strength,
            attackers_killed,
            blocks_destroyed,
            machines_destroyed,
            guards_lost,
            items_stolen,
            duration_ticks: duration_ticks as u64,
        });
        obs(
            &mut out,
            name,
            wire::IND_FIGHT,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_SURVEY";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let revision = f.u32("revision", g.u32());
        f.sym("mob_griefing", true);
        let perimeter_columns = f.u32("perimeter_columns", g.u32());
        let perimeter_gaps = f.u32("perimeter_gaps", g.u32());
        let dark_spawnable = f.u32("dark_spawnable", g.u32());
        let machine_blocks = f.u32("machine_blocks", g.u32());
        let roofed_machines = f.u32("roofed_machines", g.u32());
        let kill_machines_at_gaps = f.u32("kill_machines_at_gaps", g.u32());
        let ammo = Good(f.u32("ammo_good", g.u32()));
        let turret_strength: [f32; 4] =
            std::array::from_fn(|i| f.f32(&format!("turret{i}"), g.f32()));
        let guards: [u32; 4] = std::array::from_fn(|i| f.u32(&format!("guards{i}"), g.u32()));
        let ammo_per_engagement = f.u32("ammo_per_engagement", g.u32());
        let o = Obs::Survey(DefenceSurvey {
            district,
            revision,
            mob_griefing: true,
            perimeter_columns,
            perimeter_gaps,
            dark_spawnable,
            machine_blocks,
            roofed_machines,
            kill_machines_at_gaps,
            turret_strength,
            guards,
            ammo_good: Some(ammo),
            ammo_per_engagement,
        });
        obs(
            &mut out,
            name,
            wire::IND_SURVEY,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    {
        let name = "IND_SURVEY_BLOCKS";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let revision = f.u32("revision", g.u32());
        let part = f.u32("part", g.u32());
        let chunk_x = f.i32("chunk_x", g.i32(Sign::Neg));
        let chunk_z = f.i32("chunk_z", g.i32(Sign::Pos));
        f.sym("partial", true);
        f.sym("n_blocks", 2);
        let blocks: Vec<ExposedBlock> = (0..2)
            .map(|i| {
                let p = pos(&mut f, &mut g, &format!("block{i}."));
                let state = f.u32(&format!("block{i}.state"), g.u32());
                // Flag bits are small and may repeat across blocks: symbols, not numbers.
                let flags = if i == 0 {
                    BlockFlags::PERIMETER | BlockFlags::ROOFLESS
                } else {
                    BlockFlags::MACHINE | BlockFlags::INVENTORY
                };
                f.sym(&format!("block{i}.flags"), flags);
                let inventory = if i == 0 {
                    f.sym(&format!("block{i}.inventory"), "none");
                    None
                } else {
                    Some(f.u32(&format!("block{i}.inventory"), g.u32() & 0x7FFF_FFFF))
                };
                ExposedBlock {
                    pos: p,
                    state,
                    flags: BlockFlags(flags),
                    inventory,
                }
            })
            .collect();
        let o = Obs::SurveyBlocks {
            district,
            revision,
            part,
            chunk: (chunk_x, chunk_z),
            partial: true,
            blocks,
        };
        obs(
            &mut out,
            name,
            wire::IND_SURVEY_BLOCKS,
            seq,
            Timed { at, obs: o },
            &f,
        );
    }

    // ---- commands ----

    {
        let name = "IND_CHUNK_RECONCILE";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("cmd_seq", g.u64());
        let op = g.engine_op();
        f.num("op", op.0);
        let district = district(&mut f, &mut g);
        let epoch = f.u32("epoch", g.u32());
        let dim = f.u32("dim", g.u32());
        let x = f.i32("chunk_x", g.i32(Sign::Neg));
        let z = f.i32("chunk_z", g.i32(Sign::Pos));
        f.sym("n_deltas", 2);
        let deltas: Vec<InvDelta> = (0..2)
            .map(|i| {
                let inventory = f.u32(&format!("delta{i}.inventory"), g.u32());
                let good = Good(f.u32(&format!("delta{i}.good"), g.u32()));
                let p = pos(&mut f, &mut g, &format!("delta{i}."));
                let sign = if i == 0 { Sign::Neg } else { Sign::Pos };
                let delta = f.i64(&format!("delta{i}.delta"), g.i64(sign));
                InvDelta {
                    inventory,
                    pos: p,
                    good,
                    delta,
                }
            })
            .collect();
        f.sym("n_destroy", 1);
        let p = pos(&mut f, &mut g, "destroy0.");
        let expected_state = f.u32("destroy0.expected_state", g.u32());
        let c = Cmd::ChunkReconcile(ChunkReconcile {
            op,
            district,
            epoch,
            chunk: ChunkPos { dim, x, z },
            deltas,
            destroy: vec![BlockDestroy {
                pos: p,
                expected_state,
            }],
        });
        cmd(&mut out, name, wire::IND_CHUNK_RECONCILE, seq, c, &f);
    }

    {
        let name = "IND_CARRIAGE_DELTA";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("cmd_seq", g.u64());
        let op = g.engine_op();
        f.num("op", op.0);
        let train = TrainId(f.u64("train", g.u64()));
        let reason = CarriageReason::Loot;
        f.sym("reason", format!("{reason:?}"));
        f.sym("n_entries", 2);
        let entries: Vec<(u32, Good, i64)> = (0..2)
            .map(|i| {
                let carriage = f.u32(&format!("entry{i}.carriage"), g.u32());
                let good = Good(f.u32(&format!("entry{i}.good"), g.u32()));
                let sign = if i == 0 { Sign::Neg } else { Sign::Pos };
                let delta = f.i64(&format!("entry{i}.delta"), g.i64(sign));
                (carriage, good, delta)
            })
            .collect();
        let c = Cmd::CarriageDelta(CarriageDelta {
            op,
            train,
            reason,
            entries,
        });
        cmd(&mut out, name, wire::IND_CARRIAGE_DELTA, seq, c, &f);
    }

    {
        let name = "IND_DEPOSIT_DELTA";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("cmd_seq", g.u64());
        let op = g.engine_op();
        f.num("op", op.0);
        let plugin = DepositPluginId::OreExcavation;
        f.sym("plugin", format!("{plugin:?}"));
        let dim = f.u32("dim", g.u32());
        let x = f.i32("chunk_x", g.i32(Sign::Pos));
        let z = f.i32("chunk_z", g.i32(Sign::Neg));
        let good = Good(f.u32("good", g.u32()));
        let delta = f.i64("delta", g.i64(Sign::Neg));
        let c = Cmd::DepositDelta(DepositDelta {
            op,
            plugin,
            chunk: ChunkPos { dim, x, z },
            good,
            delta,
        });
        cmd(&mut out, name, wire::IND_DEPOSIT_DELTA, seq, c, &f);
    }

    {
        let name = "IND_WRITE_PACKAGE";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("cmd_seq", g.u64());
        let op = g.engine_op();
        f.num("op", op.0);
        let district = district(&mut f, &mut g);
        let inventory = f.u32("inventory", g.u32());
        let p = pos(&mut f, &mut g, "");
        let address = f.u32("address", g.u32());
        let count = f.u32("count", g.u32());
        f.sym("n_entries", 2);
        let goods = goods(&mut f, &mut g, "entry", 2);
        let c = Cmd::WritePackage(WritePackage {
            op,
            district,
            inventory,
            pos: p,
            address,
            count,
            goods,
        });
        cmd(&mut out, name, wire::IND_WRITE_PACKAGE, seq, c, &f);
    }

    code_tables(&mut out);

    print!("{out}");
}
