package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DeathWormEntity;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonConfig;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.Nests;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.BaseSpawner;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.SpawnerBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * The oil nests (OILGATE.md §4.6, OG-9e). On the stub the tests script PLACE_NEST, NEST_RENEW and
 * NEST_SORTIE as the engine would send them and read what the plugin answers: the chamber and its inert
 * spawner heart, the worms it places and the census, the boss rule on their deaths, a broken heart, and
 * raids that stop under an armoured floor or surface. The native test lets the engine found the nest under
 * an oil chunk, keep it asleep until a test well drills beside it, renew its brood and regrow a worm the
 * players killed. Each test runs at its own far site of the flat, reused GameTest world, in a chunk filled
 * with stone above the flat ground (the nest floor is forced into it), in its own batch.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class NestGameTests {
    private NestGameTests() {}

    /** One test's far site and what the test must undo. */
    static final class Site {
        final GameTestHelper helper;
        final ServerLevel level;
        final BlockPos at;
        final ChunkPos chunk;
        final String dim;
        final int radius;
        final Runnable unforce;
        /** The first free y over the flat ground, and the nest floor forced over it. */
        int ground, floor;
        final List<Long> nests = new ArrayList<>();
        final List<ServerPlayer> players = new ArrayList<>();
        final List<net.minecraft.world.entity.Entity> extras = new ArrayList<>();
        final List<Runnable> undo = new ArrayList<>();

        Site(GameTestHelper helper, int radius) {
            this.helper = helper;
            this.level = helper.getLevel();
            this.at = DragonKit.farSite();
            this.chunk = new ChunkPos(at);
            this.dim = DragonKit.dimension(helper);
            this.radius = radius;
            this.unforce = DragonKit.forceChunks(level, chunk, radius);
            DragonConfig.Test.placeNests = Boolean.TRUE;
            DragonKit.onEnd(helper, this::cleanUp);
        }

        boolean ready() {
            return DragonKit.ready(level, chunk, radius);
        }

        /** Fills chunk {@code c} with stone from the flat ground to 11 over the forced floor (every spot's shell). */
        void stone(ChunkPos c) {
            BlockState stone = Blocks.STONE.defaultBlockState();
            BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
            for (int x = c.getMinBlockX(); x <= c.getMaxBlockX(); x++)
                for (int z = c.getMinBlockZ(); z <= c.getMaxBlockZ(); z++)
                    for (int y = ground; y <= floor + Nests.AIR + 1 + Nests.SHIFT; y++)
                        level.setBlock(p.set(x, y, z), stone, 2);
        }

        /** The ground's height and the forced floor; the site's chunk filled with stone. */
        void build() {
            ground = level.getHeight(Heightmap.Types.MOTION_BLOCKING, at.getX(), at.getZ());
            floor = ground + 1 + Nests.SHIFT + 1; // the lowest spot's shell (floor - SHIFT - 1) is on the ground
            DragonConfig.Test.nestFloorY = floor;
            stone(chunk);
        }

        long nest() {
            long id = ThreadLocalRandom.current().nextLong(1L << 40, 1L << 50);
            nests.add(id);
            return id;
        }

        ServerPlayer player(Vec3 where, String name) {
            ServerPlayer p = FakePlayers.add(level, where, name);
            players.add(p);
            return p;
        }

        /** The live worms of nest {@code id}. */
        List<DeathWormEntity> worms(long id) {
            List<DeathWormEntity> out = new ArrayList<>();
            Nests.Nest n = bridge().nests.nest(id);
            if (n != null)
                for (UUID u : n.worms)
                    if (level.getEntity(u) instanceof DeathWormEntity w && w.isAlive())
                        out.add(w);
            return out;
        }

        /** Every Death Worm near the site, tagged or not. */
        List<DeathWormEntity> allWorms() {
            int r = (radius + 1) * 16;
            return level.getEntitiesOfClass(DeathWormEntity.class, new AABB(at.getX() - r, level.getMinBuildHeight(), at.getZ() - r,
                    at.getX() + r, level.getMaxBuildHeight(), at.getZ() + r));
        }

        void cleanUp() {
            for (Runnable r : undo)
                r.run();
            players.forEach(FakePlayers::remove);
            for (net.minecraft.world.entity.Entity e : extras)
                if (!e.isRemoved())
                    e.discard();
            for (DeathWormEntity w : allWorms())
                w.discard();
            for (long id : nests)
                bridge().nests.forgetForTest(id);
            DragonConfig.Test.reset();
            unforce.run();
        }
    }

    static Nests nests() {
        return bridge().nests;
    }

    static List<ScriptedEngine.Received> found(long id) {
        return DragonKit.received(DragonRecords.NEST_FOUND, DragonRecords.NestFound.NEST, id);
    }

    static List<ScriptedEngine.Received> censuses(long id) {
        return DragonKit.received(DragonRecords.NEST_CENSUS, DragonRecords.NestCensus.NEST, id);
    }

    static List<ScriptedEngine.Received> losses(long id) {
        return DragonKit.received(DragonRecords.NEST_LOSS, DragonRecords.NestLoss.NEST, id);
    }

    static BlockPos heartOf(ScriptedEngine.Received f) {
        return new BlockPos(f.view().getInt(DragonRecords.NestFound.HEART_X), f.view().getInt(DragonRecords.NestFound.HEART_Y),
                f.view().getInt(DragonRecords.NestFound.HEART_Z));
    }

    static CompoundTag spawnerTag(ServerLevel level, BlockPos pos) {
        return level.getBlockEntity(pos) instanceof SpawnerBlockEntity sp ? sp.getSpawner().save(new CompoundTag()) : null;
    }

    static void setRequiredRange(ServerLevel level, BlockPos pos, int range) {
        if (level.getBlockEntity(pos) instanceof SpawnerBlockEntity sp) {
            BaseSpawner b = sp.getSpawner();
            CompoundTag t = b.save(new CompoundTag());
            t.putShort("RequiredPlayerRange", (short) range);
            b.load(level, pos, t);
        }
    }

    // ---- placement --------------------------------------------------------------------------------

    /**
     * PLACE_NEST carves a 9 by 9 chamber (a sand floor, 4 air over it) at the forced floor in the asked
     * chunk, with a spawner heart set to the Death Worm and RequiredPlayerRange 0 at the floor's centre, and
     * answers NEST_FOUND placed with the heart. A repeat answers again with the same heart and carves
     * nothing; the saved state reads back.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_place")
    public static void placeNestCarvesAChamberOnce(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "placeNestCarvesAChamberOnce";
        Site s = new Site(helper, 1);
        ServerLevel level = s.level;
        long id = s.nest();
        long repeats0 = nests().repeats;
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    s.build();
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30);
                })
                .thenWaitUntil(waits.logic(() -> check("the nest placed (" + nests().placedCount + " placed, " + nests().failedCount + " failed, "
                        + nests().refused + " refused)", nests().nest(id) != null)))
                .thenWaitUntil(waits.logic(() -> check("NEST_FOUND", found(id).size() == 1)))
                .thenExecute(() -> {
                    BlockPos h = nests().nest(id).heart;
                    check("the heart in the asked chunk: " + h, new ChunkPos(h).equals(s.chunk));
                    check("the floor at the forced y (" + h.getY() + " vs " + s.floor + ")", h.getY() == s.floor);
                    check("a spawner at the heart", level.getBlockState(h).is(Blocks.SPAWNER));
                    CompoundTag t = spawnerTag(level, h);
                    String type = t.getCompound("SpawnData").getCompound("entity").getString("id");
                    check("set to the Death Worm (" + type + ")", type.equals("iceandfire:deathworm"));
                    check("RequiredPlayerRange 0 (" + t.getShort("RequiredPlayerRange") + ")", t.getShort("RequiredPlayerRange") == 0);
                    int sand = 0, air = 0, shell = 0;
                    for (int dx = -Nests.HALF; dx <= Nests.HALF; dx++)
                        for (int dz = -Nests.HALF; dz <= Nests.HALF; dz++) {
                            if (level.getBlockState(h.offset(dx, 0, dz)).is(Blocks.SAND))
                                sand++;
                            for (int dy = 1; dy <= Nests.AIR; dy++)
                                if (level.getBlockState(h.offset(dx, dy, dz)).isAir())
                                    air++;
                            if (level.getBlockState(h.offset(dx, -1, dz)).is(Blocks.STONE)
                                    && level.getBlockState(h.offset(dx, Nests.AIR + 1, dz)).is(Blocks.STONE))
                                shell++;
                        }
                    check("a sand floor round the heart (" + sand + "/80)", sand == 80);
                    check("4 air over it (" + air + "/324)", air == 324);
                    check("stone under and over it (" + shell + "/81)", shell == 81);
                    ScriptedEngine.Received f = found(id).get(0);
                    check("placed (" + f.aux() + ")", f.aux() == DragonRecords.NestFound.PLACED);
                    check("its heart in NEST_FOUND (" + heartOf(f) + ")", heartOf(f).equals(h));
                    check("its dimension", f.view().getInt(DragonRecords.NestFound.DIM) == DragonKit.session().dimId(level.dimension()));
                    record(test, "placed", "nest " + id + " heart at " + h.toShortString() + " in chunk " + s.chunk + ", floor " + s.floor
                            + " over ground " + s.ground + "; 80 sand, 324 air, a Death Worm spawner with RequiredPlayerRange 0; NEST_FOUND placed");
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30); // an engine re-sending it after an OIL_CONFIG
                })
                .thenWaitUntil(waits.logic(() -> check("NEST_FOUND again", found(id).size() == 2)))
                .thenExecute(() -> {
                    BlockPos h = nests().nest(id).heart;
                    ScriptedEngine.Received again = found(id).get(1);
                    check("placed again with the same heart", again.aux() == DragonRecords.NestFound.PLACED && heartOf(again).equals(h));
                    check("counted a repeat", nests().repeats == repeats0 + 1);
                    int spawners = 0;
                    for (BlockPos p : BlockPos.betweenClosed(s.chunk.getMinBlockX(), s.ground, s.chunk.getMinBlockZ(), s.chunk.getMaxBlockX(),
                            s.floor + 12, s.chunk.getMaxBlockZ()))
                        if (level.getBlockState(p).is(Blocks.SPAWNER))
                            spawners++;
                    check("one spawner in the chunk (" + spawners + ")", spawners == 1);
                    Nests.Data d = nests().data();
                    Nests.Data back = Nests.Data.load(d.save(new CompoundTag(), level.registryAccess()), level.registryAccess());
                    Nests.Nest n = back.placed.get(id);
                    check("the nest reads back", n != null && n.heart.equals(h) && n.dim.equals(s.dim) && !n.broken);
                    record(test, "repeat", "a second PLACE_NEST answered NEST_FOUND placed with the same heart, carved nothing (1 spawner); "
                            + "saved state reads back (" + d.placed.size() + " placed, " + d.pending.size() + " waiting)");
                })
                .thenSucceed();
    }

    /**
     * A chamber is carved only in natural ground with no fluid and no block entity: water at the first
     * spot moves the nest to a later one (the water is left), chests at every spot fail it, and so does
     * open air (the flat world over its ground). NEST_FOUND says failed, with no heart, and nothing is kept.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_avoid")
    public static void placeNestAvoidsFluidsAndBlockEntities(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "placeNestAvoidsFluidsAndBlockEntities";
        Site s = new Site(helper, 2);
        ServerLevel level = s.level;
        long chests = s.nest(), open = s.nest();
        long[] wet = {0};
        ChunkPos chestChunk = new ChunkPos(s.chunk.x + 1, s.chunk.z), openChunk = new ChunkPos(s.chunk.x - 1, s.chunk.z);
        BlockPos[] water = {null};
        BlockPos[] expected = {null};
        List<BlockPos> chestsAt = new ArrayList<>();
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    s.build();
                    s.stone(chestChunk);
                    // Water in the first spot of the wet nest. Its id is drawn so that a later spot clears the water
                    // (a shift of 4 or more either way): with 7 later spots, about 1 id in 80 has none.
                    LevelChunk wc = level.getChunk(s.chunk.x, s.chunk.z);
                    List<BlockPos> spots = List.of();
                    for (int i = 0; i < 1000 && wet[0] == 0; i++) {
                        long id = ThreadLocalRandom.current().nextLong(1L << 40, 1L << 50);
                        List<BlockPos> l = Nests.spots(id, wc, 30, s.floor);
                        for (BlockPos p : l.subList(1, l.size()))
                            if (Math.abs(p.getY() - l.get(0).getY()) >= 4) {
                                wet[0] = id;
                                spots = l;
                                break;
                            }
                    }
                    check("a nest id with a spot clear of the water", wet[0] != 0);
                    s.nests.add(wet[0]);
                    water[0] = spots.get(0).above(2);
                    level.setBlock(water[0], Blocks.WATER.defaultBlockState(), 2);
                    check("the first spot no longer fits", !Nests.fits(wc, spots.get(0)));
                    for (BlockPos p : spots)
                        if (Nests.fits(wc, p)) {
                            expected[0] = p;
                            break;
                        }
                    check("a later spot fits", expected[0] != null);
                    // A chest in every spot of the other.
                    LevelChunk cc = level.getChunk(chestChunk.x, chestChunk.z);
                    for (BlockPos p : Nests.spots(chests, cc, 30, s.floor)) {
                        BlockPos c = p.offset(1, 1, 1);
                        level.setBlock(c, Blocks.CHEST.defaultBlockState(), 2);
                        chestsAt.add(c);
                    }
                    for (BlockPos p : Nests.spots(chests, cc, 30, s.floor))
                        check("no spot fits by the chests", !Nests.fits(cc, p));
                    DragonKit.placeNest(wet[0], s.dim, s.chunk.x, s.chunk.z, 30);
                    DragonKit.placeNest(chests, s.dim, chestChunk.x, chestChunk.z, 30);
                    DragonKit.placeNest(open, s.dim, openChunk.x, openChunk.z, 30);
                })
                .thenWaitUntil(waits.logic(() -> check("three NEST_FOUNDs", found(wet[0]).size() == 1 && found(chests).size() == 1 && found(open).size() == 1)))
                .thenExecute(() -> {
                    ScriptedEngine.Received w = found(wet[0]).get(0);
                    check("the wet nest placed (" + w.aux() + ")", w.aux() == DragonRecords.NestFound.PLACED);
                    check("at the first dry spot (" + heartOf(w) + " vs " + expected[0] + ")", heartOf(w).equals(expected[0]));
                    check("the water left alone", level.getFluidState(water[0]).isSource());
                    for (long id : new long[] {chests, open}) {
                        ScriptedEngine.Received f = found(id).get(0);
                        check("nest " + id + " failed (" + f.aux() + ")", f.aux() == DragonRecords.NestFound.FAILED);
                        check("with no heart", heartOf(f).equals(BlockPos.ZERO));
                        check("and not kept", nests().nest(id) == null && !nests().data().pending.containsKey(id));
                    }
                    for (BlockPos c : chestsAt)
                        check("the chest at " + c.toShortString() + " left alone", level.getBlockState(c).is(Blocks.CHEST));
                    record(test, "moved", "water at " + water[0].toShortString() + " in the first spot: the nest went to "
                            + expected[0].toShortString() + ", the water is still there");
                    record(test, "failed", "chests in all " + chestsAt.size() + " spots: NEST_FOUND failed; open air: NEST_FOUND failed; "
                            + "neither kept");
                })
                .thenSucceed();
    }

    // ---- worms ----------------------------------------------------------------------------------------

    /**
     * NEST_RENEW 2 at stage 2 places two worms on the chamber floor: grown (worm age 5) at scale 2 (age
     * scale 2), the nest's variant, homed on the heart, marked with the nest id and carrying no agent tag.
     * The census then says 2.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_renew")
    public static void nestRenewPlacesTaggedWorms(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "nestRenewPlacesTaggedWorms";
        Site s = new Site(helper, 1);
        long id = s.nest();
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    s.build();
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30);
                })
                .thenWaitUntil(waits.logic(() -> check("the nest placed", nests().nest(id) != null)))
                .thenExecute(() -> DragonKit.nestRenew(id, s.dim, 2, 2))
                .thenWaitUntil(waits.logic(() -> check("two worms (" + s.worms(id).size() + ", " + nests().nest(id).owed + " owed)", s.worms(id).size() == 2)))
                .thenWaitUntil(waits.logic(() -> check("a census", !censuses(id).isEmpty())))
                .thenExecute(() -> {
                    Nests.Nest n = nests().nest(id);
                    StringBuilder seen = new StringBuilder();
                    List<DeathWormEntity> two = s.worms(id);
                    check("on two spots (" + two.get(0).blockPosition().toShortString() + ", " + two.get(1).blockPosition().toShortString() + ")",
                            !two.get(0).blockPosition().equals(two.get(1).blockPosition()));
                    for (DeathWormEntity w : two) {
                        check("marked with the nest", w.getPersistentData().getLong(Nests.TAG) == id);
                        check("grown (" + w.getWormAge() + ")", w.getWormAge() == 5);
                        check("scale 2 (" + w.getDeathwormScale() + ")", Math.abs(w.getDeathwormScale() - 2f) < 1e-3);
                        check("age scale 2 (" + w.getAgeScale() + ")", Math.abs(w.getAgeScale() - 2f) < 1e-3);
                        check("the nest's variant (" + w.getVariant() + ")", w.getVariant() == Nests.variant(id));
                        check("homed on the heart (" + w.getWormHome() + ")", w.getWormHome().equals(n.heart));
                        check("no agent tag", !RtsimAttachments.isTagged(w));
                        check("in the chamber", w.blockPosition().distManhattan(n.heart) <= 10);
                        seen.append(w.getUUID()).append(" at ").append(w.blockPosition().toShortString()).append("; ");
                    }
                    ScriptedEngine.Received c = censuses(id).get(censuses(id).size() - 1);
                    check("the census says 2 (" + c.view().getInt(DragonRecords.NestCensus.WORMS) + ")", c.view().getInt(DragonRecords.NestCensus.WORMS) == 2);
                    check("with its heart (" + c.aux() + ")", c.aux() == 0);
                    record(test, "worms", seen + "age 5, scale 2 (age scale 2), variant " + Nests.variant(id) + ", home " + n.heart.toShortString()
                            + ", tagged mic_rtsim_nest, no agent tag; NEST_CENSUS worms 2");
                })
                .thenSucceed();
    }

    /**
     * The boss rule on a nest worm's death: a player's kill is NEST_LOSS Player with the player; a wild
     * mob's is Predator; a death with no killer (a /kill) counts as a player's, with no player. A natural
     * worm's death sends nothing.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_losses")
    public static void nestWormDeathsFollowTheBossRule(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "nestWormDeathsFollowTheBossRule";
        Site s = new Site(helper, 1);
        ServerLevel level = s.level;
        long id = s.nest();
        long[] losses0 = {0};
        ServerPlayer editor = FakePlayers.editor(level);
        UUID[] naturalKiller = {null};
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    s.build();
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30);
                })
                .thenWaitUntil(waits.logic(() -> check("the nest placed", nests().nest(id) != null)))
                .thenExecute(() -> DragonKit.nestRenew(id, s.dim, 1, 3))
                .thenWaitUntil(waits.logic(() -> check("three worms", s.worms(id).size() == 3)))
                .thenExecute(() -> {
                    losses0[0] = nests().losses;
                    List<DeathWormEntity> w = s.worms(id);
                    w.get(0).hurt(level.damageSources().playerAttack(editor), 1000f);
                    Zombie z = EntityType.ZOMBIE.create(level);
                    z.moveTo(s.at.getX() + 0.5, s.ground, s.at.getZ() + 0.5);
                    level.addFreshEntity(z);
                    s.extras.add(z);
                    w.get(1).hurt(level.damageSources().mobAttack(z), 1000f);
                    w.get(2).kill();
                    // A natural worm, killed by a player.
                    DeathWormEntity natural = (DeathWormEntity) BuiltInRegistries.ENTITY_TYPE.get(Nests.DEATHWORM).create(level);
                    natural.moveTo(s.at.getX() + 20.5, s.ground, s.at.getZ() + 0.5);
                    level.addFreshEntity(natural);
                    ServerPlayer killer = s.player(new Vec3(s.at.getX() + 22, s.ground, s.at.getZ()), "nest_natural_killer");
                    naturalKiller[0] = killer.getUUID();
                    natural.hurt(level.damageSources().playerAttack(killer), 1000f);
                    check("the natural worm died", !natural.isAlive());
                })
                .thenWaitUntil(waits.logic(() -> check("three NEST_LOSS (" + losses(id).size() + ")", losses(id).size() == 3)))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> l = losses(id);
                    ScriptedEngine.Received byPlayer = l.get(0), byMob = l.get(1), byNothing = l.get(2);
                    for (ScriptedEngine.Received r : l)
                        check("one worm each", r.view().getInt(DragonRecords.NestLoss.COUNT) == 1);
                    check("a player's kill: Player (" + byPlayer.aux() + ")", byPlayer.aux() == DragonRecords.NestLoss.CAUSE_PLAYER);
                    check("with the player", DragonKit.uuidAt(byPlayer, DragonRecords.NestLoss.PLAYER_MOST).equals(editor.getUUID()));
                    check("a zombie's kill: Predator (" + byMob.aux() + ")", byMob.aux() == DragonRecords.NestLoss.CAUSE_PREDATOR);
                    check("with no player", DragonKit.uuidAt(byMob, DragonRecords.NestLoss.PLAYER_MOST).equals(new UUID(0, 0)));
                    check("no killer: Player (" + byNothing.aux() + ")", byNothing.aux() == DragonRecords.NestLoss.CAUSE_PLAYER);
                    check("with no player", DragonKit.uuidAt(byNothing, DragonRecords.NestLoss.PLAYER_MOST).equals(new UUID(0, 0)));
                    check("the natural worm's death sent nothing (" + (nests().losses - losses0[0]) + " losses)", nests().losses == losses0[0] + 3);
                    for (ScriptedEngine.Received r : DragonKit.engine().received(DragonRecords.NEST_LOSS))
                        check("no NEST_LOSS names the natural worm's killer", !DragonKit.uuidAt(r, DragonRecords.NestLoss.PLAYER_MOST).equals(naturalKiller[0]));
                    check("the nest knows no worm", nests().nest(id).worms.isEmpty());
                    record(test, "losses", "player kill: cause " + byPlayer.aux() + " with the player; zombie kill: cause " + byMob.aux()
                            + "; /kill: cause " + byNothing.aux() + " with no player; a natural worm killed by a player: nothing sent");
                })
                .thenSucceed();
    }

    /**
     * Breaking the heart: NEST_HEART with the player, the nest broken and its owed worms forgotten, so
     * none is placed once the player that held them back is gone; a census then says HEART_MISSING. The
     * engine's reform, a PLACE_NEST for the same nest (OGC79), sets the heart back where it was, inert,
     * and answers NEST_FOUND placed with it; a NEST_RENEW after it places its worm.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_heart")
    public static void breakingTheHeartStopsTheNest(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "breakingTheHeartStopsTheNest";
        Site s = new Site(helper, 1);
        ServerLevel level = s.level;
        long id = s.nest();
        ServerPlayer editor = FakePlayers.editor(level);
        ServerPlayer[] watcher = {null};
        int[] censuses0 = {0}, found0 = {0};
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    s.build();
                    DragonConfig.Test.nestRetryTicks = 20;
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30);
                })
                .thenWaitUntil(waits.logic(() -> check("the nest placed", nests().nest(id) != null)))
                .thenExecute(() -> {
                    BlockPos h = nests().nest(id).heart;
                    watcher[0] = s.player(Vec3.atCenterOf(h.offset(2, 1, 0)), "nest_watcher");
                    DragonKit.nestRenew(id, s.dim, 1, 2);
                })
                .thenWaitUntil(waits.logic(() -> check("two worms owed", nests().nest(id).owed == 2)))
                .thenIdle(60)
                .thenExecute(() -> {
                    Nests.Nest n = nests().nest(id);
                    check("none placed with a player by the heart", n.worms.isEmpty() && n.owed == 2);
                    check("the heart breaks", editor.gameMode.destroyBlock(n.heart));
                })
                .thenWaitUntil(waits.logic(() -> check("NEST_HEART", !DragonKit.received(DragonRecords.NEST_HEART, DragonRecords.NestHeart.NEST, id).isEmpty())))
                .thenExecute(() -> {
                    Nests.Nest n = nests().nest(id);
                    ScriptedEngine.Received r = DragonKit.received(DragonRecords.NEST_HEART, DragonRecords.NestHeart.NEST, id).get(0);
                    check("with the player who broke it", DragonKit.uuidAt(r, DragonRecords.NestHeart.PLAYER_MOST).equals(editor.getUUID()));
                    check("the nest broken, nothing owed", n.broken && n.owed == 0);
                    check("the heart gone", !level.getBlockState(n.heart).is(Blocks.SPAWNER));
                    FakePlayers.remove(watcher[0]);
                    s.players.remove(watcher[0]);
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    Nests.Nest n = nests().nest(id);
                    check("no worm placed after the player left", n.worms.isEmpty() && s.allWorms().isEmpty());
                    censuses0[0] = censuses(id).size();
                    nests().loaded(level.getChunkAt(n.heart)); // a fresh load of the heart's chunk
                })
                .thenWaitUntil(waits.logic(() -> check("a census", censuses(id).size() > censuses0[0])))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = censuses(id).get(censuses(id).size() - 1);
                    check("HEART_MISSING (" + c.aux() + ")", (c.aux() & DragonRecords.NestCensus.FLAG_HEART_MISSING) != 0);
                    check("no worms (" + c.view().getInt(DragonRecords.NestCensus.WORMS) + ")", c.view().getInt(DragonRecords.NestCensus.WORMS) == 0);
                    record(test, "broken", "NEST_HEART with the breaking player; 2 owed worms forgotten, none placed in 100 ticks after the "
                            + "watcher left; NEST_CENSUS worms 0 flags " + c.aux() + " (HEART_MISSING)");
                    found0[0] = found(id).size();
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30); // the engine's reform
                })
                .thenWaitUntil(waits.logic(() -> check("NEST_FOUND for the reform", found(id).size() > found0[0])))
                .thenExecute(() -> {
                    Nests.Nest n = nests().nest(id);
                    ScriptedEngine.Received f = found(id).get(found(id).size() - 1);
                    check("placed with the same heart (" + f.aux() + ", " + heartOf(f) + ")", f.aux() == DragonRecords.NestFound.PLACED && heartOf(f).equals(n.heart));
                    check("the heart re-formed", !n.broken && level.getBlockState(n.heart).is(Blocks.SPAWNER));
                    CompoundTag t = spawnerTag(level, n.heart);
                    check("a Death Worm spawner, inert again", t.getShort("RequiredPlayerRange") == 0
                            && t.getCompound("SpawnData").getCompound("entity").getString("id").equals("iceandfire:deathworm"));
                    censuses0[0] = censuses(id).size();
                    nests().loaded(level.getChunkAt(n.heart));
                })
                .thenWaitUntil(waits.logic(() -> check("a census after the reform", censuses(id).size() > censuses0[0])))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = censuses(id).get(censuses(id).size() - 1);
                    check("its heart there (" + c.aux() + ")", c.aux() == 0);
                    DragonKit.nestRenew(id, s.dim, 1, 1);
                })
                .thenWaitUntil(waits.logic(() -> check("a worm after the renew", s.worms(id).size() == 1)))
                .thenExecute(() -> {
                    Nests.Nest n = nests().nest(id);
                    record(test, "reformed", "a PLACE_NEST for the broken nest set the heart back at " + n.heart.toShortString()
                            + " (NEST_FOUND placed, RequiredPlayerRange 0, census flags 0); a NEST_RENEW 1 then placed 1 worm");
                })
                .thenSucceed();
    }

    // ---- raids --------------------------------------------------------------------------------------

    /**
     * NEST_SORTIE: a worm sent at a column with obsidian on the ground glides there through the ground,
     * rises, stops under the obsidian and glides home; one sent at an open column surfaces into the air
     * with its AI and physics back, then glides home at the timeout. Home, each has its AI and physics
     * and no sortie mark.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_sortie")
    public static void sortiesStopUnderArmourOrSurface(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 2400);
        String test = "sortiesStopUnderArmourOrSurface";
        Site s = new Site(helper, 2);
        ServerLevel level = s.level;
        long id = s.nest();
        int timeout = 400;
        BlockPos[] armoured = {null}, open = {null};
        UUID[] toArmour = {null}, toOpen = {null};
        long[] sent = {0};
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    s.build();
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30);
                })
                .thenWaitUntil(waits.logic(() -> check("the nest placed", nests().nest(id) != null)))
                .thenExecute(() -> DragonKit.nestRenew(id, s.dim, 1, 2))
                .thenWaitUntil(waits.logic(() -> check("two worms", s.worms(id).size() == 2)))
                .thenIdle(20)
                .thenExecute(() -> {
                    BlockPos h = nests().nest(id).heart;
                    armoured[0] = new BlockPos(h.getX() + 20, s.ground - 1, h.getZ());
                    open[0] = new BlockPos(h.getX(), s.ground - 1, h.getZ() + 20);
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            level.setBlock(armoured[0].offset(dx, 0, dz), Blocks.OBSIDIAN.defaultBlockState(), 2);
                    List<DeathWormEntity> w = s.worms(id);
                    toArmour[0] = w.get(0).getUUID();
                    toOpen[0] = w.get(1).getUUID();
                    sent[0] = nests().wormsSent;
                    DragonKit.nestSortie(id, s.dim, armoured[0].getX(), armoured[0].getZ(), timeout, 1);
                    DragonKit.nestSortie(id, s.dim, open[0].getX(), open[0].getZ(), timeout, 1);
                })
                .thenWaitUntil(waits.logic(() -> check("both sent (" + (nests().wormsSent - sent[0]) + ")", nests().wormsSent == sent[0] + 2)))
                .thenExecute(() -> {
                    check("the first worm out", nests().onSortie(toArmour[0]));
                    DeathWormEntity w = (DeathWormEntity) level.getEntity(toArmour[0]);
                    check("gliding: no AI, no physics", w.isNoAi() && w.noPhysics && w.getPersistentData().getBoolean(Nests.SORTIE_TAG));
                })
                .thenWaitUntil(waits.logic(() -> check("the first stopped under the obsidian", has(nests().outcomes(toArmour[0]), Nests.OUT_STOPPED))))
                .thenWaitUntil(waits.logic(() -> check("the second surfaced", has(nests().outcomes(toOpen[0]), Nests.OUT_SURFACED))))
                .thenExecute(() -> {
                    Nests.Outcome stop = first(nests().outcomes(toArmour[0]), Nests.OUT_STOPPED);
                    check("in the armoured column", stop.x() == armoured[0].getX() && stop.z() == armoured[0].getZ());
                    check("below the obsidian (" + stop.at().getY() + " vs " + armoured[0].getY() + ")", stop.at().getY() < armoured[0].getY());
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            check("the obsidian intact", level.getBlockState(armoured[0].offset(dx, 0, dz)).is(Blocks.OBSIDIAN));
                    Nests.Outcome up = first(nests().outcomes(toOpen[0]), Nests.OUT_SURFACED);
                    check("in the open column", up.x() == open[0].getX() && up.z() == open[0].getZ());
                    check("above the ground (" + up.at().getY() + " vs " + s.ground + ")", up.at().getY() >= s.ground);
                    DeathWormEntity w = (DeathWormEntity) level.getEntity(toOpen[0]);
                    if (w != null && nests().onSortie(toOpen[0]) && !has(nests().outcomes(toOpen[0]), Nests.OUT_HOME))
                        check("fighting: AI and physics back", !w.isNoAi() && !w.noPhysics);
                    record(test, "out", "armoured column " + armoured[0].toShortString() + ": stopped at y " + stop.at().getY()
                            + " under the obsidian at tick +" + stop.tick() + "; open column " + open[0].toShortString() + ": surfaced at "
                            + up.at().toShortString());
                })
                .thenWaitUntil(waits.logic(() -> check("both home", has(nests().outcomes(toArmour[0]), Nests.OUT_HOME) && has(nests().outcomes(toOpen[0]), Nests.OUT_HOME))))
                .thenExecute(() -> {
                    BlockPos h = nests().nest(id).heart;
                    for (UUID u : new UUID[] {toArmour[0], toOpen[0]}) {
                        DeathWormEntity w = (DeathWormEntity) level.getEntity(u);
                        check("alive and loaded", w != null && w.isAlive());
                        check("AI and physics back", !w.isNoAi() && !w.noPhysics);
                        check("no sortie mark", !w.getPersistentData().getBoolean(Nests.SORTIE_TAG));
                        BlockPos d = w.blockPosition().subtract(h);
                        check("home in the chamber (" + w.blockPosition().toShortString() + ")", Math.abs(d.getX()) <= Nests.HALF + 1
                                && Math.abs(d.getZ()) <= Nests.HALF + 1 && d.getY() >= -1 && d.getY() <= Nests.AIR + 1);
                        check("off its sortie", !nests().onSortie(u));
                    }
                    record(test, "home", "both worms glided home to " + h.toShortString() + " with their AI and physics back ("
                            + nests().stoppedUnderArmour + " stopped, " + nests().surfaced + " surfaced, " + nests().homeComings + " home this session)");
                })
                .thenSucceed();
    }

    static boolean has(List<Nests.Outcome> l, int kind) {
        return first(l, kind) != null;
    }

    static Nests.Outcome first(List<Nests.Outcome> l, int kind) {
        for (Nests.Outcome o : l)
            if (o.kind() == kind)
                return o;
        return null;
    }

    // ---- the heart never spawns ---------------------------------------------------------------------

    /**
     * The heart is a spawner that never spawns: with a player beside it for 200 ticks no worm appears
     * (RequiredPlayerRange 0). With its range set to 16 (as a player's edit could), a spawn it tries is
     * cancelled at FinalizeSpawn (recorded; Ice and Fire's placement rules may refuse first), and still no
     * worm appears.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_inert")
    public static void theHeartNeverSpawns(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 1200);
        String test = "theHeartNeverSpawns";
        Site s = new Site(helper, 1);
        ServerLevel level = s.level;
        long id = s.nest();
        long[] cancelled0 = {0};
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    s.build();
                    DragonKit.placeNest(id, s.dim, s.chunk.x, s.chunk.z, 30);
                })
                .thenWaitUntil(waits.logic(() -> check("the nest placed", nests().nest(id) != null)))
                .thenExecute(() -> s.player(Vec3.atCenterOf(nests().nest(id).heart.offset(2, 1, 1)), "nest_neighbour"))
                .thenIdle(200)
                .thenExecute(() -> {
                    check("no worm with RequiredPlayerRange 0 (" + s.allWorms().size() + ")", s.allWorms().isEmpty());
                    cancelled0[0] = nests().spawnerCancelled;
                    setRequiredRange(level, nests().nest(id).heart, 16);
                })
                .thenIdle(300)
                .thenExecute(() -> {
                    check("no worm with range 16 either (" + s.allWorms().size() + ")", s.allWorms().isEmpty());
                    long c = nests().spawnerCancelled - cancelled0[0];
                    setRequiredRange(level, nests().nest(id).heart, 0);
                    record(test, "inert", "a player 2 blocks from the heart for 200 ticks: no worm; RequiredPlayerRange 16 for 300 ticks: no worm, "
                            + c + " spawn(s) cancelled at FinalizeSpawn");
                })
                .thenSucceed();
    }

    // ---- the engine's nest (native) -------------------------------------------------------------------

    /**
     * The engine's nest (native only): with nests on, an oil chunk founds a nest the plugin places in it.
     * (a) No well drills near it: asleep, no worms. (b) A test well drills in the chunk beside it, in a
     * district seeded by a machine on obsidian: awake, NEST_RENEW 2, two worms placed. (c) A player kills
     * both: the engine counts them lost to players and a regrowth wait runs; the well stops and the wait
     * stays put; the well drills again and a worm regrows.
     */
    @GameTest(template = DragonKit.ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "nest_native")
    public static void theEngineWakesItsNestByAWell(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 16_000);
        String test = "theEngineWakesItsNestByAWell";
        if (!DragonKit.nativeEngine()) {
            record(test, "skipped", "on the stub: the engine founds and wakes nests (native only)");
            helper.succeed();
            return;
        }
        NativeEngine engine = (NativeEngine) DragonKit.engine();
        Site s = new Site(helper, 2);
        ServerLevel level = s.level;
        ChunkPos well = new ChunkPos(s.chunk.x + 1, s.chunk.z);
        long[] nest = {0};
        long[] renews0 = {0};
        long[] wait = {0};
        long[] t0 = {0};
        s.undo.add(() -> {
            engine.nestDay(0);
            TestWells.clear();
            IndustryConfig.Test.set(IndustryConfig.SWEEP_PERIOD, null);
            IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, null);
            IndustryTestKit.ind().districts().forgetArea(level, well.x - 1, well.z - 1, well.x + 1, well.z + 1);
            DragonKit.restoreOilConfig();
            // Nests on, the engine founded a nest under every oil chunk earlier tests reported and asked for
            // each: those waiting for their chunk are forgotten, so no later test carves one.
            for (long id : List.copyOf(nests().data().pending.keySet()))
                nests().forgetForTest(id);
        });
        BlockPos machine = new BlockPos(well.getMinBlockX() + 8, 0, well.getMinBlockZ() + 8);
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the site's chunks loaded", s.ready())))
                .thenExecute(() -> {
                    engine.nestDay(20);
                    IndustryTestKit.fastConfig();
                    s.build();
                    BlockPos m = machine.atY(s.ground);
                    level.setBlock(m.below(), Blocks.OBSIDIAN.defaultBlockState(), 3);
                    level.setBlock(m, TestMachines.MACHINE.get().defaultBlockState(), 3);
                    IndustryTestKit.playerPlaced(helper, m);
                    renews0[0] = nests().renews;
                    DragonKit.oilConfig(DragonRecords.OilConfig.FLAG_PLACE_NESTS);
                    DragonKit.oilChunks(level, List.of(new int[] {s.chunk.x, s.chunk.z, 40_000}));
                })
                .thenWaitUntil(waits.logic(() -> check("the well's district", IndustryTestKit.ind().districts().at(level.dimension(), well.toLong()) != null)))
                .thenWaitUntil(waits.logic(() -> {
                    Nests.Nest n = nests().placedIn(s.dim, s.chunk);
                    check("the engine's nest placed in the oil chunk (asked: " + nests().asked + "; " + nests().failedCount + " failed)", n != null);
                    nest[0] = n.id;
                    s.nests.add(n.id);
                }))
                .thenWaitUntil(waits.logic(() -> {
                    long[] i = engine.nestInfo(nest[0]);
                    check("the engine has it placed", i != null && i[NativeEngine.NEST_STATE] == 1);
                }))
                .thenIdle(240)
                .thenExecute(() -> {
                    long[] i = engine.nestInfo(nest[0]);
                    check("(a) asleep (" + i[NativeEngine.NEST_AWAKE] + ")", i[NativeEngine.NEST_AWAKE] == 0);
                    check("(a) no worms (" + i[NativeEngine.NEST_WORMS] + ", " + s.worms(nest[0]).size() + ")",
                            i[NativeEngine.NEST_WORMS] == 0 && s.worms(nest[0]).isEmpty() && nests().nest(nest[0]).owed == 0);
                    record(test, "a", "nest " + nest[0] + " at " + nests().nest(nest[0]).heart.toShortString() + " asleep with no well: awake 0, "
                            + "worms 0, K " + i[NativeEngine.NEST_K] + ", nests " + i[NativeEngine.NEST_COUNT]);
                    TestWells.drill(level.dimension(), well.x, well.z);
                })
                .thenWaitUntil(waits.logic(() -> check("(b) awake", engine.nestInfo(nest[0])[NativeEngine.NEST_AWAKE] == 1)))
                .thenWaitUntil(waits.logic(() -> check("(b) two worms (" + s.worms(nest[0]).size() + ")", s.worms(nest[0]).size() == 2)))
                .thenWaitUntil(waits.logic(() -> check("(b) the engine counts them", engine.nestInfo(nest[0])[NativeEngine.NEST_WORMS] == 2)))
                .thenExecute(() -> {
                    long[] i = engine.nestInfo(nest[0]);
                    check("(b) one NEST_RENEW (" + (nests().renews - renews0[0]) + ")", nests().renews - renews0[0] >= 1);
                    record(test, "b", "a well drilling in " + well + ": awake 1, NEST_RENEW " + (nests().renews - renews0[0]) + ", worms "
                            + i[NativeEngine.NEST_WORMS] + " at stage " + i[NativeEngine.NEST_STAGE] + ", renewed " + i[NativeEngine.NEST_RENEWED]
                            + ", hatched " + i[NativeEngine.NEST_HATCHED]);
                    ServerPlayer editor = FakePlayers.editor(level);
                    for (DeathWormEntity w : s.worms(nest[0]))
                        w.hurt(level.damageSources().playerAttack(editor), 1000f);
                })
                .thenWaitUntil(waits.logic(() -> {
                    long[] i = engine.nestInfo(nest[0]);
                    check("(c) the engine lost both to players (worms " + i[NativeEngine.NEST_WORMS] + ", lost " + i[NativeEngine.NEST_LOST_PLAYER]
                            + ", wait " + i[NativeEngine.NEST_WAIT_TICKS] + ")", i[NativeEngine.NEST_WORMS] == 0 && i[NativeEngine.NEST_LOST_PLAYER] == 2
                            && i[NativeEngine.NEST_WAIT_TICKS] >= 0);
                }))
                .thenExecute(() -> {
                    long[] i = engine.nestInfo(nest[0]);
                    record(test, "c", "both killed by a player: worms 0, lost to players 2, pressure " + i[NativeEngine.NEST_PRESSURE_MILLI]
                            + "/1000, regrowth wait " + i[NativeEngine.NEST_WAIT_TICKS] + " ticks");
                    TestWells.stop(level.dimension(), well.x, well.z);
                })
                .thenWaitUntil(waits.logic(() -> check("(c) asleep once the well stops", engine.nestInfo(nest[0])[NativeEngine.NEST_AWAKE] == 0)))
                .thenExecute(() -> wait[0] = engine.nestInfo(nest[0])[NativeEngine.NEST_WAIT_TICKS])
                .thenIdle(240)
                .thenExecute(() -> {
                    long[] i = engine.nestInfo(nest[0]);
                    check("(c) the wait stays put asleep (" + wait[0] + " then " + i[NativeEngine.NEST_WAIT_TICKS] + ")",
                            i[NativeEngine.NEST_WAIT_TICKS] == wait[0] && i[NativeEngine.NEST_AWAKE] == 0);
                    check("(c) no worm regrew asleep", s.worms(nest[0]).isEmpty());
                    TestWells.drill(level.dimension(), well.x, well.z);
                    t0[0] = level.getGameTime();
                })
                .thenWaitUntil(waits.logic(() -> check("(c) a worm regrows", !s.worms(nest[0]).isEmpty())))
                .thenExecute(() -> {
                    long[] i = engine.nestInfo(nest[0]);
                    record(test, "c", "asleep for 240 ticks the wait stayed at " + wait[0] + " ticks; drilling again, a worm regrew after "
                            + (level.getGameTime() - t0[0]) + " ticks (worms " + i[NativeEngine.NEST_WORMS] + ", renewed " + i[NativeEngine.NEST_RENEWED] + ")");
                })
                .thenSucceed();
    }
}
