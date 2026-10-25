package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import net.minecraft.core.BlockPos;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.TreeMap;

/**
 * Decodes the glass box's graph records the engine received (docs/industry-boundary.md §9.2) back into a
 * graph, for GameTests: the latest compile of a district whose END arrived, with every NODE, EDGE, RECIPE
 * and DOMAIN record of that compile number. A compile is complete only when the records received match
 * the END's count (the engine's own rule, F5). Works on either engine: both keep a received log.
 */
public final class GlassKit {
    private GlassKit() {}

    public record Rate(int level, int recipe, int batch, long ticksNum, long ticksDen, int flags) {
        public double perTick() {
            return ticksNum == 0 ? 0 : batch * (double) ticksDen / ticksNum;
        }
    }

    public record Node(int index, int kind, int model, int domain, int inventory, int flags, int x, int y, int z, int rule,
                       long capacity, long[] params, List<Rate> rates) {
        public boolean has(int flag) {
            return (flags & flag) != 0;
        }
    }

    public record Edge(int index, int kind, int from, int to, int order, int flags, long capNum, long capDen, int latency, long param,
                       int[] filter) {}

    public record Term(int good, int flags, long num, long den) {}

    public record Recipe(int index, int type, int heat, int duration, int name, List<Term> inputs, List<Term> outputs) {}

    public record Level(int selector, int speedNum, long speedDen, double capacity) {
        public double rpm() {
            return speedNum / (double) speedDen;
        }
    }

    public record Member(int node, int count, int ratioNum, long ratioDen, double impact) {}

    /** A source of a domain: its driver node (NONE), its rows, its row now (NONE), its ratio to the reference (den 0: unknown). */
    public record Source(long pos, int driver, int current, int ratioNum, long ratioDen, List<Level> rows) {
        public boolean ratioKnown() {
            return ratioDen != 0;
        }
    }

    /**
     * A domain: {@code levels} are its speed rows (the reference's speed), {@code current} the one now (NONE: none).
     * {@code members} are the district's own, then the tail ({@code foreign} of them, in no district's chunks); a
     * shared part's {@code peers} are the other districts on its network, ascending.
     */
    public record Domain(int index, long network, int current, int flags, double liveStress, double liveCapacity,
                         List<Level> levels, List<Source> sources, List<Member> members, int foreign, List<Long> peers) {
        /** The sources that have a driver (a boiler node). */
        public List<Source> driven() {
            return sources.stream().filter(s -> s.driver() != GlassRecords.NONE).toList();
        }

        /** A part of a network that crosses the district's border (the SHARED flag). */
        public boolean shared() {
            return (flags & GlassRecords.Domain.SHARED) != 0;
        }

        /** The members in the district's own chunks (the tail left out). */
        public List<Member> ownMembers() {
            return members.subList(0, members.size() - foreign);
        }

        /** The members in no district's chunks. */
        public List<Member> tail() {
            return members.subList(members.size() - foreign, members.size());
        }

        /** The source at {@code pos}, or null. */
        public Source source(long pos) {
            for (Source s : sources)
                if (s.pos() == pos)
                    return s;
            return null;
        }

        public String describe() {
            StringBuilder b = new StringBuilder("domain ").append(index).append(shared() ? " SHARED" : "").append(" flags ").append(flags)
                    .append(" current ").append(current == GlassRecords.NONE ? "none" : current).append(", ").append(levels.size())
                    .append(" speed rows, ").append(members.size() - foreign).append(" members + ").append(foreign).append(" tail, peers ")
                    .append(peers).append(", sources");
            for (Source s : sources)
                b.append(" [").append(BlockPos.of(s.pos()).toShortString()).append(" rows ").append(s.rows().size()).append(" driver ")
                        .append(s.driver() == GlassRecords.NONE ? "none" : s.driver()).append(" ratio ")
                        .append(s.ratioKnown() ? s.ratioNum() + "/" + s.ratioDen() : "unknown").append(s.rows().isEmpty() ? " foreign" : "").append("]");
            return b.toString();
        }

        @Override
        public String toString() {
            return describe();
        }
    }

    public record End(int compile, int flags, int window, int records, int nodes, int edges, int recipes, int domains, int reason,
                      long fingerprint, long frame, int revision, int compileTicks) {}

    /** One compile as received. */
    public record Graph(long district, End end, List<Node> nodes, List<Edge> edges, List<Recipe> recipes, List<Domain> domains,
                        int received) {
        /** The END says COMPLETE and exactly its count of records arrived, each index once. */
        public boolean complete() {
            return (end.flags & GlassRecords.End.COMPLETE) != 0 && arrived();
        }

        /**
         * The END is a compile's, COMPLETE or BLACK_BOX (a graph with residual groups, which is never COMPLETE),
         * and exactly its count of records arrived, each index once.
         */
        public boolean whole() {
            return (end.flags & (GlassRecords.End.COMPLETE | GlassRecords.End.BLACK_BOX)) != 0 && arrived();
        }

        private boolean arrived() {
            return received == end.records && nodes.size() == end.nodes && edges.size() == end.edges
                    && recipes.size() == end.recipes && domains.size() == end.domains;
        }

        public List<Node> nodes(int kind, int model) {
            List<Node> out = new ArrayList<>();
            for (Node n : nodes)
                if ((kind < 0 || n.kind == kind) && (model < 0 || n.model == model))
                    out.add(n);
            return out;
        }

        public List<Edge> edgesFrom(int node) {
            List<Edge> out = new ArrayList<>();
            for (Edge e : edges)
                if (e.from == node)
                    out.add(e);
            return out;
        }

        public List<Edge> edgesTo(int node) {
            List<Edge> out = new ArrayList<>();
            for (Edge e : edges)
                if (e.to == node)
                    out.add(e);
            return out;
        }

        /** A recipe by its NAME dictionary text (Java's dictionary), or null. */
        public Recipe recipeNamed(String name) {
            for (Recipe r : recipes)
                if (name.equals(IndustryKit.javaName(r.name)))
                    return r;
            return null;
        }

        public String describe() {
            return "compile " + end.compile + " flags " + end.flags + ": " + nodes.size() + "/" + end.nodes + " nodes, " + edges.size() + "/"
                    + end.edges + " edges, " + recipes.size() + "/" + end.recipes + " recipes, " + domains.size() + "/" + end.domains
                    + " domains, " + received + "/" + end.records + " records, frame " + end.frame + ", " + end.compileTicks + " ticks";
        }
    }

    /** The latest compile of {@code district} whose END the engine received, or null. */
    public static Graph latest(long district) {
        return latest(district, IndustryKit.engine().received());
    }

    /** As {@link #latest(long)} over the records a {@link GlassExitKit.Recorder}-style log kept. */
    public static Graph latest(long district, List<ScriptedEngine.Received> log) {
        ByteBuffer endRec = null;
        int endFlags = 0;
        for (ScriptedEngine.Received r : log) {
            if (r.skipped() || r.kind() != GlassRecords.IND_GRAPH_END)
                continue;
            ByteBuffer v = r.view();
            if (v.getLong(GlassRecords.DISTRICT) != district)
                continue;
            endRec = v;
            endFlags = r.aux();
        }
        if (endRec == null)
            return null;
        End end = new End(endRec.getInt(GlassRecords.COMPILE), endFlags, endRec.getInt(GlassRecords.End.WINDOW), endRec.getInt(GlassRecords.End.RECORDS),
                endRec.getInt(GlassRecords.End.NODES), endRec.getInt(GlassRecords.End.EDGES), endRec.getInt(GlassRecords.End.RECIPES),
                endRec.getInt(GlassRecords.End.DOMAINS), endRec.getInt(GlassRecords.End.REASON), endRec.getLong(GlassRecords.End.FINGERPRINT),
                endRec.getLong(GlassRecords.End.FRAME), endRec.getInt(GlassRecords.End.REVISION), endRec.getInt(GlassRecords.End.COMPILE_TICKS));
        TreeMap<Integer, Node> nodes = new TreeMap<>();
        TreeMap<Integer, Edge> edges = new TreeMap<>();
        TreeMap<Integer, Recipe> recipes = new TreeMap<>();
        TreeMap<Integer, Domain> domains = new TreeMap<>();
        for (ScriptedEngine.Received r : log) {
            if (r.skipped())
                continue;
            int k = r.kind();
            if (k != GlassRecords.IND_GRAPH_NODE && k != GlassRecords.IND_GRAPH_EDGE && k != GlassRecords.IND_GRAPH_RECIPE && k != GlassRecords.IND_GRAPH_DOMAIN)
                continue;
            ByteBuffer v = r.view();
            if (v.getLong(GlassRecords.DISTRICT) != district || v.getInt(GlassRecords.COMPILE) != end.compile)
                continue;
            int idx = v.getInt(GlassRecords.INDEX);
            switch (k) {
                case GlassRecords.IND_GRAPH_NODE -> nodes.put(idx, node(v, r.aux()));
                case GlassRecords.IND_GRAPH_EDGE -> edges.put(idx, edge(v, r.aux()));
                case GlassRecords.IND_GRAPH_RECIPE -> recipes.put(idx, recipe(v));
                default -> domains.put(idx, domain(v, r.aux()));
            }
        }
        int received = nodes.size() + edges.size() + recipes.size() + domains.size();
        return new Graph(district, end, new ArrayList<>(nodes.values()), new ArrayList<>(edges.values()), new ArrayList<>(recipes.values()),
                new ArrayList<>(domains.values()), received);
    }

    private static long u32(int v) {
        return v & 0xFFFF_FFFFL;
    }

    private static Node node(ByteBuffer v, int kind) {
        int n = v.getInt(GlassRecords.Node.N_RATES);
        List<Rate> rates = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            int at = GlassRecords.Node.RATES + i * GlassRecords.Node.RATE;
            rates.add(new Rate(v.getInt(at + GlassRecords.Node.R_LEVEL), v.getInt(at + GlassRecords.Node.R_RECIPE), v.getInt(at + GlassRecords.Node.R_BATCH),
                    u32(v.getInt(at + GlassRecords.Node.R_TICKS_NUM)), u32(v.getInt(at + GlassRecords.Node.R_TICKS_DEN)), v.getInt(at + GlassRecords.Node.R_FLAGS)));
        }
        long[] params = {v.getLong(GlassRecords.Node.PARAM0), v.getLong(GlassRecords.Node.PARAM1), v.getLong(GlassRecords.Node.PARAM2),
                v.getLong(GlassRecords.Node.PARAM3)};
        return new Node(v.getInt(GlassRecords.INDEX), kind, v.getInt(GlassRecords.Node.MODEL), v.getInt(GlassRecords.Node.DOMAIN),
                v.getInt(GlassRecords.Node.INVENTORY), v.getInt(GlassRecords.Node.FLAGS), v.getInt(GlassRecords.Node.X), v.getInt(GlassRecords.Node.Y),
                v.getInt(GlassRecords.Node.Z), v.getInt(GlassRecords.Node.RULE), v.getLong(GlassRecords.Node.CAPACITY), params, rates);
    }

    private static Edge edge(ByteBuffer v, int kind) {
        int n = v.getInt(GlassRecords.Edge.N_FILTER);
        int[] filter = new int[n];
        for (int i = 0; i < n; i++)
            filter[i] = v.getInt(GlassRecords.Edge.FILTER + 4 * i);
        return new Edge(v.getInt(GlassRecords.INDEX), kind, v.getInt(GlassRecords.Edge.FROM), v.getInt(GlassRecords.Edge.TO), v.getInt(GlassRecords.Edge.ORDER),
                v.getInt(GlassRecords.Edge.FLAGS), u32(v.getInt(GlassRecords.Edge.CAP_NUM)), u32(v.getInt(GlassRecords.Edge.CAP_DEN)),
                v.getInt(GlassRecords.Edge.LATENCY), v.getLong(GlassRecords.Edge.PARAM), filter);
    }

    private static Recipe recipe(ByteBuffer v) {
        int ni = v.getInt(GlassRecords.Recipe.N_INPUTS), no = v.getInt(GlassRecords.Recipe.N_OUTPUTS);
        List<Term> in = new ArrayList<>(ni), out = new ArrayList<>(no);
        for (int i = 0; i < ni + no; i++) {
            int at = GlassRecords.Recipe.TERMS + i * GlassRecords.Recipe.TERM;
            Term t = new Term(v.getInt(at + GlassRecords.Recipe.T_GOOD), v.getInt(at + GlassRecords.Recipe.T_FLAGS), v.getLong(at + GlassRecords.Recipe.T_NUM),
                    v.getLong(at + GlassRecords.Recipe.T_DEN));
            (i < ni ? in : out).add(t);
        }
        return new Recipe(v.getInt(GlassRecords.INDEX), v.getInt(GlassRecords.Recipe.TYPE), v.getInt(GlassRecords.Recipe.HEAT),
                v.getInt(GlassRecords.Recipe.DURATION), v.getInt(GlassRecords.Recipe.NAME), in, out);
    }

    /** A DOMAIN record; aux is its member count. */
    private static Domain domain(ByteBuffer v, int nm) {
        int nl = v.getInt(GlassRecords.Domain.N_LEVELS);
        int ns = v.getInt(GlassRecords.Domain.N_SOURCES);
        int nsl = v.getInt(GlassRecords.Domain.N_SOURCE_LEVELS);
        List<Level> levels = new ArrayList<>(nl);
        for (int i = 0; i < nl; i++)
            levels.add(level(v, GlassRecords.Domain.LEVELS + i * GlassRecords.Domain.LEVEL));
        List<Level> rows = new ArrayList<>(nsl);
        int r0 = GlassRecords.Domain.sourceLevels(nl, ns);
        for (int i = 0; i < nsl; i++)
            rows.add(level(v, r0 + i * GlassRecords.Domain.LEVEL));
        List<Source> sources = new ArrayList<>(ns);
        int s0 = GlassRecords.Domain.sources(nl);
        for (int i = 0; i < ns; i++) {
            int at = s0 + i * GlassRecords.Domain.SOURCE;
            int first = v.getInt(at + GlassRecords.Domain.S_FIRST), count = v.getInt(at + GlassRecords.Domain.S_ROWS);
            sources.add(new Source(v.getLong(at + GlassRecords.Domain.S_POS), v.getInt(at + GlassRecords.Domain.S_DRIVER),
                    v.getInt(at + GlassRecords.Domain.S_CURRENT), v.getInt(at + GlassRecords.Domain.S_RATIO_NUM),
                    u32(v.getInt(at + GlassRecords.Domain.S_RATIO_DEN)), List.copyOf(rows.subList(first, first + count))));
        }
        List<Member> members = new ArrayList<>(nm);
        int m0 = GlassRecords.Domain.members(nl, ns, nsl);
        for (int i = 0; i < nm; i++) {
            int at = m0 + i * GlassRecords.Domain.MEMBER;
            members.add(new Member(v.getInt(at + GlassRecords.Domain.M_NODE), v.getInt(at + GlassRecords.Domain.M_COUNT),
                    v.getInt(at + GlassRecords.Domain.M_RATIO_NUM), u32(v.getInt(at + GlassRecords.Domain.M_RATIO_DEN)),
                    v.getDouble(at + GlassRecords.Domain.M_IMPACT)));
        }
        int np = v.getInt(GlassRecords.Domain.N_PEERS);
        int p0 = GlassRecords.Domain.peers(nl, ns, nsl, nm);
        List<Long> peers = new ArrayList<>(np);
        for (int i = 0; i < np; i++)
            peers.add(v.getLong(p0 + i * GlassRecords.Domain.PEER));
        return new Domain(v.getInt(GlassRecords.INDEX), v.getLong(GlassRecords.Domain.NETWORK), v.getInt(GlassRecords.Domain.CURRENT),
                v.getInt(GlassRecords.Domain.FLAGS), v.getDouble(GlassRecords.Domain.LIVE_STRESS),
                v.getDouble(GlassRecords.Domain.LIVE_CAPACITY), levels, sources, members, v.getInt(GlassRecords.Domain.N_FOREIGN),
                List.copyOf(peers));
    }

    private static Level level(ByteBuffer v, int at) {
        return new Level(v.getInt(at + GlassRecords.Domain.L_SELECTOR), v.getInt(at + GlassRecords.Domain.L_SPEED_NUM),
                u32(v.getInt(at + GlassRecords.Domain.L_SPEED_DEN)), v.getDouble(at + GlassRecords.Domain.L_CAPACITY));
    }
}
