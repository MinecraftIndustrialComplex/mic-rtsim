package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.gametest;

import com.jesz.createdieselgenerators.CDGConfig;
import com.jesz.createdieselgenerators.CDGTags;
import com.jesz.createdieselgenerators.world.OilChunksSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.api.OilSource;
import com.minecraftindustrialcomplex.mic_rtsim.oil.OilSources;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.CdgFirstLook;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.CdgOilSource;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.DieselOilPlugin;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.MicRtsimDieselGenerators;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.biome.MultiNoiseBiomeSource;
import net.minecraft.world.level.biome.MultiNoiseBiomeSourceParameterLists;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.NoiseGeneratorSettings;
import net.minecraft.world.level.levelgen.RandomState;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.Set;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_dieselgenerators on Create Diesel Generators 1.3.15's own jar. */
@GameTestHolder(MicRtsimDieselGenerators.MODID)
@PrefixGameTestTemplate(false)
public final class DieselGameTests {
    private DieselGameTests() {}

    /**
     * A pumpjack hole's chunk oil is IND_DEPOSIT_SAMPLE (plugin 2) in millibuckets; IND_DEPOSIT_DELTA
     * -1,234 is applied to {@code OilChunksSavedData} at once (APPLIED), the same op again is DUPLICATE, and
     * a delta for a far chunk nobody has loaded is applied too (level SavedData, F15).
     */
    @GameTest(template = "yard", batch = "diesel_1", timeoutTicks = 600)
    public static void chunkOilIsADepositWrittenAtOnce(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos chest = inChunk(helper, 3, 3);
        BlockPos hole = inChunk(helper, 8, 8);
        ChunkPos cp = new ChunkPos(hole);
        ChunkPos far = new ChunkPos(cp.x + 9000, cp.z + 9000);
        long[] id = {0};
        String good = Deposits.name(DieselOilPlugin.OIL, helper.getLevel().dimension(), cp.x, cp.z);
        String farGood = Deposits.name(DieselOilPlugin.OIL, helper.getLevel().dimension(), far.x, far.z);
        long op = engineOp(), opFar = engineOp();
        helper.startSequence()
                .thenExecute(() -> {
                    OilChunksSavedData.setChunkOilAmount(helper.getLevel(), cp, 5_000_000);
                    OilChunksSavedData.setChunkOilAmount(helper.getLevel(), far, 6_000_000);
                    set(helper, chest, com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                    set(helper, hole, block("createdieselgenerators:pumpjack_hole").defaultBlockState());
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                    List<ByteBuffer> s = received(IndRecords.IND_DEPOSIT_SAMPLE, r -> r.getLong(IndRecords.DepositSample.DISTRICT) == d.id
                            && good.equals(javaName(r.getInt(IndRecords.DepositSample.GOOD))));
                    check("an IND_DEPOSIT_SAMPLE of the chunk's oil", !s.isEmpty());
                    ByteBuffer last = s.get(s.size() - 1);
                    check("plugin 2 (Diesel oil)", aux(last) == IndRecords.DepositPlugin.DIESEL_OIL);
                    check("5,000,000 mB: " + last.getLong(IndRecords.DepositSample.REMAINING), last.getLong(IndRecords.DepositSample.REMAINING) == 5_000_000);
                    record("chunkOilIsADepositWrittenAtOnce", "sample (" + engineName() + ")", good + " = " + last.getLong(IndRecords.DepositSample.REMAINING));
                    depositDelta(op, IndRecords.DepositPlugin.DIESEL_OIL, helper.getLevel(), cp.x, cp.z, good, -1234);
                    depositDelta(opFar, IndRecords.DepositPlugin.DIESEL_OIL, helper.getLevel(), far.x, far.z, farGood, -500_000);
                })
                .thenWaitUntil(() -> {
                    check("applied", commandResults(op).contains(Obs.CommandResult.APPLIED));
                    check("the far one applied", commandResults(opFar).contains(Obs.CommandResult.APPLIED));
                    int now = OilChunksSavedData.getChunkOilAmount(helper.getLevel(), cp);
                    int farNow = OilChunksSavedData.getChunkOilAmount(helper.getLevel(), far);
                    record("chunkOilIsADepositWrittenAtOnce", "after", now + " here, " + farNow + " far away (chunk loaded: "
                            + (helper.getLevel().getChunkSource().getChunkNow(far.x, far.z) != null) + ")");
                    check("4,998,766 left: " + now, now == 4_998_766);
                    check("5,500,000 left far away: " + farNow, farNow == 5_500_000);
                    depositDelta(op, IndRecords.DepositPlugin.DIESEL_OIL, helper.getLevel(), cp.x, cp.z, good, -1234);
                })
                .thenWaitUntil(() -> check("the same op again is a duplicate", commandResults(op).contains(Obs.CommandResult.DUPLICATE_OP)))
                .thenExecute(() -> check("still 4,998,766", OilChunksSavedData.getChunkOilAmount(helper.getLevel(), cp) == 4_998_766))
                .thenSucceed();
    }

    /**
     * The oil gate's sampler on Diesel's own method (OILGATE.md §4.1, §7 test 4): the core's oil source for
     * the level is this plugin's ({@code getBaseOilAmount}, source 2), and what one sample costs on chunks
     * in memory and on chunks never generated. An ungenerated chunk answers as it does once generated (a
     * survey ahead of generation and the new-chunk sample agree). The GameTest world's flat preset has one
     * fixed biome, so its ungenerated samples are a floor; the 12,800 uncached lookups a real overworld pays
     * are timed on vanilla's overworld multi-noise source for the same seed (what {@code ServerLevel.getBiome}
     * reads for a chunk not in memory).
     */
    @GameTest(template = "yard", batch = "diesel_oil_bench", timeoutTicks = 1200)
    public static void oilSamplesCostLittleOnGeneratedChunks(GameTestHelper helper) {
        String test = "oilSamplesCostLittleOnGeneratedChunks";
        ServerLevel level = helper.getLevel();
        OilSource src = OilSources.forLevel(level);
        check("the level's oil source is Diesel's (" + (src == null ? "none" : src.name()) + ")", src instanceof CdgOilSource && src.source() == OilSource.CDG);
        ChunkPos here = new ChunkPos(helper.absolutePos(BlockPos.ZERO));
        java.util.Random rnd = new java.util.Random();
        int ox = 200_000 + rnd.nextInt(400_000), oz = 200_000 + rnd.nextInt(400_000);
        long[] gen = new long[3], ungen = new long[3], noise = new long[3]; // samples, nanos, max
        List<ChunkPos> far = new java.util.ArrayList<>();
        for (int i = 0; i < 48; i++)
            far.add(new ChunkPos(ox + 3 * i, oz));
        int[] farAmount = new int[far.size()];
        int[] cursor = {0}, withOil = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    for (int dx = -3; dx <= 3; dx++)
                        for (int dz = -3; dz <= 3; dz++) {
                            ChunkPos p = new ChunkPos(here.x + dx, here.z + dz);
                            if (level.getChunkSource().getChunk(p.x, p.z, ChunkStatus.BIOMES, false) == null)
                                continue;
                            long t0 = System.nanoTime();
                            int a = src.baseOil(level, p);
                            time(gen, System.nanoTime() - t0);
                            check("the same as getBaseOilAmount", a == OilChunksSavedData.getBaseOilAmount(level, p));
                            if (a > 0)
                                withOil[0]++;
                        }
                    check("chunks in memory sampled (" + gen[0] + ")", gen[0] >= 9);
                })
                .thenExecuteFor(far.size() / 4 + 1, () -> {
                    for (int k = 0; k < 4 && cursor[0] < far.size(); k++, cursor[0]++) {
                        ChunkPos p = far.get(cursor[0]);
                        check("never generated", level.getChunkSource().getChunk(p.x, p.z, ChunkStatus.BIOMES, false) == null);
                        long t0 = System.nanoTime();
                        farAmount[cursor[0]] = src.baseOil(level, p);
                        time(ungen, System.nanoTime() - t0);
                    }
                })
                .thenExecute(() -> {
                    check("every far chunk sampled", ungen[0] == far.size());
                    int agree = 0;
                    for (int i = 0; i < 6; i++) {
                        ChunkPos p = far.get(i * 7);
                        level.getChunk(p.x, p.z); // generates it now
                        int after = src.baseOil(level, p);
                        check("chunk " + p + " answers " + farAmount[i * 7] + " before it generates and " + after + " after", after == farAmount[i * 7]);
                        agree++;
                    }
                    // What a real overworld's ungenerated chunk costs: vanilla's overworld multi-noise biomes for this seed,
                    // through BiomeManager as ServerLevel.getBiome reads them for a chunk not in memory, at CDG's 12,800 points.
                    var reg = level.registryAccess();
                    MultiNoiseBiomeSource biomes = MultiNoiseBiomeSource.createFromPreset(
                            reg.registryOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST).getHolderOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD));
                    RandomState state = RandomState.create(reg.asGetterLookup(), NoiseGeneratorSettings.OVERWORLD, level.getSeed());
                    BiomeManager manager = new BiomeManager((x, y, z) -> biomes.getNoiseBiome(x, y, z, state.sampler()), BiomeManager.obfuscateSeed(level.getSeed()));
                    BlockPos.MutableBlockPos m = new BlockPos.MutableBlockPos();
                    int distinct = 0;
                    for (int i = 0; i < 8; i++) {
                        ChunkPos p = far.get(far.size() - 1 - i);
                        List<Holder<Biome>> seen = new java.util.ArrayList<>();
                        long t0 = System.nanoTime();
                        for (int x = p.getMinBlockX(); x <= p.getMaxBlockX(); x++)
                            for (int y = 60; y < 110; y++)
                                for (int z = p.getMinBlockZ(); z <= p.getMaxBlockZ(); z++) {
                                    Holder<Biome> b = manager.getBiome(m.set(x, y, z));
                                    if (!seen.contains(b))
                                        seen.add(b);
                                }
                        time(noise, System.nanoTime() - t0);
                        distinct += seen.size();
                    }
                    record(test, "source", src.name() + " (source " + src.source() + "), " + withOil[0] + " of " + gen[0] + " chunks in memory hold oil");
                    record(test, "in memory", gen[0] + " samples: mean " + us(gen) + " us, max " + gen[2] / 1000 + " us");
                    record(test, "never generated (flat preset, one biome)", ungen[0] + " samples: mean " + us(ungen) + " us, max " + ungen[2] / 1000 + " us");
                    record(test, "a real overworld's uncached biomes", noise[0] + " chunks x 12,800 lookups on vanilla's overworld multi-noise source: mean "
                            + us(noise) + " us a chunk, max " + noise[2] / 1000 + " us (" + distinct / Math.max(1, noise[0]) + " biomes a chunk)");
                    record(test, "agree", agree + " chunks give the same oil before and after they generate");
                })
                .thenSucceed();
    }

    /**
     * The first look before {@code getBaseOilAmount} (OGB-5, {@link CdgFirstLook}). On vanilla's overworld
     * multi-noise biomes for the level's seed (the GameTest world is one flat biome), around the edge of an
     * oil biome and at random far chunks: the look's 504 noise cells hold every biome CDG's 12,800 points read
     * through {@code BiomeManager} (so a chunk the look skips has no oil biome for CDG), and the look passes
     * every chunk CDG would find an oil biome in. On the level, under the pack's rule (CDG's normal multiplier
     * 0, set for the check and put back), the source answers what {@code getBaseOilAmount} does, chunk for
     * chunk; under CDG's own defaults the look stands aside.
     */
    @GameTest(template = "yard", batch = "diesel_first_look", timeoutTicks = 1200)
    public static void theFirstLookAgreesWithGetBaseOilAmount(GameTestHelper helper) {
        String test = "theFirstLookAgreesWithGetBaseOilAmount";
        ServerLevel level = helper.getLevel();
        OilSource src = OilSources.forLevel(level);
        check("the level's oil source is Diesel's", src instanceof CdgOilSource);
        var reg = level.registryAccess();
        MultiNoiseBiomeSource biomes = MultiNoiseBiomeSource.createFromPreset(
                reg.registryOrThrow(Registries.MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST).getHolderOrThrow(MultiNoiseBiomeSourceParameterLists.OVERWORLD));
        RandomState state = RandomState.create(reg.asGetterLookup(), NoiseGeneratorSettings.OVERWORLD, level.getSeed());
        BiomeManager.NoiseBiomeSource cells = (x, y, z) -> biomes.getNoiseBiome(x, y, z, state.sampler());
        BiomeManager manager = new BiomeManager(cells, BiomeManager.obfuscateSeed(level.getSeed()));
        java.util.Random rnd = new java.util.Random();
        List<ChunkPos> chunks = new java.util.ArrayList<>();
        int edge = 13 * 5, far = 25; // around the oil biome's cell, and at random
        int[] cursor = {0};
        long[] n = new long[5]; // chunks, with an oil biome for CDG, skipped by the look, passed without one, look nanos
        long[] exactNanos = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    // An oil biome's cell, walking 64 blocks a step at y 64 from a random start.
                    int qx = (200_000 + rnd.nextInt(400_000)) >> 2, qz = (200_000 + rnd.nextInt(400_000)) >> 2;
                    int found = -1;
                    for (int i = 0; i < 20_000 && found < 0; i++)
                        if (cells.getNoiseBiome(qx + 16 * i, 16, qz).is(CDGTags.OIL_BIOMES))
                            found = i;
                    check("an oil biome on the walk (" + CDGTags.OIL_BIOMES.location() + ")", found >= 0);
                    ChunkPos c = new ChunkPos((qx + 16 * found) >> 2, qz >> 2);
                    for (int dx = -6; dx <= 6; dx++)
                        for (int dz = -2; dz <= 2; dz++)
                            chunks.add(new ChunkPos(c.x + dx * 2, c.z + dz * 2));
                    for (int i = 0; i < far; i++)
                        chunks.add(new ChunkPos(rnd.nextInt(100_000) - 50_000, rnd.nextInt(100_000) - 50_000));
                    check("the chunks to read (" + chunks.size() + ")", chunks.size() == edge + far);
                })
                .thenExecuteFor((edge + far) / 3 + 1, () -> {
                    BlockPos.MutableBlockPos m = new BlockPos.MutableBlockPos();
                    for (int k = 0; k < 3 && cursor[0] < chunks.size(); k++, cursor[0]++) {
                        ChunkPos p = chunks.get(cursor[0]);
                        long t0 = System.nanoTime();
                        Set<Holder<Biome>> seen = new java.util.HashSet<>();
                        for (int x = p.getMinBlockX(); x <= p.getMaxBlockX(); x++)
                            for (int y = CdgFirstLook.MIN_Y; y <= CdgFirstLook.MAX_Y; y++)
                                for (int z = p.getMinBlockZ(); z <= p.getMaxBlockZ(); z++)
                                    seen.add(manager.getBiome(m.set(x, y, z)));
                        long t1 = System.nanoTime();
                        boolean look = CdgFirstLook.mayHoldOil(cells, p);
                        long t2 = System.nanoTime();
                        Set<Holder<Biome>> held = CdgFirstLook.cellBiomes(cells, p);
                        boolean oil = seen.stream().anyMatch(b -> b.is(CDGTags.OIL_BIOMES));
                        check("chunk " + p + ": the cells hold every biome CDG reads", held.containsAll(seen));
                        check("chunk " + p + ": an oil biome for CDG passes the look", !oil || look);
                        n[0]++;
                        if (oil)
                            n[1]++;
                        if (!look)
                            n[2]++;
                        if (look && !oil)
                            n[3]++;
                        n[4] += t2 - t1;
                        exactNanos[0] += t1 - t0;
                    }
                })
                .thenExecute(() -> {
                    check("every chunk read (" + n[0] + " of " + chunks.size() + ")", n[0] == chunks.size());
                    check("some chunks hold an oil biome (" + n[1] + ") and some are skipped (" + n[2] + ")", n[1] > 0 && n[2] > 0);
                    record(test, "noise", n[0] + " chunks of vanilla's overworld: " + n[1] + " with an oil biome for CDG, " + n[2]
                            + " skipped by the look, " + n[3] + " passed without one (an edge cell); the look "
                            + String.format("%.1f", n[4] / 1000.0 / n[0]) + " us a chunk against CDG's points "
                            + String.format("%.1f", exactNanos[0] / 1000.0 / n[0]) + " us");
                    // On the level: CDG's defaults first (normal chunks hold oil: the look stands aside), then the pack's rule.
                    ChunkPos here = new ChunkPos(helper.absolutePos(BlockPos.ZERO));
                    List<ChunkPos> onLevel = new java.util.ArrayList<>();
                    for (int dx = -3; dx <= 3; dx++)
                        for (int dz = -3; dz <= 3; dz++)
                            onLevel.add(new ChunkPos(here.x + dx, here.z + dz));
                    for (int i = 0; i < 8; i++)
                        onLevel.add(new ChunkPos(300_000 + rnd.nextInt(100_000), 300_000 + rnd.nextInt(100_000)));
                    boolean asShipped = CdgFirstLook.applies();
                    double normal = CDGConfig.OIL_MULTIPLIER.get();
                    long looked0 = CdgFirstLook.looked, skipped0 = CdgFirstLook.skipped;
                    int same = 0;
                    boolean packRule;
                    try {
                        CDGConfig.OIL_MULTIPLIER.set(0.0);
                        packRule = CdgFirstLook.applies();
                        for (ChunkPos p : onLevel) {
                            int a = src.baseOil(level, p), b = OilChunksSavedData.getBaseOilAmount(level, p);
                            check("chunk " + p + " under the pack's rule: " + a + " and getBaseOilAmount " + b, a == b);
                            same++;
                        }
                    } finally {
                        CDGConfig.OIL_MULTIPLIER.set(normal);
                    }
                    if (normal > 0 && !CDGConfig.DISABLE_NORMAL_OIL_CHUNKS.get())
                        check("CDG's own defaults (normal multiplier " + normal + "): the look stands aside", !asShipped);
                    check("under the pack's rule the look answers", packRule);
                    long looked = CdgFirstLook.looked - looked0, skipped = CdgFirstLook.skipped - skipped0;
                    check("it looked at every chunk (" + looked + ")", looked == onLevel.size());
                    record(test, "level", same + " chunks answer as getBaseOilAmount under the pack's rule (" + skipped
                            + " skipped by the look, the flat preset's one biome); under CDG's defaults (normal " + normal
                            + ") the look " + (asShipped ? "answers" : "stands aside"));
                })
                .thenSucceed();
    }

    private static void time(long[] t, long nanos) {
        t[0]++;
        t[1] += nanos;
        t[2] = Math.max(t[2], nanos);
    }

    private static String us(long[] t) {
        return t[0] == 0 ? "-" : String.format("%.1f", t[1] / 1000.0 / t[0]);
    }
}

