package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Shared helpers for the mic_rtsim GameTests ({@code ./gametest.sh}).
 *
 * <p>The tests drive the server's real bridge session, whose engine is the {@link StubEngine}: a
 * test scripts commands into the stub, waits for the bridge to apply them on a later tick (the bridge
 * never waits, so neither can a test), and reads back the observations the stub received. Tests in
 * the default batch run at the same time against the one session, so each uses its own agent ids
 * and only looks at records for them.
 */
public final class RtsimTests {
    private RtsimTests() {}

    /**
     * {@code data/mic_rtsim/structure/arena.nbt}: an 11 x 5 x 11 box, polished andesite floor and a
     * two-block glass ring (tools/make_test_structure.py).
     */
    public static final String TEMPLATE = "arena";

    private static final AtomicLong NEXT_AGENT = new AtomicLong(10_000);

    public static long newAgent() {
        return NEXT_AGENT.getAndIncrement();
    }

    public static RtsimSession session() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s;
    }

    public static StubEngine stub() {
        if (!(session().engine() instanceof StubEngine stub))
            throw new GameTestAssertException("the engine is not the stub: " + session().engine().describe());
        return stub;
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
    public static List<StubEngine.Received> recordsFor(StubEngine stub, int kind, long agent) {
        List<StubEngine.Received> out = new ArrayList<>();
        for (StubEngine.Received r : stub.received(kind))
            if (r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    /** Every journaled record for one agent, in the order the stub received them. */
    public static List<StubEngine.Received> allFor(StubEngine stub, long agent) {
        List<StubEngine.Received> out = new ArrayList<>();
        for (StubEngine.Received r : stub.received())
            if (Obs.isJournaled(r.kind()) && r.kind() != Obs.TRANSFER && r.kind() != Obs.COMMAND_RESULT
                    && r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    public static long gameTimeOf(StubEngine.Received r) {
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
