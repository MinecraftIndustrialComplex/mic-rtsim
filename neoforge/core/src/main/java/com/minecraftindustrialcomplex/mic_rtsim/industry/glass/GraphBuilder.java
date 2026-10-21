package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.fluids.FluidStack;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * The plugin-facing graph of one district's compile (GLASSBOX.md §1, docs/industry-boundary.md §9.2 and
 * §9.3). Plugins add nodes, edges, recipes and kinetic domains; the builder hands out the indices the
 * records carry, dedupes recipes, validates every reference and record size, and encodes the records.
 *
 * <p><b>Stores</b> are the core's: every boundary inventory of the district's current revision is a STORE
 * node with its IND_INVENTORY index, made the first time a plugin asks for it ({@link #storeAt}) and at
 * the end for the rest, unless a plugin claimed the block as something else ({@link #claimStore}: a fluid
 * tank that is a boiler). <b>The residual</b> (G-B13) is one BLACK_BOX node for every block entity with a
 * capability that no extractor claimed; {@link #residualAt} joins a plugin's edge to it.
 *
 * <p>Units are the graph's (§9.3): items in items, fluids in millibuckets, rates per tick.
 */
public final class GraphBuilder {

    public static final int NONE = GlassRecords.NONE;
    /** The largest record the bridge carries (docs/boundary.md: sizes at most 65,528). */
    public static final int MAX_RECORD = 65_528;

    public final Industry ind;
    public final ServerLevel level;
    public final District district;
    /** The compile's frame (GB6): 0 the level, else a Sable sub-level id. Set by an extractor. */
    public long frame;

    private final List<Node> nodes = new ArrayList<>();
    private final List<Edge> edges = new ArrayList<>();
    private final List<Recipe> recipes = new ArrayList<>();
    private final Map<Recipe, Integer> recipeIndex = new HashMap<>();
    private final List<Domain> domains = new ArrayList<>();
    private final Long2IntOpenHashMap domainByNetwork = new Long2IntOpenHashMap();
    private final Long2IntOpenHashMap stores = new Long2IntOpenHashMap();
    private final LongOpenHashSet claimedStores = new LongOpenHashSet();
    private final LongOpenHashSet residualPositions = new LongOpenHashSet();
    private final Map<Long, District.Inventory> inventoryByPos = new HashMap<>();
    private int residual = NONE;
    /** Why the district is not compilable (a REASON code), or 0. */
    int notCompilable;
    String notCompilableWhy;

    GraphBuilder(Industry ind, ServerLevel level, District district) {
        this.ind = ind;
        this.level = level;
        this.district = district;
        domainByNetwork.defaultReturnValue(-1);
        stores.defaultReturnValue(-1);
        for (District.Inventory inv : district.inventories)
            inventoryByPos.put(inv.pos().asLong(), inv);
    }

    // ---- goods ----------------------------------------------------------------------------------

    /** A GOOD dictionary id (docs/industry-boundary.md §2.2). */
    public int good(String name) {
        return ind.goodId(name);
    }

    public int good(ItemStack stack) {
        return ind.goodId(ind.goods().name(stack));
    }

    public int good(FluidStack stack) {
        return ind.goodId(ind.goods().name(stack));
    }

    public String goodName(ItemStack stack) {
        return ind.goods().name(stack);
    }

    public String goodName(FluidStack stack) {
        return ind.goods().name(stack);
    }

    // ---- nodes ----------------------------------------------------------------------------------

    /** One graph node; fields default to NONE or 0. Setters return the node. */
    public static final class Node {
        public final int index;
        public final int kind;
        public int model;
        public int domain = NONE;
        public int inventory = NONE;
        public int flags;
        public final int x, y, z;
        public int rule;
        public long capacity;
        public final long[] params = new long[4];
        final List<int[]> rates = new ArrayList<>(); // level, recipe, batch, ticksNum, ticksDen, flags

        Node(int index, int kind, int model, BlockPos pos) {
            this.index = index;
            this.kind = kind;
            this.model = model;
            this.x = pos.getX();
            this.y = pos.getY();
            this.z = pos.getZ();
        }

        public Node domain(int d) {
            domain = d;
            return this;
        }

        public Node flag(int f) {
            flags |= f;
            return this;
        }

        public Node rule(int r) {
            rule = r;
            return this;
        }

        public Node capacity(long c) {
            capacity = c;
            return this;
        }

        public Node params(long p0, long p1, long p2, long p3) {
            params[0] = p0;
            params[1] = p1;
            params[2] = p2;
            params[3] = p3;
            return this;
        }

        /**
         * A rate entry (Rate24): at speed level {@code level}, one operation on {@code batch} crafts or units
         * every {@code ticks} ticks. {@code recipe} NONE: the node's transfer or its only recipe.
         */
        public Node rate(int level, int recipe, int batch, Rat ticks, int flags) {
            if (ticks.num() <= 0 || batch <= 0)
                return this;
            Rat t = ticks.fitPeriod();
            rates.add(new int[] {level, recipe, batch, (int) t.num(), (int) t.den(), flags});
            return this;
        }

        public Node rate(int level, int recipe, int batch, long ticks, int flags) {
            return rate(level, recipe, batch, Rat.of(ticks, 1), flags);
        }

        /**
         * A zero-capacity entry at {@code level}: a stopped machine or a paused mover whose other levels
         * have no entry either is capacity 0, not unlimited (a node with no entries at all is unlimited).
         */
        public Node stopped(int level) {
            rates.add(new int[] {level, NONE, 0, 1, 1, 0});
            return this;
        }

        public int rateCount() {
            return rates.size();
        }

        public BlockPos pos() {
            return new BlockPos(x, y, z);
        }
    }

    public Node node(int kind, int model, BlockPos pos) {
        Node n = new Node(nodes.size(), kind, model, pos);
        nodes.add(n);
        return n;
    }

    public Node node(int index) {
        return nodes.get(index);
    }

    public int nodeCount() {
        return nodes.size();
    }

    public List<Node> nodes() {
        return nodes;
    }

    // ---- stores and the residual ----------------------------------------------------------------

    /** The district inventory at {@code pos} (canonicalized: a vault's controller, a double chest's half), or null. */
    public District.Inventory inventoryAt(BlockPos pos) {
        District.Inventory inv = inventoryByPos.get(pos.asLong());
        if (inv != null)
            return inv;
        BlockPos c = Districts.canonicalInventory(level, pos);
        return c == null ? null : inventoryByPos.get(c.asLong());
    }

    /** A plugin models this boundary inventory as something else (a boiler): no STORE node is made for it. */
    public void claimStore(BlockPos pos) {
        District.Inventory inv = inventoryAt(pos);
        if (inv != null)
            claimedStores.add(inv.pos().asLong());
    }

    public boolean storeClaimed(BlockPos pos) {
        District.Inventory inv = inventoryAt(pos);
        return inv != null && claimedStores.contains(inv.pos().asLong());
    }

    /** The STORE node of the boundary inventory at {@code pos}, made on first use; NONE when there is none or it is claimed. */
    public int storeAt(BlockPos pos) {
        District.Inventory inv = inventoryAt(pos);
        if (inv == null)
            return NONE;
        long key = inv.pos().asLong();
        if (claimedStores.contains(key))
            return NONE;
        int i = stores.get(key);
        if (i >= 0)
            return i;
        boolean fluid = inv.itemSlots() == 0 && inv.fluidCapacity() > 0;
        boolean energy = inv.itemSlots() == 0 && inv.fluidCapacity() == 0 && inv.energyCapacity() > 0;
        Node n = node(GlassRecords.Node.STORE,
                fluid ? GlassRecords.Model.STORE_FLUID : energy ? GlassRecords.Model.STORE_ENERGY : GlassRecords.Model.STORE_ITEMS, inv.pos());
        n.inventory = inv.index();
        n.capacity = fluid ? inv.fluidCapacity() : energy ? inv.energyCapacity() : inv.itemSlots();
        if (fluid)
            n.flags |= GlassRecords.Node.FLUID;
        stores.put(key, n.index);
        return n.index;
    }

    /** True for a block entity position the residual took (a capability no extractor claimed). */
    public boolean isResidual(BlockPos pos) {
        return residualPositions.contains(pos.asLong());
    }

    /** The residual BLACK_BOX node if {@code pos} is one of its blocks, else NONE. */
    public int residualAt(BlockPos pos) {
        if (!residualPositions.contains(pos.asLong()))
            return NONE;
        return residual();
    }

    /** The district's one residual node (G-B13), made on first use. */
    public int residual() {
        if (residual == NONE) {
            Node n = node(GlassRecords.Node.BLACK_BOX, GlassRecords.Model.BLACK_BOX, district.watch.isEmpty() ? BlockPos.ZERO : district.watch.get(0));
            n.flags |= GlassRecords.Node.BOUNDED;
            residual = n.index;
        }
        return residual;
    }

    void addResidualPosition(BlockPos pos) {
        residualPositions.add(pos.asLong());
    }

    int residualPositions() {
        return residualPositions.size();
    }

    boolean hasResidual() {
        return residual != NONE;
    }

    /** Every boundary inventory gets its STORE node (unless claimed), referenced or not. */
    void completeStores() {
        for (District.Inventory inv : district.inventories)
            if (!claimedStores.contains(inv.pos().asLong()))
                storeAt(inv.pos());
    }

    // ---- edges ----------------------------------------------------------------------------------

    public static final class Edge {
        public final int index;
        public final int kind;
        public final int from, to;
        public int order;
        public int flags;
        /** Units per tick as num/den; 0/0 unlimited. */
        public long capNum, capDen;
        public int latency;
        public long param;
        public final IntArrayList filter = new IntArrayList();

        Edge(int index, int kind, int from, int to) {
            this.index = index;
            this.kind = kind;
            this.from = from;
            this.to = to;
        }

        public Edge order(int o) {
            order = o;
            return this;
        }

        public Edge flag(int f) {
            flags |= f;
            return this;
        }

        public Edge capacity(Rat perTick) {
            Rat r = perTick;
            // A capacity that does not fit is rounded down (a floor), as §3.7 says.
            while (r.num() > 0xFFFF_FFFFL || r.den() > 0xFFFF_FFFFL)
                r = Rat.of(Math.max(1, r.num() / 2), Math.max(1, (r.den() + 1) / 2));
            capNum = r.num();
            capDen = r.den();
            return this;
        }

        public Edge latency(int ticks) {
            latency = Math.max(0, ticks);
            return this;
        }

        public Edge param(long p) {
            param = p;
            return this;
        }

        public Edge filter(int good) {
            if (!filter.contains(good))
                filter.add(good);
            return this;
        }
    }

    public Edge edge(int kind, int from, int to) {
        if (from == NONE || to == NONE)
            throw new IllegalArgumentException("an edge needs both ends: " + from + " -> " + to);
        Edge e = new Edge(edges.size(), kind, from, to);
        edges.add(e);
        return e;
    }

    public List<Edge> edges() {
        return edges;
    }

    // ---- recipes --------------------------------------------------------------------------------

    /** One recipe term (Term24): {@code num/den} of a good per craft. */
    public record Term(int good, int flags, long num, long den) {
        public Term {
            Rat r = Rat.of(num, den);
            num = r.num();
            den = r.den();
        }
    }

    /** A recipe (IND_GRAPH_RECIPE): equal recipes share one index. */
    public static final class Recipe {
        public final int type;
        public final int heat;
        public final int duration;
        public final int name;
        public final List<Term> inputs = new ArrayList<>();
        public final List<Term> outputs = new ArrayList<>();

        public Recipe(int type, int heat, int duration, int name) {
            this.type = type;
            this.heat = heat;
            this.duration = duration;
            this.name = name;
        }

        public Recipe in(int good, long num, long den) {
            return in(good, num, den, 0);
        }

        public Recipe in(int good, long num, long den, int flags) {
            merge(inputs, new Term(good, flags, num, den));
            return this;
        }

        public Recipe out(int good, long num, long den) {
            return out(good, num, den, 0);
        }

        public Recipe out(int good, long num, long den, int flags) {
            merge(outputs, new Term(good, flags, num, den));
            return this;
        }

        private static void merge(List<Term> terms, Term t) {
            if (t.num() == 0)
                return;
            for (int i = 0; i < terms.size(); i++) {
                Term o = terms.get(i);
                if (o.good() == t.good() && o.flags() == t.flags()) {
                    Rat sum = new Rat(o.num() * t.den() + t.num() * o.den(), o.den() * t.den());
                    terms.set(i, new Term(o.good(), o.flags(), sum.num(), sum.den()));
                    return;
                }
            }
            terms.add(t);
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof Recipe r && r.type == type && r.heat == heat && r.duration == duration && r.name == name
                    && r.inputs.equals(inputs) && r.outputs.equals(outputs);
        }

        @Override
        public int hashCode() {
            return Objects.hash(type, heat, duration, name, inputs, outputs);
        }
    }

    /** Adds a recipe, or finds the equal one already added; returns its index. */
    public int recipe(Recipe r) {
        Integer i = recipeIndex.get(r);
        if (i != null)
            return i;
        int idx = recipes.size();
        recipes.add(r);
        recipeIndex.put(r, idx);
        return idx;
    }

    public Recipe recipe(int index) {
        return recipes.get(index);
    }

    public int recipeCount() {
        return recipes.size();
    }

    /** A NAME dictionary id for a recipe id. */
    public int name(String text) {
        return ind.nameId(text);
    }

    // ---- kinetic domains ------------------------------------------------------------------------

    /** Level24: the root source's speed (RPM, signed) and the domain's capacity in SU at that level. */
    public record Level(int selector, Rat speed, double capacity) {}

    /** Member24: {@code count} blocks at {@code ratio × root speed}, {@code impact} SU per RPM each. */
    public record Member(int node, int count, Rat ratio, double impact) {}

    public static final class Domain {
        public final int index;
        public final long network;
        public int current;
        public int driver = NONE;
        public int flags;
        public double liveStress, liveCapacity;
        public final List<Level> levels = new ArrayList<>();
        public final List<Member> members = new ArrayList<>();

        Domain(int index, long network) {
            this.index = index;
            this.network = network;
        }

        /** Adds a member, merging it into an equal one without a node (same ratio and impact). */
        public void member(int node, Rat ratio, double impact) {
            if (node == NONE) {
                for (int i = 0; i < members.size(); i++) {
                    Member m = members.get(i);
                    if (m.node() == NONE && m.ratio().equals(ratio) && m.impact() == impact) {
                        members.set(i, new Member(NONE, m.count() + 1, ratio, impact));
                        return;
                    }
                }
            }
            members.add(new Member(node, 1, ratio, impact));
        }
    }

    /** The domain of a Create network id (or any plugin's key), made on first use. */
    public Domain domain(long network) {
        int i = domainByNetwork.get(network);
        if (i >= 0)
            return domains.get(i);
        Domain d = new Domain(domains.size(), network);
        domains.add(d);
        domainByNetwork.put(network, d.index);
        return d;
    }

    public Domain domainOf(long network) {
        int i = domainByNetwork.get(network);
        return i < 0 ? null : domains.get(i);
    }

    public List<Domain> domains() {
        return domains;
    }

    // ---- validation and encoding ----------------------------------------------------------------

    /** Marks the compile not compilable (END flag 2) with a reason code and a log line. */
    public void notCompilable(int reason, String why) {
        if (notCompilable == 0) {
            notCompilable = reason;
            notCompilableWhy = why;
        }
    }

    /** Null when every reference is in range and every record fits, else what is wrong. */
    String validate() {
        int n = nodes.size(), r = recipes.size(), dn = domains.size();
        for (Node x : nodes) {
            if (x.domain != NONE && (x.domain < 0 || x.domain >= dn))
                return "node " + x.index + " names domain " + x.domain + " of " + dn;
            for (int[] rate : x.rates) {
                if (rate[1] != NONE && (rate[1] < 0 || rate[1] >= r))
                    return "node " + x.index + " names recipe " + rate[1] + " of " + r;
                if (x.domain == NONE && rate[0] != 0)
                    return "node " + x.index + " has no domain but a rate at level " + rate[0];
                if (x.domain != NONE && rate[0] >= domains.get(x.domain).levels.size())
                    return "node " + x.index + " has a rate at level " + rate[0] + " of " + domains.get(x.domain).levels.size();
            }
            if (GlassRecords.Node.size(x.rates.size()) > MAX_RECORD)
                return "node " + x.index + " has " + x.rates.size() + " rates";
        }
        for (Edge e : edges) {
            if (e.from < 0 || e.from >= n || e.to < 0 || e.to >= n)
                return "edge " + e.index + " " + e.from + " -> " + e.to + " of " + n + " nodes";
            if (e.capNum != 0 && e.capDen == 0)
                return "edge " + e.index + " capacity " + e.capNum + "/" + e.capDen;
            if (GlassRecords.Edge.size(e.filter.size()) > MAX_RECORD)
                return "edge " + e.index + " has " + e.filter.size() + " filter goods";
        }
        for (Recipe x : recipes)
            if (GlassRecords.Recipe.size(x.inputs.size() + x.outputs.size()) > MAX_RECORD)
                return "a recipe has " + (x.inputs.size() + x.outputs.size()) + " terms";
        for (Domain d : domains) {
            if (d.driver != NONE && (d.driver < 0 || d.driver >= n))
                return "domain " + d.index + " driver " + d.driver;
            if (!d.levels.isEmpty() && (d.current < 0 || d.current >= d.levels.size()))
                return "domain " + d.index + " current level " + d.current + " of " + d.levels.size();
            for (Member m : d.members)
                if (m.node() != NONE && (m.node() < 0 || m.node() >= n))
                    return "domain " + d.index + " member node " + m.node();
            if (GlassRecords.Domain.size(d.levels.size(), d.members.size()) > MAX_RECORD)
                return "domain " + d.index + " has " + d.levels.size() + " levels and " + d.members.size() + " members";
        }
        return null;
    }

    /** One encoded record: its kind, aux and bytes (header zero; the sender writes it and the game time). */
    record Encoded(int kind, int aux, byte[] bytes) {}

    /** The compile's records in send order (nodes, edges, recipes, domains), END excluded. */
    List<Encoded> encode(long districtId, int compile) {
        List<Encoded> out = new ArrayList<>(nodes.size() + edges.size() + recipes.size() + domains.size());
        int frameFlag = frame != 0 ? GlassRecords.Node.FRAME : 0;
        for (Node x : nodes) {
            ByteBuffer b = buf(GlassRecords.Node.size(x.rates.size()));
            head(b, districtId, compile, x.index);
            b.putInt(GlassRecords.Node.MODEL, x.model);
            b.putInt(GlassRecords.Node.DOMAIN, x.domain);
            b.putInt(GlassRecords.Node.INVENTORY, x.inventory);
            b.putInt(GlassRecords.Node.FLAGS, x.flags | frameFlag);
            b.putInt(GlassRecords.Node.X, x.x);
            b.putInt(GlassRecords.Node.Y, x.y);
            b.putInt(GlassRecords.Node.Z, x.z);
            b.putInt(GlassRecords.Node.RULE, x.rule);
            b.putInt(GlassRecords.Node.N_RATES, x.rates.size());
            b.putLong(GlassRecords.Node.CAPACITY, x.capacity);
            b.putLong(GlassRecords.Node.PARAM0, x.params[0]);
            b.putLong(GlassRecords.Node.PARAM1, x.params[1]);
            b.putLong(GlassRecords.Node.PARAM2, x.params[2]);
            b.putLong(GlassRecords.Node.PARAM3, x.params[3]);
            for (int i = 0; i < x.rates.size(); i++) {
                int[] rt = x.rates.get(i);
                int at = GlassRecords.Node.RATES + i * GlassRecords.Node.RATE;
                b.putInt(at + GlassRecords.Node.R_LEVEL, rt[0]);
                b.putInt(at + GlassRecords.Node.R_RECIPE, rt[1]);
                b.putInt(at + GlassRecords.Node.R_BATCH, rt[2]);
                b.putInt(at + GlassRecords.Node.R_TICKS_NUM, rt[3]);
                b.putInt(at + GlassRecords.Node.R_TICKS_DEN, rt[4]);
                b.putInt(at + GlassRecords.Node.R_FLAGS, rt[5]);
            }
            out.add(new Encoded(GlassRecords.IND_GRAPH_NODE, x.kind, b.array()));
        }
        for (Edge e : edges) {
            ByteBuffer b = buf(GlassRecords.Edge.size(e.filter.size()));
            head(b, districtId, compile, e.index);
            b.putInt(GlassRecords.Edge.FROM, e.from);
            b.putInt(GlassRecords.Edge.TO, e.to);
            b.putInt(GlassRecords.Edge.ORDER, e.order);
            b.putInt(GlassRecords.Edge.FLAGS, e.flags);
            b.putInt(GlassRecords.Edge.CAP_NUM, (int) e.capNum);
            b.putInt(GlassRecords.Edge.CAP_DEN, (int) e.capDen);
            b.putInt(GlassRecords.Edge.LATENCY, e.latency);
            b.putInt(GlassRecords.Edge.N_FILTER, e.filter.size());
            b.putLong(GlassRecords.Edge.PARAM, e.param);
            for (int i = 0; i < e.filter.size(); i++)
                b.putInt(GlassRecords.Edge.FILTER + 4 * i, e.filter.getInt(i));
            out.add(new Encoded(GlassRecords.IND_GRAPH_EDGE, e.kind, b.array()));
        }
        for (int ri = 0; ri < recipes.size(); ri++) {
            Recipe x = recipes.get(ri);
            int terms = x.inputs.size() + x.outputs.size();
            ByteBuffer b = buf(GlassRecords.Recipe.size(terms));
            head(b, districtId, compile, ri);
            b.putInt(GlassRecords.Recipe.TYPE, x.type);
            b.putInt(GlassRecords.Recipe.HEAT, x.heat);
            b.putInt(GlassRecords.Recipe.N_INPUTS, x.inputs.size());
            b.putInt(GlassRecords.Recipe.N_OUTPUTS, x.outputs.size());
            b.putInt(GlassRecords.Recipe.DURATION, x.duration);
            b.putInt(GlassRecords.Recipe.NAME, x.name);
            int i = 0;
            for (List<Term> list : List.of(x.inputs, x.outputs))
                for (Term t : list) {
                    int at = GlassRecords.Recipe.TERMS + (i++) * GlassRecords.Recipe.TERM;
                    b.putInt(at + GlassRecords.Recipe.T_GOOD, t.good());
                    b.putInt(at + GlassRecords.Recipe.T_FLAGS, t.flags());
                    b.putLong(at + GlassRecords.Recipe.T_NUM, t.num());
                    b.putLong(at + GlassRecords.Recipe.T_DEN, t.den());
                }
            out.add(new Encoded(GlassRecords.IND_GRAPH_RECIPE, terms, b.array()));
        }
        for (Domain d : domains) {
            ByteBuffer b = buf(GlassRecords.Domain.size(d.levels.size(), d.members.size()));
            head(b, districtId, compile, d.index);
            b.putLong(GlassRecords.Domain.NETWORK, d.network);
            b.putInt(GlassRecords.Domain.N_LEVELS, d.levels.size());
            b.putInt(GlassRecords.Domain.CURRENT, d.current);
            b.putInt(GlassRecords.Domain.DRIVER, d.driver);
            b.putInt(GlassRecords.Domain.FLAGS, d.flags);
            b.putDouble(GlassRecords.Domain.LIVE_STRESS, d.liveStress);
            b.putDouble(GlassRecords.Domain.LIVE_CAPACITY, d.liveCapacity);
            for (int i = 0; i < d.levels.size(); i++) {
                Level l = d.levels.get(i);
                Rat s = l.speed().fitSigned();
                int at = GlassRecords.Domain.LEVELS + i * GlassRecords.Domain.LEVEL;
                b.putInt(at + GlassRecords.Domain.L_SELECTOR, l.selector());
                b.putInt(at + GlassRecords.Domain.L_SPEED_NUM, (int) s.num());
                b.putInt(at + GlassRecords.Domain.L_SPEED_DEN, (int) s.den());
                b.putDouble(at + GlassRecords.Domain.L_CAPACITY, l.capacity());
            }
            int m0 = GlassRecords.Domain.members(d.levels.size());
            for (int i = 0; i < d.members.size(); i++) {
                Member m = d.members.get(i);
                Rat q = m.ratio().fitSigned();
                int at = m0 + i * GlassRecords.Domain.MEMBER;
                b.putInt(at + GlassRecords.Domain.M_NODE, m.node());
                b.putInt(at + GlassRecords.Domain.M_COUNT, m.count());
                b.putInt(at + GlassRecords.Domain.M_RATIO_NUM, (int) q.num());
                b.putInt(at + GlassRecords.Domain.M_RATIO_DEN, (int) q.den());
                b.putDouble(at + GlassRecords.Domain.M_IMPACT, m.impact());
            }
            out.add(new Encoded(GlassRecords.IND_GRAPH_DOMAIN, d.members.size(), b.array()));
        }
        return out;
    }

    private static ByteBuffer buf(int size) {
        return ByteBuffer.allocate(size).order(Layout.ORDER);
    }

    private static void head(ByteBuffer b, long district, int compile, int index) {
        b.putLong(GlassRecords.DISTRICT, district);
        b.putInt(GlassRecords.COMPILE, compile);
        b.putInt(GlassRecords.INDEX, index);
    }
}
