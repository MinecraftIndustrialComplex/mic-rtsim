package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.phys.Vec3;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * The {@link Lurker}s plugins registered, by entity type id (a string, so a plugin's constructor can name
 * a type before the registries freeze, as {@code BodyKinds.registerStandIn} does).
 */
public final class Lurkers {
    private Lurkers() {}

    /** A body is held within this many blocks of its spot (blocks, PLACEHOLDER, MZB-360). */
    public static final int HOLD_RADIUS = 2;
    /** The default hold walks a body that strayed this far back to its spot (blocks, PLACEHOLDER). */
    public static final double STRAY = 4.0;

    private static final Map<String, Lurker> BY_TYPE = new LinkedHashMap<>();

    /** A type with no Lurker of its own: the restriction and the walk back, no strike, no roar. */
    public static final Lurker DEFAULT = new Lurker() {
        @Override
        public void hold(Mob mob, Vec3 spot) {
            if (!mob.hasRestriction())
                mob.restrictTo(net.minecraft.core.BlockPos.containing(spot), HOLD_RADIUS);
            if (mob.getTarget() == null && mob.distanceToSqr(spot) > STRAY * STRAY && mob.getNavigation().isDone())
                mob.getNavigation().moveTo(spot.x, spot.y, spot.z, 1.0);
        }

        @Override
        public boolean strike(Mob mob, Vec3 at) {
            return false;
        }

        @Override
        public void release(Mob mob) {
            mob.clearRestriction();
        }

        @Override
        public void roar(Mob mob) {}
    };

    /** Registers the Lurker of an entity type ({@code mowziesmobs:frostmaw}); a later call replaces an earlier one. */
    public static synchronized void register(String entityType, Lurker lurker) {
        BY_TYPE.put(entityType, lurker);
    }

    /** The Lurker for a body: its type's, else the default. */
    public static synchronized Lurker of(Mob mob) {
        Lurker l = BY_TYPE.get(BuiltInRegistries.ENTITY_TYPE.getKey(mob.getType()).toString());
        return l != null ? l : DEFAULT;
    }
}
