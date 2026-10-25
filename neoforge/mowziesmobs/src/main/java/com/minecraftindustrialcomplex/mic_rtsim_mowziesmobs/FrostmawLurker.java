package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.ilexiconn.llibrary.server.animation.AnimationHandler;
import com.ilexiconn.llibrary.server.animation.IAnimatedEntity;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Lurker;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.navigation.PathNavigation;
import net.minecraft.world.phys.Vec3;

import java.util.Map;
import java.util.WeakHashMap;

/**
 * How a Frostmaw lies in wait by a rail (M8, MZB-361): it holds its spot awake, swipes the riders of a
 * carriage that comes within its reach, and roars when a train its party robbed has gone. The jar does the
 * rest: the swipe is {@link EntityFrostmaw#SWIPE_ANIMATION} with its own area attack (135 degrees, 6.5 blocks,
 * at animation tick 9) and the roar its {@link EntityFrostmaw#ROAR_ANIMATION}; both are public animations, sent
 * as the jar sends them.
 *
 * <p>An awake Frostmaw with a target walks at it every tick, after its goals ran; the core calls
 * {@link #hold} once every level ticked, so the path set here is the one followed next. The restriction
 * keeps it to the spot (and gates its targeting, so the core gives a held puppet a target, MZB-362).
 */
public final class FrostmawLurker implements Lurker {
    /** The restriction radius round the spot (blocks). */
    public static final int RADIUS = 2;
    /** Within this many blocks of its spot it stands; beyond, it walks back. */
    public static final double STAND = 1.5;
    /** The swipe hits at this animation tick (AnimationAreaAttackAI damage frame): the body faces the strike until then. */
    public static final int SWIPE_HIT_TICK = 9;

    /** Path searches the hold asked for, and live paths it dropped (D1 diagnostics, {@link Lurker#pathSearches}). */
    private long searches, dropped;

    /** The bearing (yaw) of a strike in progress, by body: dropped in {@link #hold} once its swing is past the hit frame. */
    private final Map<Mob, Float> bearings = new WeakHashMap<>();

    @Override
    public void hold(Mob mob, Vec3 spot) {
        if (!(mob instanceof EntityFrostmaw f) || !f.isAlive())
            return;
        // Awake for good (the jar skips its sleep and its waking roar for an always-active Frostmaw)
        if (!f.isAlwaysActive())
            f.setAlwaysActive(true);
        BlockPos centre = BlockPos.containing(spot);
        if (!f.hasRestriction() || !centre.equals(f.getRestrictCenter()) || f.getRestrictRadius() != RADIUS)
            f.restrictTo(centre, RADIUS);

        if (f.getAnimation() == EntityFrostmaw.SWIPE_ANIMATION && f.getAnimationTick() <= SWIPE_HIT_TICK) {
            Float yaw = bearings.get(f);
            if (yaw != null) {
                // Its swipe turns toward its target (the nearest player) until the hit frame; the strike wins
                f.yBodyRot = yaw;
                f.yBodyRotO = yaw;
                f.setYRot(yaw);
                f.setYHeadRot(yaw);
            }
            stop(f);
            return;
        }
        // The strike's swing is over (any other animation, or past the hit frame): its bearing must not turn the
        // Frostmaw's own swipes at a player later (MZB-361)
        bearings.remove(f);
        if (f.getAnimation() != IAnimatedEntity.NO_ANIMATION) {
            stop(f);
            return;
        }
        double dx = spot.x - f.getX(), dz = spot.z - f.getZ();
        if (dx * dx + dz * dz <= STAND * STAND) {
            stop(f);
            return;
        }
        // Walk back: one search when the navigation is not already on its way to the spot, not one a tick (D1)
        PathNavigation nav = f.getNavigation();
        BlockPos heading = nav.getTargetPos();
        if (nav.isDone() || heading == null || heading.distSqr(centre) > 2.0) {
            searches++;
            nav.moveTo(spot.x, spot.y, spot.z, 1.0);
        }
    }

    /** Stops the navigation (a swing or a roar is not walked out of, and within its stand it stays), counting a live path it drops. */
    private void stop(Mob f) {
        PathNavigation nav = f.getNavigation();
        if (!nav.isDone())
            dropped++;
        nav.stop();
    }

    @Override
    public long pathSearches() {
        return searches;
    }

    @Override
    public long pathsDropped() {
        return dropped;
    }

    @Override
    public boolean strike(Mob mob, Vec3 at, Vec3 motion) {
        if (!(mob instanceof EntityFrostmaw f) || !f.isAlive() || f.getAnimation() != IAnimatedEntity.NO_ANIMATION)
            return false;
        // The swing lands SWIPE_HIT_TICK ticks from now, and a carriage moves meanwhile: aim where the point will be
        Vec3 aim = at.add(motion.scale(SWIPE_HIT_TICK));
        float yaw = (float) (Math.atan2(aim.z - f.getZ(), aim.x - f.getX()) * 57.29577951308232 - 90.0);
        bearings.put(f, yaw);
        f.yBodyRot = yaw;
        f.setYRot(yaw);
        f.setYHeadRot(yaw);
        AnimationHandler.INSTANCE.sendAnimationMessage(f, EntityFrostmaw.SWIPE_ANIMATION);
        return true;
    }

    @Override
    public void release(Mob mob) {
        bearings.remove(mob);
        if (!(mob instanceof EntityFrostmaw f))
            return;
        f.setAlwaysActive(false);
        f.clearRestriction();
    }

    @Override
    public void roar(Mob mob) {
        if (mob instanceof EntityFrostmaw f && f.isAlive() && f.getAnimation() == IAnimatedEntity.NO_ANIMATION)
            AnimationHandler.INSTANCE.sendAnimationMessage(f, EntityFrostmaw.ROAR_ANIMATION);
    }
}
