package com.minecraftindustrialcomplex.mic_rtsim.herd;

import net.minecraft.world.entity.PathfinderMob;
import net.minecraft.world.entity.ai.goal.Goal;
import net.minecraft.world.phys.Vec3;

import java.util.EnumSet;

/**
 * Keeps a herd member with its herd (docs/boundary.md §8.5, choice Y14): while the member is more than
 * {@link HerdBridge#LEASH} blocks from its slot (its place in the group as the group walks toward the
 * engine's waypoint), it walks there; otherwise vanilla's own goals run (grazing, strolling, following
 * a parent). Added at priority {@link #PRIORITY} with the MOVE flag, so panic, breeding and tempting
 * still win, and strolling and eating wait while the member catches up. Goals are never saved: the goal
 * lives only on the puppet, and a taken member loses it.
 *
 * <p>Sheep, cows, pigs and horses run goals. Goats run a brain instead, and are steered through their
 * {@code WALK_TARGET} memory by the bridge's pass ({@link HerdBridge}).
 */
final class HerdFollowGoal extends Goal {
    /** Below panic (1), breeding (2) and tempting (3) for sheep and cows; PLACEHOLDER. */
    static final int PRIORITY = 4;
    /** Ticks between new paths while catching up. PLACEHOLDER. */
    static final int REPATH = 20;

    private final PathfinderMob mob;
    private final HerdBridge.Member member;
    private int nextPath;

    HerdFollowGoal(PathfinderMob mob, HerdBridge.Member member) {
        this.mob = mob;
        this.member = member;
        setFlags(EnumSet.of(Flag.MOVE));
    }

    @Override
    public boolean canUse() {
        return member.lag(mob) > HerdBridge.LEASH;
    }

    @Override
    public boolean canContinueToUse() {
        return member.lag(mob) > HerdBridge.LEASH / 2;
    }

    @Override
    public void start() {
        nextPath = 0;
    }

    @Override
    public void stop() {
        mob.getNavigation().stop();
    }

    @Override
    public boolean requiresUpdateEveryTick() {
        return true;
    }

    @Override
    public void tick() {
        if (--nextPath > 0 && !mob.getNavigation().isDone())
            return;
        nextPath = REPATH;
        Vec3 slot = member.slot(mob);
        mob.getNavigation().moveTo(slot.x, slot.y, slot.z, 1.0);
    }
}
