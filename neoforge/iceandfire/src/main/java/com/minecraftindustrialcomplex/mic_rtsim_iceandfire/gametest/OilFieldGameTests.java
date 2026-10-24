package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.api.OilSource;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonConfig;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.IafDragons;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.LairPlacer;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.OilSampler;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;
import java.util.function.ToIntFunction;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.engine;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * The oil gate's oil in the world (OILGATE.md §4.1, §4.2, §4.4): the sampler's answers to OIL_SURVEY and
 * its samples of new chunks (OG-2), PLACE_LAIR through the {@code /place} path (OG-7, §7 test 2), and field
 * flights (OG-3, native only: the stub decides nothing). Every test runs at its own far site (the GameTest
 * world is flat and reused) and logs what it compared on a {@code [gametest]} line; each has its own batch.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class OilFieldGameTests {
    private OilFieldGameTests() {}

    static final String ROOST = "iceandfire:fire_dragon_roost";

    /**
     * An oil source the tests switch on: amounts from a function, for one level, while active. The
     * Ice and Fire server has no other source, so {@code OilSources.forLevel} finds it.
     */
    static final class TestOil implements OilSource {
        volatile ServerLevel level;
        volatile ToIntFunction<ChunkPos> amounts;
        volatile long calls;

        @Override
        public String name() {
            return "gametest";
        }

        @Override
        public int source() {
            return OilSource.CDG;
        }

        @Override
        public boolean covers(ServerLevel l) {
            return amounts != null && l == level;
        }

        @Override
        public int baseOil(ServerLevel l, ChunkPos c) {
            calls++;
            ToIntFunction<ChunkPos> f = amounts;
            return f == null ? 0 : f.applyAsInt(c);
        }

        void on(ServerLevel l, ToIntFunction<ChunkPos> f) {
            level = l;
            amounts = f;
        }

        void off() {
            amounts = null;
            level = null;
        }
    }

    private static TestOil oil;

    static synchronized TestOil oil() {
        if (oil == null) {
            oil = new TestOil();
            RtsimApi.registerOilSource(oil);
        }
        return oil;
    }

    /** OIL_CHUNKS entries the engine received for {@code dim}: {cx, cz, amount, flags, source, gameTime, record index}. */
    static List<long[]> oilEntries(int dim) {
        List<long[]> out = new ArrayList<>();
        List<ScriptedEngine.Received> rs = engine().received(DragonRecords.OIL_CHUNKS);
        for (int i = 0; i < rs.size(); i++) {
            ScriptedEngine.Received r = rs.get(i);
            var v = r.view();
            if (v.getInt(DragonRecords.OilChunks.DIM) != dim)
                continue;
            int src = v.getInt(DragonRecords.OilChunks.SOURCE);
            long time = v.getLong(16);
            for (int k = 0; k < r.aux(); k++) {
                int e = DragonRecords.OilChunks.ENTRIES + DragonRecords.OilChunks.ENTRY * k;
                out.add(new long[] {v.getInt(e + DragonRecords.OilChunks.E_CX), v.getInt(e + DragonRecords.OilChunks.E_CZ),
                        v.getInt(e + DragonRecords.OilChunks.E_AMOUNT), v.getInt(e + DragonRecords.OilChunks.E_FLAGS), src, time,
                        r.obsSeq()});
            }
        }
        return out;
    }

    /** A test field's amount: the site's chunk bottomless, a quarter of the others dry, the rest by a hash. */
    static int amountAt(ChunkPos site, ChunkPos c) {
        if (c.equals(site))
            return Integer.MAX_VALUE;
        int h = Math.floorMod(c.x * 31 + c.z * 17, 4);
        return h == 0 ? 0 : 1_000 * h + Math.floorMod(c.x ^ c.z, 997);
    }

    // ---- OIL_SURVEY ---------------------------------------------------------------------------------

    /**
     * OIL_SURVEY (OG-2, OGC43): every chunk whose centre lies within the radius is covered, nearest first,
     * under the per-tick budget: a chunk in memory is sampled at once, any other when it next loads, so no
     * sample reads the biome source of a chunk not in memory. Only chunks with oil go back, as OIL_CHUNKS
     * source 2, a bottomless chunk with flag 1; chunks outside the radius are not asked; a repeat samples
     * nothing. The cost split (§7 test 4) is the plugin's own: CDG's real cost is the dieselgenerators bench.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = 2400, batch = "oil_survey")
    public static void surveyAnswersWithTheOilWithinItsRadius(GameTestHelper helper) {
        String test = "surveyAnswersWithTheOilWithinItsRadius";
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        ChunkPos sc = new ChunkPos(site);
        int radius = 96;
        TestOil src = oil();
        src.on(level, c -> amountAt(sc, c));
        DragonConfig.Test.answerSurveys = Boolean.TRUE;
        OilSampler s = bridge().sampler;
        int dim = DragonKit.session().dimId(level.dimension());
        long[] within = OilSampler.chunksWithin(site.getX(), site.getZ(), radius);
        Set<Long> inside = new HashSet<>();
        Map<Long, Integer> expected = new HashMap<>();
        for (long p : within) {
            inside.add(p);
            int a = amountAt(sc, new ChunkPos(p));
            if (a > 0)
                expected.put(p, a);
        }
        // The site's 3x3 is forced, which holds its 7x7 FULL (ticket level 31 reaches two chunks out): those
        // are in memory when the survey comes, the rest of the radius is not. More may load while the survey
        // is on its way, so the split is checked against the held 7x7, not a snapshot.
        List<ChunkPos> near = new ArrayList<>();
        for (int dx = -1; dx <= 1; dx++)
            for (int dz = -1; dz <= 1; dz++)
                near.add(new ChunkPos(sc.x + dx, sc.z + dz));
        List<ChunkPos> held = new ArrayList<>();
        for (int dx = -3; dx <= 3; dx++)
            for (int dz = -3; dz <= 3; dz++) {
                ChunkPos c = new ChunkPos(sc.x + dx, sc.z + dz);
                if (inside.contains(c.toLong()))
                    held.add(c);
            }
        List<ChunkPos> forced = new ArrayList<>();
        for (ChunkPos c : near)
            if (level.setChunkForced(c.x, c.z, true))
                forced.add(c);
        Set<Long> inMemory = new HashSet<>();
        long[] c0 = new long[7];
        long[] took = {0, 0};
        long[] later0 = {0};
        int budget = DragonConfig.samplesPerTick();
        long[] oil0 = DragonKit.nativeOil(0);
        helper.startSequence()
                .thenWaitUntil(() -> {
                    for (ChunkPos c : held)
                        check("the site's chunk " + c + " is in memory", level.getChunkSource().getChunkNow(c.x, c.z) != null);
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    for (long p : within)
                        if (level.getChunkSource().getChunkNow(ChunkPos.getX(p), ChunkPos.getZ(p)) != null)
                            inMemory.add(p);
                    check("the site's 7x7 is in memory and the radius's edge is not (" + inMemory.size() + " of " + within.length + ")",
                            inMemory.size() >= held.size() && inMemory.size() < within.length);
                    c0[0] = s.surveysDone;
                    c0[1] = s.surveySampled;
                    c0[2] = s.surveyDeferred;
                    c0[3] = s.deferredSampled;
                    c0[4] = s.ungenerated.samples;
                    c0[5] = s.repeats;
                    c0[6] = src.calls;
                    took[0] = level.getGameTime();
                    DragonKit.oilSurvey(DragonKit.dimension(helper), site.getX(), site.getZ(), radius);
                })
                .thenWaitUntil(() -> check("the survey finished (" + s.surveysDone + ", " + s.surveysPending() + " pending)",
                        s.surveysDone > c0[0] && s.surveysPending() == 0))
                .thenExecute(() -> {
                    long now = s.surveySampled - c0[1], later = s.surveyDeferred - c0[2];
                    later0[0] = later;
                    check("every chunk within the radius was sampled or deferred, once (" + now + " + " + later + " of " + within.length + ", "
                            + (s.repeats - c0[5]) + " repeats)", now + later == within.length && s.repeats == c0[5]);
                    check("the held chunks were sampled at once (" + now + " of at least " + held.size() + ")", now >= held.size());
                    check("the radius's edge waits for its load (" + later + ")", later > 0);
                    check("no sample read a chunk not in memory (" + (s.ungenerated.samples - c0[4]) + ")", s.ungenerated.samples == c0[4]);
                    record(test, "survey", "radius " + radius + " at " + site.toShortString() + ": " + within.length + " chunks, "
                            + now + " in memory sampled at once (" + inMemory.size() + " in memory when sent), " + later
                            + " waiting for their load");
                    // Their loads: each deferred chunk is sampled once it is in memory.
                    for (long p : within)
                        if (level.setChunkForced(ChunkPos.getX(p), ChunkPos.getZ(p), true))
                            forced.add(new ChunkPos(p));
                })
                .thenWaitUntil(() -> check("the deferred chunks were sampled as they loaded (" + (s.deferredSampled - c0[3]) + " of "
                        + later0[0] + ")", s.deferredSampled - c0[3] == later0[0]))
                .thenIdle(5)
                .thenExecute(() -> {
                    took[1] = level.getGameTime() - took[0];
                    for (ChunkPos c : forced)
                        level.setChunkForced(c.x, c.z, false);
                    List<long[]> got = new ArrayList<>();
                    for (long[] e : oilEntries(dim))
                        if (Math.abs(e[0] - sc.x) <= 16 && Math.abs(e[1] - sc.z) <= 16)
                            got.add(e);
                    Map<Long, long[]> byChunk = new HashMap<>();
                    Map<Long, Integer> perRecord = new HashMap<>();
                    for (long[] e : got) {
                        long p = ChunkPos.asLong((int) e[0], (int) e[1]);
                        check("chunk " + e[0] + "," + e[1] + " sent once", byChunk.put(p, e) == null);
                        check("chunk " + e[0] + "," + e[1] + " lies within the radius", inside.contains(p));
                        check("source 2 (" + e[4] + ")", e[4] == OilSource.CDG);
                        perRecord.merge(e[6], 1, Integer::sum);
                    }
                    check("every chunk with oil was sent (" + byChunk.size() + " of " + expected.size() + ")",
                            byChunk.keySet().equals(expected.keySet()));
                    for (Map.Entry<Long, Integer> x : expected.entrySet()) {
                        long[] e = byChunk.get(x.getKey());
                        boolean bottomless = x.getValue() == Integer.MAX_VALUE;
                        check("amount and flags of " + new ChunkPos(x.getKey()), e[2] == x.getValue()
                                && e[3] == (bottomless ? DragonRecords.OilChunks.FLAG_BOTTOMLESS : 0));
                    }
                    int most = perRecord.values().stream().mapToInt(Integer::intValue).max().orElse(0);
                    check("no record carries more than a tick's budget (" + most + " > " + budget + ")", most <= budget);
                    check("every chunk within the radius sampled once (" + (src.calls - c0[6]) + " of " + within.length + ")",
                            src.calls - c0[6] == within.length);
                    check("over at least " + (within.length / budget) + " ticks (" + took[1] + ")", took[1] >= within.length / budget - 1);
                    check("still no sample of a chunk not in memory", s.ungenerated.samples == c0[4]);
                    record(test, "loads", within.length + " chunks sampled in " + took[1] + " ticks (budget " + budget + " a tick), "
                            + byChunk.size() + " with oil sent in " + perRecord.size() + " records, the bottomless one with flag 1; cost "
                            + s.generated + " in memory (the test source: CDG's real cost is the dieselgenerators bench)");
                    if (oil0 != null) {
                        long[] oil1 = DragonKit.nativeOil(0);
                        check("the engine indexed them (" + (oil1[NativeEngine.OIL_CHUNKS] - oil0[NativeEngine.OIL_CHUNKS]) + ")",
                                oil1[NativeEngine.OIL_CHUNKS] - oil0[NativeEngine.OIL_CHUNKS] >= byChunk.size());
                        record(test, "engine", "oil chunks " + oil0[NativeEngine.OIL_CHUNKS] + " -> " + oil1[NativeEngine.OIL_CHUNKS]
                                + ", fields " + oil1[NativeEngine.OIL_FIELDS]);
                    } else {
                        record(test, "engine", "skipped on the stub: no oil index to read");
                    }
                    // The same survey again: every chunk is a repeat this session.
                    DragonKit.oilSurvey(DragonKit.dimension(helper), site.getX(), site.getZ(), radius);
                })
                .thenWaitUntil(() -> check("the repeat finished", s.surveysDone > c0[0] + 1 && s.surveysPending() == 0))
                .thenExecute(() -> {
                    check("the repeat sampled nothing (" + (s.repeats - c0[5]) + " repeats)",
                            s.repeats - c0[5] == within.length && src.calls - c0[6] == within.length);
                    record(test, "repeat", within.length + " repeats, no sample");
                    src.off();
                    DragonConfig.Test.reset();
                })
                .thenSucceed();
    }

    // ---- new chunks ---------------------------------------------------------------------------------

    /**
     * New chunks (OG-2): a chunk generated for the first time is sampled once, at the end of a tick, and
     * its oil goes to the engine; chunks with none send nothing.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "oil_new_chunks")
    public static void newChunksAreSampledAsTheyGenerate(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "newChunksAreSampledAsTheyGenerate";
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        ChunkPos sc = new ChunkPos(site);
        int r = 2;
        TestOil src = oil();
        src.on(level, c -> Math.abs(c.x - sc.x) <= r && Math.abs(c.z - sc.z) <= r ? amountAt(sc, c) : 0);
        DragonConfig.Test.sampleNewChunks = Boolean.TRUE;
        OilSampler s = bridge().sampler;
        int dim = DragonKit.session().dimId(level.dimension());
        Map<Long, Integer> expected = new HashMap<>();
        for (int dx = -r; dx <= r; dx++)
            for (int dz = -r; dz <= r; dz++) {
                ChunkPos p = new ChunkPos(sc.x + dx, sc.z + dz);
                int a = amountAt(sc, p);
                if (a > 0)
                    expected.put(p.toLong(), a);
            }
        long new0 = s.newSampled;
        Runnable unforce = DragonKit.forceChunks(level, sc, r);
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks generated", DragonKit.ready(level, sc, r))))
                .thenWaitUntil(waits.logic(() -> check("their samples ran (" + s.newChunksPending() + " pending)", s.newChunksPending() == 0)))
                .thenIdle(2)
                .thenExecute(() -> {
                    Map<Long, long[]> got = new HashMap<>();
                    for (long[] e : oilEntries(dim))
                        if (Math.abs(e[0] - sc.x) <= r + 4 && Math.abs(e[1] - sc.z) <= r + 4)
                            check("chunk " + e[0] + "," + e[1] + " sent once", got.put(ChunkPos.asLong((int) e[0], (int) e[1]), e) == null);
                    check("every new chunk with oil was sent (" + got.size() + " of " + expected.size() + ")",
                            got.keySet().equals(expected.keySet()));
                    for (Map.Entry<Long, Integer> x : expected.entrySet())
                        check("amount of " + new ChunkPos(x.getKey()), got.get(x.getKey())[2] == x.getValue());
                    long n = s.newSampled - new0;
                    check("at least the site's " + (2 * r + 1) * (2 * r + 1) + " chunks sampled (" + n + ")", n >= (2 * r + 1) * (2 * r + 1));
                    record(test, "new chunks", n + " new chunks sampled (the site's " + (2 * r + 1) * (2 * r + 1) + " and their border), "
                            + got.size() + " with oil sent; cost " + s.generated + " generated");
                    unforce.run();
                    src.off();
                    DragonConfig.Test.reset();
                })
                .thenSucceed();
    }

    // ---- PLACE_LAIR ---------------------------------------------------------------------------------

    /**
     * PLACE_LAIR (OG-7) and OILGATE.md §7 test 2: a fire roost placed through the {@code /place} path at
     * a far site, outside world generation, in the flat world's plains. On the native engine the engine
     * asks for it (a field with no fire lair in reach, a player near it); on the stub a script does. The
     * roost's blocks are there, Ice and Fire spawns its own dragon, the plugin reports the lair late, and
     * the dragon as untagged with that lair; the engine adopts it (native). A repeat for the field places
     * nothing; the saved state reads back.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "oil_place_lair")
    public static void placeLairPlacesARoostWithItsOwnDragon(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 2400);
        String test = "placeLairPlacesARoostWithItsOwnDragon";
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        ChunkPos sc = new ChunkPos(site);
        boolean nat = DragonKit.nativeEngine();
        LairPlacer placer = bridge().placer;
        int placed0 = placer.placements.size();
        long surveys0 = bridge().sampler.surveysAsked;
        long scriptedField = ThreadLocalRandom.current().nextLong(1L << 40, 1L << 50);
        UUID player = UUID.randomUUID();
        DragonConfig.Test.placeLairs = Boolean.TRUE;
        Runnable unforce = DragonKit.forceChunks(level, sc, 5);
        LairPlacer.Placed[] got = {null};
        DragonBaseEntity[] own = {null};
        long[] oil0 = DragonKit.nativeOil(0);
        int[] ticks = {0};
        long start = level.getGameTime();
        Data0 before = Data0.of(placer.data());
        waits.eachTick(() -> {
            if (nat && got[0] == null && ticks[0]++ % 20 == 0)
                DragonKit.reportPlayerAt(player, level, site.getX() + 0.5, 70, site.getZ() + 0.5);
        });
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", DragonKit.ready(level, sc, 5))))
                .thenExecute(() -> {
                    if (nat) {
                        // The engine's path: an oil field there, a player near it, the switch on.
                        List<int[]> field = new ArrayList<>();
                        for (int dx = -1; dx <= 1; dx++)
                            for (int dz = -1; dz <= 1; dz++)
                                field.add(new int[] {sc.x + dx, sc.z + dz, 40_000});
                        DragonKit.oilChunks(level, field);
                        DragonKit.oilConfig(DragonRecords.OilConfig.FLAG_PLACE_LAIRS);
                    } else {
                        DragonKit.placeLair(scriptedField, DragonKit.dimension(helper), ROOST, site.getX(), site.getZ(), 100);
                    }
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (int i = placed0; i < placer.placements.size(); i++) {
                        LairPlacer.Placed p = placer.placements.get(i);
                        if (p.dimension().equals(DragonKit.dimension(helper)) && p.pivot().distManhattan(site.atY(p.pivot().getY())) <= 260)
                            got[0] = p;
                    }
                    check("a roost was placed near the site (" + placer.requests + " requests, " + placer.sitesTried + " sites tried, "
                            + placer.refused + " refused, " + placer.badRequests + " bad)", got[0] != null);
                }))
                .thenExecute(() -> {
                    LairPlacer.Placed p = got[0];
                    if (!nat)
                        check("for the scripted field", p.field() == scriptedField);
                    check("a fire roost", p.structure().equals(ROOST));
                    // Its blocks.
                    int iaf = 0;
                    for (BlockPos b : BlockPos.betweenClosed(p.pivot().offset(-24, -6, -24), p.pivot().offset(24, 24, 24)))
                        if (BuiltInRegistries.BLOCK.getKey(level.getBlockState(b).getBlock()).getNamespace().equals("iceandfire"))
                            iaf++;
                    check("Ice and Fire's blocks are there (" + iaf + ")", iaf > 50);
                    Structure st = level.registryAccess().registryOrThrow(Registries.STRUCTURE).get(ResourceLocation.parse(ROOST));
                    StructureStart at = level.structureManager().getStructureAt(p.pivot(), st);
                    check("the world knows the structure at its pivot", at.isValid());
                    record(test, "placed", ROOST + " at " + p.pivot().toShortString() + " (box " + p.box() + "), " + p.sitesTried()
                            + " sites tried, " + p.placeNanos() / 1000 + " us to place, " + (level.getGameTime() - start)
                            + " ticks after the request; " + iaf + " Ice and Fire blocks; field " + p.field()
                            + (nat ? " (the engine's)" : " (scripted)"));
                    // LAIR_FOUND, late.
                    List<ScriptedEngine.Received> found = DragonKit.received(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.LAIR, p.lair());
                    check("LAIR_FOUND for it (" + found.size() + ")", found.size() == 1);
                    ScriptedEngine.Received f = found.get(0);
                    check("flag late (" + f.aux() + ")", (f.aux() & DragonRecords.LairFound.FLAG_LATE) != 0);
                    check("naming the fire roost", f.view().getInt(DragonRecords.LairFound.STRUCTURE) == DragonKit.session().dictId(Obs.DICT_STRUCTURE, ROOST));
                    // Its own dragon.
                    EntityType<?> fire = IafDragons.speciesType(DragonKit.FIRE);
                    for (DragonBaseEntity d : level.getEntitiesOfClass(DragonBaseEntity.class, AABB.of(p.box()).inflate(48)))
                        if (d.getType() == fire && !d.isRemoved())
                            own[0] = d;
                    check("Ice and Fire spawned the roost's own fire dragon", own[0] != null);
                })
                .thenWaitUntil(waits.logic(() -> check("DRAGON_UNTAGGED for it", !DragonKit.untagged(own[0].getUUID()).isEmpty())))
                .thenExecute(() -> {
                    ScriptedEngine.Received u = DragonKit.untagged(own[0].getUUID()).get(0);
                    check("spawned by a structure (" + u.aux() + ")", u.aux() == DragonRecords.Untagged.SPAWN_STRUCTURE);
                    check("in the placed lair", u.view().getLong(DragonRecords.Untagged.LAIR) == got[0].lair());
                    record(test, "dragon", "its own fire dragon " + own[0].getUUID() + " at " + own[0].blockPosition().toShortString()
                            + ", age " + IafDragons.ageTicks(own[0]) / 24_000 + " days, reported DRAGON_UNTAGGED kind STRUCTURE in lair "
                            + Long.toHexString(got[0].lair()));
                    if (!nat)
                        record(test, "adopted", "skipped on the stub (it decides nothing)");
                })
                .thenWaitUntil(waits.logic(() -> check("the engine adopted it", !nat || bridge().agentOf(own[0]) != 0)))
                .thenWaitUntil(waits.logic(() -> check("and asked for the oil around it (OIL_SURVEY)", !nat || bridge().sampler.surveysAsked > surveys0)))
                .thenExecute(() -> {
                    if (nat) {
                        long[] oil1 = DragonKit.nativeOil(0);
                        record(test, "adopted", "agent " + Long.toHexString(bridge().agentOf(own[0])) + "; the engine placed "
                                + (oil1[NativeEngine.OIL_LAIRS_PLACED] - oil0[NativeEngine.OIL_LAIRS_PLACED]) + " lair(s), asked for "
                                + (oil1[NativeEngine.OIL_SURVEYS] - oil0[NativeEngine.OIL_SURVEYS]) + " survey(s)");
                    }
                    // A repeat for the field (an engine replaying its outbox) places nothing.
                    long repeats0 = placer.repeats;
                    placer.request(got[0].field(), DragonKit.dimension(helper), ROOST, site.getX(), site.getZ(), 100);
                    check("a repeat is refused as one (" + (placer.repeats - repeats0) + ")", placer.repeats == repeats0 + 1);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    long mine = placer.placements.stream().filter(x -> x.field() == got[0].field()).count();
                    check("one roost for the field (" + mine + ")", mine == 1);
                    // The saved state reads back.
                    LairPlacer.Data d = placer.data();
                    CompoundTag t = d.save(new CompoundTag(), level.registryAccess());
                    LairPlacer.Data back = LairPlacer.Data.load(t, level.registryAccess());
                    check("fields read back", back.fields.equals(d.fields) && back.fields.contains(got[0].field()));
                    check("pending read back", back.pending.equals(d.pending));
                    boolean same = back.placed.stream().anyMatch(x -> x.lair() == got[0].lair() && x.box().equals(got[0].box())
                            && x.pivot().equals(got[0].pivot()) && x.field() == got[0].field());
                    check("the placed lair reads back", same && back.placed.size() == d.placed.size());
                    record(test, "saved", d.fields.size() + " fields asked for, " + d.placed.size() + " lairs placed in the saved state ("
                            + before.fields + " fields and " + before.placed + " lairs from earlier runs: the state outlives restarts)");
                    record(test, "answer", "§7 test 2: the /place path places a fire roost outside world generation (any biome); "
                            + "Ice and Fire spawns the roost's own dragon while it builds; the plugin reports it DRAGON_UNTAGGED kind "
                            + "STRUCTURE in the placed lair" + (nat ? ", and the engine adopts it" : ""));
                    unforce.run();
                    if (nat)
                        DragonKit.restoreOilConfig();
                    else
                        DragonConfig.Test.reset();
                })
                .thenSucceed();
    }

    /** The saved state's sizes before a test. */
    record Data0(int fields, int placed) {
        static Data0 of(LairPlacer.Data d) {
            return new Data0(d.fields.size(), d.placed.size());
        }
    }

    // ---- field flights ------------------------------------------------------------------------------

    /**
     * Field flights (OG-3, native only): with the switch on, a player at y 15 or above near an oil field
     * draws a fire dragon, materialized hostile (flag 8) with its oil reserve (flag 16). Below y 15, far
     * from oil, or with the switch off, none is drawn (each watched for a minute, In Control's 0.1 a
     * second leaving 0.2 % of no draw by chance where one is due).
     */
    // An unthrottled GameTest server runs ~1,000 ticks a second, so one engine step spans many ticks and
    // a roll can find the player's presence stale (r1 drew in 162 ticks, r2 in 6,427): the draw is waited
    // for at 5 ms a tick and gets room.
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "oil_flights")
    public static void fieldFlightsDrawADragonOnlyOverOilAndHighEnough(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 24000);
        String test = "fieldFlightsDrawADragonOnlyOverOilAndHighEnough";
        if (!DragonKit.nativeEngine()) {
            record(test, "skipped", "on the stub: field flights are the engine's");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        ChunkPos sc = new ChunkPos(site);
        int force = 11; // the engine's view radius (160) and a chunk
        Runnable unforce = DragonKit.forceChunks(level, sc, force);
        ServerPlayer[] p = {null};
        // What is reported: 0 nothing, 1 the real player, 2 the low one, 3 the far one, 4 the real player's spot.
        int[] phase = {0};
        int[] ticks = {0};
        UUID ghost = UUID.randomUUID();
        long[] mark = new long[8];
        long[] drawn = {0};
        long[] agent = {0};
        Vec3 high = new Vec3(site.getX() + 0.5, 70, site.getZ() + 0.5);
        waits.eachTick(() -> {
            if (ticks[0]++ % 20 != 0)
                return;
            switch (phase[0]) {
                case 1 -> DragonKit.reportPlayer(p[0]);
                case 2 -> DragonKit.reportPlayerAt(ghost, level, high.x, -59, high.z);
                case 3 -> DragonKit.reportPlayerAt(ghost, level, high.x + 1_000, 70, high.z);
                case 4 -> DragonKit.reportPlayerAt(ghost, level, high.x, 70, high.z);
                default -> {}
            }
        });
        int quiet = 1200, stale = 260;
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", DragonKit.ready(level, sc, force))))
                .thenExecute(() -> {
                    List<int[]> field = new ArrayList<>();
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            field.add(new int[] {sc.x + dx, sc.z + dz, 40_000});
                    DragonKit.oilChunks(level, field);
                    DragonKit.oilConfig(DragonRecords.OilConfig.FLAG_FIELD_FLIGHTS);
                    p[0] = OilGameTests.guardedPlayer(helper, high, "oil_flights");
                    mark[0] = DragonKit.nativeOil(0)[NativeEngine.OIL_FIELD_DRAWS];
                    mark[5] = DragonKit.nativeOil(0)[NativeEngine.OIL_FIELD_ROLLS];
                    mark[1] = level.getGameTime();
                    record(test, "start", flightProbe(DragonKit.nativeOil(0), level));
                    phase[0] = 1;
                })
                // The engine's rolls run on its own thread: io() holds each failing tick 5 ms.
                .thenWaitUntil(waits.engine(() -> {
                    long[] o = DragonKit.nativeOil(0);
                    check("a field flight was drawn (" + (o[NativeEngine.OIL_FIELD_ROLLS] - mark[5]) + " rolls, switches "
                            + o[NativeEngine.OIL_FLAGS] + ", " + o[NativeEngine.OIL_FIELDS] + " fields, " + o[NativeEngine.OIL_CHUNKS]
                            + " oil chunks, " + o[NativeEngine.OIL_FIELD_DRAWS] + " draws and " + o[NativeEngine.OIL_VISITORS_LEFT]
                            + " visitors gone in all, " + (level.getGameTime() - mark[1]) + " ticks; " + flightProbe(o, level) + ")",
                            o[NativeEngine.OIL_FIELD_DRAWS] > mark[0]);
                }))
                .thenExecute(() -> drawn[0] = level.getGameTime() - mark[1])
                .thenWaitUntil(waits.logic(() -> {
                    for (ScriptedEngine.Received r : engine().received(DragonRecords.DRAGON_MATERIALIZED)) {
                        long a = r.view().getLong(DragonRecords.Materialized.AGENT);
                        int flags = bridge().materializeFlagsOf(a);
                        if (r.view().getLong(16) >= mark[1] && flags >= 0
                                && (flags & (DragonRecords.Materialize.FLAG_HOSTILE | DragonRecords.Materialize.FLAG_OIL))
                                == (DragonRecords.Materialize.FLAG_HOSTILE | DragonRecords.Materialize.FLAG_OIL))
                            agent[0] = a;
                    }
                    check("a fire dragon materialized hostile with its oil", agent[0] != 0);
                }))
                .thenExecute(() -> {
                    DragonBaseEntity d = DragonKit.body(agent[0]);
                    check("its body is a fire dragon", d != null && d.getType() == IafDragons.speciesType(DragonKit.FIRE));
                    int reserve = bridge().oilOf(agent[0]);
                    check("carrying a reserve (" + reserve + ")", reserve > 0);
                    record(test, "drawn", "a field flight drawn " + drawn[0] + " ticks after the player came, materialized "
                            + (level.getGameTime() - mark[1]) + " ticks after, flags " + bridge().materializeFlagsOf(agent[0])
                            + ", reserve " + reserve + ", " + (int) d.distanceTo(p[0]) + " blocks from the player, targeting "
                            + (d.getTarget() == p[0] ? "the player" : String.valueOf(d.getTarget())));
                    OilGameTests.release(p[0]);
                    phase[0] = 2; // below y 15, over the oil
                })
                .thenIdle(stale)
                .thenExecute(() -> mark[2] = DragonKit.nativeOil(0)[NativeEngine.OIL_FIELD_DRAWS])
                .thenIdle(quiet)
                .thenExecute(() -> {
                    long n = DragonKit.nativeOil(0)[NativeEngine.OIL_FIELD_DRAWS] - mark[2];
                    check("none below y 15 (" + n + ")", n == 0);
                    phase[0] = 3; // high, a thousand blocks from the oil
                })
                .thenIdle(stale)
                .thenExecute(() -> mark[3] = DragonKit.nativeOil(0)[NativeEngine.OIL_FIELD_DRAWS])
                .thenIdle(quiet)
                .thenExecute(() -> {
                    long n = DragonKit.nativeOil(0)[NativeEngine.OIL_FIELD_DRAWS] - mark[3];
                    check("none far from oil (" + n + ")", n == 0);
                    DragonKit.oilConfig(0);
                    phase[0] = 4; // high over the oil, the switch off
                })
                .thenIdle(stale)
                .thenExecute(() -> mark[4] = DragonKit.nativeOil(0)[NativeEngine.OIL_FIELD_DRAWS])
                .thenIdle(quiet)
                .thenExecute(() -> {
                    long[] o = DragonKit.nativeOil(0);
                    long n = o[NativeEngine.OIL_FIELD_DRAWS] - mark[4];
                    check("none with the switch off (" + n + ", engine flags " + o[NativeEngine.OIL_FLAGS] + ")", n == 0 && o[NativeEngine.OIL_FLAGS] == 0);
                    record(test, "negatives", "no draw in " + quiet + " ticks each: below y 15 over oil, at y 70 1,000 blocks from oil, "
                            + "the switch off over oil; " + o[NativeEngine.OIL_FIELD_ROLLS] + " rolls, " + o[NativeEngine.OIL_FIELD_DRAWS]
                            + " draws in the world");
                    phase[0] = 0;
                    unforce.run();
                    DragonKit.restoreOilConfig();
                })
                .thenSucceed();
    }

    /** The engine's field flight probe (NativeEngine OIL_NOW on), for a failing wait's message. */
    static String flightProbe(long[] o, ServerLevel level) {
        return "game time " + level.getGameTime() + ", engine " + o[NativeEngine.OIL_NOW] + ", wheel " + o[NativeEngine.OIL_WHEEL_NOW]
                + "; presences " + o[NativeEngine.OIL_PRESENCES] + " (" + o[NativeEngine.OIL_PRESENCES_FRESH] + " fresh, "
                + o[NativeEngine.OIL_PRESENCES_AHEAD] + " ahead of the clock); rolls active " + o[NativeEngine.OIL_ROLLS_ACTIVE]
                + ", booked " + o[NativeEngine.OIL_ROLLS_BOOKED] + " (next at " + o[NativeEngine.OIL_ROLL_NEXT] + "); visitors alive "
                + o[NativeEngine.OIL_VISITORS] + ", most on a field " + o[NativeEngine.OIL_VISITORS_MOST];
    }
}
