package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import net.neoforged.fml.loading.FMLPaths;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Shared helpers for the mic_rtsim GameTests ({@code ./gametest.sh}).
 *
 * <p>The tests drive the server's real bridge session, whose engine is a {@link ScriptedEngine}: the
 * Rust engine ({@link NativeEngine}, the default) or the {@link StubEngine}
 * ({@code ./gametest.sh -PrtsimEngine=stub}). A test orders commands from the engine, waits for the
 * bridge to apply them on a later tick (the bridge never waits, so neither can a test), and reads back
 * the observations the engine received. Tests in the default batch run at the same time against the
 * one session, so each uses its own agent ids and only looks at records for them.
 */
public final class RtsimTests {
    private RtsimTests() {}

    /**
     * {@code data/mic_rtsim/structure/arena.nbt}: an 11 x 5 x 11 box, polished andesite floor and a
     * two-block glass ring (tools/make_test_structure.py).
     */
    public static final String TEMPLATE = "arena";

    /**
     * Agent ids start at a random base each server run. The GameTest world is reused (B37), so a run
     * starts by replaying what the last one journaled after its final hand-over (its shutdown
     * captures), and any journal the engine's pages do not include (a world first run on the stub,
     * then on the native engine): those records name the last run's agents, and must never be
     * mistaken for this run's. Far below the {@code /rtsim stub caravan} ids (2^40).
     */
    private static final AtomicLong NEXT_AGENT = new AtomicLong(
            10_000 + 1_000_000L * java.util.concurrent.ThreadLocalRandom.current().nextInt(1, 1 << 19));

    public static long newAgent() {
        return NEXT_AGENT.getAndIncrement();
    }

    public static RtsimSession session() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s;
    }

    /** The session's engine, as the test surface both engines implement. */
    public static ScriptedEngine engine() {
        if (!(session().engine() instanceof ScriptedEngine engine))
            throw new GameTestAssertException("the session's engine takes no scripted orders: " + session().engine().describe());
        return engine;
    }

    /**
     * A new engine of the same kind as the session's, with empty pages, for a test that runs its own
     * sessions: the native engine on a fresh directory under {@code <game dir>/mic_rtsim-test/}, or a
     * stub with no pages file.
     */
    public static ScriptedEngine freshEngine() {
        ScriptedEngine kind = engine();
        if (kind instanceof NativeEngine)
            return NativeEngine.open(FMLPaths.GAMEDIR.get().resolve("mic_rtsim-test").resolve(UUID.randomUUID().toString()),
                    0, NativeEngine.FLAG_TEST | NativeEngine.FLAG_NO_FSYNC);
        if (kind instanceof StubEngine)
            return new StubEngine(StubEngine.Pages.EMPTY, null, StubEngine.DEFAULT_CAPACITY);
        throw new GameTestAssertException("no fresh engine of the kind " + kind.getClass().getSimpleName());
    }

    /** The arena's middle, standing on the floor, in absolute coordinates. */
    public static Vec3 centre(GameTestHelper helper) {
        return helper.absoluteVec(new Vec3(5.5, 1, 5.5));
    }

    public static String dimension(GameTestHelper helper) {
        return helper.getLevel().dimension().location().toString();
    }

    public static PuppetGroup group(long agent) {
        return session().registry().byAgent(agent);
    }

    /** Waits (inside a thenWaitUntil) for the group to be embodied, and returns it. */
    public static PuppetGroup embodied(long agent) {
        PuppetGroup g = group(agent);
        if (g == null)
            throw new GameTestAssertException("agent " + agent + " is not embodied yet");
        return g;
    }

    /** Journaled records of one kind for one agent (every journaled agent field is at offset 24). */
    public static List<ScriptedEngine.Received> recordsFor(ScriptedEngine engine, int kind, long agent) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine.received(kind))
            if (r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    /** Every journaled record for one agent, in the order the engine received them. */
    public static List<ScriptedEngine.Received> allFor(ScriptedEngine engine, long agent) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine.received())
            if (Obs.isJournaled(r.kind()) && r.kind() != Obs.TRANSFER && r.kind() != Obs.COMMAND_RESULT
                    && r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    public static long gameTimeOf(ScriptedEngine.Received r) {
        return r.view().getLong(Obs.GAME_TIME);
    }

    public static void check(String what, boolean ok) {
        if (!ok)
            throw new GameTestAssertException(what);
    }

    public static void record(String test, String what, Object value) {
        MicRtsim.LOGGER.info("[gametest] {}: {} = {}", test, what, value);
    }

    public static Entity member(PuppetGroup g, int index) {
        PuppetGroup.Member m = g.members[index];
        check("member " + index + " of agent " + g.agent + " is still in the group", !m.gone && m.entity != null);
        return m.entity;
    }
}
