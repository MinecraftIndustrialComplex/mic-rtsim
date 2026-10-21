package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import net.jelly.sandworm_mod.block.ModBlocks;
import net.jelly.sandworm_mod.entity.IK.ChainSegment;
import net.jelly.sandworm_mod.entity.IK.worm.WormChainEntity;
import net.jelly.sandworm_mod.entity.ModEntities;
import net.jelly.sandworm_mod.helper.WarningSpawnHelper;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

import java.lang.reflect.Field;
import java.util.List;

/**
 * Everything that names a Sandworms class, kept out of the @GameTest holder classes so a server without the mod can still load them
 * (the start-time baselines). All signatures use vanilla types.
 */
final class Sw {
    private Sw() {}

    /** A worm not yet in the level: set its targets, then {@link #add}. */
    static Entity create(ServerLevel level, Vec3 at) {
        WormChainEntity w = new WormChainEntity(ModEntities.WORM_CHAIN.get(), level);
        w.moveTo(at.x, at.y, at.z);
        return w;
    }

    static void add(ServerLevel level, Entity worm) {
        level.addFreshEntity(worm);
    }

    static void aggro(Entity worm, LivingEntity target) {
        ((WormChainEntity) worm).setAggroTargetEntity(target);
    }

    static void thumperTarget(Entity worm, Vec3 pos) {
        ((WormChainEntity) worm).thumperTarget = pos;
    }

    static Vec3 thumperTarget(Entity worm) {
        return ((WormChainEntity) worm).thumperTarget;
    }

    static LivingEntity aggroTarget(Entity worm) {
        return ((WormChainEntity) worm).aggroTargetEntity;
    }

    static int segmentCount(Entity worm) {
        return ((WormChainEntity) worm).segmentCount;
    }

    static int segmentsNonNull(Entity worm) {
        int n = 0;
        for (ChainSegment s : ((WormChainEntity) worm).segments)
            if (s != null && s.isAlive()) n++;
        return n;
    }

    /** The head segment's position, or null before the first tick has built the body. */
    static Vec3 head(Entity worm) {
        WormChainEntity w = (WormChainEntity) worm;
        if (w.segments.isEmpty() || w.segments.size() < w.segmentCount) return null;
        ChainSegment h = w.segments.get(w.segmentCount - 1);
        return h == null ? null : h.position();
    }

    static Vec3 tail(Entity worm) {
        WormChainEntity w = (WormChainEntity) worm;
        if (w.segments.isEmpty()) return null;
        ChainSegment h = w.segments.get(0);
        return h == null ? null : h.position();
    }

    static int stage(Entity worm) {
        return ((WormChainEntity) worm).stage;
    }

    static Object field(Entity worm, String name) {
        try {
            Field f = WormChainEntity.class.getDeclaredField(name);
            f.setAccessible(true);
            return f.get(worm);
        } catch (ReflectiveOperationException e) {
            return "?" + e;
        }
    }

    /** Zeroes the head's velocity (a protected field the mod's own AI integrates), so the worm holds its place: a steady-state load. */
    static void freeze(Entity worm) {
        try {
            Field f = net.jelly.sandworm_mod.entity.IK.KinematicChainEntity.class.getDeclaredField("targetV");
            f.setAccessible(true);
            f.set(worm, Vec3.ZERO);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    static boolean isChain(Entity e) {
        return e instanceof WormChainEntity;
    }

    static boolean isSegment(Entity e) {
        return e instanceof ChainSegment;
    }

    /** All the mod's entities (chain and segments) in a box, as vanilla entities. */
    static List<? extends Entity> segmentsIn(ServerLevel level, AABB box) {
        return level.getEntitiesOfClass(ChainSegment.class, box);
    }

    static List<? extends Entity> chainsIn(ServerLevel level, AABB box) {
        return level.getEntitiesOfClass(WormChainEntity.class, box);
    }

    static BlockState thumperBlock(boolean thumping) {
        BlockState s = ModBlocks.THUMPER.get().defaultBlockState();
        return thumping ? s.setValue(net.jelly.sandworm_mod.block.ThumperBlock.THUMPING, true) : s;
    }

    static boolean isThumper(Level level, BlockPos p) {
        return level.getBlockState(p).is(ModBlocks.THUMPER.get());
    }

    /** The mod's own spawn helpers (they pick a point 100 to 200 blocks away in each of x and z, and spin until it finds a "desert" biome). */
    static void spawnWorm(net.minecraft.world.entity.player.Player player) {
        WarningSpawnHelper.spawnWorm(player);
    }

    static Entity spawnWormThumper(Level level, BlockPos pos) {
        return WarningSpawnHelper.spawnWormThumper(level, pos);
    }

    /** The WormSign attachment of a player: add to it (what jumping and sprinting do). */
    static int wormSign(net.minecraft.world.entity.player.Player p) {
        return p.getData(net.jelly.sandworm_mod.capabilities.wormsign.ModAttachments.WORM_SIGN.get()).getWS();
    }

    static void setWormSign(net.minecraft.world.entity.player.Player p, int value) {
        p.getData(net.jelly.sandworm_mod.capabilities.wormsign.ModAttachments.WORM_SIGN.get()).setWS(value);
    }

    /** Everything the player-side spawn check reads: the sign, whether the spot counts as desert, sky light, the respawn and sign timers. */
    static String signState(net.minecraft.server.level.ServerLevel level, net.minecraft.world.entity.player.Player p) {
        var ws = p.getData(net.jelly.sandworm_mod.capabilities.wormsign.ModAttachments.WORM_SIGN.get());
        return "sign " + ws.getWS() + ", desert " + net.jelly.sandworm_mod.helper.BiomeHelper.isDesertBiome(level, p.blockPosition())
                + ", sky light " + level.getBrightness(net.minecraft.world.level.LightLayer.SKY, p.blockPosition()) + ", canRespawn " + ws.canRespawn()
                + ", signTimer " + ws.getSignTimer() + ", ticked " + p.tickCount;
    }

    /** The thumper block entity's private counters, by reflection. */
    static String thumperState(Level level, BlockPos pos) {
        var be = level.getBlockEntity(pos);
        if (be == null) return "no block entity";
        StringBuilder sb = new StringBuilder();
        for (String n : new String[] {"wormSign", "pauseTicks", "thumping"}) {
            try {
                Field f = be.getClass().getDeclaredField(n);
                f.setAccessible(true);
                sb.append(n).append(' ').append(f.get(be)).append(", ");
            } catch (ReflectiveOperationException e) {
                sb.append(n).append(" ?, ");
            }
        }
        return sb.toString();
    }

    static void blast(Entity worm) {
        ((WormChainEntity) worm).blastHit();
    }
}
