package com.minecraftindustrialcomplex.mic_rtsim.command;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.context.CommandContext;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;
import net.minecraft.world.phys.Vec3;

import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

/**
 * {@code /rtsim} (PLAN.md §6.5). For now: {@code perf} (the bridge's counters) and {@code stub caravan}
 * (order the engine, stub or native, to spawn a human caravan next to you, to see the puppet protocol
 * in a real game; the name is from when the stub was the only engine).
 */
public final class RtsimCommands {
    private RtsimCommands() {}

    /** Agent ids the {@code stub caravan} command hands out; far above anything a test uses. */
    private static final AtomicLong NEXT_DEBUG_AGENT = new AtomicLong(1L << 40);

    public static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
        dispatcher.register(Commands.literal("rtsim")
                .requires(src -> src.hasPermission(2))
                .then(Commands.literal("perf").executes(RtsimCommands::perf))
                .then(Commands.literal("village").executes(RtsimCommands::village))
                .then(Commands.literal("stub")
                        .then(Commands.literal("caravan").executes(RtsimCommands::stubCaravan))
                        .then(Commands.literal("market").executes(RtsimCommands::stubMarket))
                        .then(Commands.literal("gossip").executes(RtsimCommands::stubGossip))));
    }

    private static int perf(CommandContext<CommandSourceStack> ctx) {
        RtsimSession s = RtsimSession.current();
        if (s == null) {
            String why = RtsimSession.startFailure();
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no session" + (why == null ? "" : ": " + why)));
            return 0;
        }
        BoundarySession b = s.boundary();
        String text = String.join("\n",
                "engine: " + s.engine().describe() + (s.failed() ? " (bridge FAILED)" : ""),
                "hand-overs: " + b.handover().submits() + " submitted, " + b.handover().ticksWaiting() + " ticks waited on the engine",
                "journal: " + b.journal().size() + " unacknowledged (" + b.journal().bytes() + " bytes), last applied command " + b.journal().lastAppliedCmdSeq(),
                "puppets: " + s.registry().size() + " groups; spawned " + s.spawnedGroups() + ", rejected " + s.rejectedSpawns()
                        + ", captured " + s.capturedGroups() + ", taken " + s.takes() + ", died " + s.deaths()
                        + ", removed by others " + s.removedByOther() + ", stale copies discarded " + s.staleDiscarded(),
                "puppet pass: " + s.passNanos() / 1000 + " us last tick; dropped latest-wins records " + b.droppedRecords(),
                "modules: " + Integer.toBinaryString(RtsimConfig.effectiveMask()),
                "phase 1: " + s.phase1.describe() + enginePhase1(s),
                "far feed: " + com.minecraftindustrialcomplex.mic_rtsim.far.FarFeed.describe() + "; legs " + s.farLegs.describe(),
                com.minecraftindustrialcomplex.mic_rtsim.habitat.HabitatSampler.INSTANCE.summary());
        ctx.getSource().sendSuccess(() -> Component.literal(text), false);
        return 1;
    }

    /**
     * What the native engine's phase 1 world holds, beside what Java counts: after a sim reset the two
     * village counts say whether the engine was told again (runbook 8.6 of docs/deploy-rehearsal-live.md).
     */
    private static String enginePhase1(RtsimSession s) {
        if (!(s.engine() instanceof NativeEngine n))
            return "";
        long[] p = n.phase1Stats();
        return "; in the engine: villages " + p[NativeEngine.P1_VILLAGES] + ", inputs " + p[NativeEngine.P1_INPUTS]
                + ", world tick " + p[NativeEngine.P1_WORLD_TICK];
    }

    private static int stubCaravan(CommandContext<CommandSourceStack> ctx) {
        RtsimSession s = RtsimSession.current();
        if (s == null || !(s.engine() instanceof ScriptedEngine engine)) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: the engine takes no scripted orders"));
            return 0;
        }
        Vec3 at = ctx.getSource().getPosition().add(4, 0, 0);
        long agent = NEXT_DEBUG_AGENT.getAndIncrement();
        engine.spawnGroup(agent, 1, BodyKinds.HUMAN_CARAVAN, ctx.getSource().getLevel().dimension().location().toString(),
                at.x, at.y, at.z, List.of(
                        ScriptedEngine.MemberSpec.plain(0),
                        new ScriptedEngine.MemberSpec(1, 0f, -1, -1, List.of(new ScriptedEngine.ItemSpec(0, "minecraft:emerald", 12))),
                        ScriptedEngine.MemberSpec.plain(2)));
        ctx.getSource().sendSuccess(() -> Component.literal("mic_rtsim: the engine will spawn caravan agent " + agent), false);
        return 1;
    }

    // ---- phase 1 --------------------------------------------------------------------------------

    private static com.minecraftindustrialcomplex.mic_rtsim.village.Village villageHere(CommandContext<CommandSourceStack> ctx, RtsimSession s) {
        return s.phase1.villages.at(ctx.getSource().getLevel().dimension(), net.minecraft.core.BlockPos.containing(ctx.getSource().getPosition()));
    }

    /** {@code /rtsim village}: the adopted village you stand in, its market and what it feels about you. */
    private static int village(CommandContext<CommandSourceStack> ctx) {
        RtsimSession s = RtsimSession.current();
        if (s == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no session"));
            return 0;
        }
        var v = villageHere(ctx, s);
        if (v == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no adopted village here (" + s.phase1.villages.size() + " adopted)"));
            return 0;
        }
        StringBuilder text = new StringBuilder();
        text.append("village ").append(Long.toHexString(v.id)).append(" (").append(v.culture).append(", ").append(v.structure).append(")\n")
                .append("box ").append(v.box).append(", meeting point ").append(v.meeting.toShortString())
                .append(v.meetingFromPoi ? " (a meeting POI)" : " (the start piece)").append("\n")
                .append(v.pieces.size()).append(" pieces, ").append(v.houses.size()).append(" houses the sim built, ")
                .append(v.gossip.size()).append(" gossip entries waiting for residents\n");
        var book = s.phase1.markets.known(v.id);
        if (book == null)
            text.append("no market snapshot yet");
        else
            text.append("market: currency ").append(book.currency()).append(", treasury ").append(book.treasury() / 1000.0)
                    .append(book.cashRefused() ? " (refuses cash)" : "").append(", ").append(book.pendingLocals())
                    .append(" local trades not in the snapshot yet");
        var sentiment = s.phase1.sentiments.get(v.id);
        var player = ctx.getSource().getPlayer();
        if (sentiment != null && player != null)
            text.append("\nsentiment toward you: ").append(sentiment.of(player.getUUID()))
                    .append(sentiment.dislikes(player.getUUID()) ? " (disliked)" : sentiment.friend(player.getUUID()) ? " (a friend)" : "");
        ctx.getSource().sendSuccess(() -> Component.literal(text.toString()), false);
        return 1;
    }

    /** {@code /rtsim stub market}: the stub sends a demonstration market for the village you stand in. */
    private static int stubMarket(CommandContext<CommandSourceStack> ctx) {
        RtsimSession s = RtsimSession.current();
        if (s == null || !(s.engine() instanceof StubEngine stub)) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: the engine is not the stub"));
            return 0;
        }
        var v = villageHere(ctx, s);
        if (v == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no adopted village here"));
            return 0;
        }
        boolean ribbit = "ribbit".equals(v.culture);
        // Demonstration numbers only (thousandths; millionths of a coin per item), not a balance.
        stub.market(v.id, s.boundary().sentObsSeq(), ribbit ? "minecraft:amethyst_shard" : "minecraft:emerald", 8_000, 40_000, 1, 2, List.of(
                new StubEngine.GoodSpec("minecraft:wheat", 300_000, 400_000, 50_000, 3, 4),
                new StubEngine.GoodSpec("minecraft:bread", 60_000, 40_000, 180_000, 3, 4),
                new StubEngine.GoodSpec("minecraft:potato", 200_000, 300_000, 40_000, 3, 4),
                new StubEngine.GoodSpec("minecraft:carrot", 200_000, 300_000, 45_000, 3, 4),
                new StubEngine.GoodSpec("minecraft:slime_ball", 40_000, 40_000, 120_000, 3, 4)));
        ctx.getSource().sendSuccess(() -> Component.literal("mic_rtsim: the stub will send a demonstration market for village " + Long.toHexString(v.id)), false);
        return 1;
    }

    /** {@code /rtsim stub gossip}: the stub writes a robbery heard as news (minor_negative 25) about you into the village. */
    private static int stubGossip(CommandContext<CommandSourceStack> ctx) {
        RtsimSession s = RtsimSession.current();
        var player = ctx.getSource().getPlayer();
        if (s == null || !(s.engine() instanceof StubEngine stub) || player == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: needs the stub and a player"));
            return 0;
        }
        var v = villageHere(ctx, s);
        if (v == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no adopted village here"));
            return 0;
        }
        long op = java.util.concurrent.ThreadLocalRandom.current().nextLong() & Long.MAX_VALUE;
        stub.writeGossip(op, v.id, player.getUUID(), 2, 25, 3, 0);
        ctx.getSource().sendSuccess(() -> Component.literal("mic_rtsim: the stub will write minor_negative 25 about you into village " + Long.toHexString(v.id)), false);
        return 1;
    }
}
