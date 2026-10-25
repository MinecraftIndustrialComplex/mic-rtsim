package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import com.google.gson.stream.JsonWriter;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.context.CommandContext;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;

import java.io.IOException;
import java.io.Writer;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;

/**
 * {@code /rtsim district graph [name|id]} (SECTIONS.md §8.1, slice S1): writes a district's last compile
 * this session and its members to {@code <server>/debug/rtsim-graph-<id>-<game time>.json}, for an
 * operator to look at what the glass box was given. It reads only what the compiler and the district
 * index already hold: no compile is asked for, no chunk is loaded (a member chunk's state is
 * {@code getChunkNow}'s), and the engine is not asked. Permission 2, as every {@code /rtsim district}
 * command. Slices S2, S4, S5 and S6 add sections, package stores, shared networks and join pairs.
 */
public final class GlassGraphCommand {
    private GlassGraphCommand() {}

    public static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
        dispatcher.register(Commands.literal("rtsim")
                .requires(src -> src.hasPermission(2))
                .then(Commands.literal("district")
                        .requires(src -> src.hasPermission(2))
                        .then(Commands.literal("graph").executes(c -> graph(c, null))
                                .then(Commands.argument("name", StringArgumentType.greedyString())
                                        .executes(c -> graph(c, StringArgumentType.getString(c, "name")))))));
    }

    private static int graph(CommandContext<CommandSourceStack> ctx, String name) {
        Industry ind = IndustryApi.current();
        if (ind == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no industry module (no session, or the industry module is off)"));
            return 0;
        }
        District d = name == null
                ? ind.districtAt(ctx.getSource().getLevel(), BlockPos.containing(ctx.getSource().getPosition()))
                : ind.districts().byName(name);
        if (d == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no district " + (name == null ? "here" : "'" + name + "'")));
            return 0;
        }
        MinecraftServer server = ctx.getSource().getServer();
        try {
            Path file = write(ind, server, d);
            GraphBuilder g = ind.glass().lastGraph(d.id);
            String what = g == null ? "no compile this session" : g.nodeCount() + " nodes, " + g.edges().size() + " edges, "
                    + g.unloadedStops + " reads stopped at unloaded chunks";
            ctx.getSource().sendSuccess(() -> Component.literal("district " + d.id + " '" + d.displayName() + "': " + what
                    + "; written to " + server.getServerDirectory().relativize(file)), false);
            return 1;
        } catch (IOException e) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: could not write the graph: " + e));
            return 0;
        }
    }

    /** Writes the dump and returns its path (GameTests call it directly). */
    public static Path write(Industry ind, MinecraftServer server, District d) throws IOException {
        ServerLevel level = server.getLevel(d.dim);
        long now = level == null ? server.overworld().getGameTime() : level.getGameTime();
        Path dir = server.getServerDirectory().resolve("debug");
        Files.createDirectories(dir);
        Path file = dir.resolve("rtsim-graph-" + d.id + "-" + now + ".json");
        try (Writer out = Files.newBufferedWriter(file, StandardCharsets.UTF_8); JsonWriter w = new JsonWriter(out)) {
            w.setIndent(" ");
            w.beginObject();
            w.name("district").value(d.id);
            w.name("name").value(d.displayName());
            w.name("dimension").value(d.dim.location().toString());
            w.name("gameTime").value(now);
            w.name("revision").value(d.revision);
            members(w, level, d);
            GraphBuilder g = ind.glass().lastGraph(d.id);
            w.name("compile");
            if (g == null)
                w.nullValue();
            else
                compile(w, ind, g);
            w.endObject();
        }
        return file;
    }

    private static void members(JsonWriter w, ServerLevel level, District d) throws IOException {
        long[] chunks = d.chunks.toLongArray();
        java.util.Arrays.sort(chunks);
        w.name("chunks").beginArray();
        for (long c : chunks) {
            int cx = ChunkPos.getX(c), cz = ChunkPos.getZ(c);
            w.beginObject();
            w.name("x").value(cx).name("z").value(cz);
            w.name("seeds").value(d.seedCounts.getOrDefault(c, 0));
            boolean loaded = level != null && level.getChunkSource().getChunkNow(cx, cz) != null;
            w.name("loaded").value(loaded);
            w.name("ticking").value(loaded && level.shouldTickBlocksAt(c));
            w.endObject();
        }
        w.endArray();
        w.name("inventories").beginArray();
        for (District.Inventory inv : d.inventories) {
            w.beginObject();
            w.name("index").value(inv.index());
            pos(w, inv.pos());
            w.name("kind").value(inv.kind());
            w.name("itemSlots").value(inv.itemSlots());
            w.name("fluidCapacity").value(inv.fluidCapacity());
            w.name("energyCapacity").value(inv.energyCapacity());
            w.endObject();
        }
        w.endArray();
        w.name("stations").beginArray();
        for (District.Station s : d.stations) {
            w.beginObject();
            w.name("id").value(s.id());
            pos(w, s.pos());
            w.name("name").value(s.name());
            w.endObject();
        }
        w.endArray();
    }

    private static void compile(JsonWriter w, Industry ind, GraphBuilder g) throws IOException {
        w.beginObject();
        w.name("frame").value(g.frame);
        w.name("fingerprint").value(Long.toHexString(ind.glass().fingerprint(g.district.id)));
        w.name("notCompilable").value(g.notCompilable);
        w.name("notCompilableWhy").value(g.notCompilableWhy);
        w.name("unloadedStops").value(g.unloadedStops);
        w.name("nodes").beginArray();
        for (GraphBuilder.Node n : g.nodes()) {
            w.beginObject();
            w.name("index").value(n.index);
            w.name("kind").value(name(NODE_KINDS, n.kind));
            w.name("model").value(name(MODELS, n.model));
            pos(w, n.pos());
            w.name("domain").value(n.domain == GraphBuilder.NONE ? -1 : n.domain);
            w.name("inventory").value(n.inventory == GraphBuilder.NONE ? -1 : n.inventory);
            w.name("flags").value(flags(n.flags, NODE_FLAGS));
            if (n.rule != 0)
                w.name("rule").value(name(RULES, n.rule));
            if (n.capacity != 0)
                w.name("capacity").value(n.capacity);
            w.name("rates").value(n.rateCount());
            w.endObject();
        }
        w.endArray();
        w.name("edges").beginArray();
        for (GraphBuilder.Edge e : g.edges()) {
            w.beginObject();
            w.name("index").value(e.index);
            w.name("kind").value(name(EDGE_KINDS, e.kind));
            w.name("from").value(e.from).name("to").value(e.to);
            w.name("flags").value(flags(e.flags, EDGE_FLAGS));
            if (e.capDen != 0)
                w.name("capacity").value(e.capNum + "/" + e.capDen);
            if (e.latency != 0)
                w.name("latency").value(e.latency);
            if (!e.filter.isEmpty()) {
                w.name("filter").beginArray();
                for (int i = 0; i < e.filter.size(); i++)
                    w.value(good(ind, e.filter.getInt(i)));
                w.endArray();
            }
            w.endObject();
        }
        w.endArray();
        w.name("recipes").beginArray();
        for (int i = 0; i < g.recipeCount(); i++) {
            GraphBuilder.Recipe r = g.recipe(i);
            w.beginObject();
            w.name("index").value(i);
            w.name("type").value(name(RECIPE_TYPES, r.type));
            w.name("heat").value(r.heat);
            w.name("duration").value(r.duration);
            terms(w, ind, "inputs", r.inputs);
            terms(w, ind, "outputs", r.outputs);
            w.endObject();
        }
        w.endArray();
        w.name("domains").beginArray();
        for (GraphBuilder.Domain dm : g.domains()) {
            w.beginObject();
            w.name("index").value(dm.index);
            pos(w, BlockPos.of(dm.network));
            w.name("current").value(dm.current == GraphBuilder.NONE ? -1 : dm.current);
            w.name("flags").value(dm.flags);
            w.name("liveStress").value(dm.liveStress);
            w.name("liveCapacity").value(dm.liveCapacity);
            w.name("levels").beginArray();
            for (GraphBuilder.Level l : dm.levels)
                level(w, l);
            w.endArray();
            w.name("sources").beginArray();
            for (GraphBuilder.Source sr : dm.sources) {
                w.beginObject();
                pos(w, BlockPos.of(sr.pos));
                w.name("driver").value(sr.driver == GraphBuilder.NONE ? -1 : sr.driver);
                if (sr.ratio == null)
                    w.name("ratio").nullValue();
                else
                    w.name("ratio").value(sr.ratio.num() + "/" + sr.ratio.den());
                w.name("current").value(sr.current == GraphBuilder.NONE ? -1 : sr.current);
                w.name("rows").beginArray();
                for (GraphBuilder.Level l : sr.levels)
                    level(w, l);
                w.endArray();
                w.endObject();
            }
            w.endArray();
            w.name("members").beginArray();
            for (GraphBuilder.Member m : dm.members)
                w.beginObject().name("node").value(m.node() == GraphBuilder.NONE ? -1 : m.node()).name("count").value(m.count())
                        .name("ratio").value(m.ratio().toString()).name("impact").value(m.impact()).endObject();
            w.endArray();
            w.endObject();
        }
        w.endArray();
        w.endObject();
    }

    private static void level(JsonWriter w, GraphBuilder.Level l) throws IOException {
        w.beginObject().name("selector").value(l.selector()).name("speed").value(l.speed().toString())
                .name("capacity").value(l.capacity()).endObject();
    }

    private static void terms(JsonWriter w, Industry ind, String key, java.util.List<GraphBuilder.Term> terms) throws IOException {
        w.name(key).beginArray();
        for (GraphBuilder.Term t : terms)
            w.beginObject().name("good").value(good(ind, t.good())).name("amount").value(t.num() + "/" + t.den())
                    .name("flags").value(t.flags()).endObject();
        w.endArray();
    }

    private static void pos(JsonWriter w, BlockPos p) throws IOException {
        w.name("pos").beginArray().value(p.getX()).value(p.getY()).value(p.getZ()).endArray();
    }

    private static String good(Industry ind, int id) {
        String n = ind.goodName(id);
        return n == null ? "#" + id : n;
    }

    private static String name(Map<Integer, String> names, int code) {
        String n = names.get(code);
        return n == null ? Integer.toString(code) : n;
    }

    private static String flags(int flags, String[] bits) {
        StringBuilder sb = new StringBuilder();
        for (int b = 0; b < 32; b++) {
            if ((flags & (1 << b)) == 0)
                continue;
            if (!sb.isEmpty())
                sb.append(' ');
            sb.append(b < bits.length && bits[b] != null ? bits[b] : "bit" + b);
        }
        return sb.toString();
    }

    private static final String[] NODE_FLAGS = {"bounded", "any_output_blocks", "stochastic", "fluid", "calibrated", "frame", "pass_through"};
    private static final String[] EDGE_FLAGS = {"deny", "fluid", "bounded", "filtered_side", "through"};
    private static final Map<Integer, String> NODE_KINDS = Map.ofEntries(
            Map.entry(GlassRecords.Node.STORE, "store"), Map.entry(GlassRecords.Node.BUFFER, "buffer"),
            Map.entry(GlassRecords.Node.MACHINE, "machine"), Map.entry(GlassRecords.Node.MOVER, "mover"),
            Map.entry(GlassRecords.Node.JUNCTION, "junction"), Map.entry(GlassRecords.Node.HEATER, "heater"),
            Map.entry(GlassRecords.Node.SOURCE, "source"), Map.entry(GlassRecords.Node.SINK, "sink"),
            Map.entry(GlassRecords.Node.BLACK_BOX, "black_box"), Map.entry(GlassRecords.Node.CONTROLLER, "controller"),
            Map.entry(GlassRecords.Node.PORT, "port"), Map.entry(GlassRecords.Node.BOILER, "boiler"));
    private static final Map<Integer, String> EDGE_KINDS = Map.ofEntries(
            Map.entry(GlassRecords.Edge.ADJACENCY, "adjacency"), Map.entry(GlassRecords.Edge.BELT, "belt"),
            Map.entry(GlassRecords.Edge.CHUTE, "chute"), Map.entry(GlassRecords.Edge.PIPE, "pipe"),
            Map.entry(GlassRecords.Edge.CHAIN, "chain"), Map.entry(GlassRecords.Edge.AIR_ZONE, "air_zone"),
            Map.entry(GlassRecords.Edge.ARM_REACH, "arm_reach"), Map.entry(GlassRecords.Edge.HEAT, "heat"),
            Map.entry(GlassRecords.Edge.CONTROL, "control"), Map.entry(GlassRecords.Edge.OBSERVE, "observe"),
            Map.entry(GlassRecords.Edge.LINE, "line"), Map.entry(GlassRecords.Edge.DROP, "drop"));
    /** Model, rule and recipe type names from their constants, lower-cased. */
    private static final Map<Integer, String> MODELS = constants(GlassRecords.Model.class);
    private static final Map<Integer, String> RULES = constants(GlassRecords.Rule.class);
    private static final Map<Integer, String> RECIPE_TYPES = constants(GlassRecords.RecipeType.class);

    private static Map<Integer, String> constants(Class<?> c) {
        Map<Integer, String> out = new HashMap<>();
        for (Field f : c.getFields()) {
            if (f.getType() != int.class || !Modifier.isStatic(f.getModifiers()))
                continue;
            try {
                out.putIfAbsent(f.getInt(null), f.getName().toLowerCase(java.util.Locale.ROOT));
            } catch (IllegalAccessException ignored) {
            }
        }
        return out;
    }
}
