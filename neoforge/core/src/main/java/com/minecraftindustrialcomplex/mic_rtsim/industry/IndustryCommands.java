package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.arguments.IntegerArgumentType;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.context.CommandContext;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;

import java.util.UUID;

/**
 * {@code /rtsim district}: an operator and developer diagnostic and editing tool, not a player feature
 * (I18 as decided: players are shown nothing of what a distant factory does). Registered under the core's
 * {@code /rtsim} root (Brigadier merges the two literal trees); the district node requires permission
 * level 2 itself, so every subcommand that shows district output or edits districts needs it:
 * <ul>
 *   <li>{@code list}, {@code info [name]}, {@code debug [name]}: what Java knows of a district (chunks and
 *       which are loaded, the load state sent to the engine, the last sweep and defence survey, the last
 *       reconcile, plugin warnings); {@code debug} adds the chunk, inventory and station lists. Neither asks
 *       the engine: there is no ledger on the wire;</li>
 *   <li>{@code sections [name|id]} (registered by {@code GlassSectionsCommand}, SECTIONS.md §8.1) and {@code graph}:
 *       the glass box's sections and shared stores, and the compile dump; {@code info} and {@code debug} add one line
 *       with the section count;</li>
 *   <li>{@code rename <name> <new name>}: the district's name, over its station's;</li>
 *   <li>{@code merge <a> <b>}: one district from two (a merge link between their anchor chunks);</li>
 *   <li>{@code split [radius]}: the member chunks within the radius of your chunk become a district of
 *       their own (pinned);</li>
 *   <li>{@code exclude} and {@code include}: your chunk is in no district, or detection decides again;</li>
 *   <li>{@code adopt [radius]}: industrial block entities around you become seeds (worlds made before the
 *       mod, structures placed by commands); detection runs around them only (DS-12);</li>
 *   <li>{@code rescan}, {@code perf}.</li>
 * </ul>
 * No new items or blocks (D3). Every edit is an IND_EDIT (aux 4, command) for the districts it touches.
 *
 * <p>{@code rescan}, {@code merge}, {@code split}, {@code exclude} and {@code include} record their edit at once and
 * then ask for a full detection of the dimension that runs over the next ticks like a ticked one, a block entity
 * budget a tick (DS-16, SG60). Each says so when it replies, and says again, in the same chat or console and in the
 * server log, when the districts have changed. A source that cannot hear later (an RCON client) reads the log or
 * {@code perf}.
 */
public final class IndustryCommands {
    private IndustryCommands() {}

    public static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
        dispatcher.register(Commands.literal("rtsim")
                .requires(src -> src.hasPermission(2))
                .then(Commands.literal("district")
                        .requires(src -> src.hasPermission(2))
                        .then(Commands.literal("list").executes(IndustryCommands::list))
                        .then(Commands.literal("perf").executes(IndustryCommands::perf))
                        .then(Commands.literal("rescan").executes(IndustryCommands::rescan))
                        .then(Commands.literal("info").executes(c -> info(c, null))
                                .then(Commands.argument("name", StringArgumentType.string()).executes(c -> info(c, StringArgumentType.getString(c, "name")))))
                        .then(Commands.literal("debug").executes(c -> debug(c, null))
                                .then(Commands.argument("name", StringArgumentType.string()).executes(c -> debug(c, StringArgumentType.getString(c, "name")))))
                        .then(Commands.literal("rename")
                                .then(Commands.argument("name", StringArgumentType.string())
                                        .then(Commands.argument("newName", StringArgumentType.greedyString()).executes(IndustryCommands::rename))))
                        .then(Commands.literal("merge")
                                .then(Commands.argument("a", StringArgumentType.string())
                                        .then(Commands.argument("b", StringArgumentType.string()).executes(IndustryCommands::merge))))
                        .then(Commands.literal("split").executes(c -> split(c, 0))
                                .then(Commands.argument("radius", IntegerArgumentType.integer(0, 32)).executes(c -> split(c, IntegerArgumentType.getInteger(c, "radius")))))
                        .then(Commands.literal("exclude").executes(c -> exclude(c, true)))
                        .then(Commands.literal("include").executes(c -> exclude(c, false)))
                        .then(Commands.literal("adopt").executes(c -> adopt(c, 0))
                                .then(Commands.argument("radius", IntegerArgumentType.integer(0, 16)).executes(c -> adopt(c, IntegerArgumentType.getInteger(c, "radius")))))));
    }

    private static Industry industry(CommandContext<CommandSourceStack> ctx) {
        Industry ind = IndustryApi.current();
        if (ind == null)
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no industry module (no session, or the industry module is off)"));
        return ind;
    }

    private static District find(CommandContext<CommandSourceStack> ctx, Industry ind, String name) {
        District d;
        if (name == null) {
            ServerLevel level = ctx.getSource().getLevel();
            d = ind.districtAt(level, net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        } else {
            d = ind.districts.byName(name);
        }
        if (d == null)
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no district " + (name == null ? "here" : "'" + name + "'")));
        return d;
    }

    private static void say(CommandContext<CommandSourceStack> ctx, String text) {
        ctx.getSource().sendSuccess(() -> Component.literal(text), false);
    }

    /**
     * The edit is recorded; the districts change when a full detection of the source's dimension has run over the
     * next ticks (DS-16). Says so now, and {@code done} (what the districts are then) when it is, to the source
     * and to the log.
     */
    private static void detectThenSay(CommandContext<CommandSourceStack> ctx, Industry ind, ServerLevel level, String now,
                                      java.util.function.Supplier<String> done) {
        CommandSourceStack src = ctx.getSource();
        say(ctx, now + "; the detection runs over the next ticks and the result follows");
        ind.districts.rescanSoon(level, () -> {
            String text = done.get();
            MicRtsim.LOGGER.info("[industry] {}", text);
            src.sendSuccess(() -> Component.literal(text), false);
        });
    }

    /**
     * {@code merge} and {@code split} read the district set, which the detection of an earlier command still under way has
     * not changed yet (a split of a chunk that an unfinished {@code exclude} takes out would pin a district that never
     * exists): they are refused until its result line (DS-16). {@code rescan}, {@code exclude} and {@code include} record
     * their edit alone and stack.
     */
    private static boolean busy(CommandContext<CommandSourceStack> ctx, Industry ind,
                                net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level> dim) {
        if (!ind.districts.commandPending(dim))
            return false;
        ctx.getSource().sendFailure(Component.literal(
                "mic_rtsim: an earlier command's detection is still running; wait for its result line and try again"));
        return true;
    }

    private static UUID player(CommandContext<CommandSourceStack> ctx) {
        Entity e = ctx.getSource().getEntity();
        return e == null ? null : e.getUUID();
    }

    private static int list(CommandContext<CommandSourceStack> ctx) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        StringBuilder sb = new StringBuilder("districts: " + ind.districts.all().size());
        for (District d : ind.districts.all())
            sb.append("\n  ").append(d.id).append(" '").append(d.displayName()).append("' ").append(d.dim.location())
                    .append(", ").append(d.chunks.size()).append(" chunks, ").append(d.inventories.size()).append(" inventories, load ")
                    .append(d.loadState).append(d.watched ? ", watched" : "");
        say(ctx, sb.toString());
        return ind.districts.all().size();
    }

    private static int info(CommandContext<CommandSourceStack> ctx, String name) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        District d = find(ctx, ind, name);
        if (d == null)
            return 0;
        for (MutableComponent c : ind.diagnostics.debugLines(d))
            ctx.getSource().sendSuccess(() -> c, false);
        return 1;
    }

    private static int debug(CommandContext<CommandSourceStack> ctx, String name) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        District d = find(ctx, ind, name);
        if (d == null)
            return 0;
        for (MutableComponent c : ind.diagnostics.debugLines(d))
            ctx.getSource().sendSuccess(() -> c, false);
        for (MutableComponent c : ind.diagnostics.detailLines(d))
            ctx.getSource().sendSuccess(() -> c, false);
        return 1;
    }

    private static int rename(CommandContext<CommandSourceStack> ctx) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        District d = find(ctx, ind, StringArgumentType.getString(ctx, "name"));
        if (d == null)
            return 0;
        d.customName = StringArgumentType.getString(ctx, "newName");
        ind.districts.edit(d, IndRecords.Edit.COMMAND, player(ctx), net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        ind.districts.redeclare(d);
        ind.data.setDirty();
        say(ctx, "mic_rtsim: district " + d.id + " is now '" + d.displayName() + "'");
        return 1;
    }

    private static int merge(CommandContext<CommandSourceStack> ctx) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        District a = find(ctx, ind, StringArgumentType.getString(ctx, "a"));
        District b = find(ctx, ind, StringArgumentType.getString(ctx, "b"));
        if (a == null || b == null)
            return 0;
        if (a == b || a.dim != b.dim) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: merge needs two districts in one dimension"));
            return 0;
        }
        if (busy(ctx, ind, a.dim))
            return 0;
        // Pins win over links: a pinned district merges by moving its pins.
        var pins = ind.data.pins(a.dim);
        boolean aPinned = pins.containsValue(a.id), bPinned = pins.containsValue(b.id);
        if (aPinned || bPinned) {
            long into = aPinned ? a.id : b.id, from = aPinned ? b.id : a.id;
            District src = into == a.id ? b : a;
            for (long c : src.chunks)
                pins.put(c, into);
            for (var e : pins.long2LongEntrySet())
                if (e.getLongValue() == from)
                    e.setValue(into);
        } else {
            ind.data.mergeLinks(a.dim).add(new long[] {a.anchorChunk(), b.anchorChunk()});
        }
        ind.districts.edit(a, IndRecords.Edit.COMMAND, player(ctx), net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        ind.districts.edit(b, IndRecords.Edit.COMMAND, player(ctx), net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        ind.data.setDirty();
        ServerLevel level = ind.level(a.dim);
        if (level == null) {
            say(ctx, "mic_rtsim: merge recorded; the dimension is not loaded, the next full detection of it joins them");
            return 1;
        }
        detectThenSay(ctx, ind, level, "mic_rtsim: merge recorded",
                () -> "mic_rtsim: merged; " + ind.districts.all().size() + " districts now");
        return 1;
    }

    private static int split(CommandContext<CommandSourceStack> ctx, int radius) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        District d = find(ctx, ind, null);
        if (d == null)
            return 0;
        if (busy(ctx, ind, d.dim))
            return 0;
        ChunkPos here = new ChunkPos(net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        long id = ind.data.allocateId();
        var pins = ind.data.pins(d.dim);
        int n = 0;
        for (long c : d.chunks)
            if (Math.abs(ChunkPos.getX(c) - here.x) <= radius && Math.abs(ChunkPos.getZ(c) - here.z) <= radius) {
                pins.put(c, id);
                n++;
            }
        ind.districts.edit(d, IndRecords.Edit.COMMAND, player(ctx), net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        ind.data.setDirty();
        int chunks = n;
        detectThenSay(ctx, ind, ctx.getSource().getLevel(), "mic_rtsim: " + n + " chunks pinned as district " + id,
                () -> "mic_rtsim: " + chunks + " chunks split off as district " + id + "; " + ind.districts.all().size() + " districts now");
        return n;
    }

    private static int exclude(CommandContext<CommandSourceStack> ctx, boolean exclude) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        ServerLevel level = ctx.getSource().getLevel();
        ChunkPos here = new ChunkPos(net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        District d = ind.districtAt(level.dimension(), here.toLong());
        if (exclude)
            ind.data.excluded(level.dimension()).add(here.toLong());
        else
            ind.data.excluded(level.dimension()).remove(here.toLong());
        ind.data.pins(level.dimension()).remove(here.toLong());
        if (d != null)
            ind.districts.edit(d, IndRecords.Edit.COMMAND, player(ctx), net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
        ind.data.setDirty();
        ind.districts.markDirty(level.dimension(), here.toLong());
        detectThenSay(ctx, ind, level, "mic_rtsim: chunk " + here + (exclude ? " is excluded" : " is released"),
                () -> "mic_rtsim: chunk " + here + (exclude ? " is in no district now" : " is detected again") + "; "
                        + ind.districts.all().size() + " districts now");
        return 1;
    }

    private static int adopt(CommandContext<CommandSourceStack> ctx, int radius) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        ServerLevel level = ctx.getSource().getLevel();
        // Detects around the adopted chunks only (DS-12), never the whole dimension.
        Districts.Adopted a = ind.districts.adoptAround(level, new ChunkPos(net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition())), radius);
        say(ctx, "mic_rtsim: " + a.seeds() + " block entities adopted as seeds; " + ind.districts.all().size() + " districts"
                + (a.detected() ? "" : " so far, the detection around them goes on over the next ticks"));
        return a.seeds();
    }

    private static int rescan(CommandContext<CommandSourceStack> ctx) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        detectThenSay(ctx, ind, ctx.getSource().getLevel(), "mic_rtsim: " + ind.districts.all().size() + " districts",
                () -> "mic_rtsim: " + ind.districts.all().size() + " districts; the detection took " + ind.districts.lastRescanNanos / 1000
                        + " us of work over " + ind.districts.lastRescanSlices + " ticks (" + ind.districts.lastRescanPhases() + ")");
        return 1;
    }

    private static int perf(CommandContext<CommandSourceStack> ctx) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        Sampler s = ind.sampler;
        Reconciler r = ind.reconciler;
        PlayerPorts p = ind.ports;
        say(ctx, String.join("\n",
                "districts: " + ind.districts.all().size() + "; detections " + ind.districts.rescans + " (" + ind.districts.rescanNanos / 1000
                        + " us in all, last " + ind.districts.lastRescanNanos / 1000 + " us over " + ind.districts.lastRescanSlices + " ticks ("
                        + ind.districts.lastRescanPhases() + "), longest tick " + ind.districts.maxSliceNanos / 1000 + " us: "
                        + ind.districts.maxSliceWhat + "), declarations " + ind.districts.declared + ", detection "
                        + (ind.districts.rescanning() ? "under way or waiting" : "idle"),
                "last detection: " + ind.districts.lastRescanCold(),
                "sweeps: " + s.sweeps + " (" + s.liveSweeps + " live), inventories read " + s.inventoriesRead + ", slots " + s.slotsRead
                        + ", read time " + s.readNanos / 1000 + " us (slowest inventory " + s.maxInventoryNanos / 1000 + " us), records "
                        + s.recordsSent + " sent, " + s.recordsDropped + " dropped",
                "reconciles: " + r.applied + " applied, " + r.duplicates + " duplicates, " + r.held() + " held, " + r.shortfalls
                        + " short deltas, " + r.blocksBroken + " blocks broken, " + r.blocksSkipped + " skipped, " + r.staleEpochs
                        + " epochs of an earlier engine life dropped (engine life " + Long.toHexString(ind.engineLife()) + ")",
                "ports: sessions " + p.sessionPorts + ", tosses " + p.tossPorts + ", pickups " + p.pickupPorts + ", broken stock " + p.brokenPorts
                        + ", unattributed sessions " + p.unattributed,
                "commands applied " + ind.commandsApplied + ", failed " + ind.commandsFailed));
        for (IndustryPlugin plugin : IndustryApi.PLUGINS) {
            String line = plugin.perfLine();
            if (line != null)
                say(ctx, line);
        }
        return 1;
    }
}
