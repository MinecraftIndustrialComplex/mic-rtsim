package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.phys.Vec3;

import java.util.List;

/**
 * The Frostmaw's den (MOWZIE.md §4.2, §4.9): a camp of one, its Frostmaw its chief and its pool's one
 * member. Its body is the jar's {@link EntityFrostmaw}, asleep on its spot with its crystal or without.
 */
public final class DenKind implements CampKind {
    public static final DenKind INSTANCE = new DenKind();

    /** A home Frostmaw is restricted to its den's spot this far each time it joins (§4.9, PLACEHOLDER). */
    public static final int RESTRICT = 8;
    /** A Frostmaw's spawn waits while a player is this near its spot: its tracking range (§4.9, test 5). */
    public static final double HOLD_RANGE = 160.0;

    private DenKind() {}

    @Override
    public boolean isBody(Entity e) {
        return e instanceof EntityFrostmaw;
    }

    @Override
    public String bodyName(Entity e) {
        return MicRtsimMowziesMobs.FROSTMAW;
    }

    @Override
    public boolean isChief(Entity e) {
        return e instanceof EntityFrostmaw;
    }

    /** The Frostmaw nearest the spot, not a puppet: present, its crystal and health. */
    @Override
    public Chief chief(ServerLevel level, Camps.Site site, List<Entity> footprint) {
        EntityFrostmaw best = null;
        double bd = Double.MAX_VALUE;
        for (Entity e : footprint) {
            if (!(e instanceof EntityFrostmaw f) || !f.isAlive() || RtsimAttachments.isTagged(f))
                continue;
            double d = f.distanceToSqr(site.spot);
            if (d < bd) {
                bd = d;
                best = f;
            }
        }
        if (best == null)
            return Chief.NONE;
        int flags = ConflictRecords.CampCensus.CHIEF | (best.getHasCrystal() ? ConflictRecords.CampCensus.CRYSTAL : 0);
        return new Chief(flags, best.getHealth(), 0, 0, 0, 0f);
    }

    /**
     * As the template's own: made, {@code finalizeSpawn} with STRUCTURE called directly as the jar's
     * worldgen does (no FinalizeSpawnEvent, MZB-219), then its crystal as the line says, asleep.
     */
    @Override
    @SuppressWarnings("deprecation")
    public Mob make(ServerLevel level, Camps.Site site, String body, int lineFlags, double x, double y, double z, float yaw) {
        if (!MicRtsimMowziesMobs.FROSTMAW.equals(body))
            return null;
        EntityFrostmaw f = EntityHandler.FROSTMAW.get().create(level);
        if (f == null)
            return null;
        f.moveTo(x, y, z, yaw, 0f);
        f.setYHeadRot(yaw);
        f.finalizeSpawn(level, level.getCurrentDifficultyAt(f.blockPosition()), MobSpawnType.STRUCTURE, null);
        f.setHasCrystal((lineFlags & ConflictRecords.CampReconcile.LINE_CRYSTAL) != 0);
        f.setActive(false);
        return f;
    }

    @Override
    public Vec3 spawnAt(ServerLevel level, Camps.Site site, String body, int lineFlags) {
        return site.spot;
    }

    @Override
    public boolean holdSpawn(ServerLevel level, Camps.Site site, String body, int lineFlags) {
        return Camps.playerNear(level, site.spot, HOLD_RANGE);
    }

    /** The jar never restricts a Frostmaw, and a restriction is not saved: restricted at each join (§4.9). */
    @Override
    public void joined(Camps.Site site, Mob body) {
        body.restrictTo(BlockPos.containing(site.spot), RESTRICT);
    }

    @Override
    public void copy(Mob from, Mob to) {
        to.setHealth(Math.min(from.getHealth(), to.getMaxHealth()));
        if (from instanceof EntityFrostmaw a && to instanceof EntityFrostmaw b)
            b.setHasCrystal(a.getHasCrystal());
    }
}
