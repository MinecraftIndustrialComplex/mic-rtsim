package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;

/**
 * Where and when a renewal arrival may appear (EQUILIBRIUM.md §2.2, "Arrival"; slice S6, EQC174, EQC177, EQC182): the one
 * place the rule "never into a watched site" lives, so a village's settlers, a roosting dragon's newcomer and a grove's
 * walk-in (slices S5 and S9) read the same radius and find a standable spot the same way. No state of its own.
 *
 * <p>A site is watched when a player (not a spectator) is within {@link #WATCH_BLOCKS} of it: a chunk normally loads at
 * the server's view distance, past that, so a body placed as it loads is there before it can be seen, and a player inside
 * the radius at a load (a login or a teleport) could see it appear.
 */
public final class Arrivals {
    private Arrivals() {}

    /** §2.2's 128 blocks (TUNABLE). */
    public static final int WATCH_BLOCKS = 128;

    /** Whether a player is within {@code radius} blocks (straight line) of {@code point}. */
    public static boolean watched(ServerLevel level, BlockPos point, double radius) {
        double r2 = radius * radius;
        for (ServerPlayer p : level.players())
            if (!p.isSpectator() && p.distanceToSqr(point.getX() + 0.5, point.getY(), point.getZ() + 0.5) <= r2)
                return true;
        return false;
    }

    /**
     * Whether a player is within {@code radius} blocks of a footprint, measured in the horizontal plane to the nearest
     * point of its box (a house at the far edge of a village is as visible as the centre).
     */
    public static boolean watched(ServerLevel level, BoundingBox footprint, double radius) {
        double r2 = radius * radius;
        for (ServerPlayer p : level.players()) {
            if (p.isSpectator())
                continue;
            double dx = Math.max(Math.max(footprint.minX() - p.getX(), 0), p.getX() - (footprint.maxX() + 1));
            double dz = Math.max(Math.max(footprint.minZ() - p.getZ(), 0), p.getZ() - (footprint.maxZ() + 1));
            if (dx * dx + dz * dz <= r2)
                return true;
        }
        return false;
    }

    /** Whether every player (not a spectator) is at least {@code distance} blocks from {@code point}. */
    public static boolean farFromPlayers(ServerLevel level, BlockPos point, double distance) {
        return !watched(level, point, distance);
    }

    /**
     * The standable surface in the column of ({@code x}, {@code z}) when its chunk is loaded and entity-ticking: the
     * motion-blocking height, a sturdy block below, no liquid and room for a body 0.6 wide and 1.95 high. Null otherwise.
     */
    public static BlockPos standableSurface(ServerLevel level, int x, int z) {
        if (level.getChunkSource().getChunkNow(x >> 4, z >> 4) == null)
            return null;
        int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z);
        BlockPos at = new BlockPos(x, y, z);
        if (!level.isPositionEntityTicking(at))
            return null;
        BlockPos below = at.below();
        if (!level.getBlockState(below).isFaceSturdy(level, below, Direction.UP))
            return null;
        if (!level.getFluidState(below).isEmpty() || !level.getFluidState(at).isEmpty())
            return null;
        var box = new net.minecraft.world.phys.AABB(x + 0.2, y, z + 0.2, x + 0.8, y + 1.95, z + 0.8);
        return level.noCollision(box) && !level.containsAnyLiquid(box) ? at : null;
    }

    /**
     * The first standable spot on the ray from {@code from} toward ({@code toX}, {@code toZ}) at one of {@code distances}
     * (tried in order, blocks from {@code from}) that is entity-ticking and at least {@code clear} blocks from every
     * player; null when none is. A zero ray points along +x. The target is in the plain coordinates of the level (the engine
     * sends a settlers' source as the village's own offset, which near a toroidal frame's seam lies past it, EQC199): the ray
     * is a straight line in them and never wraps.
     */
    public static BlockPos spotOnRay(ServerLevel level, BlockPos from, double toX, double toZ, int[] distances, double clear) {
        double dx = toX - from.getX(), dz = toZ - from.getZ();
        double len = Math.hypot(dx, dz);
        if (len < 1e-6) {
            dx = 1;
            dz = 0;
            len = 1;
        }
        dx /= len;
        dz /= len;
        for (int d : distances) {
            BlockPos s = standableSurface(level, (int) Math.floor(from.getX() + dx * d), (int) Math.floor(from.getZ() + dz * d));
            if (s != null && farFromPlayers(level, s, clear))
                return s;
        }
        return null;
    }

    /** Horizontal distance between an entity and a block position. */
    public static double horizontal(Entity e, BlockPos p) {
        return Math.hypot(e.getX() - (p.getX() + 0.5), e.getZ() - (p.getZ() + 0.5));
    }

    /** The chunk of a position, for callers that ask {@code areEntitiesLoaded}. */
    public static long chunkKey(BlockPos p) {
        return ChunkPos.asLong(p.getX() >> 4, p.getZ() >> 4);
    }
}
