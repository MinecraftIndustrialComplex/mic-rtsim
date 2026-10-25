package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.EntitySectionManagerAccessor;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ServerLevelAccessor;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonBridge;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.Nests;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.entity.ChunkEntities;
import net.minecraft.world.level.entity.EntityPersistentStorage;
import net.minecraft.world.phys.Vec3;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ThreadLocalRandom;
import java.util.function.Consumer;

/**
 * Shared helpers for the Ice and Fire GameTests ({@code ./gametest.sh iceandfire}). The tests drive the
 * server's real bridge session, whose engine is the Rust one (the default, with the dragons behind it)
 * or the stub ({@code -PrtsimEngine=stub}). A test scripts dragon commands as the engine's dragons would
 * send them (docs/boundary.md §6.4): the stub writes them with its own dictionary, the native engine
 * takes them as DRAGON_COMMAND orders with Java's dictionary ids and writes them as its dragons' own.
 * Untagged dragons are answered by the engine itself on the native engine (its adoption rules) and
 * scripted on the stub, which decides nothing.
 */
public final class DragonKit {
    private DragonKit() {}

    public static final String ARENA = "arena";
    public static final String FIRE = "iceandfire:fire_dragon";
    /** 30 days: stage 2 (Ice and Fire's stages begin at 25, 50, 75 and 100 days), no longer a baby, can fly. */
    public static final int AGE_30_DAYS = 30 * 24_000;

    /** Test agents: namespace 0x44 (R13) over random 48-bit handles, far from the engine's own rows. */
    public static long newAgent() {
        return (0x44L << 56) | (1L << 47) | (ThreadLocalRandom.current().nextLong() & 0x7FFF_FFFF_FFFFL);
    }

    public static RtsimSession session() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s;
    }

    public static ScriptedEngine engine() {
        if (!(session().engine() instanceof ScriptedEngine e))
            throw new GameTestAssertException("the session's engine takes no scripted orders: " + session().engine().describe());
        return e;
    }

    public static boolean nativeEngine() {
        return engine() instanceof NativeEngine;
    }

    public static DragonBridge bridge() {
        return DragonBridge.INSTANCE;
    }

    public static void check(String what, boolean ok) {
        if (!ok)
            throw new GameTestAssertException(what);
    }

    public static void record(String test, String what, Object value) {
        MicRtsim.LOGGER.info("[gametest] {}: {} = {}", test, what, value);
    }

    public static String dimension(GameTestHelper helper) {
        return helper.getLevel().dimension().location().toString();
    }

    /** The arena's middle on its floor, absolute. */
    public static Vec3 centre(GameTestHelper helper) {
        return helper.absoluteVec(new Vec3(5.5, 2, 5.5));
    }

    /**
     * Forces the chunks within {@code radius} of the arena (a GameTest forces only its structure's own), so
     * a flying dragon stays entity-ticking. Returns the undo.
     */
    public static Runnable forceAround(GameTestHelper helper, int radius) {
        ServerLevel level = helper.getLevel();
        ChunkPos c = new ChunkPos(BlockPos.containing(centre(helper)));
        List<ChunkPos> forced = new ArrayList<>();
        for (int dx = -radius; dx <= radius; dx++)
            for (int dz = -radius; dz <= radius; dz++) {
                ChunkPos p = new ChunkPos(c.x + dx, c.z + dz);
                if (level.setChunkForced(p.x, p.z, true))
                    forced.add(p);
            }
        return () -> forced.forEach(p -> level.setChunkForced(p.x, p.z, false));
    }

    // ---- scripting the engine side --------------------------------------------------------------

    /** Writes one dragon command record: {@code out.dict} gives a dictionary id, {@code reserve} the record. */
    private static void script(Consumer<StubEngine.CommandOut> command) {
        ScriptedEngine e = engine();
        if (e instanceof StubEngine stub) {
            stub.script(command);
            return;
        }
        if (!(e instanceof NativeEngine n))
            throw new GameTestAssertException("no scripted dragon commands on " + e.describe());
        RecordOut out = new RecordOut();
        command.accept(out);
        if (out.b == null)
            throw new GameTestAssertException("the script wrote no record");
        n.dragonCommand(out.names, out.b.array());
    }

    /** One record with Java's dictionary ids, for the native engine's DRAGON_COMMAND order. */
    private static final class RecordOut implements StubEngine.CommandOut {
        private final List<NativeEngine.DictEntry> names = new ArrayList<>();
        private ByteBuffer b;

        @Override
        public int dict(int dictKind, String name) {
            int id = dictKind == Obs.DICT_DIMENSION
                    ? session().dictId(Obs.DICT_DIMENSION, name)
                    : session().dictId(dictKind, name);
            if (dictKind != Obs.DICT_DIMENSION)
                names.add(new NativeEngine.DictEntry(dictKind, id, name));
            return id;
        }

        @Override
        public int reserve(int kind, int size, int aux) {
            if (b != null)
                throw new IllegalStateException("one record per DRAGON_COMMAND order");
            int aligned = (size + 7) & ~7;
            b = ByteBuffer.allocate(aligned).order(Layout.ORDER);
            b.putShort(Layout.R_KIND, (short) kind);
            b.putShort(Layout.R_SIZE, (short) aligned);
            b.putInt(Layout.R_AUX, aux);
            return 0;
        }

        @Override
        public ByteBuffer buffer() {
            return b;
        }
    }

    /** What a scripted DRAGON_MATERIALIZE carries (docs/boundary.md §6.4). */
    public record Spawn(long agent, int epoch, String species, String dimension, Vec3 at, int ageTicks, int hunger, boolean male,
                        int variant, float health, BlockPos home, Vec3 target, UUID hostileTo, int flags) {}

    public static void materialize(Spawn s) {
        materialize(s, -1);
    }

    /** A scripted DRAGON_MATERIALIZE whose body carries {@code oil} reserve points (MAT_OIL); -1 none. */
    public static void materialize(Spawn s, int oil) {
        script(out -> {
            int species = out.dict(Obs.DICT_BODY, s.species());
            int dim = out.dict(Obs.DICT_DIMENSION, s.dimension());
            int flags = s.flags() | (s.home() != null ? DragonRecords.Materialize.FLAG_HAS_HOME : 0)
                    | (s.hostileTo() != null ? DragonRecords.Materialize.FLAG_HOSTILE : 0)
                    | (oil >= 0 ? DragonRecords.Materialize.FLAG_OIL : 0);
            int at = out.reserve(DragonRecords.DRAGON_MATERIALIZE, DragonRecords.Materialize.SIZE, flags);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.Materialize.AGENT, s.agent());
            b.putInt(at + DragonRecords.Materialize.EPOCH, s.epoch());
            b.putInt(at + DragonRecords.Materialize.SPECIES, species);
            b.putInt(at + DragonRecords.Materialize.DIM, dim);
            b.putInt(at + DragonRecords.Materialize.AGE_TICKS, s.ageTicks());
            b.putDouble(at + DragonRecords.Materialize.X, s.at().x);
            b.putDouble(at + DragonRecords.Materialize.Y, s.at().y);
            b.putDouble(at + DragonRecords.Materialize.Z, s.at().z);
            b.putInt(at + DragonRecords.Materialize.HUNGER, s.hunger());
            b.putInt(at + DragonRecords.Materialize.GENDER, s.male() ? 1 : 0);
            b.putInt(at + DragonRecords.Materialize.VARIANT, s.variant());
            b.putFloat(at + DragonRecords.Materialize.HEALTH, s.health());
            if (oil >= 0)
                b.putInt(at + DragonRecords.Materialize.OIL, oil);
            if (s.home() != null) {
                b.putInt(at + DragonRecords.Materialize.HOME_X, s.home().getX());
                b.putInt(at + DragonRecords.Materialize.HOME_Y, s.home().getY());
                b.putInt(at + DragonRecords.Materialize.HOME_Z, s.home().getZ());
            }
            Vec3 t = s.target() != null ? s.target() : s.at();
            b.putDouble(at + DragonRecords.Materialize.TARGET_X, t.x);
            b.putDouble(at + DragonRecords.Materialize.TARGET_Y, t.y);
            b.putDouble(at + DragonRecords.Materialize.TARGET_Z, t.z);
            if (s.hostileTo() != null) {
                b.putLong(at + DragonRecords.Materialize.TARGET_PLAYER_MOST, s.hostileTo().getMostSignificantBits());
                b.putLong(at + DragonRecords.Materialize.TARGET_PLAYER_LEAST, s.hostileTo().getLeastSignificantBits());
            }
        });
    }

    /** A young fire dragon at {@code at}: 30 days, hunger 50, female, the first colour, full health. */
    public static Spawn young(long agent, GameTestHelper helper, Vec3 at, int flags) {
        return new Spawn(agent, 1, FIRE, dimension(helper), at, AGE_30_DAYS, 50, false, 0, 0f, null, at, null, flags);
    }

    public static void steer(long agent, int epoch, int flags, Vec3 target) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE); // the native order needs one name; the stub sends its DICT once
            int at = out.reserve(DragonRecords.DRAGON_STEER, DragonRecords.Steer.SIZE, flags);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.Steer.AGENT, agent);
            b.putInt(at + DragonRecords.Steer.EPOCH, epoch);
            b.putDouble(at + DragonRecords.Steer.X, target.x);
            b.putDouble(at + DragonRecords.Steer.Y, target.y);
            b.putDouble(at + DragonRecords.Steer.Z, target.z);
        });
    }

    public static void depart(long agent, int epoch, int reason, int timeoutTicks, Vec3 target) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE);
            int at = out.reserve(DragonRecords.DRAGON_DEPART, DragonRecords.Depart.SIZE, reason);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.Depart.AGENT, agent);
            b.putInt(at + DragonRecords.Depart.EPOCH, epoch);
            b.putInt(at + DragonRecords.Depart.TIMEOUT_TICKS, timeoutTicks);
            b.putDouble(at + DragonRecords.Depart.X, target.x);
            b.putDouble(at + DragonRecords.Depart.Y, target.y);
            b.putDouble(at + DragonRecords.Depart.Z, target.z);
        });
    }

    public static void adopt(UUID entity, long agent, int epoch, BlockPos home) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE);
            int at = out.reserve(DragonRecords.DRAGON_ADOPT, DragonRecords.Adopt.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.Adopt.ENTITY_MOST, entity.getMostSignificantBits());
            b.putLong(at + DragonRecords.Adopt.ENTITY_LEAST, entity.getLeastSignificantBits());
            b.putLong(at + DragonRecords.Adopt.AGENT, agent);
            b.putInt(at + DragonRecords.Adopt.EPOCH, epoch);
            if (home != null) {
                b.putInt(at + DragonRecords.Adopt.FLAGS, DragonRecords.Adopt.FLAG_HAS_HOME);
                b.putInt(at + DragonRecords.Adopt.HOME_X, home.getX());
                b.putInt(at + DragonRecords.Adopt.HOME_Y, home.getY());
                b.putInt(at + DragonRecords.Adopt.HOME_Z, home.getZ());
            }
        });
    }

    public static void discard(UUID entity, int reason) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE);
            int at = out.reserve(DragonRecords.DRAGON_DISCARD, DragonRecords.Discard.SIZE, reason);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.Discard.ENTITY_MOST, entity.getMostSignificantBits());
            b.putLong(at + DragonRecords.Discard.ENTITY_LEAST, entity.getLeastSignificantBits());
        });
    }

    /**
     * A scripted PLACE_LAIR (the stub's only: the native engine's DRAGON_COMMAND order names no structure,
     * so the native tests have the engine place its own).
     */
    public static void placeLair(long field, String dimension, String structure, int x, int z, int radius) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int st = out.dict(Obs.DICT_STRUCTURE, structure);
            int at = out.reserve(DragonRecords.PLACE_LAIR, DragonRecords.PlaceLair.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.PlaceLair.FIELD, field);
            b.putInt(at + DragonRecords.PlaceLair.DIM, dim);
            b.putInt(at + DragonRecords.PlaceLair.STRUCTURE, st);
            b.putInt(at + DragonRecords.PlaceLair.X, x);
            b.putInt(at + DragonRecords.PlaceLair.Z, z);
            b.putInt(at + DragonRecords.PlaceLair.RADIUS, radius);
        });
    }

    /** A scripted OIL_SURVEY. */
    public static void oilSurvey(String dimension, int x, int z, int radius) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE); // the native order needs one name; the stub sends its DICT once
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(DragonRecords.OIL_SURVEY, DragonRecords.OilSurvey.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putInt(at + DragonRecords.OilSurvey.DIM, dim);
            b.putInt(at + DragonRecords.OilSurvey.X, x);
            b.putInt(at + DragonRecords.OilSurvey.Z, z);
            b.putInt(at + DragonRecords.OilSurvey.RADIUS, radius);
        });
    }

    /** A scripted PLACE_NEST (OG-9e): nest {@code nest} in chunk (cx, cz), its floor {@code depth} under the surface. */
    public static void placeNest(long nest, String dimension, int cx, int cz, int depth) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE); // the native order needs one name; the stub sends its DICT once
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(DragonRecords.PLACE_NEST, DragonRecords.PlaceNest.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.PlaceNest.NEST, nest);
            b.putInt(at + DragonRecords.PlaceNest.DIM, dim);
            b.putInt(at + DragonRecords.PlaceNest.CX, cx);
            b.putInt(at + DragonRecords.PlaceNest.CZ, cz);
            b.putInt(at + DragonRecords.PlaceNest.DEPTH, depth);
        });
    }

    /**
     * A scripted NEST_RENEW: {@code count} worms of size {@code stage} (1 to 3) for nest {@code nest}, numbered as
     * the engine would number the nest's next renewal (one above the last this session applied, RP-14).
     */
    public static void nestRenew(long nest, String dimension, int stage, int count) {
        Nests.Nest n = bridge().nests.nest(nest);
        nestRenew(nest, dimension, stage, count, n == null ? 1 : n.nextSerial(session().engineLife()));
    }

    /** A scripted NEST_RENEW with its own renewal serial (RP-14): one at or below the last applied is a repeat. */
    public static void nestRenew(long nest, String dimension, int stage, int count, long serial) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE);
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(DragonRecords.NEST_RENEW, DragonRecords.NestRenew.SIZE, count);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.NestRenew.NEST, nest);
            b.putInt(at + DragonRecords.NestRenew.DIM, dim);
            b.putInt(at + DragonRecords.NestRenew.STAGE, stage);
            b.putInt(at + DragonRecords.NestRenew.SERIAL, (int) serial);
            b.putInt(at + DragonRecords.NestRenew.RESERVED, 0);
        });
    }

    /** A scripted NEST_SORTIE: up to {@code count} of nest {@code nest}'s worms raid column (x, z) for {@code timeout} ticks. */
    public static void nestSortie(long nest, String dimension, int x, int z, int timeout, int count) {
        script(out -> {
            out.dict(Obs.DICT_BODY, FIRE);
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(DragonRecords.NEST_SORTIE, DragonRecords.NestSortie.SIZE, count);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + DragonRecords.NestSortie.NEST, nest);
            b.putInt(at + DragonRecords.NestSortie.DIM, dim);
            b.putInt(at + DragonRecords.NestSortie.X, x);
            b.putInt(at + DragonRecords.NestSortie.Z, z);
            b.putInt(at + DragonRecords.NestSortie.TIMEOUT_TICKS, timeout);
        });
    }

    /**
     * Runs {@code undo} once when the test ends, passed or failed (a failed assertion skips the rest of
     * its sequence, so the sequence's own last step cannot restore what the test changed).
     */
    public static void onEnd(GameTestHelper helper, Runnable undo) {
        boolean[] done = {false};
        Runnable once = () -> {
            if (done[0])
                return;
            done[0] = true;
            try {
                undo.run();
            } catch (RuntimeException e) {
                MicRtsim.LOGGER.error("[gametest] restoring after a test failed", e);
            }
        };
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                once.run();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                once.run();
            }

            @Override
            public void testAddedForRerun(GameTestInfo old, GameTestInfo fresh, GameTestRunner runner) {}
        });
    }

    // ---- observations a test sends as the plugin or the core would ------------------------------

    /** OIL_CONFIG with these flags (the engine takes the last one); {@link #restoreOilConfig} undoes it. */
    public static void oilConfig(int flags) {
        bridge().sendOilConfig(flags, session().gameTime());
    }

    /** OIL_CONFIG from the plugin's switches again (the GameTest defaults: everything off). */
    public static void restoreOilConfig() {
        com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonConfig.Test.reset();
        oilConfig(com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonConfig.oilFlags());
    }

    /** OIL_CHUNKS for chunks {cx, cz, amount} of one dimension (source 2, CDG's), as the sampler sends them. */
    public static void oilChunks(ServerLevel level, List<int[]> chunks) {
        RtsimSession s = session();
        int dim = s.dimId(level.dimension());
        for (int from = 0; from < chunks.size(); from += DragonRecords.OilChunks.MAX) {
            int n = Math.min(DragonRecords.OilChunks.MAX, chunks.size() - from);
            ByteBuffer r = s.boundary().beginJournaled(DragonRecords.OIL_CHUNKS, DragonRecords.OilChunks.size(n), n, s.gameTime());
            r.putInt(DragonRecords.OilChunks.DIM, dim);
            r.putInt(DragonRecords.OilChunks.SOURCE, DragonRecords.OilChunks.SOURCE_CDG);
            for (int k = 0; k < n; k++) {
                int[] c = chunks.get(from + k);
                int e = DragonRecords.OilChunks.ENTRIES + DragonRecords.OilChunks.ENTRY * k;
                r.putInt(e + DragonRecords.OilChunks.E_CX, c[0]);
                r.putInt(e + DragonRecords.OilChunks.E_CZ, c[1]);
                r.putInt(e + DragonRecords.OilChunks.E_AMOUNT, c[2]);
                r.putInt(e + DragonRecords.OilChunks.E_FLAGS, c.length > 3 ? c[3] : 0);
            }
            s.boundary().commit(r);
        }
    }

    /**
     * A PLAYER record for a GameTest player (the core reports the server's player list, which a
     * GameTest's players are not on): where it is, with its height.
     */
    public static void reportPlayer(net.minecraft.server.level.ServerPlayer p) {
        RtsimSession s = session();
        int dim = s.dimId(p.level().dimension()); // before direct(): an id is defined before its first use
        int at = s.boundary().direct(Obs.PLAYER, Obs.Player.SIZE, dim, s.gameTime());
        if (at < 0)
            return;
        ByteBuffer b = s.boundary().openBuffer();
        b.putLong(at + Obs.Player.UUID_MOST, p.getUUID().getMostSignificantBits());
        b.putLong(at + Obs.Player.UUID_LEAST, p.getUUID().getLeastSignificantBits());
        b.putDouble(at + Obs.Player.X, p.getX());
        b.putDouble(at + Obs.Player.Y, p.getY());
        b.putDouble(at + Obs.Player.Z, p.getZ());
    }

    /**
     * A PLAYER record for a player who is nowhere in the level (the engine reads the record alone): a
     * presence for the oil gate's field flights and lair checks without a body in the world.
     */
    public static void reportPlayerAt(UUID uuid, ServerLevel level, double x, double y, double z) {
        RtsimSession s = session();
        int dim = s.dimId(level.dimension());
        int at = s.boundary().direct(Obs.PLAYER, Obs.Player.SIZE, dim, s.gameTime());
        if (at < 0)
            return;
        ByteBuffer b = s.boundary().openBuffer();
        b.putLong(at + Obs.Player.UUID_MOST, uuid.getMostSignificantBits());
        b.putLong(at + Obs.Player.UUID_LEAST, uuid.getLeastSignificantBits());
        b.putDouble(at + Obs.Player.X, x);
        b.putDouble(at + Obs.Player.Y, y);
        b.putDouble(at + Obs.Player.Z, z);
    }

    /**
     * A site far from the arenas and from every earlier run's (the GameTest world is reused): a block
     * column on a 4,096-block grid, 300,000 to 2,300,000 blocks out, so it is past Ice and Fire's
     * dangerous distance from spawn and its chunks were never generated.
     */
    public static BlockPos farSite() {
        java.util.concurrent.ThreadLocalRandom r = java.util.concurrent.ThreadLocalRandom.current();
        int x = (300_000 + r.nextInt(0, 500) * 4096) * (r.nextBoolean() ? 1 : -1);
        int z = (300_000 + r.nextInt(0, 500) * 4096) * (r.nextBoolean() ? 1 : -1);
        return new BlockPos(x + 8, 0, z + 8);
    }

    /** Forces the chunks within {@code radius} chunks of {@code centre}; returns the undo. */
    public static Runnable forceChunks(ServerLevel level, ChunkPos centre, int radius) {
        List<ChunkPos> forced = new ArrayList<>();
        for (int dx = -radius; dx <= radius; dx++)
            for (int dz = -radius; dz <= radius; dz++) {
                ChunkPos p = new ChunkPos(centre.x + dx, centre.z + dz);
                if (level.setChunkForced(p.x, p.z, true))
                    forced.add(p);
            }
        return () -> forced.forEach(p -> level.setChunkForced(p.x, p.z, false));
    }

    /** True when every chunk within {@code radius} chunks of {@code centre} is loaded and entity-ticking. */
    public static boolean ready(ServerLevel level, ChunkPos centre, int radius) {
        for (int dx = -radius; dx <= radius; dx++)
            for (int dz = -radius; dz <= radius; dz++) {
                ChunkPos p = new ChunkPos(centre.x + dx, centre.z + dz);
                if (level.getChunkSource().getChunkNow(p.x, p.z) == null
                        || !RtsimSession.entitiesTickAt(level, p.getMiddleBlockPosition(0)))
                    return false;
            }
        return true;
    }

    /** The native engine's oil gate counters ({@code NativeEngine.OIL_*}), with {@code agent}'s reserve. */
    public static long[] nativeOil(long agent) {
        return engine() instanceof com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine n ? n.dragonsOil(agent) : null;
    }

    // ---- reading what the engine received -------------------------------------------------------

    /** Records of one kind the engine received (not skipped) whose u64 at {@code offset} is {@code value}. */
    public static List<ScriptedEngine.Received> received(int kind, int offset, long value) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine().received(kind))
            if (!r.skipped() && r.bytes().length >= offset + 8 && r.view().getLong(offset) == value)
                out.add(r);
        return out;
    }

    /** Journaled dragon records about one agent (the agent at offset 24). */
    public static List<ScriptedEngine.Received> about(int kind, long agent) {
        return received(kind, Obs.BODY, agent);
    }

    /** DRAGON_UNTAGGED records for one entity. */
    public static List<ScriptedEngine.Received> untagged(UUID entity) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : received(DragonRecords.DRAGON_UNTAGGED, DragonRecords.Untagged.ENTITY_MOST, entity.getMostSignificantBits()))
            if (r.view().getLong(DragonRecords.Untagged.ENTITY_LEAST) == entity.getLeastSignificantBits())
                out.add(r);
        return out;
    }

    public static UUID uuidAt(ScriptedEngine.Received r, int offset) {
        return new UUID(r.view().getLong(offset), r.view().getLong(offset + 8));
    }

    /** The live puppet body of an agent, waiting for it (inside thenWaitUntil). */
    public static DragonBaseEntity body(long agent) {
        Entity e = bridge().body(agent);
        if (!(e instanceof DragonBaseEntity d))
            throw new GameTestAssertException("dragon " + Long.toHexString(agent) + " is not embodied yet");
        return d;
    }

    // ---- vanilla's own entity-chunk save --------------------------------------------------------

    /** Stores every loaded entity chunk (vanilla's autosave) and reads the given chunks back from storage. */
    @SuppressWarnings("unchecked")
    public static CompletableFuture<List<Entity>> saveAndReadBack(ServerLevel level, ChunkPos... chunks) {
        var manager = ((ServerLevelAccessor) level).mic_rtsim$entityManager();
        manager.autoSave();
        EntityPersistentStorage<Entity> storage =
                (EntityPersistentStorage<Entity>) ((EntitySectionManagerAccessor) manager).mic_rtsim$permanentStorage();
        CompletableFuture<List<Entity>> all = CompletableFuture.completedFuture(List.of());
        for (ChunkPos chunk : new java.util.LinkedHashSet<>(List.of(chunks))) {
            CompletableFuture<ChunkEntities<Entity>> one = storage.loadEntities(chunk);
            all = all.thenCombine(one, (list, ce) -> {
                List<Entity> out = new ArrayList<>(list);
                ce.getEntities().forEach(out::add);
                return out;
            });
        }
        return all;
    }
}
