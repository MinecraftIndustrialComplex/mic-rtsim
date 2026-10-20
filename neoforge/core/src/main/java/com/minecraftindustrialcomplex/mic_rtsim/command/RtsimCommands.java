package com.minecraftindustrialcomplex.mic_rtsim.command;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
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
                .then(Commands.literal("stub")
                        .then(Commands.literal("caravan").executes(RtsimCommands::stubCaravan))));
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
                "modules: " + Integer.toBinaryString(RtsimConfig.effectiveMask()));
        ctx.getSource().sendSuccess(() -> Component.literal(text), false);
        return 1;
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
}
