package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.DenKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MowzieCamps;
import com.mojang.logging.LogUtils;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Mth;
import net.minecraft.world.Difficulty;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Marker;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;
import net.neoforged.neoforge.event.level.ChunkTicketLevelUpdatedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;

/**
 * Helpers for the Mowzie camp GameTests (MOWZIE.md §4.9, §8; docs/choices-mowzie-build.md, M5): a far
 * place of a test's own, test camps declared under names no culture claims (MZB-227), the jar's bodies
 * made as strays, kills, and a hook that catches the record markers the reconciler leaves (a marker is
 * discarded a tick or two after its acknowledgement).
 */
public final class MowzieKit {
    private MowzieKit() {}

    public static final String TEST_DEN = "mic_rtsim_mowziesmobs:test_den";
    public static final String TEST_GROVE = "mic_rtsim_mowziesmobs:test_grove";
    public static final String FROSTMAW = MicRtsimMowziesMobs.FROSTMAW;
    public static final String WARRIOR = MicRtsimMowziesMobs.WARRIOR;

    // ---- the end of every tick ------------------------------------------------------------------

    private static final List<Runnable> TICKERS = new CopyOnWriteArrayList<>();

    static {
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, false, ServerTickEvent.Post.class, e -> {
            for (Runnable r : TICKERS) {
                try {
                    r.run();
                } catch (RuntimeException ex) {
                    LogUtils.getLogger().error("[gametest] a per-tick hook failed", ex);
                }
            }
        });
    }

    /** Runs {@code r} at the end of every server tick (after the camps', which listen at the same priority) until the test ends. */
    public static void everyTick(GameTestHelper helper, Runnable r) {
        TICKERS.add(r);
        ConflictKit.onEnd(helper, () -> TICKERS.remove(r));
    }

    // ---- join and leave hooks -------------------------------------------------------------------

    /**
     * While true, a join of a body a camp holds (tagged, no puppet) is cancelled before the camps see it, as
     * another mod's (In Control's) would: the GameTests of a hand-back whose add fails. Reset when the test ends.
     */
    public static volatile boolean cancelCampJoins;

    private static final Map<UUID, int[]> LEAVES = new ConcurrentHashMap<>();

    static {
        NeoForge.EVENT_BUS.addListener(EventPriority.HIGHEST, false, EntityJoinLevelEvent.class, e -> {
            Entity en = e.getEntity();
            if (cancelCampJoins && !e.getLevel().isClientSide() && en instanceof Mob && RtsimAttachments.campMember(en) != null
                    && !RtsimAttachments.isTagged(en))
                e.setCanceled(true);
        });
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, false, EntityLeaveLevelEvent.class, e -> {
            int[] n = LEAVES.get(e.getEntity().getUUID());
            if (n != null && !e.getLevel().isClientSide())
                n[e.getEntity().getRemovalReason() == null ? 0 : 1]++;
        });
    }

    /** Arms {@link #cancelCampJoins} for the test (off again when it ends). */
    public static void cancelCampJoins(GameTestHelper helper, boolean on) {
        cancelCampJoins = on;
        if (on)
            ConflictKit.onEnd(helper, () -> cancelCampJoins = false);
    }

    /** Counts the leave events of an entity from now on (until the test ends): [0] with no removal reason (hidden), [1] with one. */
    public static int[] watchLeaves(GameTestHelper helper, Entity e) {
        int[] n = new int[2];
        LEAVES.put(e.getUUID(), n);
        ConflictKit.onEnd(helper, () -> LEAVES.remove(e.getUUID()));
        return n;
    }

    // ---- the ticket event ---------------------------------------------------------------------

    private record TicketWatch(ServerLevel level, Set<Long> chunks, int[] count) {}

    private static final List<TicketWatch> TICKET_WATCHES = new CopyOnWriteArrayList<>();

    static {
        // Registered once; costs a list walk per event only while a test watches (the list is empty otherwise).
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, false, ChunkTicketLevelUpdatedEvent.class, e -> {
            for (TicketWatch w : TICKET_WATCHES)
                if (w.level() == e.getLevel() && w.chunks().contains(e.getChunkPos()))
                    w.count()[0]++;
        });
    }

    /**
     * Counts the {@code ChunkTicketLevelUpdatedEvent}s of the given chunks from now on (until the test ends):
     * zero where the chunk system never posts it (C2ME, MZB-249), which a test that asserts what the event
     * path alone shows (a camp seen waiting) uses to know whether it can.
     */
    public static int[] watchTicketEvents(GameTestHelper helper, Iterable<ChunkPos> chunks) {
        Set<Long> set = new LinkedHashSet<>();
        for (ChunkPos p : chunks)
            set.add(p.toLong());
        TicketWatch w = new TicketWatch(helper.getLevel(), set, new int[1]);
        TICKET_WATCHES.add(w);
        ConflictKit.onEnd(helper, () -> TICKET_WATCHES.remove(w));
        return w.count();
    }

    // ---- a test's far place ---------------------------------------------------------------------

    /**
     * A test's own place: a chunk-aligned base far from every other run's (the world is reused), the
     * chunks it forces (released when the test ends, passed or failed), and Peaceful turned to Normal for
     * its length (a hostile body would be removed at once).
     */
    public static final class Far {
        public final ServerLevel level;
        public final String dim;
        /** The base block and its chunk. */
        public final int bx, bz, k, z0;
        private final Set<Long> forced = new LinkedHashSet<>();

        public Far(GameTestHelper helper, String test) {
            level = helper.getLevel();
            dim = RtsimTests.dimension(helper);
            int[] base = ConflictKit.farBase();
            bx = base[0];
            bz = base[1];
            k = bx >> 4;
            z0 = bz >> 4;
            int released = ConflictKit.releaseFarChunks(level);
            RtsimTests.record(test, "released", released + " chunks an earlier run left forced; base chunk (" + k + ", " + z0 + ")");
            MinecraftServer server = level.getServer();
            Difficulty before = server.getWorldData().getDifficulty();
            if (before == Difficulty.PEACEFUL)
                server.setDifficulty(Difficulty.NORMAL, true);
            ConflictKit.onEnd(helper, () -> server.setDifficulty(before, true));
            ConflictKit.onEnd(helper, this::releaseAll);
        }

        /** Sets the difficulty for the rest of the test (restored when it ends). */
        public void difficulty(Difficulty d) {
            level.getServer().setDifficulty(d, true);
        }

        public void force(int cx, int cz) {
            if (ConflictKit.force(level, cx, cz, true))
                forced.add(ChunkPos.asLong(cx, cz));
        }

        /** Forces every chunk of the rectangle, both ends included. */
        public void forceRect(int cx0, int cz0, int cx1, int cz1) {
            for (int cx = cx0; cx <= cx1; cx++)
                for (int cz = cz0; cz <= cz1; cz++)
                    force(cx, cz);
        }

        public void release(int cx, int cz) {
            ConflictKit.force(level, cx, cz, false);
            forced.remove(ChunkPos.asLong(cx, cz));
        }

        public void releaseRect(int cx0, int cz0, int cx1, int cz1) {
            for (int cx = cx0; cx <= cx1; cx++)
                for (int cz = cz0; cz <= cz1; cz++)
                    release(cx, cz);
        }

        public void releaseAll() {
            for (long c : List.copyOf(forced))
                ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
            forced.clear();
        }

        /** Inside {@code thenWaitUntil}: every chunk this place has forced is loaded, has its entities in, and ticks them. */
        public Runnable ready() {
            return RtsimTests.io(() -> {
                for (long c : forced) {
                    int cx = ChunkPos.getX(c), cz = ChunkPos.getZ(c);
                    check("chunk (" + cx + ", " + cz + ") is loaded with its entities, ticking",
                            level.hasChunk(cx, cz) && level.areEntitiesLoaded(c)
                                    && RtsimSession.entitiesTickAt(level, new BlockPos(cx * 16 + 8, 64, cz * 16 + 8)));
                }
            });
        }

        /** Inside {@code thenWaitUntil}: the chunk is out of memory. */
        public Runnable unloaded(int cx, int cz) {
            return RtsimTests.io(() -> check("chunk (" + cx + ", " + cz + ") is unloaded", !level.hasChunk(cx, cz)));
        }
    }

    /** A stone floor over the area at its highest surface, three blocks of air above; the stone's y (bodies stand at y + 1). */
    public static int platform(ServerLevel level, int x0, int x1, int z0, int z1) {
        int top = level.getMinBuildHeight();
        for (int x = x0; x <= x1; x++)
            for (int z = z0; z <= z1; z++)
                top = Math.max(top, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z));
        for (int x = x0; x <= x1; x++)
            for (int z = z0; z <= z1; z++) {
                level.setBlock(new BlockPos(x, top, z), Blocks.STONE.defaultBlockState(), 2);
                for (int y = 1; y <= 3; y++)
                    level.setBlock(new BlockPos(x, top + y, z), Blocks.AIR.defaultBlockState(), 2);
            }
        return top;
    }

    // ---- test camps -----------------------------------------------------------------------------

    public static Camps camps() {
        return ConflictBridge.INSTANCE.reconciler();
    }

    public static Camps.Site site(long faction) {
        Camps.Site s = camps().site(faction);
        if (s == null)
            throw new IllegalStateException("no camp " + Long.toHexString(faction));
        return s;
    }

    /** A grove at the middle of a chunk (MZB-218's round 25 blocks, 21 footprint chunks), declared with no fire pit piece. */
    public static final class Grove {
        public final Far far;
        public final int cx, cz;
        public long faction;
        public Vec3 spot;

        public Grove(Far far, int cx, int cz) {
            this.far = far;
            this.cx = cx;
            this.cz = cz;
        }

        /** Forces the 5 x 5 chunks round the centre (the corners are no part of the footprint). */
        public Grove force() {
            far.forceRect(cx - 2, cz - 2, cx + 2, cz + 2);
            return this;
        }

        /** A stone floor 30 blocks each way round the centre. */
        public Grove lay() {
            platform(far.level, cx * 16 + 8 - 30, cx * 16 + 8 + 30, cz * 16 + 8 - 30, cz * 16 + 8 + 30);
            return this;
        }

        public Grove declare() {
            ServerLevel level = far.level;
            int x = cx * 16 + 8, z = cz * 16 + 8;
            int y = level.hasChunk(cx, cz) ? level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z) : level.getSeaLevel();
            Vec3 s = new Vec3(x + 0.5, y, z + 0.5);
            faction = RtsimApi.declareCamp(level, new ChunkPos(cx, cz), TEST_GROVE, GroveKind.INSTANCE, new BlockPos(x, y, z), s,
                    MowzieCamps.GROVE_RADIUS, 0, false);
            check("the grove is declared (a live session)", faction != 0);
            spot = s;
            return this;
        }

        public Camps.Site site() {
            return MowzieKit.site(faction);
        }

        /** A point {@code dx}, {@code dz} from the centre, at standing height. */
        public Vec3 at(double dx, double dz) {
            double x = spot.x + dx, z = spot.z + dz;
            return new Vec3(x, far.level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, Mth.floor(x), Mth.floor(z)), z);
        }

        public AABB area() {
            return new AABB(spot.x - 32, far.level.getMinBuildHeight(), spot.z - 32, spot.x + 32, far.level.getMaxBuildHeight(), spot.z + 32);
        }
    }

    /**
     * A den at the middle of a chunk, declared with no piece: the cylinder of {@link DenKind#FOOTPRINT_RADIUS}
     * round its spot (MZB-230), which touches the chunk and its four neighbours (the corners are 11.3 blocks
     * from the spot).
     */
    public static final class Den {
        public final Far far;
        public final int cx, cz;
        public long faction;
        public Vec3 spot;

        public Den(Far far, int cx, int cz) {
            this.far = far;
            this.cx = cx;
            this.cz = cz;
        }

        /** Forces the 3 x 3 chunks round the den (the footprint is the chunk and its four neighbours). */
        public Den force() {
            return force(0);
        }

        /** As above, and {@code east} more columns of chunks east of them (a stray far from the spot stands in one). */
        public Den force(int east) {
            far.forceRect(cx - 1, cz - 1, cx + 1 + east, cz + 1);
            return this;
        }

        /** Releases what {@link #force(int)} forced. */
        public Den release(int east) {
            far.releaseRect(cx - 1, cz - 1, cx + 1 + east, cz + 1);
            return this;
        }

        public Den release() {
            return release(0);
        }

        /** A stone floor over the 3 x 3 chunks. */
        public Den lay() {
            return lay(0);
        }

        public Den lay(int east) {
            platform(far.level, (cx - 1) * 16, (cx + 1 + east) * 16 + 15, (cz - 1) * 16, (cz + 1) * 16 + 15);
            return this;
        }

        public Den declare() {
            return declare(false);
        }

        public Den declare(boolean late) {
            ServerLevel level = far.level;
            int x = cx * 16 + 8, z = cz * 16 + 8;
            int y = level.hasChunk(cx, cz) ? level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z) : level.getSeaLevel();
            Vec3 s = new Vec3(x, y, z);
            faction = RtsimApi.declareCamp(level, new ChunkPos(cx, cz), TEST_DEN, DenKind.INSTANCE, BlockPos.containing(s), s,
                    DenKind.FOOTPRINT_RADIUS, DenKind.FOOTPRINT_RADIUS, late);
            check("the den is declared (a live session)", faction != 0);
            spot = s;
            return this;
        }

        public Camps.Site site() {
            return MowzieKit.site(faction);
        }

        /** A point {@code dx}, {@code dz} from the spot, at standing height. */
        public Vec3 at(double dx, double dz) {
            double x = spot.x + dx, z = spot.z + dz;
            return new Vec3(x, far.level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, Mth.floor(x), Mth.floor(z)), z);
        }

        public AABB area() {
            return new AABB((cx - 1) * 16, far.level.getMinBuildHeight(), (cz - 1) * 16, (cx + 2) * 16, far.level.getMaxBuildHeight(), (cz + 2) * 16);
        }
    }

    // ---- the jar's bodies -----------------------------------------------------------------------

    /**
     * A body of the jar standing where it is told, untagged (a stray), added to the level; {@code noAi}
     * keeps it where it stands. Finalized as the jar's worldgen does, with STRUCTURE.
     */
    public static <T extends Mob> T stray(ServerLevel level, EntityType<T> type, double x, double y, double z, boolean noAi) {
        return stray(level, type, x, y, z, noAi, true);
    }

    /** As above; {@code finalize} false leaves the body as its constructor made it (no mask or weapon roll, no escort). */
    @SuppressWarnings("deprecation")
    public static <T extends Mob> T stray(ServerLevel level, EntityType<T> type, double x, double y, double z, boolean noAi, boolean finalize) {
        T m = type.create(level);
        if (m == null)
            throw new IllegalStateException("no " + type);
        m.moveTo(x, y, z, level.random.nextFloat() * 360f - 180f, 0f);
        m.setYHeadRot(m.getYRot());
        if (finalize)
            m.finalizeSpawn(level, level.getCurrentDifficultyAt(m.blockPosition()), MobSpawnType.STRUCTURE, null);
        m.setNoAi(noAi);
        check(type + " is added", level.addFreshEntity(m));
        return m;
    }

    /** A sleeping Frostmaw (the template's: active false) with or without its crystal. */
    public static EntityFrostmaw frostmaw(ServerLevel level, Vec3 at, boolean crystal) {
        EntityFrostmaw f = stray(level, EntityHandler.FROSTMAW.get(), at.x, at.y, at.z, false);
        f.setHasCrystal(crystal);
        f.setActive(false);
        return f;
    }

    /** Kills a body as {@code source} would; if the jar's own hurt does not (a guard, an animation), its health goes to 0 and it dies. */
    public static void kill(LivingEntity e, DamageSource source) {
        e.hurt(source, 10_000f);
        if (!e.isDeadOrDying()) {
            e.setHealth(0f);
            e.die(source);
        }
    }

    // ---- tags -----------------------------------------------------------------------------------

    /** The camp tag of an entity, or null. */
    public static CampMember tag(Entity e) {
        CampMember m = RtsimAttachments.campMember(e);
        return m == null || m.isNone() ? null : m;
    }

    public static boolean tagged(Entity e, long faction, int slot, int generation) {
        CampMember m = tag(e);
        return m != null && m.faction() == faction && m.slot() == slot && m.generation() == generation;
    }

    /** The loaded, living bodies a camp holds in a slot and generation. */
    public static List<Entity> held(long faction, int slot, int generation) {
        List<Entity> out = new ArrayList<>();
        for (Entity e : camps().held(faction))
            if (tagged(e, faction, slot, generation) && e.isAlive() && !e.isRemoved())
                out.add(e);
        return out;
    }

    /** The loaded, living bodies a camp holds. */
    public static List<Entity> held(long faction) {
        List<Entity> out = new ArrayList<>();
        for (Entity e : camps().held(faction))
            if (tag(e) != null && e.isAlive() && !e.isRemoved())
                out.add(e);
        return out;
    }

    public static boolean persistent(Mob m) {
        return m.isPersistenceRequired();
    }

    // ---- records --------------------------------------------------------------------------------

    public static int censusCount(long faction) {
        return ConflictKit.censuses(faction).size();
    }

    /** The {@code n}-th census (1-based). */
    public static ScriptedEngine.Received census(long faction, int n) {
        List<ScriptedEngine.Received> l = ConflictKit.censuses(faction);
        check("census " + n + " has come", l.size() >= n);
        return l.get(n - 1);
    }

    public static long epoch(ScriptedEngine.Received census) {
        return census.view().getLong(ConflictRecords.CampCensus.EPOCH);
    }

    public static float chiefHealth(ScriptedEngine.Received census) {
        return census.view().getFloat(ConflictRecords.CampCensus.HEALTH);
    }

    public static int chiefFlags(ScriptedEngine.Received census) {
        return census.view().getInt(ConflictRecords.CampCensus.FLAGS);
    }

    public static ConflictKit.CensusBody bodyOf(ScriptedEngine.Received census, UUID id) {
        for (ConflictKit.CensusBody b : ConflictKit.censusBodies(census))
            if (b.uuid().equals(id))
                return b;
        return null;
    }

    /** Scripts a CAMP_RECONCILE at an epoch with a fresh op id; the op. */
    public static long reconcile(long faction, long epoch, List<ConflictKit.CampLine> lines) {
        long op = ConflictKit.newOp();
        ConflictKit.reconcile(faction, epoch, op, lines);
        return op;
    }

    /** One CAMP_BODIES record's outcomes by line index. */
    public static Map<Integer, Integer> outcomesOf(ScriptedEngine.Received r) {
        Map<Integer, Integer> out = new LinkedHashMap<>();
        var b = r.view();
        for (int i = 0; i < r.aux(); i++) {
            int o = ConflictRecords.CampBodies.LINES + i * ConflictRecords.CampBodies.LINE;
            out.put(b.getInt(o + ConflictRecords.CampBodies.L_LINE), b.get(o + ConflictRecords.CampBodies.L_OUTCOME) & 0xFF);
        }
        return out;
    }

    public static String outcomeName(int outcome) {
        return switch (outcome) {
            case ConflictRecords.CampBodies.DONE -> "DONE";
            case ConflictRecords.CampBodies.NOTHING -> "NOTHING";
            case ConflictRecords.CampBodies.STALE -> "STALE";
            default -> "outcome " + outcome;
        };
    }

    /** Inside {@code thenWaitUntil}: every one of {@code lines} lines of the op is answered; the first answer of each, by line index. */
    public static Map<Integer, Integer> answered(long faction, long op, int lines) {
        Map<Integer, Integer> o = ConflictKit.outcomes(faction, op);
        check("the op's " + lines + " lines are answered (" + o.size() + ")", o.size() >= lines);
        return o;
    }

    public static void allOutcomes(String what, Map<Integer, Integer> o, int outcome) {
        for (Map.Entry<Integer, Integer> e : o.entrySet())
            check(what + ": line " + e.getKey() + " is " + outcomeName(outcome) + ", not " + outcomeName(e.getValue()), e.getValue() == outcome);
    }

    // ---- record markers -------------------------------------------------------------------------

    /**
     * Catches the record markers (vanilla markers tagged {@code mic_rtsim.record}) standing in an area, at
     * the end of every tick: the reconciler discards one a tick or two after its acknowledgement, so the
     * NBT it carried is kept here.
     */
    public static final class Markers {
        /** A marker as last seen: where, and its saved NBT (its tags and the {@code neoforge:attachments} compound). */
        public record Seen(UUID id, double x, double y, double z, CompoundTag nbt) {
            public boolean tagged() {
                return nbt.getList("Tags", 8).stream().anyMatch(t -> RtsimSession.RECORD_MARKER_TAG.equals(t.getAsString()));
            }

            public boolean carriesRecords() {
                return nbt.getCompound("neoforge:attachments").contains("mic_rtsim:pending");
            }
        }

        private final ServerLevel level;
        private final Map<UUID, Seen> seen = new LinkedHashMap<>();

        public Markers(GameTestHelper helper, ServerLevel level, AABB area) {
            this.level = level;
            everyTick(helper, () -> {
                for (Marker m : level.getEntities(EntityType.MARKER, area, e -> e.getTags().contains(RtsimSession.RECORD_MARKER_TAG))) {
                    CompoundTag t = new CompoundTag();
                    m.saveWithoutId(t);
                    seen.put(m.getUUID(), new Seen(m.getUUID(), m.getX(), m.getY(), m.getZ(), t));
                }
            });
        }

        public List<Seen> all() {
            return new ArrayList<>(seen.values());
        }

        /** The markers seen within {@code r} blocks (horizontally) of a point. */
        public List<Seen> near(Vec3 p, double r) {
            List<Seen> out = new ArrayList<>();
            for (Seen s : seen.values())
                if ((s.x() - p.x) * (s.x() - p.x) + (s.z() - p.z) * (s.z() - p.z) <= r * r)
                    out.add(s);
            return out;
        }

        public boolean gone(UUID id) {
            Entity e = level.getEntity(id);
            return e == null || e.isRemoved();
        }
    }
}
