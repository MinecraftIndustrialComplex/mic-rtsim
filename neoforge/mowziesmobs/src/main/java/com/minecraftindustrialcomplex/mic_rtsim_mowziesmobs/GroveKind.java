package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthana;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

import java.util.List;
import java.util.function.Predicate;

/**
 * The Umvuthana's grove (MOWZIE.md §4.5, §4.9): its warriors are the jar's grove warriors
 * ({@link EntityUmvuthanaMinion}, {@code mowziesmobs:umvuthana}), restricted 25 blocks round the fire
 * pit's centre. The Umvuthi is the census's chief fields only, never one of its bodies: its slot and its
 * lines are M9's (MZB-209, MZB-220).
 */
public final class GroveKind implements CampKind {
    public static final GroveKind INSTANCE = new GroveKind();

    /** A warrior is restricted this far round the centre, as the jar's own ({@code UmvuthanaGrovePieces.java:507}). */
    public static final int RESTRICT = 25;
    /** A spawned warrior appears this far from the centre, on the side away from the nearest player (§4.9). */
    public static final double EDGE = 20.0;
    /** The players a spawn turns away from: within this of the centre (a warrior's tracking range is 128). */
    public static final double SEEN_RANGE = 256.0;

    private GroveKind() {}

    /** A grove warrior, exactly: not the followers, cranes or raptors that share its base class. */
    @Override
    public boolean isBody(Entity e) {
        return e.getType() == EntityHandler.UMVUTHANA_MINION.get();
    }

    @Override
    public String bodyName(Entity e) {
        return MicRtsimMowziesMobs.WARRIOR;
    }

    @Override
    public boolean isChief(Entity e) {
        return false;
    }

    /** The Umvuthi standing in the footprint (not a puppet): present, on its throne, its health and direction. */
    @Override
    public Chief chief(ServerLevel level, Camps.Site site, List<Entity> footprint) {
        for (Entity e : footprint) {
            if (e instanceof EntityUmvuthi u && u.isAlive() && !RtsimAttachments.isTagged(u))
                return new Chief(ConflictRecords.CampCensus.CHIEF | ConflictRecords.CampCensus.THRONE, u.getHealth(), u.getX(), u.getY(),
                        u.getZ(), u.getDirectionData());
        }
        return Chief.NONE;
    }

    /** No chief lines: the Umvuthi's are M9's (MZB-209). */
    @Override
    public boolean chiefLines() {
        return false;
    }

    /**
     * As the fire pit's own: made, {@code finalizeSpawn} with STRUCTURE called directly as the jar's
     * worldgen does (its weapon rolled there, its mask in its constructor; MZB-219).
     */
    @Override
    @SuppressWarnings("deprecation")
    public Mob make(ServerLevel level, Camps.Site site, String body, int lineFlags, double x, double y, double z, float yaw) {
        if (!MicRtsimMowziesMobs.WARRIOR.equals(body))
            return null;
        EntityUmvuthanaMinion m = EntityHandler.UMVUTHANA_MINION.get().create(level);
        if (m == null)
            return null;
        m.moveTo(x, y, z, yaw, 0f);
        m.setYHeadRot(yaw);
        m.finalizeSpawn(level, level.getCurrentDifficultyAt(m.blockPosition()), MobSpawnType.STRUCTURE, null);
        return m;
    }

    /** The places tried for one spawn, as the jar's fire pit does (MZB-243). */
    public static final int SPAWN_TRIES = 20;

    /**
     * At the grove's edge on the side away from the nearest player within {@link #SEEN_RANGE}, so it
     * walks in (§4.9, "Coming home"); with none, where the fire pit places its own (2 to 11 blocks out). Up
     * to {@link #SPAWN_TRIES} places are tried, as the jar does: each on loaded ground, free of collisions
     * and of fluid for the warrior's box (the later tries jitter round the first). Null when none passes:
     * the line is answered stale.
     */
    @Override
    public Vec3 spawnAt(ServerLevel level, Camps.Site site, String body, int lineFlags) {
        Vec3 c = site.spot;
        Player near = null;
        double nd = SEEN_RANGE * SEEN_RANGE;
        for (Player p : level.players()) {
            if (p.isSpectator())
                continue;
            double d = p.distanceToSqr(c);
            if (d < nd) {
                nd = d;
                near = p;
            }
        }
        boolean away = near != null && (near.getX() != c.x || near.getZ() != c.z);
        double base = away ? Math.atan2(c.z - near.getZ(), c.x - near.getX()) : 0; // away from the player
        EntityType<?> type = EntityHandler.UMVUTHANA_MINION.get();
        for (int i = 0; i < SPAWN_TRIES; i++) {
            double angle, dist;
            if (away) {
                angle = i == 0 ? base : base + (level.random.nextDouble() - 0.5) * (Math.PI / 3);
                dist = i == 0 ? EDGE : EDGE - level.random.nextDouble() * 4;
            } else {
                angle = level.random.nextDouble() * Math.PI * 2;
                dist = 2 + level.random.nextInt(10);
            }
            int x = (int) Math.floor(c.x + Math.cos(angle) * dist), z = (int) Math.floor(c.z + Math.sin(angle) * dist);
            if (!level.hasChunk(x >> 4, z >> 4))
                continue;
            int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z);
            if (y <= level.getMinBuildHeight())
                continue;
            AABB box = type.getDimensions().makeBoundingBox(x + 0.5, y, z + 0.5);
            if (level.noCollision(box) && !level.containsAnyLiquid(box))
                return new Vec3(x + 0.5, y, z + 0.5);
        }
        return null;
    }

    @Override
    public boolean holdSpawn(ServerLevel level, Camps.Site site, String body, int lineFlags) {
        return false;
    }

    /** Restricted round the centre (the jar's own restore it from their save; made and adopted ones are set here). */
    @Override
    public void joined(Camps.Site site, Mob body) {
        body.restrictTo(BlockPos.containing(site.spot), RESTRICT);
    }

    @Override
    public void copy(Mob from, Mob to) {
        to.setHealth(Math.min(from.getHealth(), to.getMaxHealth()));
        if (from instanceof EntityUmvuthana a && to instanceof EntityUmvuthana b) {
            b.setMask(a.getMaskType());
            b.setWeapon(a.getWeapon());
        }
    }

    /** The grove's kin: every Umvuthana and the Umvuthi (the killing signal). */
    @Override
    public Predicate<Entity> kin() {
        return e -> e instanceof EntityUmvuthana || e instanceof EntityUmvuthi;
    }
}
