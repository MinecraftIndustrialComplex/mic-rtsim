package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology;
import com.minecraftindustrialcomplex.mic_rtsim.topology.ToroidalWorldProbe;
import com.minecraftindustrialcomplex.mic_rtsim.topology.TopologyRecords;
import com.minecraftindustrialcomplex.mic_rtsim.topology.TopologySources;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The world's topology at the bridge (docs/choices-topology.md, docs/boundary.md §11), on whichever
 * engine the session runs (native or stub). The GameTest world is a vanilla superflat, so Toroidal World
 * is stood in for by a fake shape probe ({@link TopologySources#setToroidalProbe}): an x wrap of 64 blocks
 * whose east edge is a few blocks east of the arena. Both tests swap the probe server-wide: their own batch.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class TopologyGameTests {
    private TopologyGameTests() {}

    /** The fake wrap's period: small, so both sides of the seam are loaded at once. */
    static final int PERIOD = 64;

    /** A fake Toroidal World looping x over [x0, x0 + PERIOD) on the test's level only. */
    static ToroidalWorldProbe fakeWrap(ServerLevel level, long x0) {
        return l -> l == level
                ? new ToroidalWorldProbe.Axis[]{new ToroidalWorldProbe.Axis(true, x0, PERIOD), new ToroidalWorldProbe.Axis(false, 0, 0)}
                : null;
    }

    /** The TOPOLOGY records for a dimension the engine received, latest last. */
    static List<ScriptedEngine.Received> topologies(int dim) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : RtsimTests.engine().received(TopologyRecords.TOPOLOGY))
            if (r.aux() == dim)
                out.add(r);
        return out;
    }

    static List<ChunkPos> forceLoad(ServerLevel level, int x0, int x1, int z) {
        List<ChunkPos> out = new ArrayList<>();
        for (int cx = x0 >> 4; cx <= x1 >> 4; cx++)
            for (int cz = (z - 16) >> 4; cz <= (z + 16) >> 4; cz++) {
                if (level.setChunkForced(cx, cz, true))
                    out.add(new ChunkPos(cx, cz));
                level.getChunk(cx, cz);
            }
        return out;
    }

    static void unforce(ServerLevel level, List<ChunkPos> chunks) {
        for (ChunkPos c : chunks)
            level.setChunkForced(c.x, c.z, false);
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300, batch = "mic_rtsim_topology")
    public static void aLevelsTopologyReachesTheEngineWhenItChanges(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        var session = RtsimTests.session();
        int dim = session.dimId(level.dimension());
        LevelTopology before = session.topology(level);
        record("topology", "the GameTest level", before);
        check("a vanilla level is flat inside its world border: " + before, before.isFlat() && before.hasRect);
        check("every level's topology went out at the session's start", session.topologies().sent() >= 1);
        long x0 = (long) Math.floor(RtsimTests.centre(helper).x) - 40;
        TopologySources.setToroidalProbe(fakeWrap(level, x0));
        RtsimApi.topologyChanged();
        helper.startSequence()
                .thenWaitUntil(() -> {
                    List<ScriptedEngine.Received> recs = topologies(dim);
                    check("the engine was sent the wrap", !recs.isEmpty());
                    var b = recs.get(recs.size() - 1).view();
                    check("an x wrap", b.get(TopologyRecords.KIND) == 1);
                    check("from Toroidal World", b.get(TopologyRecords.SOURCE) == 1);
                    check("over the fake's lap", b.getLong(TopologyRecords.X0) == x0 && b.getLong(TopologyRecords.X1) == x0 + PERIOD);
                })
                .thenExecute(() -> {
                    check("the bridge steers by it", session.topology(level).kind == LevelTopology.Kind.X_WRAP);
                    TopologySources.setToroidalProbe(ToroidalWorldProbe.reflective());
                    RtsimApi.topologyChanged();
                })
                .thenWaitUntil(() -> {
                    List<ScriptedEngine.Received> recs = topologies(dim);
                    check("and flat again when the shape goes", recs.get(recs.size() - 1).view().get(TopologyRecords.KIND) == 0);
                })
                .thenSucceed();
    }

    /**
     * A leader steered toward a target across the seam heads for the target's image on its side; when the
     * wrap folds it back into the lap (as Toroidal World does at the end of a tick, with no event), it is
     * steered on from where it is: still the same live puppet, never captured, never a stale copy (TP11).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400, batch = "mic_rtsim_topology")
    public static void aLeaderFoldedAcrossTheSeamIsSteeredOnNotCaptured(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        var session = RtsimTests.session();
        Vec3 c = RtsimTests.centre(helper);
        long cx = (long) Math.floor(c.x);
        long x0 = cx - 40; // the lap [cx - 40, cx + 24): the east edge 24 blocks east of the arena
        List<ChunkPos> forced = forceLoad(level, (int) (x0 - 16), (int) (x0 + PERIOD + 16), (int) Math.floor(c.z));
        TopologySources.setToroidalProbe(fakeWrap(level, x0));
        RtsimApi.topologyChanged();
        long agent = RtsimTests.newAgent();
        RtsimTests.engine().spawnGroup(agent, 1, "minecraft:wandering_trader", RtsimTests.dimension(helper), c.x, c.y, c.z,
                List.of(ScriptedEngine.MemberSpec.plain(0)));
        long[] before = new long[3];
        double target = x0 + 4.5; // canonical, at the lap's west end: 28 blocks east of the arena, through the seam
        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenWaitUntil(() -> check("the wrap is in force", session.topology(level).kind == LevelTopology.Kind.X_WRAP))
                .thenExecute(() -> {
                    before[0] = session.capturedGroups();
                    before[1] = session.staleDiscarded();
                    before[2] = session.resteered();
                    RtsimTests.engine().steerGroup(agent, 1, target, c.y, c.z, 0.5f);
                })
                .thenWaitUntil(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    check("the steer arrived", g != null && g.steerImage != null);
                    record("topology", "steered toward", g.steerImage);
                    check("toward the target's image on the leader's side, east through the seam: " + g.steerImage,
                            Math.abs(g.steerImage.x - (target + PERIOD)) < 1e-6);
                })
                .thenExecute(() -> {
                    // The leader crosses the east edge and the wrap folds it into the lap's west end.
                    Entity leader = RtsimTests.member(RtsimTests.group(agent), 0);
                    leader.moveTo(x0 + PERIOD + 1.5 - PERIOD, leader.getY(), leader.getZ());
                })
                .thenWaitUntil(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    check("still embodied", g != null);
                    check("re-steered from where it is now: " + g.steerImage, Math.abs(g.steerImage.x - target) < 1e-6);
                })
                .thenExecute(() -> {
                    long resteered = session.resteered() - before[2];
                    record("topology", "leaders re-steered after the fold", resteered);
                    check("re-steered once at least", resteered >= 1);
                    check("never captured", session.capturedGroups() == before[0]);
                    check("never taken for a stale copy", session.staleDiscarded() == before[1]);
                    RtsimTests.engine().despawnGroup(agent, 1);
                    TopologySources.setToroidalProbe(ToroidalWorldProbe.reflective());
                    RtsimApi.topologyChanged();
                    unforce(level, forced);
                })
                .thenWaitUntil(() -> check("despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }
}
