package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.Iterator;
import java.util.List;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.AABB;

/**
 * Villagers dragons took from villages nobody watched (TAKE_RESIDENT; docs/overnight-choices-equilibrium.md EQC556,
 * Ben's PP-2 (a)). The engine has already counted the villager out of its village; this removes the body: the
 * nearest adult, unnamed villager within the radius now if the chunk there is loaded, else the first such villager
 * that joins the level there. Held takes are not saved: a restart forgets them, and the village's next complete
 * census counts the villager again.
 */
public final class VillagerTakes {
    private record Take(ResourceKey<Level> dimension, int x, int z, int radius) {}

    private final List<Take> held = new ArrayList<>();
    /** Villagers removed this session (tests). */
    public int taken;

    /** TAKE_RESIDENT. */
    public void take(ServerLevel level, int x, int z, int radius) {
        if (level == null)
            return;
        if (level.isLoaded(new BlockPos(x, 64, z)) && removeNear(level, x, z, radius))
            return;
        held.add(new Take(level.dimension(), x, z, radius));
    }

    /** Takes held for villagers not yet loaded. */
    public int held() {
        return held.size();
    }

    /** A villager joining a level: the first within a held take's radius is the one taken (cancel its join). */
    public boolean takesOnJoin(Villager v) {
        if (held.isEmpty() || !takeable(v))
            return false;
        for (Iterator<Take> it = held.iterator(); it.hasNext();) {
            Take t = it.next();
            if (v.level().dimension() == t.dimension() && Math.abs(v.getX() - t.x()) <= t.radius()
                    && Math.abs(v.getZ() - t.z()) <= t.radius()) {
                it.remove();
                taken++;
                return true;
            }
        }
        return false;
    }

    public void clear() {
        held.clear();
    }

    private boolean removeNear(ServerLevel level, int x, int z, int radius) {
        AABB box = new AABB(x - radius, level.getMinBuildHeight(), z - radius, x + radius + 1, level.getMaxBuildHeight(),
                z + radius + 1);
        List<Villager> near = level.getEntitiesOfClass(Villager.class, box, VillagerTakes::takeable);
        if (near.isEmpty())
            return false;
        Villager v = near.stream()
                .min(Comparator.comparingDouble((Villager e) -> e.distanceToSqr(x + 0.5, e.getY(), z + 0.5))
                        .thenComparing(e -> e.getUUID()))
                .orElseThrow();
        v.ejectPassengers();
        v.stopRiding();
        v.discard();
        taken++;
        return true;
    }

    private static boolean takeable(Villager v) {
        return v.isAlive() && !v.isBaby() && !v.hasCustomName() && !v.isPassenger();
    }
}
