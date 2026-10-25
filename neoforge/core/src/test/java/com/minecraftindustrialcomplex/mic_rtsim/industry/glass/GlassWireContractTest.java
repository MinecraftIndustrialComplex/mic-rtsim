package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import org.junit.jupiter.api.Test;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The cross-language contract of the glass box's records (G-B19): the Rust reference codec
 * ({@code crates/rtsim-industry/src/glass/codec.rs}) against {@link GlassRecords}, in pure Java, as
 * {@code IndustryWireContractTest} does for the industry records.
 *
 * <p>{@code glass-wire-fixtures.txt} is printed by {@code neoforge/tools/industry-wire-fixtures} with the
 * {@code glass} argument: one record of each kind, a different value in every field. Observations (Java
 * writes) are rebuilt here from the values with nothing but {@link GlassRecords} offsets and compared byte
 * for byte; the command (the engine writes) is read at the {@link GlassRecords} offsets. The node and edge
 * kind codes are checked against the constants. Never edit the fixture file: fix {@link GlassRecords} (or
 * the codec and the page together) and regenerate it.
 */
class GlassWireContractTest {

    private static final String RESOURCE = "/glass-wire-fixtures.txt";

    private static final List<String> REQUIRED = List.of("IND_GRAPH_NODE", "IND_GRAPH_NODE_NONE", "IND_GRAPH_EDGE",
            "IND_GRAPH_EDGE_UNLIMITED", "IND_GRAPH_RECIPE", "IND_GRAPH_DOMAIN", "IND_GRAPH_END", "IND_PROBE_SAMPLE",
            "IND_CHUNK_TICKING", "IND_PROBE_SET", "IND_COUNT_SAMPLE", "IND_COUNT_SET");

    private record Fixture(byte[] bytes, Map<String, String> f) {
        String sym(String k) {
            String v = f.get(k);
            if (v == null)
                throw new AssertionError("fixture has no field " + k + ": " + f);
            return v;
        }

        long u32(String k) {
            return Long.parseLong(sym(k));
        }

        /** A u32 index that reads {@code none} for NONE. */
        int index(String k) {
            return sym(k).equals("none") ? GlassRecords.NONE : (int) u32(k);
        }

        long u64(String k) {
            return Long.parseUnsignedLong(sym(k));
        }

        long i64(String k) {
            return Long.parseLong(sym(k));
        }

        int i32(String k) {
            return Integer.parseInt(sym(k));
        }

        int count(String k) {
            return Integer.parseInt(sym(k));
        }

        double f64(String k) {
            return Double.parseDouble(sym(k));
        }
    }

    private static final Map<String, Fixture> RECORDS = new LinkedHashMap<>();
    private static final Map<String, Map<String, Integer>> CODES = new LinkedHashMap<>();

    static {
        Map<String, byte[]> bytes = new LinkedHashMap<>();
        Map<String, Map<String, String>> fields = new HashMap<>();
        try (InputStream in = GlassWireContractTest.class.getResourceAsStream(RESOURCE)) {
            if (in == null)
                throw new AssertionError(RESOURCE + " is missing; generate it with neoforge/tools/industry-wire-fixtures glass");
            BufferedReader r = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
            String line;
            while ((line = r.readLine()) != null) {
                if (line.isBlank() || line.startsWith("#"))
                    continue;
                int sp = line.indexOf(' ');
                String head = line.substring(0, sp), rest = line.substring(sp + 1).trim();
                if (head.startsWith("CODES.")) {
                    Map<String, Integer> m = new LinkedHashMap<>();
                    for (String kv : rest.split(" +"))
                        m.put(kv.substring(0, kv.indexOf('=')), Integer.parseInt(kv.substring(kv.indexOf('=') + 1)));
                    CODES.put(head.substring(6), m);
                } else if (head.endsWith(".fields")) {
                    Map<String, String> m = new LinkedHashMap<>();
                    for (String kv : rest.split(" +"))
                        m.put(kv.substring(0, kv.indexOf('=')), kv.substring(kv.indexOf('=') + 1));
                    fields.put(head.substring(0, head.length() - 7), m);
                } else {
                    bytes.put(head, HexFormat.of().parseHex(rest));
                }
            }
        } catch (IOException e) {
            throw new AssertionError(e);
        }
        bytes.forEach((k, v) -> RECORDS.put(k, new Fixture(v, fields.get(k))));
    }

    private static Fixture fx(String name) {
        Fixture f = RECORDS.get(name);
        if (f == null)
            throw new AssertionError("the fixtures lack " + name);
        return f;
    }

    /** A record buffer built the way the Java side writes one. */
    private static ByteBuffer rec(int kind, int size, int aux) {
        ByteBuffer b = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN);
        b.putShort(0, (short) kind);
        b.putShort(2, (short) size);
        b.putInt(4, aux);
        return b;
    }

    private static void same(String name, ByteBuffer built, byte[] rust) {
        assertArrayEquals(rust, built.array(), name + ": Java's bytes differ from the Rust codec's\n  rust "
                + HexFormat.of().formatHex(rust) + "\n  java " + HexFormat.of().formatHex(built.array()));
    }

    private static void headOf(ByteBuffer b, Fixture f) {
        b.putLong(GlassRecords.GAME_TIME, f.i64("game_time"));
        b.putLong(GlassRecords.DISTRICT, f.u64("district"));
        b.putInt(GlassRecords.COMPILE, (int) f.u32("compile"));
        b.putInt(GlassRecords.INDEX, (int) f.u32("index"));
    }

    private static int kindCode(String group, String variant) {
        return CODES.get(group).get(variant);
    }

    @Test
    void everyFixtureIsPresent() {
        for (String r : REQUIRED)
            assertTrue(RECORDS.containsKey(r), "the fixtures lack " + r);
        assertTrue(CODES.containsKey("node_kind") && CODES.containsKey("edge_kind"), "code tables");
    }

    @Test
    void nodeAndEdgeKindCodes() {
        Map<String, Integer> node = Map.ofEntries(Map.entry("Store", GlassRecords.Node.STORE), Map.entry("Buffer", GlassRecords.Node.BUFFER),
                Map.entry("Machine", GlassRecords.Node.MACHINE), Map.entry("Mover", GlassRecords.Node.MOVER),
                Map.entry("Junction", GlassRecords.Node.JUNCTION), Map.entry("Heater", GlassRecords.Node.HEATER),
                Map.entry("Source", GlassRecords.Node.SOURCE), Map.entry("Sink", GlassRecords.Node.SINK),
                Map.entry("BlackBox", GlassRecords.Node.BLACK_BOX), Map.entry("Controller", GlassRecords.Node.CONTROLLER),
                Map.entry("Port", GlassRecords.Node.PORT), Map.entry("Boiler", GlassRecords.Node.BOILER));
        assertEquals(node, CODES.get("node_kind"), "node kinds");
        Map<String, Integer> edge = Map.ofEntries(Map.entry("Adjacency", GlassRecords.Edge.ADJACENCY), Map.entry("Belt", GlassRecords.Edge.BELT),
                Map.entry("Chute", GlassRecords.Edge.CHUTE), Map.entry("Pipe", GlassRecords.Edge.PIPE), Map.entry("Chain", GlassRecords.Edge.CHAIN),
                Map.entry("AirZone", GlassRecords.Edge.AIR_ZONE), Map.entry("ArmReach", GlassRecords.Edge.ARM_REACH),
                Map.entry("Heat", GlassRecords.Edge.HEAT), Map.entry("Control", GlassRecords.Edge.CONTROL),
                Map.entry("Observe", GlassRecords.Edge.OBSERVE), Map.entry("Line", GlassRecords.Edge.LINE), Map.entry("Drop", GlassRecords.Edge.DROP));
        assertEquals(edge, CODES.get("edge_kind"), "edge kinds");
    }

    private static void node(String name) {
        Fixture f = fx(name);
        int n = f.count("n_rates");
        ByteBuffer b = rec(GlassRecords.IND_GRAPH_NODE, GlassRecords.Node.size(n), kindCode("node_kind", f.sym("kind")));
        headOf(b, f);
        b.putInt(GlassRecords.Node.MODEL, (int) f.u32("model"));
        b.putInt(GlassRecords.Node.DOMAIN, f.sym("domain").equals("none") ? GlassRecords.NONE : (int) f.u32("domain"));
        b.putInt(GlassRecords.Node.INVENTORY, f.sym("inventory").equals("none") ? GlassRecords.NONE : (int) f.u32("inventory"));
        b.putInt(GlassRecords.Node.FLAGS, (int) f.u32("flags"));
        b.putInt(GlassRecords.Node.X, f.i32("x"));
        b.putInt(GlassRecords.Node.Y, f.i32("y"));
        b.putInt(GlassRecords.Node.Z, f.i32("z"));
        b.putInt(GlassRecords.Node.RULE, (int) f.u32("rule"));
        b.putInt(GlassRecords.Node.N_RATES, n);
        b.putLong(GlassRecords.Node.CAPACITY, f.i64("capacity"));
        b.putLong(GlassRecords.Node.PARAM0, f.i64("param0"));
        b.putLong(GlassRecords.Node.PARAM1, f.i64("param1"));
        b.putLong(GlassRecords.Node.PARAM2, f.i64("param2"));
        b.putLong(GlassRecords.Node.PARAM3, f.i64("param3"));
        for (int i = 0; i < n; i++) {
            int e = GlassRecords.Node.RATES + i * GlassRecords.Node.RATE;
            String p = "rate" + i + ".";
            b.putInt(e + GlassRecords.Node.R_LEVEL, (int) f.u32(p + "level"));
            b.putInt(e + GlassRecords.Node.R_RECIPE, f.sym(p + "recipe").equals("none") ? GlassRecords.NONE : (int) f.u32(p + "recipe"));
            b.putInt(e + GlassRecords.Node.R_BATCH, (int) f.u32(p + "batch"));
            b.putInt(e + GlassRecords.Node.R_TICKS_NUM, (int) f.u32(p + "ticks_num"));
            b.putInt(e + GlassRecords.Node.R_TICKS_DEN, (int) f.u32(p + "ticks_den"));
            b.putInt(e + GlassRecords.Node.R_FLAGS, (int) f.u32(p + "flags"));
        }
        same(name, b, f.bytes());
    }

    @Test
    void graphNode() {
        node("IND_GRAPH_NODE");
        node("IND_GRAPH_NODE_NONE");
    }

    private static void edge(String name) {
        Fixture f = fx(name);
        int n = f.count("n_filter");
        ByteBuffer b = rec(GlassRecords.IND_GRAPH_EDGE, GlassRecords.Edge.size(n), kindCode("edge_kind", f.sym("kind")));
        headOf(b, f);
        b.putInt(GlassRecords.Edge.FROM, (int) f.u32("from"));
        b.putInt(GlassRecords.Edge.TO, (int) f.u32("to"));
        b.putInt(GlassRecords.Edge.ORDER, (int) f.u32("order"));
        b.putInt(GlassRecords.Edge.FLAGS, (int) f.u32("flags"));
        if (!f.f().containsKey("capacity")) {
            b.putInt(GlassRecords.Edge.CAP_NUM, (int) f.u32("cap_num"));
            b.putInt(GlassRecords.Edge.CAP_DEN, (int) f.u32("cap_den"));
        }
        b.putInt(GlassRecords.Edge.LATENCY, (int) f.u32("latency"));
        b.putInt(GlassRecords.Edge.N_FILTER, n);
        b.putLong(GlassRecords.Edge.PARAM, f.i64("param"));
        for (int i = 0; i < n; i++)
            b.putInt(GlassRecords.Edge.FILTER + 4 * i, (int) f.u32("filter" + i));
        same(name, b, f.bytes());
    }

    @Test
    void graphEdge() {
        edge("IND_GRAPH_EDGE");
        edge("IND_GRAPH_EDGE_UNLIMITED");
    }

    @Test
    void graphRecipe() {
        Fixture f = fx("IND_GRAPH_RECIPE");
        int ni = f.count("n_inputs"), no = f.count("n_outputs");
        ByteBuffer b = rec(GlassRecords.IND_GRAPH_RECIPE, GlassRecords.Recipe.size(ni + no), ni + no);
        headOf(b, f);
        b.putInt(GlassRecords.Recipe.TYPE, (int) f.u32("recipe_type"));
        b.putInt(GlassRecords.Recipe.HEAT, (int) f.u32("heat"));
        b.putInt(GlassRecords.Recipe.N_INPUTS, ni);
        b.putInt(GlassRecords.Recipe.N_OUTPUTS, no);
        b.putInt(GlassRecords.Recipe.DURATION, (int) f.u32("duration"));
        b.putInt(GlassRecords.Recipe.NAME, (int) f.u32("name"));
        for (int i = 0; i < ni + no; i++) {
            int e = GlassRecords.Recipe.TERMS + i * GlassRecords.Recipe.TERM;
            String p = i < ni ? "input" + i + "." : "output" + (i - ni) + ".";
            b.putInt(e + GlassRecords.Recipe.T_GOOD, (int) f.u32(p + "good"));
            b.putInt(e + GlassRecords.Recipe.T_FLAGS, (int) f.u32(p + "flags"));
            b.putLong(e + GlassRecords.Recipe.T_NUM, f.i64(p + "num"));
            b.putLong(e + GlassRecords.Recipe.T_DEN, f.i64(p + "den"));
        }
        same("IND_GRAPH_RECIPE", b, f.bytes());
    }

    @Test
    void graphDomain() {
        Fixture f = fx("IND_GRAPH_DOMAIN");
        int nl = f.count("n_levels"), ns = f.count("n_sources"), nm = f.count("n_members");
        int nsl = 0;
        for (int i = 0; i < ns; i++)
            nsl += f.count("source" + i + ".n_levels");
        ByteBuffer b = rec(GlassRecords.IND_GRAPH_DOMAIN, GlassRecords.Domain.size(nl, ns, nsl, nm), nm);
        headOf(b, f);
        b.putLong(GlassRecords.Domain.NETWORK, f.u64("network"));
        b.putInt(GlassRecords.Domain.N_LEVELS, nl);
        b.putInt(GlassRecords.Domain.CURRENT, f.index("current"));
        b.putInt(GlassRecords.Domain.FLAGS, (int) f.u32("flags"));
        b.putDouble(GlassRecords.Domain.LIVE_STRESS, f.f64("live_stress"));
        b.putDouble(GlassRecords.Domain.LIVE_CAPACITY, f.f64("live_capacity"));
        b.putInt(GlassRecords.Domain.N_SOURCES, ns);
        b.putInt(GlassRecords.Domain.N_SOURCE_LEVELS, nsl);
        b.putInt(GlassRecords.Domain.N_FOREIGN, (int) f.u32("foreign"));
        for (int i = 0; i < nl; i++)
            level(b, GlassRecords.Domain.LEVELS + i * GlassRecords.Domain.LEVEL, f, "level" + i + ".");
        int first = 0;
        for (int i = 0; i < ns; i++) {
            int e = GlassRecords.Domain.sources(nl) + i * GlassRecords.Domain.SOURCE;
            String p = "source" + i + ".";
            int rows = f.count(p + "n_levels");
            b.putInt(e + GlassRecords.Domain.S_DRIVER, f.index(p + "driver"));
            b.putInt(e + GlassRecords.Domain.S_FIRST, first);
            b.putInt(e + GlassRecords.Domain.S_ROWS, rows);
            b.putInt(e + GlassRecords.Domain.S_CURRENT, f.index(p + "current"));
            b.putInt(e + GlassRecords.Domain.S_RATIO_NUM, f.i32(p + "ratio_num"));
            b.putInt(e + GlassRecords.Domain.S_RATIO_DEN, (int) f.u32(p + "ratio_den"));
            b.putLong(e + GlassRecords.Domain.S_POS, f.i64(p + "pos"));
            for (int j = 0; j < rows; j++)
                level(b, GlassRecords.Domain.sourceLevels(nl, ns) + (first++) * GlassRecords.Domain.LEVEL, f, p + "level" + j + ".");
        }
        for (int i = 0; i < nm; i++) {
            int e = GlassRecords.Domain.members(nl, ns, nsl) + i * GlassRecords.Domain.MEMBER;
            String p = "member" + i + ".";
            b.putInt(e + GlassRecords.Domain.M_NODE, f.sym(p + "node").equals("none") ? GlassRecords.NONE : (int) f.u32(p + "node"));
            b.putInt(e + GlassRecords.Domain.M_COUNT, (int) f.u32(p + "count"));
            b.putInt(e + GlassRecords.Domain.M_RATIO_NUM, f.i32(p + "ratio_num"));
            b.putInt(e + GlassRecords.Domain.M_RATIO_DEN, (int) f.u32(p + "ratio_den"));
            b.putDouble(e + GlassRecords.Domain.M_IMPACT, f.f64(p + "impact"));
        }
        same("IND_GRAPH_DOMAIN", b, f.bytes());
    }

    /** A Level24 (a speed row or a source row) from the fixture's {@code prefix}selector, speed_num, speed_den, capacity. */
    private static void level(ByteBuffer b, int at, Fixture f, String prefix) {
        b.putInt(at + GlassRecords.Domain.L_SELECTOR, (int) f.u32(prefix + "selector"));
        b.putInt(at + GlassRecords.Domain.L_SPEED_NUM, f.i32(prefix + "speed_num"));
        b.putInt(at + GlassRecords.Domain.L_SPEED_DEN, (int) f.u32(prefix + "speed_den"));
        b.putDouble(at + GlassRecords.Domain.L_CAPACITY, f.f64(prefix + "capacity"));
    }

    @Test
    void graphEnd() {
        Fixture f = fx("IND_GRAPH_END");
        ByteBuffer b = rec(GlassRecords.IND_GRAPH_END, GlassRecords.End.SIZE, (int) f.u32("flags"));
        b.putLong(GlassRecords.GAME_TIME, f.i64("game_time"));
        b.putLong(GlassRecords.DISTRICT, f.u64("district"));
        b.putInt(GlassRecords.COMPILE, (int) f.u32("compile"));
        b.putInt(GlassRecords.End.RECORDS, (int) f.u32("records"));
        b.putInt(GlassRecords.End.NODES, (int) f.u32("nodes"));
        b.putInt(GlassRecords.End.EDGES, (int) f.u32("edges"));
        b.putInt(GlassRecords.End.RECIPES, (int) f.u32("recipes"));
        b.putInt(GlassRecords.End.DOMAINS, (int) f.u32("domains"));
        b.putInt(GlassRecords.End.REASON, (int) f.u32("reason"));
        b.putLong(GlassRecords.End.FINGERPRINT, f.u64("fingerprint"));
        b.putLong(GlassRecords.End.FRAME, f.u64("frame"));
        b.putInt(GlassRecords.End.REVISION, (int) f.u32("revision"));
        b.putInt(GlassRecords.End.COMPILE_TICKS, (int) f.u32("compile_ticks"));
        b.putInt(GlassRecords.End.WINDOW, (int) f.u32("window"));
        same("IND_GRAPH_END", b, f.bytes());
    }

    @Test
    void probeSample() {
        Fixture f = fx("IND_PROBE_SAMPLE");
        int n = f.count("n_entries");
        ByteBuffer b = rec(GlassRecords.IND_PROBE_SAMPLE, GlassRecords.ProbeSample.size(n), n);
        b.putLong(GlassRecords.GAME_TIME, f.i64("game_time"));
        b.putLong(GlassRecords.DISTRICT, f.u64("district"));
        b.putInt(GlassRecords.ProbeSample.SWEEP, (int) f.u32("sweep"));
        b.putInt(GlassRecords.ProbeSample.NODE, (int) f.u32("node"));
        b.putInt(GlassRecords.ProbeSample.COMPILE, (int) f.u32("compile"));
        for (int i = 0; i < n; i++) {
            int e = GlassRecords.ProbeSample.ENTRIES + 16 * i;
            b.putInt(e, (int) f.u32("entry" + i + ".good"));
            b.putLong(e + 8, f.i64("entry" + i + ".amount"));
        }
        same("IND_PROBE_SAMPLE", b, f.bytes());
    }

    @Test
    void countSample() {
        Fixture f = fx("IND_COUNT_SAMPLE");
        int n = f.count("n_entries");
        ByteBuffer b = rec(GlassRecords.IND_COUNT_SAMPLE, GlassRecords.CountSample.size(n), n);
        b.putLong(GlassRecords.GAME_TIME, f.i64("game_time"));
        b.putLong(GlassRecords.DISTRICT, f.u64("district"));
        b.putInt(GlassRecords.CountSample.SWEEP, (int) f.u32("sweep"));
        b.putInt(GlassRecords.CountSample.COMPILE, (int) f.u32("compile"));
        for (int i = 0; i < n; i++) {
            int e = GlassRecords.CountSample.ENTRIES + GlassRecords.CountSample.ENTRY * i;
            b.putInt(e + GlassRecords.CountSample.E_MOVER, (int) f.u32("entry" + i + ".mover"));
            b.putInt(e + GlassRecords.CountSample.E_STORE, (int) f.u32("entry" + i + ".store"));
            // The second entry has counted no good yet: NONE on the wire.
            String good = f.sym("entry" + i + ".good");
            b.putInt(e + GlassRecords.CountSample.E_GOOD, good.equals("none") ? GlassRecords.NONE : (int) Long.parseLong(good));
            b.putInt(e + GlassRecords.CountSample.E_ENTRIES, (int) f.u32("entry" + i + ".entries"));
            b.putLong(e + GlassRecords.CountSample.E_SINCE, f.i64("entry" + i + ".since"));
            b.putLong(e + GlassRecords.CountSample.E_IN, f.i64("entry" + i + ".in"));
            b.putLong(e + GlassRecords.CountSample.E_OUT, f.i64("entry" + i + ".out"));
        }
        assertEquals("none", f.sym("entry1.good"), "the fixture's second pair counted no good");
        same("IND_COUNT_SAMPLE", b, f.bytes());
    }

    @Test
    void chunkTicking() {
        Fixture f = fx("IND_CHUNK_TICKING");
        ByteBuffer b = rec(GlassRecords.IND_CHUNK_TICKING, GlassRecords.ChunkTicking.SIZE, Boolean.parseBoolean(f.sym("ticking")) ? 1 : 0);
        // The bridge's journaled prefix: u64 obsSeq at 8, i64 gameTime at 16.
        b.putLong(8, f.u64("obs_seq"));
        b.putLong(16, f.i64("game_time"));
        b.putLong(GlassRecords.ChunkTicking.DISTRICT, f.u64("district"));
        b.putInt(GlassRecords.ChunkTicking.DIM, (int) f.u32("dim"));
        b.putInt(GlassRecords.ChunkTicking.CHUNK_X, f.i32("chunk_x"));
        b.putInt(GlassRecords.ChunkTicking.CHUNK_Z, f.i32("chunk_z"));
        same("IND_CHUNK_TICKING", b, f.bytes());
    }

    @Test
    void probeSetIsReadAtTheJavaOffsets() {
        Fixture f = fx("IND_PROBE_SET");
        ByteBuffer b = ByteBuffer.wrap(f.bytes()).order(ByteOrder.LITTLE_ENDIAN);
        int n = f.count("n_nodes");
        assertEquals(GlassRecords.IND_PROBE_SET, b.getShort(0) & 0xFFFF, "kind");
        assertEquals(GlassRecords.ProbeSet.size(n), b.getShort(2) & 0xFFFF, "size");
        assertEquals(n, b.getInt(4), "aux");
        assertEquals(f.u64("cmd_seq"), b.getLong(8), "cmdSeq");
        assertEquals(f.u64("district"), b.getLong(GlassRecords.ProbeSet.DISTRICT), "district");
        assertEquals(f.u32("compile"), Integer.toUnsignedLong(b.getInt(GlassRecords.ProbeSet.COMPILE)), "compile");
        for (int i = 0; i < n; i++)
            assertEquals(f.u32("node" + i), Integer.toUnsignedLong(b.getInt(GlassRecords.ProbeSet.NODES + 4 * i)), "node" + i);
        // Rebuilt from the values, the record is the same bytes (reserved and padding zero).
        ByteBuffer built = rec(GlassRecords.IND_PROBE_SET, GlassRecords.ProbeSet.size(n), n);
        built.putLong(8, f.u64("cmd_seq"));
        built.putLong(GlassRecords.ProbeSet.DISTRICT, f.u64("district"));
        built.putInt(GlassRecords.ProbeSet.COMPILE, (int) f.u32("compile"));
        for (int i = 0; i < n; i++)
            built.putInt(GlassRecords.ProbeSet.NODES + 4 * i, (int) f.u32("node" + i));
        same("IND_PROBE_SET", built, f.bytes());
    }

    @Test
    void countSetIsReadAtTheJavaOffsets() {
        Fixture f = fx("IND_COUNT_SET");
        ByteBuffer b = ByteBuffer.wrap(f.bytes()).order(ByteOrder.LITTLE_ENDIAN);
        int n = f.count("n_pairs");
        assertEquals(GlassRecords.IND_COUNT_SET, b.getShort(0) & 0xFFFF, "kind");
        assertEquals(GlassRecords.CountSet.size(n), b.getShort(2) & 0xFFFF, "size");
        assertEquals(n, b.getInt(4), "aux");
        assertEquals(f.u64("cmd_seq"), b.getLong(8), "cmdSeq");
        assertEquals(f.u64("district"), b.getLong(GlassRecords.CountSet.DISTRICT), "district");
        assertEquals(f.u32("compile"), Integer.toUnsignedLong(b.getInt(GlassRecords.CountSet.COMPILE)), "compile");
        for (int i = 0; i < n; i++) {
            int p = GlassRecords.CountSet.PAIRS + GlassRecords.CountSet.PAIR * i;
            assertEquals(f.u32("pair" + i + ".mover"), Integer.toUnsignedLong(b.getInt(p + GlassRecords.CountSet.P_MOVER)), "pair" + i + ".mover");
            assertEquals(f.u32("pair" + i + ".store"), Integer.toUnsignedLong(b.getInt(p + GlassRecords.CountSet.P_STORE)), "pair" + i + ".store");
        }
        ByteBuffer built = rec(GlassRecords.IND_COUNT_SET, GlassRecords.CountSet.size(n), n);
        built.putLong(8, f.u64("cmd_seq"));
        built.putLong(GlassRecords.CountSet.DISTRICT, f.u64("district"));
        built.putInt(GlassRecords.CountSet.COMPILE, (int) f.u32("compile"));
        for (int i = 0; i < n; i++) {
            int p = GlassRecords.CountSet.PAIRS + GlassRecords.CountSet.PAIR * i;
            built.putInt(p + GlassRecords.CountSet.P_MOVER, (int) f.u32("pair" + i + ".mover"));
            built.putInt(p + GlassRecords.CountSet.P_STORE, (int) f.u32("pair" + i + ".store"));
        }
        same("IND_COUNT_SET", built, f.bytes());
    }
}
