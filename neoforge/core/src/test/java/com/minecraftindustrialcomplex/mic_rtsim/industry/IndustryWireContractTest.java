package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import org.junit.jupiter.api.DynamicTest;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.TestFactory;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.BitSet;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Stream;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

/**
 * The cross-language contract between the Rust reference codec ({@code crates/rtsim-industry/src/wire.rs})
 * and its Java mirror ({@link IndRecords}), in pure Java.
 *
 * <p>The fixtures are {@code industry-wire-fixtures.txt}, which the Rust program in
 * {@code neoforge/tools/industry-wire-fixtures} prints from {@code wire.rs}'s encoders: one record of
 * each kind with a different value in every field, and the values it used. For an observation (Java
 * writes, the engine reads) this test builds the same record from those values with nothing but
 * {@link IndRecords} constants and asserts the bytes are identical, size and padding included. For a
 * command (the engine writes, Java reads) it reads every field of the Rust bytes at the
 * {@link IndRecords} offsets and asserts each is the value the fixture says, and that every byte it did
 * not read is zero. A third set of tests checks the enum codes the codec writes against the
 * constants.
 *
 * <p>When a fixture disagrees, the failure names the field, the {@link IndRecords} offset and where
 * the value is in the Rust record. Fix {@link IndRecords} (or {@code wire.rs} and the page
 * {@code docs/industry-boundary.md} together) and regenerate the fixtures; never edit the fixture file.
 */
class IndustryWireContractTest {

    private static final String RESOURCE = "/industry-wire-fixtures.txt";

    // The 8-byte record header of docs/boundary.md: u16 kind, u16 size, u32 aux. IndRecords does not
    // repeat it (the bridge owns it); the journaled prefix is the bridge's Obs.OBS_SEQ and Obs.GAME_TIME.
    private static final int H_KIND = 0;
    private static final int H_SIZE = 2;
    private static final int H_AUX = 4;

    // Sub-record fields that IndRecords documents (Entry16, Result24, Pos16, chunk pairs) but gives no
    // constant for.
    private static final int E_GOOD = 0;     // Entry16: u32 good, u32 reserved, i64 amount
    private static final int E_AMOUNT = 8;
    private static final int R_FIRST = 0;    // Result24: u32 inventory (or carriage), u32 good, i64 requested, i64 applied
    private static final int R_GOOD = 4;
    private static final int R_REQUESTED = 8;
    private static final int R_APPLIED = 16;
    private static final int P_X = 0;        // Pos16: i32 x, i32 y, i32 z, u32 reserved
    private static final int P_Y = 4;
    private static final int P_Z = 8;
    private static final int CHUNK_X = 0;    // DistrictChunks entry: i32 x, i32 z
    private static final int CHUNK_Z = 4;
    private static final int CHUNK_PAIR = 8;

    /** The command fixtures; every other record fixture is an observation. */
    private static final Set<String> COMMANDS = Set.of("IND_CHUNK_RECONCILE", "IND_CARRIAGE_DELTA",
            "IND_DEPOSIT_DELTA", "IND_WRITE_PACKAGE");

    /** What the Rust program must have printed. A missing one fails, so a shrunken file cannot pass. */
    private static final List<String> REQUIRED = List.of(
            "IND_STOCK_SAMPLE", "IND_SWEEP_END", "IND_WATCHED", "IND_APPROACH", "IND_DISTRICT",
            "IND_DISTRICT_CHUNKS", "IND_INVENTORY", "IND_DISTRICT_REMOVED", "IND_EDIT", "IND_DISCARD",
            "IND_LOAD_STATE", "IND_CHUNK_LOADED", "IND_CHUNK_UNLOADED", "IND_PORT", "IND_PORT_NO_INVENTORY", "IND_TRAIN_ARRIVED",
            "IND_TRAIN_DEPARTED", "IND_STATION", "IND_RECONCILE_RESULT", "IND_CARRIAGE_RESULT",
            "IND_CHUNK_RECONCILE", "IND_CARRIAGE_DELTA",
            // overnight agent 14: the plugin, threat and level-store records
            "IND_DEPOSIT_SAMPLE", "IND_CONDITIONS", "IND_POWER_LINK", "IND_PACKAGE",
            "IND_SPAWN_SAMPLE", "IND_FIGHT", "IND_SURVEY", "IND_SURVEY_BLOCKS",
            "IND_DEPOSIT_DELTA", "IND_WRITE_PACKAGE",
            // stress worlds' station fixes (SG30, SG32)
            "IND_TRAIN_ARRIVED_APPROACH", "IND_STATION_PORTS",
            // the syn200 fixes (SG50)
            "IND_TRAIN_UNWATCHED");

    // ---- the enum codes, as IndRecords names them ---------------------------------------------------

    /** group, then variant (the Rust name), to the IndRecords constant. */
    private static final Map<String, Map<String, Integer>> CODES = new LinkedHashMap<>();

    private static void group(String name, Object... variantAndCode) {
        Map<String, Integer> m = new LinkedHashMap<>();
        for (int i = 0; i < variantAndCode.length; i += 2) {
            m.put((String) variantAndCode[i], (Integer) variantAndCode[i + 1]);
        }
        CODES.put(name, m);
    }

    static {
        group("edit_kind",
                "Placed", IndRecords.Edit.PLACED, "Broken", IndRecords.Edit.BROKEN,
                "Configured", IndRecords.Edit.CONFIGURED, "Command", IndRecords.Edit.COMMAND);
        group("discard_reason",
                "UnattributedSession", IndRecords.Discard.UNATTRIBUTED_SESSION,
                "CreativeBlock", IndRecords.Discard.CREATIVE_BLOCK,
                "Overstressed", IndRecords.Discard.OVERSTRESSED,
                "ProgressiveContraption", IndRecords.Discard.PROGRESSIVE_CONTRAPTION,
                "Other", IndRecords.Discard.OTHER);
        group("load_state",
                "Unloaded", IndRecords.LoadState.UNLOADED, "Partial", IndRecords.LoadState.PARTIAL,
                "Live", IndRecords.LoadState.LIVE);
        group("port_kind",
                "TrainCargo", IndRecords.Port.TRAIN_CARGO, "Package", IndRecords.Port.PACKAGE,
                "PlayerSession", IndRecords.Port.PLAYER_SESSION, "PlayerToss", IndRecords.Port.PLAYER_TOSS,
                "PlayerPickup", IndRecords.Port.PLAYER_PICKUP, "StockBroken", IndRecords.Port.STOCK_BROKEN,
                "Other", IndRecords.Port.OTHER);
        group("package_direction", "In", IndRecords.Package.IN, "Out", IndRecords.Package.OUT);
        group("cargo_from", "Arrival", 0, "Approach", IndRecords.Train.FLAG_APPROACH,
                "Departure", IndRecords.Train.FLAG_DEPARTURE);
        group("not_learnable",
                "RotationCrossesBoundary", IndRecords.NotLearnable.ROTATION_CROSSES,
                "ElectricLineCrossesBoundary", IndRecords.NotLearnable.ELECTRIC_LINE_CROSSES,
                "MixtureNotSteady", IndRecords.NotLearnable.MIXTURE_NOT_STEADY,
                "FiniteDepositUnknown", IndRecords.NotLearnable.FINITE_DEPOSIT_UNKNOWN,
                "MovingContraption", IndRecords.NotLearnable.MOVING_CONTRAPTION,
                "SpawnFedFarm", IndRecords.NotLearnable.SPAWN_FED_FARM);
        group("sweep_flag",
                "live", IndRecords.SweepEnd.FLAG_LIVE, "creative", IndRecords.SweepEnd.FLAG_CREATIVE,
                "overstressed", IndRecords.SweepEnd.FLAG_OVERSTRESSED);
        group("inv_kind",
                "ITEMS", IndRecords.Inventory.KIND_ITEMS, "FLUIDS", IndRecords.Inventory.KIND_FLUIDS,
                "ENERGY", IndRecords.Inventory.KIND_ENERGY, "POSTBOX", IndRecords.Inventory.KIND_POSTBOX,
                "PACKAGER_TARGET", IndRecords.Inventory.KIND_PACKAGER_TARGET,
                "PLUGIN", IndRecords.Inventory.KIND_PLUGIN);
        group("carriage_reason",
                "Exchange", IndRecords.CarriageDelta.EXCHANGE, "Loot", IndRecords.CarriageDelta.LOOT);
        group("deposit_plugin",
                "OreExcavation", IndRecords.DepositPlugin.ORE_EXCAVATION, "DieselOil", IndRecords.DepositPlugin.DIESEL_OIL,
                "DestroyCrude", IndRecords.DepositPlugin.DESTROY_CRUDE);
        group("threat_class",
                "GroundMelee", IndRecords.ThreatClass.GROUND_MELEE, "GroundRanged", IndRecords.ThreatClass.GROUND_RANGED,
                "Flying", IndRecords.ThreatClass.FLYING, "Siege", IndRecords.ThreatClass.SIEGE,
                "Burrowing", IndRecords.ThreatClass.BURROWING);
        group("block_flags",
                "PERIMETER", IndRecords.SurveyBlocks.FLAG_PERIMETER, "ROOFLESS", IndRecords.SurveyBlocks.FLAG_ROOFLESS,
                "MACHINE", IndRecords.SurveyBlocks.FLAG_MACHINE, "INVENTORY", IndRecords.SurveyBlocks.FLAG_INVENTORY,
                "ROOF", IndRecords.SurveyBlocks.FLAG_ROOF, "SUBTERRANEAN", IndRecords.SurveyBlocks.FLAG_SUBTERRANEAN,
                "ARMOURED", IndRecords.SurveyBlocks.FLAG_ARMOURED);
        group("survey_flag", "mob_griefing", IndRecords.Survey.FLAG_MOB_GRIEFING);
    }

    private static int code(String group, String variant) {
        Map<String, Integer> m = CODES.get(group);
        if (m == null) {
            throw new AssertionError("no IndRecords code group '" + group + "' in the test");
        }
        Integer c = m.get(variant);
        if (c == null) {
            throw new AssertionError("wire.rs has variant '" + variant + "' in '" + group
                    + "' but the test maps no IndRecords constant to it");
        }
        return c;
    }

    // ---- the fixture file -----------------------------------------------------------------------------

    /** One record of the file: its bytes and the values it was built from. */
    private record Fixture(String name, byte[] bytes, Fields fields) {}

    /** The "k=v" values of one fixture, as the Rust program printed them. */
    private static final class Fields {
        final String owner;
        final Map<String, String> map = new LinkedHashMap<>();

        Fields(String owner, String line) {
            this.owner = owner;
            for (String kv : line.trim().split(" +")) {
                int eq = kv.indexOf('=');
                if (eq <= 0) {
                    throw new AssertionError(owner + ".fields: malformed pair '" + kv + "'");
                }
                if (map.put(kv.substring(0, eq), kv.substring(eq + 1)) != null) {
                    throw new AssertionError(owner + ".fields: '" + kv.substring(0, eq) + "' appears twice");
                }
            }
        }

        String sym(String k) {
            String v = map.get(k);
            if (v == null) {
                throw new AssertionError(owner + ": the fixture has no field '" + k + "'");
            }
            return v;
        }

        boolean none(String k) {
            return sym(k).equals("none");
        }

        boolean bool(String k) {
            return switch (sym(k)) {
                case "true" -> true;
                case "false" -> false;
                default -> throw new AssertionError(owner + "." + k + " is not a boolean: " + sym(k));
            };
        }

        int count(String k) {
            return Integer.parseInt(sym(k));
        }

        /** u32, as an unsigned value in a long. */
        long u32(String k) {
            long v = Long.parseLong(sym(k));
            assertTrue(v >= 0 && v <= 0xFFFF_FFFFL, owner + "." + k + " is not a u32: " + v);
            return v;
        }

        int i32(String k) {
            return Integer.parseInt(sym(k));
        }

        /** u64, as its bits in a long. */
        long u64(String k) {
            return Long.parseUnsignedLong(sym(k));
        }

        long i64(String k) {
            return Long.parseLong(sym(k));
        }

        /** An f32, as its bits (unsigned, in a long). */
        long f32(String k) {
            return Integer.toUnsignedLong(Float.floatToRawIntBits(Float.parseFloat(sym(k))));
        }
    }

    private static final class Fixtures {
        final Map<String, Fixture> records = new LinkedHashMap<>();
        final Map<String, Map<String, Integer>> codes = new LinkedHashMap<>();
    }

    private static Fixtures load() {
        Map<String, byte[]> bytes = new LinkedHashMap<>();
        Map<String, String> fieldLines = new HashMap<>();
        Fixtures out = new Fixtures();
        try (InputStream in = IndustryWireContractTest.class.getResourceAsStream(RESOURCE)) {
            if (in == null) {
                throw new AssertionError("test resource " + RESOURCE + " is missing; generate it with "
                        + "neoforge/tools/industry-wire-fixtures");
            }
            BufferedReader r = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
            String line;
            while ((line = r.readLine()) != null) {
                if (line.isBlank() || line.startsWith("#")) {
                    continue;
                }
                int sp = line.indexOf(' ');
                if (sp < 0) {
                    throw new AssertionError("malformed fixture line: " + line);
                }
                String head = line.substring(0, sp);
                String rest = line.substring(sp + 1);
                if (head.startsWith("CODES.")) {
                    Map<String, Integer> m = new LinkedHashMap<>();
                    for (String kv : rest.trim().split(" +")) {
                        int eq = kv.indexOf('=');
                        m.put(kv.substring(0, eq), Integer.parseInt(kv.substring(eq + 1)));
                    }
                    out.codes.put(head.substring("CODES.".length()), m);
                } else if (head.endsWith(".fields")) {
                    fieldLines.put(head.substring(0, head.length() - ".fields".length()), rest);
                } else {
                    bytes.put(head, HexFormat.of().parseHex(rest.trim()));
                }
            }
        } catch (IOException e) {
            throw new AssertionError("reading " + RESOURCE, e);
        }
        for (Map.Entry<String, byte[]> e : bytes.entrySet()) {
            String fl = fieldLines.get(e.getKey());
            if (fl == null) {
                throw new AssertionError("fixture " + e.getKey() + " has no .fields line");
            }
            out.records.put(e.getKey(), new Fixture(e.getKey(), e.getValue(), new Fields(e.getKey(), fl)));
        }
        return out;
    }

    // ---- reading bytes for diagnostics -------------------------------------------------------------------

    /** Where the Rust record holds these bytes (they are distinctive when no byte is zero), or -1. */
    private static int find(byte[] rust, byte[] value) {
        for (byte b : value) {
            if (b == 0) {
                return -1;
            }
        }
        outer:
        for (int i = 0; i + value.length <= rust.length; i++) {
            for (int j = 0; j < value.length; j++) {
                if (rust[i + j] != value[j]) {
                    continue outer;
                }
            }
            return i;
        }
        return -1;
    }

    private static byte[] le(long v, int len) {
        byte[] b = new byte[len];
        for (int i = 0; i < len; i++) {
            b[i] = (byte) (v >>> (8 * i));
        }
        return b;
    }

    private static String hex(byte[] b, int at, int len) {
        if (at < 0 || at + len > b.length) {
            return "(past the end of the " + b.length + "-byte record)";
        }
        return HexFormat.of().formatHex(b, at, at + len);
    }

    /** "Rust offset N" when the value is found in the Rust record, else what the Rust bytes are there. */
    private static String whereInRust(byte[] rust, long value, int len, int javaOffset) {
        int at = find(rust, le(value, len));
        return at >= 0
                ? "the Rust record has that value at offset " + at
                : "the Rust record has " + hex(rust, javaOffset, len) + " at that offset";
    }

    // ---- observations: build with IndRecords, compare bytes ----------------------------------------------

    private record Put(String field, int at, int len, long value) {}

    /** A record being built from IndRecords offsets, remembering which field went where. */
    private static final class Rec {
        final String name;
        final ByteBuffer b;
        final List<Put> puts = new ArrayList<>();

        Rec(String name, int kind, int size, int aux) {
            this.name = name;
            this.b = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN);
            put16("header kind", H_KIND, kind);
            put16("header size", H_SIZE, size);
            put32("header aux", H_AUX, aux & 0xFFFF_FFFFL);
        }

        Rec put16(String field, int at, int v) {
            b.putShort(at, (short) v);
            puts.add(new Put(field, at, 2, v));
            return this;
        }

        Rec put32(String field, int at, long v) {
            b.putInt(at, (int) v);
            puts.add(new Put(field, at, 4, v));
            return this;
        }

        Rec put64(String field, int at, long v) {
            b.putLong(at, v);
            puts.add(new Put(field, at, 8, v));
            return this;
        }

        /** Entry16 list. */
        Rec entries(int at, Fields f, String prefix, int n) {
            for (int i = 0; i < n; i++) {
                int e = at + i * IndRecords.ENTRY16;
                put32(prefix + i + ".good", e + E_GOOD, f.u32(prefix + i + ".good"));
                put64(prefix + i + ".amount", e + E_AMOUNT, f.i64(prefix + i + ".amount"));
            }
            return this;
        }

        /** Carriage blocks from {@code at}; see {@link #carriagesSize}. */
        Rec carriages(int at, Fields f) {
            for (int i = 0; i < f.count("n_carriages"); i++) {
                String p = "carriage" + i;
                int n = f.count(p + ".n_entries");
                put32(p + ".index", at + IndRecords.Train.C_INDEX, f.u32(p + ".index"));
                put32(p + ".item_slots", at + IndRecords.Train.C_ITEM_SLOTS, f.u32(p + ".item_slots"));
                put64(p + ".fluid_capacity", at + IndRecords.Train.C_FLUID_CAPACITY, f.i64(p + ".fluid_capacity"));
                put32(p + ".entry_count", at + IndRecords.Train.C_ENTRY_COUNT, n);
                entries(at + IndRecords.Train.C_ENTRIES, f, p + ".entry", n);
                at += IndRecords.Train.carriageSize(n);
            }
            return this;
        }

        /** Fails with every field that is not where the Rust record has it. */
        void assertMatches(byte[] rust) {
            List<String> problems = new ArrayList<>();
            byte[] mine = b.array();
            if (mine.length != rust.length) {
                problems.add("size: IndRecords makes this record " + mine.length + " bytes, the Rust record is "
                        + rust.length);
            }
            for (Put p : puts) {
                boolean same = p.at + p.len <= rust.length;
                for (int i = 0; same && i < p.len; i++) {
                    same = mine[p.at + i] == rust[p.at + i];
                }
                if (same) {
                    continue;
                }
                if (p.at < 8) {
                    problems.add(p.field + ": Java builds " + p.value + " at offset " + p.at + ", the Rust record has "
                            + hex(rust, p.at, p.len) + " there (little-endian)");
                } else {
                    problems.add("field " + p.field + ": IndRecords offset " + p.at + " (" + p.len + " bytes); "
                            + whereInRust(rust, p.value, p.len, p.at));
                }
            }
            if (problems.isEmpty() && !java.util.Arrays.equals(mine, rust)) {
                int d = 0;
                while (mine[d] == rust[d]) {
                    d++;
                }
                problems.add("bytes outside every named field differ (reserved or padding?): first difference at offset "
                        + d + ", Java " + hex(mine, d, 1) + " vs Rust " + hex(rust, d, 1));
            }
            if (!problems.isEmpty()) {
                fail(name + " disagrees with wire.rs:\n  " + String.join("\n  ", problems)
                        + "\n  Rust : " + HexFormat.of().formatHex(rust)
                        + "\n  Java : " + HexFormat.of().formatHex(mine));
            }
        }
    }

    private static int carriagesSize(Fields f) {
        int total = 0;
        for (int i = 0; i < f.count("n_carriages"); i++) {
            total += IndRecords.Train.carriageSize(f.count("carriage" + i + ".n_entries"));
        }
        return total;
    }

    /** A latest-wins record: i64 gameTime at {@code gameTimeAt}, no sequence. */
    private static Rec latest(String name, int kind, int size, int aux, int gameTimeAt, Fields f) {
        assertTrue(kind >= IndRecords.LATEST_FIRST && kind <= IndRecords.LATEST_LAST,
                name + ": kind 0x" + Integer.toHexString(kind) + " is not in IndRecords' latest-wins range");
        return new Rec(name, kind, size, aux).put64("game_time", gameTimeAt, f.i64("game_time"));
    }

    /** A journaled record: u64 obsSeq at 8 and i64 gameTime at 16 (the bridge's prefix). */
    private static Rec journaled(String name, int kind, int size, int aux, Fields f) {
        assertTrue(kind >= IndRecords.JOURNALED_FIRST && kind <= IndRecords.JOURNALED_LAST,
                name + ": kind 0x" + Integer.toHexString(kind) + " is not in IndRecords' journaled range");
        return new Rec(name, kind, size, aux)
                .put64("obs_seq", Obs.OBS_SEQ, f.u64("obs_seq"))
                .put64("game_time", Obs.GAME_TIME, f.i64("game_time"));
    }

    private static int flags(String group, String joined) {
        int v = 0;
        for (String part : joined.split("\\|")) {
            v |= code(group, part);
        }
        return v;
    }

    /** The record IndRecords describes for this fixture's values. */
    private static Rec build(Fixture fx) {
        Fields f = fx.fields();
        String n = fx.name();
        switch (n) {
            case "IND_STOCK_SAMPLE": {
                int c = f.count("n_entries");
                return latest(n, IndRecords.IND_STOCK_SAMPLE, IndRecords.StockSample.size(c), c,
                        IndRecords.StockSample.GAME_TIME, f)
                        .put64("district", IndRecords.StockSample.DISTRICT, f.u64("district"))
                        .put32("inventory", IndRecords.StockSample.INVENTORY, f.u32("inventory"))
                        .put32("sweep", IndRecords.StockSample.SWEEP, f.u32("sweep"))
                        .entries(IndRecords.StockSample.ENTRIES, f, "entry", c);
            }
            case "IND_SWEEP_END": {
                int aux = (f.bool("live") ? IndRecords.SweepEnd.FLAG_LIVE : 0)
                        | (f.bool("creative") ? IndRecords.SweepEnd.FLAG_CREATIVE : 0)
                        | (f.bool("overstressed") ? IndRecords.SweepEnd.FLAG_OVERSTRESSED : 0);
                return latest(n, IndRecords.IND_SWEEP_END, IndRecords.SweepEnd.SIZE, aux,
                        IndRecords.SweepEnd.GAME_TIME, f)
                        .put64("district", IndRecords.SweepEnd.DISTRICT, f.u64("district"))
                        .put32("sweep", IndRecords.SweepEnd.SWEEP, f.u32("sweep"))
                        .put32("records", IndRecords.SweepEnd.RECORDS, f.u32("records"))
                        .put32("inventories", IndRecords.SweepEnd.INVENTORIES, f.u32("inventories"));
            }
            case "IND_WATCHED":
                return latest(n, IndRecords.IND_WATCHED, IndRecords.DistrictState.SIZE, f.bool("watched") ? 1 : 0,
                        IndRecords.DistrictState.GAME_TIME, f)
                        .put64("district", IndRecords.DistrictState.DISTRICT, f.u64("district"));
            case "IND_APPROACH":
                return latest(n, IndRecords.IND_APPROACH, IndRecords.Approach.SIZE,
                        f.bool("closing") ? IndRecords.Approach.FLAG_CLOSING : 0, IndRecords.Approach.GAME_TIME, f)
                        .put64("district", IndRecords.Approach.DISTRICT, f.u64("district"))
                        .put32("distance", IndRecords.Approach.DISTANCE, f.u32("distance"))
                        .put32("eta_ticks", IndRecords.Approach.ETA_TICKS, f.u32("eta_ticks"));
            case "IND_DISTRICT":
                return journaled(n, IndRecords.IND_DISTRICT, IndRecords.District.SIZE,
                        code("not_learnable", f.sym("not_learnable")), f)
                        .put64("district", IndRecords.District.DISTRICT, f.u64("district"))
                        .put32("revision", IndRecords.District.REVISION, f.u32("revision"))
                        .put32("dim", IndRecords.District.DIM, f.u32("dim"))
                        .put32("name", IndRecords.District.NAME, f.u32("name"))
                        .put32("inventory_count", IndRecords.District.INVENTORY_COUNT, f.u32("inventory_count"));
            case "IND_DISTRICT_CHUNKS": {
                int c = f.count("n_chunks");
                Rec r = journaled(n, IndRecords.IND_DISTRICT_CHUNKS, IndRecords.DistrictChunks.size(c), c, f)
                        .put64("district", IndRecords.DistrictChunks.DISTRICT, f.u64("district"))
                        .put32("revision", IndRecords.DistrictChunks.REVISION, f.u32("revision"))
                        .put32("part", IndRecords.DistrictChunks.PART, f.u32("part"));
                for (int i = 0; i < c; i++) {
                    int at = IndRecords.DistrictChunks.CHUNKS + i * CHUNK_PAIR;
                    r.put32("chunk" + i + ".x", at + CHUNK_X, f.i32("chunk" + i + ".x") & 0xFFFF_FFFFL);
                    r.put32("chunk" + i + ".z", at + CHUNK_Z, f.i32("chunk" + i + ".z") & 0xFFFF_FFFFL);
                }
                return r;
            }
            case "IND_INVENTORY":
                return journaled(n, IndRecords.IND_INVENTORY, IndRecords.Inventory.SIZE,
                        flags("inv_kind", f.sym("kind")), f)
                        .put64("district", IndRecords.Inventory.DISTRICT, f.u64("district"))
                        .put32("revision", IndRecords.Inventory.REVISION, f.u32("revision"))
                        .put32("index", IndRecords.Inventory.INDEX, f.u32("index"))
                        .put32("x", IndRecords.Inventory.X, f.i32("x") & 0xFFFF_FFFFL)
                        .put32("y", IndRecords.Inventory.Y, f.i32("y") & 0xFFFF_FFFFL)
                        .put32("z", IndRecords.Inventory.Z, f.i32("z") & 0xFFFF_FFFFL)
                        .put32("item_slots", IndRecords.Inventory.ITEM_SLOTS, f.u32("item_slots"))
                        .put64("fluid_capacity", IndRecords.Inventory.FLUID_CAPACITY, f.i64("fluid_capacity"))
                        .put64("energy_capacity", IndRecords.Inventory.ENERGY_CAPACITY, f.i64("energy_capacity"));
            case "IND_DISTRICT_REMOVED":
                return journaled(n, IndRecords.IND_DISTRICT_REMOVED, IndRecords.DistrictRemoved.SIZE, 0, f)
                        .put64("district", IndRecords.DistrictRemoved.DISTRICT, f.u64("district"));
            case "IND_EDIT":
                return journaled(n, IndRecords.IND_EDIT, IndRecords.Edit.SIZE,
                        code("edit_kind", f.sym("edit_kind")), f)
                        .put64("district", IndRecords.Edit.DISTRICT, f.u64("district"))
                        .put64("player_most", IndRecords.Edit.PLAYER_MOST, f.u64("player_most"))
                        .put64("player_least", IndRecords.Edit.PLAYER_LEAST, f.u64("player_least"))
                        .put32("x", IndRecords.Edit.X, f.i32("x") & 0xFFFF_FFFFL)
                        .put32("y", IndRecords.Edit.Y, f.i32("y") & 0xFFFF_FFFFL)
                        .put32("z", IndRecords.Edit.Z, f.i32("z") & 0xFFFF_FFFFL);
            case "IND_DISCARD":
                return journaled(n, IndRecords.IND_DISCARD, IndRecords.Discard.SIZE,
                        code("discard_reason", f.sym("discard_reason")), f)
                        .put64("district", IndRecords.Discard.DISTRICT, f.u64("district"))
                        .put64("station", IndRecords.Discard.STATION, f.u64("station"));
            case "IND_LOAD_STATE":
                return journaled(n, IndRecords.IND_LOAD_STATE, IndRecords.LoadState.SIZE,
                        code("load_state", f.sym("load_state")), f)
                        .put64("district", IndRecords.LoadState.DISTRICT, f.u64("district"))
                        .put32("ticking_chunks", IndRecords.LoadState.TICKING_CHUNKS, f.u32("ticking_chunks"))
                        .put32("total_chunks", IndRecords.LoadState.TOTAL_CHUNKS, f.u32("total_chunks"));
            case "IND_CHUNK_LOADED":
                return journaled(n, IndRecords.IND_CHUNK_LOADED, IndRecords.ChunkLoaded.SIZE, 0, f)
                        .put64("district", IndRecords.ChunkLoaded.DISTRICT, f.u64("district"))
                        .put32("dim", IndRecords.ChunkLoaded.DIM, f.u32("dim"))
                        .put32("chunk_x", IndRecords.ChunkLoaded.X, f.i32("chunk_x") & 0xFFFF_FFFFL)
                        .put32("chunk_z", IndRecords.ChunkLoaded.Z, f.i32("chunk_z") & 0xFFFF_FFFFL)
                        .put32("applied_epoch", IndRecords.ChunkLoaded.APPLIED_EPOCH, f.u32("applied_epoch"));
            case "IND_CHUNK_UNLOADED":
                return journaled(n, IndRecords.IND_CHUNK_UNLOADED, IndRecords.ChunkUnloaded.SIZE, 0, f)
                        .put64("district", IndRecords.ChunkUnloaded.DISTRICT, f.u64("district"))
                        .put32("dim", IndRecords.ChunkUnloaded.DIM, f.u32("dim"))
                        .put32("chunk_x", IndRecords.ChunkUnloaded.X, f.i32("chunk_x") & 0xFFFF_FFFFL)
                        .put32("chunk_z", IndRecords.ChunkUnloaded.Z, f.i32("chunk_z") & 0xFFFF_FFFFL);
            case "IND_PORT":
            case "IND_PORT_NO_INVENTORY": {
                int c = f.count("n_entries");
                long inventory = f.none("inventory") ? Integer.toUnsignedLong(IndRecords.NO_INVENTORY) : f.u32("inventory");
                long op = f.none("op") ? 0 : f.u64("op");
                return journaled(n, IndRecords.IND_PORT, IndRecords.Port.size(c), c, f)
                        .put64("district", IndRecords.Port.DISTRICT, f.u64("district"))
                        .put32("port_kind", IndRecords.Port.PORT_KIND, code("port_kind", f.sym("port_kind")))
                        .put32("inventory", IndRecords.Port.INVENTORY, inventory)
                        .put64("counterpart", IndRecords.Port.COUNTERPART, f.u64("counterpart"))
                        .put64("op", IndRecords.Port.OP_ID, op)
                        .entries(IndRecords.Port.ENTRIES, f, "entry", c);
            }
            case "IND_TRAIN_ARRIVED":
            case "IND_TRAIN_ARRIVED_APPROACH": {
                int size = IndRecords.Train.ARRIVED_CARRIAGES + carriagesSize(f);
                return journaled(n, IndRecords.IND_TRAIN_ARRIVED, size, f.count("n_carriages"), f)
                        .put64("train", IndRecords.Train.TRAIN, f.u64("train"))
                        .put64("station", IndRecords.Train.STATION, f.u64("station"))
                        .put64("district", IndRecords.Train.DISTRICT, f.u64("district"))
                        .put32("simulated, cargo_from", IndRecords.Train.ARRIVED_FLAGS,
                                (f.bool("simulated") ? IndRecords.Train.FLAG_SIMULATED : 0)
                                        | code("cargo_from", f.sym("cargo_from")))
                        .carriages(IndRecords.Train.ARRIVED_CARRIAGES, f);
            }
            case "IND_STATION_PORTS": {
                int c = f.count("n_inventories");
                Rec r = journaled(n, IndRecords.IND_STATION_PORTS, IndRecords.StationPorts.size(c), c, f)
                        .put64("station", IndRecords.StationPorts.STATION, f.u64("station"))
                        .put64("district", IndRecords.StationPorts.DISTRICT, f.u64("district"));
                for (int i = 0; i < c; i++)
                    r.put32("inventory" + i, IndRecords.StationPorts.INVENTORIES + 4 * i, f.u32("inventory" + i));
                return r;
            }
            case "IND_TRAIN_UNWATCHED":
                return journaled(n, IndRecords.IND_TRAIN_UNWATCHED, IndRecords.TrainUnwatched.SIZE, 0, f)
                        .put64("train", IndRecords.TrainUnwatched.TRAIN, f.u64("train"))
                        .put64("station", IndRecords.TrainUnwatched.STATION, f.u64("station"));
            case "IND_TRAIN_DEPARTED": {
                int size = IndRecords.Train.DEPARTED_CARRIAGES + carriagesSize(f);
                return journaled(n, IndRecords.IND_TRAIN_DEPARTED, size, f.count("n_carriages"), f)
                        .put64("train", IndRecords.Train.TRAIN, f.u64("train"))
                        .put64("station", IndRecords.Train.STATION, f.u64("station"))
                        .put64("district", IndRecords.Train.DISTRICT, f.u64("district"))
                        .put64("destination", IndRecords.Train.DEPARTED_DESTINATION,
                                f.none("destination") ? 0 : f.u64("destination"))
                        .put64("eta_ticks", IndRecords.Train.DEPARTED_ETA, f.i64("eta_ticks"))
                        .carriages(IndRecords.Train.DEPARTED_CARRIAGES, f);
            }
            case "IND_STATION":
                return journaled(n, IndRecords.IND_STATION, IndRecords.Station.SIZE, 0, f)
                        .put64("station", IndRecords.Station.STATION, f.u64("station"))
                        .put64("district", IndRecords.Station.DISTRICT, f.u64("district"))
                        .put32("dim", IndRecords.Station.DIM, f.u32("dim"))
                        .put32("x", IndRecords.Station.X, f.i32("x") & 0xFFFF_FFFFL)
                        .put32("y", IndRecords.Station.Y, f.i32("y") & 0xFFFF_FFFFL)
                        .put32("z", IndRecords.Station.Z, f.i32("z") & 0xFFFF_FFFFL)
                        .put32("name", IndRecords.Station.NAME, f.u32("name"));
            case "IND_RECONCILE_RESULT": {
                int c = f.count("n_entries");
                int m = f.count("n_destroyed");
                Rec r = journaled(n, IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.size(c, m), c, f)
                        .put64("op", IndRecords.ReconcileResult.OP_ID, f.u64("op"))
                        .put64("district", IndRecords.ReconcileResult.DISTRICT, f.u64("district"))
                        .put32("epoch", IndRecords.ReconcileResult.EPOCH, f.u32("epoch"))
                        .put32("dim", IndRecords.ReconcileResult.DIM, f.u32("dim"))
                        .put32("chunk_x", IndRecords.ReconcileResult.CHUNK_X, f.i32("chunk_x") & 0xFFFF_FFFFL)
                        .put32("chunk_z", IndRecords.ReconcileResult.CHUNK_Z, f.i32("chunk_z") & 0xFFFF_FFFFL)
                        .put32("n_destroyed", IndRecords.ReconcileResult.DESTROYED, m)
                        .put32("blocks_skipped", IndRecords.ReconcileResult.SKIPPED, f.u32("blocks_skipped"));
                for (int i = 0; i < c; i++) {
                    int at = IndRecords.ReconcileResult.ENTRIES + i * IndRecords.RESULT24;
                    r.put32("entry" + i + ".inventory", at + R_FIRST, f.u32("entry" + i + ".inventory"));
                    r.put32("entry" + i + ".good", at + R_GOOD, f.u32("entry" + i + ".good"));
                    r.put64("entry" + i + ".requested", at + R_REQUESTED, f.i64("entry" + i + ".requested"));
                    r.put64("entry" + i + ".applied", at + R_APPLIED, f.i64("entry" + i + ".applied"));
                }
                for (int j = 0; j < m; j++) {
                    int at = IndRecords.ReconcileResult.ENTRIES + c * IndRecords.RESULT24 + j * IndRecords.POS16;
                    String p = "destroyed" + j + ".";
                    r.put32(p + "x", at + P_X, f.i32(p + "x") & 0xFFFF_FFFFL);
                    r.put32(p + "y", at + P_Y, f.i32(p + "y") & 0xFFFF_FFFFL);
                    r.put32(p + "z", at + P_Z, f.i32(p + "z") & 0xFFFF_FFFFL);
                }
                return r;
            }
            case "IND_CARRIAGE_RESULT": {
                int c = f.count("n_entries");
                Rec r = journaled(n, IndRecords.IND_CARRIAGE_RESULT, IndRecords.CarriageResult.size(c), c, f)
                        .put64("op", IndRecords.CarriageResult.OP_ID, f.u64("op"))
                        .put64("train", IndRecords.CarriageResult.TRAIN, f.u64("train"));
                for (int i = 0; i < c; i++) {
                    int at = IndRecords.CarriageResult.ENTRIES + i * IndRecords.RESULT24;
                    r.put32("entry" + i + ".carriage", at + R_FIRST, f.u32("entry" + i + ".carriage"));
                    r.put32("entry" + i + ".good", at + R_GOOD, f.u32("entry" + i + ".good"));
                    r.put64("entry" + i + ".requested", at + R_REQUESTED, f.i64("entry" + i + ".requested"));
                    r.put64("entry" + i + ".applied", at + R_APPLIED, f.i64("entry" + i + ".applied"));
                }
                return r;
            }
            case "IND_DEPOSIT_SAMPLE":
                return latest(n, IndRecords.IND_DEPOSIT_SAMPLE, IndRecords.DepositSample.SIZE, code("deposit_plugin", f.sym("plugin")),
                        IndRecords.DepositSample.GAME_TIME, f)
                        .put64("district", IndRecords.DepositSample.DISTRICT, f.u64("district"))
                        .put32("good", IndRecords.DepositSample.GOOD, f.u32("good"))
                        .put32("sweep", IndRecords.DepositSample.SWEEP, f.u32("sweep"))
                        .put64("remaining", IndRecords.DepositSample.REMAINING, f.i64("remaining"))
                        .put32("dim", IndRecords.DepositSample.DIM, f.u32("dim"))
                        .put32("chunk_x", IndRecords.DepositSample.CHUNK_X, f.i32("chunk_x") & 0xFFFF_FFFFL)
                        .put32("chunk_z", IndRecords.DepositSample.CHUNK_Z, f.i32("chunk_z") & 0xFFFF_FFFFL)
                        .put32("infinite", IndRecords.DepositSample.FLAGS, f.bool("infinite") ? IndRecords.DepositSample.FLAG_INFINITE : 0);
            case "IND_CONDITIONS":
                return journaled(n, IndRecords.IND_CONDITIONS, IndRecords.Conditions.SIZE, 0, f)
                        .put32("season", IndRecords.Conditions.SEASON, f.u32("season"))
                        .put32("season_count", IndRecords.Conditions.SEASON_COUNT, f.u32("season_count"))
                        .put64("season_length", IndRecords.Conditions.SEASON_LENGTH, f.i64("season_length"))
                        .put64("season_origin", IndRecords.Conditions.SEASON_ORIGIN, f.i64("season_origin"));
            case "IND_POWER_LINK":
                return journaled(n, IndRecords.IND_POWER_LINK, IndRecords.PowerLink.SIZE, 0, f)
                        .put64("sink", IndRecords.PowerLink.SINK, f.u64("sink"))
                        .put64("source", IndRecords.PowerLink.SOURCE, f.u64("source"));
            case "IND_PACKAGE": {
                int c = f.count("n_entries");
                return journaled(n, IndRecords.IND_PACKAGE, IndRecords.Package.size(c), c, f)
                        .put64("district", IndRecords.Package.DISTRICT, f.u64("district"))
                        .put32("inventory", IndRecords.Package.INVENTORY, f.u32("inventory"))
                        .put32("direction", IndRecords.Package.DIRECTION, code("package_direction", f.sym("direction")))
                        .put32("address", IndRecords.Package.ADDRESS, f.u32("address"))
                        .put32("packages", IndRecords.Package.PACKAGES, f.u32("packages"))
                        .put64("op", IndRecords.Package.OP_ID, f.u64("op"))
                        .entries(IndRecords.Package.ENTRIES, f, "entry", c);
            }
            case "IND_SPAWN_SAMPLE": {
                Rec r = journaled(n, IndRecords.IND_SPAWN_SAMPLE, IndRecords.SpawnSample.SIZE, 0, f)
                        .put64("district", IndRecords.SpawnSample.DISTRICT, f.u64("district"))
                        .put64("observed_ticks", IndRecords.SpawnSample.OBSERVED_TICKS, f.i64("observed_ticks"))
                        .put64("dark_blocks", IndRecords.SpawnSample.DARK_BLOCKS, f.i64("dark_blocks"));
                for (int i = 0; i < IndRecords.ThreatClass.COUNT; i++)
                    r.put32("spawns" + i, IndRecords.SpawnSample.SPAWNS + 4 * i, f.u32("spawns" + i));
                return r;
            }
            case "IND_FIGHT":
                return journaled(n, IndRecords.IND_FIGHT, IndRecords.Fight.SIZE, code("threat_class", f.sym("class")), f)
                        .put64("district", IndRecords.Fight.DISTRICT, f.u64("district"))
                        .put32("attackers", IndRecords.Fight.ATTACKERS, f.u32("attackers"))
                        .put32("attacker_strength", IndRecords.Fight.ATTACKER_STRENGTH, f.f32("attacker_strength"))
                        .put32("attackers_killed", IndRecords.Fight.ATTACKERS_KILLED, f.u32("attackers_killed"))
                        .put32("blocks_destroyed", IndRecords.Fight.BLOCKS_DESTROYED, f.u32("blocks_destroyed"))
                        .put32("machines_destroyed", IndRecords.Fight.MACHINES_DESTROYED, f.u32("machines_destroyed"))
                        .put32("guards_lost", IndRecords.Fight.GUARDS_LOST, f.u32("guards_lost"))
                        .put64("items_stolen", IndRecords.Fight.ITEMS_STOLEN, f.i64("items_stolen"))
                        .put64("duration_ticks", IndRecords.Fight.DURATION_TICKS, f.i64("duration_ticks"));
            case "IND_SURVEY": {
                Rec r = journaled(n, IndRecords.IND_SURVEY, IndRecords.Survey.SIZE,
                        f.bool("mob_griefing") ? IndRecords.Survey.FLAG_MOB_GRIEFING : 0, f)
                        .put64("district", IndRecords.Survey.DISTRICT, f.u64("district"))
                        .put32("revision", IndRecords.Survey.REVISION, f.u32("revision"))
                        .put32("perimeter_columns", IndRecords.Survey.PERIMETER_COLUMNS, f.u32("perimeter_columns"))
                        .put32("perimeter_gaps", IndRecords.Survey.PERIMETER_GAPS, f.u32("perimeter_gaps"))
                        .put32("dark_spawnable", IndRecords.Survey.DARK_SPAWNABLE, f.u32("dark_spawnable"))
                        .put32("machine_blocks", IndRecords.Survey.MACHINE_BLOCKS, f.u32("machine_blocks"))
                        .put32("roofed_machines", IndRecords.Survey.ROOFED_MACHINES, f.u32("roofed_machines"))
                        .put32("kill_machines_at_gaps", IndRecords.Survey.KILL_MACHINES_AT_GAPS, f.u32("kill_machines_at_gaps"))
                        .put32("ammo_good", IndRecords.Survey.AMMO_GOOD, f.u32("ammo_good"))
                        .put32("ammo_per_engagement", IndRecords.Survey.AMMO_PER_ENGAGEMENT, f.u32("ammo_per_engagement"));
                for (int i = 0; i < IndRecords.ThreatClass.COUNT; i++) {
                    r.put32("turret" + i, IndRecords.Survey.TURRET_STRENGTH + 4 * i, f.f32("turret" + i));
                    r.put32("guards" + i, IndRecords.Survey.GUARDS + 4 * i, f.u32("guards" + i));
                }
                return r;
            }
            case "IND_SURVEY_BLOCKS": {
                int c = f.count("n_blocks");
                Rec r = journaled(n, IndRecords.IND_SURVEY_BLOCKS, IndRecords.SurveyBlocks.size(c), c, f)
                        .put64("district", IndRecords.SurveyBlocks.DISTRICT, f.u64("district"))
                        .put32("revision", IndRecords.SurveyBlocks.REVISION, f.u32("revision"))
                        .put32("part", IndRecords.SurveyBlocks.PART, f.u32("part"))
                        .put32("chunk_x", IndRecords.SurveyBlocks.CHUNK_X, f.i32("chunk_x") & 0xFFFF_FFFFL)
                        .put32("chunk_z", IndRecords.SurveyBlocks.CHUNK_Z, f.i32("chunk_z") & 0xFFFF_FFFFL)
                        .put32("partial", IndRecords.SurveyBlocks.FLAGS, f.bool("partial") ? IndRecords.SurveyBlocks.PARTIAL : 0);
                for (int i = 0; i < c; i++) {
                    int at = IndRecords.SurveyBlocks.BLOCKS + i * IndRecords.SurveyBlocks.BLOCK;
                    String p = "block" + i + ".";
                    r.put32(p + "x", at + IndRecords.SurveyBlocks.B_X, f.i32(p + "x") & 0xFFFF_FFFFL);
                    r.put32(p + "y", at + IndRecords.SurveyBlocks.B_Y, f.i32(p + "y") & 0xFFFF_FFFFL);
                    r.put32(p + "z", at + IndRecords.SurveyBlocks.B_Z, f.i32(p + "z") & 0xFFFF_FFFFL);
                    r.put32(p + "state", at + IndRecords.SurveyBlocks.B_STATE, f.u32(p + "state"));
                    r.put32(p + "flags", at + IndRecords.SurveyBlocks.B_FLAGS, f.count(p + "flags"));
                    r.put32(p + "inventory", at + IndRecords.SurveyBlocks.B_INVENTORY,
                            f.none(p + "inventory") ? IndRecords.NO_INVENTORY & 0xFFFF_FFFFL : f.u32(p + "inventory"));
                }
                return r;
            }
            default:
                throw new AssertionError("fixture " + n + " is an observation the test has no IndRecords builder for");
        }
    }

    // ---- commands: read the Rust bytes at IndRecords offsets ---------------------------------------------

    /** Reads a Rust command record at IndRecords offsets, checking each field and collecting the misses. */
    private static final class Reader {
        final String name;
        final byte[] raw;
        final ByteBuffer b;
        final BitSet read = new BitSet();
        final List<String> problems = new ArrayList<>();

        Reader(String name, byte[] raw) {
            this.name = name;
            this.raw = raw;
            this.b = ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN);
        }

        private boolean fits(String field, int at, int len) {
            if (at < 0 || at + len > raw.length) {
                problems.add("field " + field + ": IndRecords offset " + at + " (" + len
                        + " bytes) is past the end of the " + raw.length + "-byte Rust record");
                return false;
            }
            read.set(at, at + len);
            return true;
        }

        private void miss(String field, int at, int len, long actual, long expected) {
            problems.add("field " + field + ": IndRecords offset " + at + " reads " + actual + " but the fixture says "
                    + expected + "; " + whereInRust(raw, expected, len, at));
        }

        Reader u16(String field, int at, long expected) {
            if (fits(field, at, 2)) {
                long v = Short.toUnsignedLong(b.getShort(at));
                if (v != expected) {
                    miss(field, at, 2, v, expected);
                }
            }
            return this;
        }

        /** An unsigned 32-bit field. */
        Reader u32(String field, int at, long expected) {
            if (fits(field, at, 4)) {
                long v = Integer.toUnsignedLong(b.getInt(at));
                if (v != (expected & 0xFFFF_FFFFL)) {
                    miss(field, at, 4, v, expected & 0xFFFF_FFFFL);
                }
            }
            return this;
        }

        Reader i32(String field, int at, int expected) {
            if (fits(field, at, 4)) {
                int v = b.getInt(at);
                if (v != expected) {
                    miss(field, at, 4, v, expected);
                }
            }
            return this;
        }

        /** A 64-bit field, signed or unsigned: the bits are compared. */
        Reader b64(String field, int at, long expected) {
            if (fits(field, at, 8)) {
                long v = b.getLong(at);
                if (v != expected) {
                    miss(field, at, 8, v, expected);
                }
            }
            return this;
        }

        /** Fixed header: kind, size (against IndRecords' size and the bytes), aux, then cmdSeq. */
        Reader header(int kind, int size, long aux, Fields f) {
            assertTrue(IndRecords.isIndustryCommand(kind),
                    name + ": kind 0x" + Integer.toHexString(kind) + " is not in IndRecords' command range");
            if (raw.length != size) {
                problems.add("size: IndRecords says " + size + " bytes for these counts, the Rust record is "
                        + raw.length);
            }
            u16("header kind", H_KIND, kind);
            u16("header size", H_SIZE, raw.length);
            u32("header aux", H_AUX, aux);
            b64("cmd_seq", Cmd.CMD_SEQ, f.u64("cmd_seq"));
            return this;
        }

        /** Every byte no field read (reserved words and padding) must be zero. */
        void finish() {
            for (int i = read.nextClearBit(0); i < raw.length; i = read.nextClearBit(i + 1)) {
                if (raw[i] != 0) {
                    problems.add("byte " + i + " is read by no IndRecords field and is not zero (0x"
                            + hex(raw, i, 1) + "): reserved or padding?");
                }
            }
            if (!problems.isEmpty()) {
                fail(name + " disagrees with wire.rs:\n  " + String.join("\n  ", problems)
                        + "\n  Rust : " + HexFormat.of().formatHex(raw));
            }
        }
    }

    private static void check(Fixture fx) {
        Fields f = fx.fields();
        Reader r = new Reader(fx.name(), fx.bytes());
        switch (fx.name()) {
            case "IND_CHUNK_RECONCILE": {
                int nd = f.count("n_deltas");
                int nb = f.count("n_destroy");
                r.header(IndRecords.IND_CHUNK_RECONCILE, IndRecords.ChunkReconcile.size(nd, nb), nd, f);
                r.b64("op", IndRecords.ChunkReconcile.OP_ID, f.u64("op"))
                        .b64("district", IndRecords.ChunkReconcile.DISTRICT, f.u64("district"))
                        .u32("epoch", IndRecords.ChunkReconcile.EPOCH, f.u32("epoch"))
                        .u32("dim", IndRecords.ChunkReconcile.DIM, f.u32("dim"))
                        .i32("chunk_x", IndRecords.ChunkReconcile.CHUNK_X, f.i32("chunk_x"))
                        .i32("chunk_z", IndRecords.ChunkReconcile.CHUNK_Z, f.i32("chunk_z"))
                        .u32("n_destroy", IndRecords.ChunkReconcile.DESTROY_COUNT, nb);
                for (int i = 0; i < nd; i++) {
                    int at = IndRecords.ChunkReconcile.DELTAS + i * IndRecords.ChunkReconcile.DELTA_SIZE;
                    String p = "delta" + i + ".";
                    r.u32(p + "inventory", at + IndRecords.ChunkReconcile.D_INVENTORY, f.u32(p + "inventory"))
                            .u32(p + "good", at + IndRecords.ChunkReconcile.D_GOOD, f.u32(p + "good"))
                            .i32(p + "x", at + IndRecords.ChunkReconcile.D_X, f.i32(p + "x"))
                            .i32(p + "y", at + IndRecords.ChunkReconcile.D_Y, f.i32(p + "y"))
                            .i32(p + "z", at + IndRecords.ChunkReconcile.D_Z, f.i32(p + "z"))
                            .b64(p + "delta", at + IndRecords.ChunkReconcile.D_DELTA, f.i64(p + "delta"));
                }
                int destroys = IndRecords.ChunkReconcile.DELTAS + nd * IndRecords.ChunkReconcile.DELTA_SIZE;
                for (int j = 0; j < nb; j++) {
                    int at = destroys + j * IndRecords.ChunkReconcile.DESTROY_SIZE;
                    String p = "destroy" + j + ".";
                    r.i32(p + "x", at + IndRecords.ChunkReconcile.B_X, f.i32(p + "x"))
                            .i32(p + "y", at + IndRecords.ChunkReconcile.B_Y, f.i32(p + "y"))
                            .i32(p + "z", at + IndRecords.ChunkReconcile.B_Z, f.i32(p + "z"))
                            .u32(p + "expected_state", at + IndRecords.ChunkReconcile.B_STATE,
                                    f.u32(p + "expected_state"));
                }
                break;
            }
            case "IND_CARRIAGE_DELTA": {
                int c = f.count("n_entries");
                r.header(IndRecords.IND_CARRIAGE_DELTA, IndRecords.CarriageDelta.size(c), c, f);
                r.b64("op", IndRecords.CarriageDelta.OP_ID, f.u64("op"))
                        .b64("train", IndRecords.CarriageDelta.TRAIN, f.u64("train"))
                        .u32("reason", IndRecords.CarriageDelta.REASON, code("carriage_reason", f.sym("reason")));
                for (int i = 0; i < c; i++) {
                    int at = IndRecords.CarriageDelta.ENTRIES + i * IndRecords.CarriageDelta.ENTRY;
                    String p = "entry" + i + ".";
                    r.u32(p + "carriage", at + IndRecords.CarriageDelta.E_CARRIAGE, f.u32(p + "carriage"))
                            .u32(p + "good", at + IndRecords.CarriageDelta.E_GOOD, f.u32(p + "good"))
                            .b64(p + "delta", at + IndRecords.CarriageDelta.E_DELTA, f.i64(p + "delta"));
                }
                break;
            }
            case "IND_DEPOSIT_DELTA": {
                r.header(IndRecords.IND_DEPOSIT_DELTA, IndRecords.DepositDelta.SIZE, code("deposit_plugin", f.sym("plugin")), f);
                r.b64("op", IndRecords.DepositDelta.OP_ID, f.u64("op"))
                        .u32("dim", IndRecords.DepositDelta.DIM, f.u32("dim"))
                        .i32("chunk_x", IndRecords.DepositDelta.CHUNK_X, f.i32("chunk_x"))
                        .i32("chunk_z", IndRecords.DepositDelta.CHUNK_Z, f.i32("chunk_z"))
                        .u32("good", IndRecords.DepositDelta.GOOD, f.u32("good"))
                        .b64("delta", IndRecords.DepositDelta.DELTA, f.i64("delta"));
                break;
            }
            case "IND_WRITE_PACKAGE": {
                int c = f.count("n_entries");
                r.header(IndRecords.IND_WRITE_PACKAGE, IndRecords.WritePackage.size(c), c, f);
                r.b64("op", IndRecords.WritePackage.OP_ID, f.u64("op"))
                        .b64("district", IndRecords.WritePackage.DISTRICT, f.u64("district"))
                        .u32("inventory", IndRecords.WritePackage.INVENTORY, f.u32("inventory"))
                        .i32("x", IndRecords.WritePackage.X, f.i32("x"))
                        .i32("y", IndRecords.WritePackage.Y, f.i32("y"))
                        .i32("z", IndRecords.WritePackage.Z, f.i32("z"))
                        .u32("address", IndRecords.WritePackage.ADDRESS, f.u32("address"))
                        .u32("count", IndRecords.WritePackage.COUNT, f.u32("count"));
                for (int i = 0; i < c; i++) {
                    int at = IndRecords.WritePackage.ENTRIES + i * IndRecords.ENTRY16;
                    r.u32("entry" + i + ".good", at + E_GOOD, f.u32("entry" + i + ".good"))
                            .b64("entry" + i + ".amount", at + E_AMOUNT, f.i64("entry" + i + ".amount"));
                }
                break;
            }
            default:
                throw new AssertionError("fixture " + fx.name() + " is a command the test has no IndRecords reader for");
        }
        r.finish();
    }

    // ---- the tests ---------------------------------------------------------------------------------------

    /** I9: IND_DISTRICT never carries not-learnable code 6; the engine reads it as learnable now. */
    @Test
    void javaNeverSendsNotLearnableCodeSix() {
        assertEquals(6, IndRecords.NotLearnable.SPAWN_FED_FARM, "the code stays named, as the engine's table has it");
        assertEquals(0, IndRecords.NotLearnable.sent(IndRecords.NotLearnable.SPAWN_FED_FARM), "6 goes out as learnable");
        assertEquals(0, IndRecords.NotLearnable.sent(0));
        for (int code : new int[] {IndRecords.NotLearnable.ROTATION_CROSSES, IndRecords.NotLearnable.ELECTRIC_LINE_CROSSES,
                IndRecords.NotLearnable.MIXTURE_NOT_STEADY, IndRecords.NotLearnable.FINITE_DEPOSIT_UNKNOWN,
                IndRecords.NotLearnable.MOVING_CONTRAPTION, IndRecords.NotLearnable.CONVEYOR_CROSSES})
            assertEquals(code, IndRecords.NotLearnable.sent(code), "a real reason is sent as it is");
    }

    /** The retired ledger kinds stay out of IndRecords: the engine reads 0x0044 and 0x8405 as unknown. */
    @Test
    void theLedgerKindsAreNotInTheFixtures() {
        Fixtures fixtures = load();
        for (String name : fixtures.records.keySet())
            assertTrue(!name.contains("LEDGER"), "retired record in the fixtures: " + name);
    }

    @Test
    void fixtureFileHoldsEveryRequiredRecord() {
        Fixtures fixtures = load();
        for (String name : REQUIRED) {
            assertTrue(fixtures.records.containsKey(name),
                    "industry-wire-fixtures.txt has no " + name + "; regenerate it with neoforge/tools/industry-wire-fixtures");
        }
        for (String name : fixtures.records.keySet()) {
            assertTrue(REQUIRED.contains(name), "industry-wire-fixtures.txt has " + name
                    + ", which the test does not list as required (add it to REQUIRED and give it a builder)");
        }
        for (String name : fixtures.records.keySet()) {
            byte[] bytes = fixtures.records.get(name).bytes();
            assertTrue(bytes.length >= 8 && bytes.length % 8 == 0, name + ": size " + bytes.length + " is not a multiple of 8");
            assertEquals(bytes.length, Short.toUnsignedInt(ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).getShort(H_SIZE)),
                    name + ": header size");
        }
    }

    @TestFactory
    Stream<DynamicTest> observationsAreByteIdenticalToWireRs() {
        return load().records.values().stream()
                .filter(fx -> !COMMANDS.contains(fx.name()))
                .map(fx -> DynamicTest.dynamicTest(fx.name(), () -> {
                    Rec mine;
                    try {
                        mine = build(fx);
                    } catch (IndexOutOfBoundsException e) {
                        throw new AssertionError(fx.name() + ": IndRecords' offsets write past its own size() for these counts: "
                                + e.getMessage(), e);
                    }
                    mine.assertMatches(fx.bytes());
                }));
    }

    @TestFactory
    Stream<DynamicTest> commandsReadBackAsWireRsWroteThem() {
        return load().records.values().stream()
                .filter(fx -> COMMANDS.contains(fx.name()))
                .map(fx -> DynamicTest.dynamicTest(fx.name(), () -> check(fx)));
    }

    @TestFactory
    Stream<DynamicTest> enumCodesMatchWireRs() {
        Fixtures fixtures = load();
        assertTrue(!fixtures.codes.isEmpty(), "industry-wire-fixtures.txt has no CODES lines");
        return fixtures.codes.entrySet().stream()
                .map(g -> DynamicTest.dynamicTest("CODES." + g.getKey(), () -> {
                    List<String> problems = new ArrayList<>();
                    for (Map.Entry<String, Integer> v : g.getValue().entrySet()) {
                        Map<String, Integer> java = CODES.get(g.getKey());
                        if (java == null || !java.containsKey(v.getKey())) {
                            problems.add(v.getKey() + ": wire.rs writes " + v.getValue()
                                    + ", the test maps no IndRecords constant to it");
                        } else if (!java.get(v.getKey()).equals(v.getValue())) {
                            problems.add(v.getKey() + ": wire.rs writes " + v.getValue()
                                    + ", the IndRecords constant is " + java.get(v.getKey()));
                        }
                    }
                    if (!problems.isEmpty()) {
                        fail("code group " + g.getKey() + " disagrees with wire.rs:\n  " + String.join("\n  ", problems));
                    }
                }));
    }

    @Test
    void everyCodeGroupTheTestMapsIsInTheFixtures() {
        Fixtures fixtures = load();
        for (String group : CODES.keySet()) {
            assertTrue(fixtures.codes.containsKey(group), "the test maps code group " + group
                    + " but industry-wire-fixtures.txt has no CODES." + group);
        }
    }
}
