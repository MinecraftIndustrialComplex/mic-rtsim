package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import com.minecraftindustrialcomplex.mic_rtsim_sandworms.Probes;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

/**
 * Test 4: can a plugin drive it? On the real (flat) world, with a survival player standing on it and every chunk within 24 chunks
 * forced so a worm that spawns 100 to 200 blocks away has a chunk to stand in. Phases, one after another:
 * A. write the player's WormSign attachment over the threshold and wait for the mod's own tick handler to spawn a worm;
 * B. call {@code WarningSpawnHelper.spawnWormThumper(level, pos)} at a real thumper block;
 * C. the same at a block that is not a thumper (the stand-in for a pumpjack hole);
 * D. a thumper block with {@code thumping=true}, left alone, to see the mod's own thumper spawn a worm.
 * Each phase follows the worm for up to 450 ticks and records where it came from and what became of the target.
 */
@GameTestHolder(MicRtsimSandworms.MODID)
@PrefixGameTestTemplate(false)
public final class DriveTests {
    private DriveTests() {}

    private static final int R = 24;

    @GameTest(template = "arena", timeoutTicks = 8000, batch = "drive")
    public static void driveFromSim(GameTestHelper helper) {
        String test = "driveFromSim";
        if (!Kit.run(helper, test, true)) return;
        if (Kit.BIOME_DEFAULT) {
            Kit.say(test, "skipped", "needs the biome datapack: the mod's spawn helpers loop forever when no desert biome is within reach (see the research note)");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        ChunkPos c0 = new ChunkPos(o);
        List<ChunkPos> forced = new ArrayList<>();
        for (int dx = -R; dx <= R; dx++)
            for (int dz = -R; dz <= R; dz++)
                if (level.setChunkForced(c0.x + dx, c0.z + dz, true))
                    forced.add(new ChunkPos(c0.x + dx, c0.z + dz));
        AABB world = new AABB(o.getX() - 600, level.getMinBuildHeight(), o.getZ() - 600, o.getX() + 600, level.getMaxBuildHeight(), o.getZ() + 600);
        Vec3 stand = new Vec3(o.getX() + 0.5, o.getY() + 1, o.getZ() + 0.5);
        ServerPlayer player = Kit.trackedPlayer(level, stand, "worm-bait");
        player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(1000);
        player.setHealth(1000);
        BlockPos pad = o.offset(4, 0, 0);
        Kit.say(test, "setup", "player at " + Kit.v(stand) + ", " + forced.size() + " chunks forced, biome at the player " + level.getBiome(o).unwrapKey().map(k -> k.location().toString()).orElse("?"));
        int[] t = {0};
        int[] phase = {-1};
        int[] phaseStart = {0};
        Entity[] worm = {null};
        double[] minDist = {1e9};
        int[] firstHit = {-1};
        float[] hp = {1000};
        int[] spawnedAt = {-1};
        Probes.HITS.clear();
        Probes.recordHits = true;
        helper.onEachTick(() -> {
            if (phase[0] >= 4) return;
            if (phase[0] < 0) {
                // wait for the chunks to load: the player's own and a ring far out
                int loaded = 0;
                for (ChunkPos c : forced) if (level.getChunkSource().getChunkNow(c.x, c.z) != null) loaded++;
                if (t[0] % 100 == 0) Kit.say(test, "loading chunks, t=" + t[0], loaded + " of " + forced.size());
                if (loaded < forced.size() * 0.98 && t[0] < 1500) {
                    t[0]++;
                    return;
                }
                Kit.say(test, "chunks loaded", loaded + " of " + forced.size() + " after " + t[0] + " ticks");
                phase[0] = 0;
                phaseStart[0] = t[0];
                begin(test, 0, level, player, pad, worm, spawnedAt, t[0]);
            }
            int into = t[0] - phaseStart[0];
            // find the worm if it was spawned by the mod itself (A, D)
            if (worm[0] == null && (phase[0] == 0 || phase[0] == 3)) {
                List<? extends Entity> cs = Sw.chainsIn(level, world);
                if (!cs.isEmpty()) {
                    worm[0] = cs.get(0);
                    spawnedAt[0] = into;
                    Kit.say(test, label(phase[0]), "the mod spawned a worm after " + into + " ticks at " + Kit.v(worm[0].position()) + ", offset from the player " + Kit.v(worm[0].position().subtract(stand)));
                }
            }
            if ((phase[0] == 0 || phase[0] == 3) && into % 100 == 0 && worm[0] == null)
                Kit.say(test, label(phase[0]) + " +" + into, Sw.signState(level, player) + (phase[0] == 3 ? "; thumper: " + Sw.thumperState(level, pad) : ""));
            Vec3 head = worm[0] != null && worm[0].isAlive() ? Sw.head(worm[0]) : null;
            if (head != null) {
                minDist[0] = Math.min(minDist[0], head.distanceTo(stand));
                if (into % 50 == 0)
                    Kit.say(test, label(phase[0]) + " +" + into, "head " + Kit.v(head) + ", " + Kit.f(head.distanceTo(stand)) + " from the player, stage " + Sw.stage(worm[0]) + ", aggro " + (Sw.aggroTarget(worm[0]) != null)
                            + ", thumperTarget " + Sw.thumperTarget(worm[0]) + ", escaping " + Sw.field(worm[0], "escaping") + ", segments " + Sw.segmentsNonNull(worm[0]) + "/" + Sw.segmentCount(worm[0]));
            }
            if (phase[0] == 1 || phase[0] == 2) {
                if (into == 2 && worm[0] != null)
                    Kit.say(test, label(phase[0]) + " +2", "thumperTarget now " + Sw.thumperTarget(worm[0]) + " (set by the helper to " + Kit.v(pad.getCenter()) + ")");
            }
            if (player.getHealth() < hp[0] && firstHit[0] < 0) firstHit[0] = into;
            hp[0] = player.getHealth();
            int limit = phase[0] == 3 ? 1200 : phase[0] == 0 ? 250 : 450;
            if (into >= limit) {
                Kit.say(test, label(phase[0]) + " end", "worm " + (worm[0] == null ? "never appeared" : worm[0].isAlive() ? "alive" : "gone") + ", closest to the player " + (minDist[0] > 1e8 ? "-" : Kit.f(minDist[0])) + " m, player health " + player.getHealth()
                        + " (first lost at +" + firstHit[0] + "), pad block " + level.getBlockState(pad).getBlock().getDescriptionId() + ", hits " + Probes.HITS.size());
                for (Entity e : new ArrayList<>(Sw.chainsIn(level, world))) e.discard();
                for (Entity e : new ArrayList<>(level.getEntitiesOfClass(Entity.class, world, Sw::isSegment))) e.discard();
                level.removeBlock(pad, false);
                level.removeBlockEntity(pad);
                worm[0] = null;
                minDist[0] = 1e9;
                firstHit[0] = -1;
                player.setHealth(1000);
                hp[0] = 1000;
                Probes.HITS.clear();
                phase[0]++;
                phaseStart[0] = t[0];
                if (phase[0] >= 4) {
                    Probes.recordHits = false;
                    Kit.removeTrackedPlayer(player);
                    for (ChunkPos c : forced) level.setChunkForced(c.x, c.z, false);
                    helper.succeed();
                    return;
                }
                begin(test, phase[0], level, player, pad, worm, spawnedAt, t[0]);
            }
            t[0]++;
        });
    }

    private static String label(int phase) {
        return switch (phase) {
            case 0 -> "A wormsign attachment";
            case 1 -> "B spawnWormThumper at a thumper";
            case 2 -> "C spawnWormThumper at a non-thumper";
            default -> "D thumping thumper alone";
        };
    }

    private static void begin(String test, int phase, ServerLevel level, ServerPlayer player, BlockPos pad, Entity[] worm, int[] spawnedAt, int t) {
        Kit.say(test, label(phase), "begin");
        spawnedAt[0] = -1;
        switch (phase) {
            case 0 -> {
                Kit.say(test, "A wormsign before", Sw.wormSign(player));
                Sw.setWormSign(player, 4001);
            }
            case 1 -> {
                level.setBlock(pad, Sw.thumperBlock(false), 3);
                worm[0] = Sw.spawnWormThumper(level, pad);
                Kit.say(test, "B spawn", worm[0] == null ? "null" : "at " + Kit.v(worm[0].position()) + ", offset from the thumper " + Kit.v(worm[0].position().subtract(pad.getCenter())));
            }
            case 2 -> {
                level.setBlock(pad, Blocks.IRON_BLOCK.defaultBlockState(), 3);
                worm[0] = Sw.spawnWormThumper(level, pad);
                Kit.say(test, "C spawn", worm[0] == null ? "null" : "at " + Kit.v(worm[0].position()) + ", offset from the block " + Kit.v(worm[0].position().subtract(pad.getCenter())));
            }
            default -> level.setBlock(pad, Sw.thumperBlock(true), 3);
        }
    }
}
