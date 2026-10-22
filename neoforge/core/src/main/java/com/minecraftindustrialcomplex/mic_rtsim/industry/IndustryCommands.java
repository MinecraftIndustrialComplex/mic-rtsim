package com.minecraftindustrialcomplex.mic_rtsim.industry;

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
 *   <li>{@code rename <name> <new name>}: the district's name, over its station's;</li>
 *   <li>{@code merge <a> <b>}: one district from two (a merge link between their anchor chunks);</li>
 *   <li>{@code split [radius]}: the member chunks within the radius of your chunk become a district of
 *       their own (pinned);</li>
 *   <li>{@code exclude} and {@code include}: your chunk is in no district, or detection decides again;</li>
 *   <li>{@code adopt [radius]}: industrial block entities around you become seeds (worlds made before the
 *       mod, structures placed by commands);</li>
 *   <li>{@code rescan}, {@code perf}.</li>
 * </ul>
 * No new items or blocks (D3). Every edit is an IND_EDIT (aux 4, command) for the districts it touches.
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
        if (level != null)
            ind.districts.rescanNow(level);
        say(ctx, "mic_rtsim: merged; " + ind.districts.all().size() + " districts now");
        return 1;
    }

    private static int split(CommandContext<CommandSourceStack> ctx, int radius) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        District d = find(ctx, ind, null);
        if (d == null)
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
        ind.districts.rescanNow(ctx.getSource().getLevel());
        say(ctx, "mic_rtsim: " + n + " chunks split off as district " + id);
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
        ind.districts.rescanNow(level);
        say(ctx, "mic_rtsim: chunk " + here + (exclude ? " is in no district now" : " is detected again"));
        return 1;
    }

    private static int adopt(CommandContext<CommandSourceStack> ctx, int radius) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        ServerLevel level = ctx.getSource().getLevel();
        int n = ind.districts.adopt(level, new ChunkPos(net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition())), radius);
        ind.districts.rescanNow(level);
        say(ctx, "mic_rtsim: " + n + " block entities adopted as seeds; " + ind.districts.all().size() + " districts");
        return n;
    }

    private static int rescan(CommandContext<CommandSourceStack> ctx) {
        Industry ind = industry(ctx);
        if (ind == null)
            return 0;
        ind.districts.rescanNow(ctx.getSource().getLevel());
        say(ctx, "mic_rtsim: " + ind.districts.all().size() + " districts; last detection " + ind.districts.lastRescanNanos / 1000 + " us");
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
                        + " us in all, last " + ind.districts.lastRescanNanos / 1000 + " us over " + ind.districts.lastRescanSlices + " ticks, longest tick "
                        + ind.districts.maxSliceNanos / 1000 + " us), declarations " + ind.districts.declared,
                "sweeps: " + s.sweeps + " (" + s.liveSweeps + " live), inventories read " + s.inventoriesRead + ", slots " + s.slotsRead
                        + ", read time " + s.readNanos / 1000 + " us (slowest inventory " + s.maxInventoryNanos / 1000 + " us), records "
                        + s.recordsSent + " sent, " + s.recordsDropped + " dropped",
                "reconciles: " + r.applied + " applied, " + r.duplicates + " duplicates, " + r.held() + " held, " + r.shortfalls
                        + " short deltas, " + r.blocksBroken + " blocks broken, " + r.blocksSkipped + " skipped",
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
