package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.goal.Goal;
import net.minecraft.world.phys.Vec3;

import java.util.EnumSet;

/**
 * Keeps a party member at its place in its stack as the stack's anchor walks (MZB-106): while it has no
 * target and is more than {@link ConflictBridge#LEASH} blocks from its place, it walks there. Added at
 * priority {@value #PRIORITY} with the MOVE flag, below the illagers' attack goals (2 to 4), so a member
 * with a target fights, and above strolling (8), so an idle member keeps up. Goals are never saved: it
 * lives only on the puppet.
 */
final class ConflictFollowGoal extends Goal {
    static final int PRIORITY = 5;
    /** Ticks between new paths while catching up. PLACEHOLDER. */
    static final int REPATH = 20;

    private final Mob mob;
    private final ConflictBridge.Slot slot;
    private int nextPath;

    ConflictFollowGoal(Mob mob, ConflictBridge.Slot slot) {
        this.mob = mob;
        this.slot = slot;
        setFlags(EnumSet.of(Flag.MOVE));
    }

    @Override
    public boolean canUse() {
        return mob.getTarget() == null && slot.lag(mob) > ConflictBridge.LEASH;
    }

    @Override
    public boolean canContinueToUse() {
        return mob.getTarget() == null && slot.lag(mob) > ConflictBridge.LEASH / 2;
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
        Vec3 p = slot.place(mob);
        mob.getNavigation().moveTo(p.x, p.y, p.z, 1.0);
    }
}
