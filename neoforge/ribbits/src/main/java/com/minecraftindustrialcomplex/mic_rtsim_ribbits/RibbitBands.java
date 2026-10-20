package com.minecraftindustrialcomplex.mic_rtsim_ribbits;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

import java.util.HashMap;
import java.util.Map;

/**
 * Touring bands (C7, P16's recommendation, choice G16). A band is one puppet group (C25): a master and
 * its musicians. It travels on the engine's STEER commands and plays when it stops: within
 * {@link #STOP} blocks of its last waypoint, or where it was spawned if it was never steered.
 *
 * <p><b>How Ribbits picks a master</b> (verified in the 4.1.6 bytecode, the question PLAN.md §8.8 left
 * UNVERIFIED): {@code RibbitPlayMusicGoal.canUse} (daytime only) scans ribbits within 64 x 16 x 64
 * blocks that are playing, and takes the master of each one it finds (the last wins); if none, the
 * ribbit becomes its own master in {@code start}. So a band walking into a village whose own band is
 * playing would split up and join it. The plugin steers it: every {@code setMasterRibbit} the goal
 * makes for a band member names the band's leader instead ({@code RibbitPlayMusicGoalMixin}), and a
 * band that is still travelling does not start playing. Village ribbits may still join a touring band
 * (they adopt its master), which is Ribbits' own behaviour.
 */
public final class RibbitBands {
    private RibbitBands() {}

    public static final String BAND = "mic_rtsim_ribbits:ribbit_band";
    /** Within this many blocks of its waypoint a band has stopped and may play (PLACEHOLDER). */
    public static final double STOP = 6;

    private static final Map<Long, Vec3> TARGETS = new HashMap<>();

    static void steered(RibbitEntity leader, Vec3 target) {
        PuppetGroup g = RtsimApi.group(leader);
        if (g != null && g.leader() == leader)
            TARGETS.put(g.agent, target);
    }

    public static Vec3 target(long agent) {
        return TARGETS.get(agent);
    }

    /** The band a ribbit plays in, or null when it is not a touring band's member. */
    public static PuppetGroup band(Entity ribbit) {
        PuppetGroup g = RtsimApi.group(ribbit);
        return g != null && BAND.equals(g.kind.name()) ? g : null;
    }

    /** The master a band member must play around: the band's leader, or null for a ribbit of no band. */
    public static RibbitEntity master(Entity ribbit) {
        PuppetGroup g = band(ribbit);
        return g != null && g.leader() instanceof RibbitEntity leader ? leader : null;
    }

    /** True when the band has stopped (or never left), so its members may play. */
    public static boolean atStop(PuppetGroup band) {
        Entity leader = band.leader();
        if (leader == null)
            return false;
        Vec3 t = TARGETS.get(band.agent);
        return t == null || leader.position().distanceToSqr(t) <= STOP * STOP;
    }

    static void forget(long agent) {
        TARGETS.remove(agent);
    }
}
