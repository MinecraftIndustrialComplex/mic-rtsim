package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.iafenvoy.iceandfire.entity.DeathWormEntity;
import com.iafenvoy.iceandfire.event.GriefBreakBlockEvent;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.industry.ThreatClasses;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.BlockTags;
import net.minecraft.util.Mth;
import net.minecraft.util.RandomSource;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.OwnableEntity;
import net.minecraft.world.entity.ai.targeting.TargetingConditions;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.BaseSpawner;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.FallingBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.SpawnerBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.saveddata.SavedData;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.Tags;
import net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedQueue;

/**
 * The oil nests (OILGATE.md §4.6, OG-9e; EQUILIBRIUM.md §2.4.8): the engine founds a nest under its oil
 * chunks, renews its Death Worms while a pumpjack drills nearby, and sends them on raids; the plugin
 * carves the nest, keeps its worms, and reports what happens to them. Server thread, except
 * {@link #loaded}.
 *
 * <p><b>The nest</b> (PLACE_NEST): a chamber of 9 by 9 blocks, one layer of sand under 4 of air, whose
 * floor lies {@code depth} blocks under the chunk's ocean-floor heightmap (the top block that blocks
 * motion, water ignored), at a spot in the chunk drawn from the nest id. It is carved only where the
 * chamber and a 1-block shell around it are all natural ground ({@code #minecraft:overworld_carver_replaceables}
 * or {@code #c:ores}) with no block entity and no fluid, and the shell's roof is nothing that falls; up
 * to {@link #TRIES} seeded spots are tried, else NEST_FOUND says failed. The whole shell lies in the
 * chunk. The heart is a vanilla spawner at the floor's centre set to the Death Worm and made inert: its
 * {@code RequiredPlayerRange} is 0 (no player is ever nearer than 0), and a spawner spawn from a known
 * heart is cancelled ({@link #fromHeart}). A request waits in the saved state until its chunk is loaded.
 *
 * <p><b>Worms</b> (NEST_RENEW): owed until the heart's chunk is entity-ticking and no player is within
 * {@link #CLEAR_OF_PLAYERS} blocks of it, then placed on the chamber floor, sized like a grown worm of
 * the stage ({@code setWormAge(5)} then {@code setDeathWormScale(stage)}: age scale = stage), the
 * nest's own variant, homed on the heart, and marked with the nest id ({@link #TAG}, persistent data).
 * They carry no agent tag: the sim counts them, it does not steer them. Natural worms are left alone.
 * Nest worms break no blocks ({@link #onGrief}).
 *
 * <p><b>Reports</b>: NEST_FOUND for each PLACE_NEST; NEST_CENSUS after worms are placed and on each load
 * of the heart's chunk (the worms the nest knows, an unloaded one counted alive, plus those still owed;
 * HEART_MISSING while the heart is not a spawner); NEST_LOSS for a nest worm's death (the boss rule:
 * a death no wild mob is credited with is a player's); NEST_HEART when a player breaks the heart, after
 * which the nest places no worm until the engine's PLACE_NEST for it re-forms its heart (a NEST_RENEW
 * re-forms it too, as a fallback).
 *
 * <p><b>Raids</b> (NEST_SORTIE): loaded worms glide through the ground (no physics, no AI) to the target
 * column at the nest's depth or deeper, then rise in it. A worm stops under an armoured floor
 * ({@code #mic_rtsim:armoured_floor}, or a block the survey counts as armoured, {@code #mic_rtsim:burrow_spared})
 * and glides home; otherwise it surfaces into the first air its body fits in, takes up its AI and
 * fights until the sortie's timeout, then glides home. A glide is never saved: the world save at a stop
 * finds every worm with its AI and physics, and a worm saved mid-glide (an autosave, an unload) is
 * mended as it next joins a level and glides home (a server start included).
 */
public final class Nests {
    /** Persistent data: the nest id of one of its worms (long). */
    public static final String TAG = "mic_rtsim_nest";
    /**
     * Persistent data: the ordinal of one of the nest's worms (int), the count of worms the nest had spawned before it
     * (RP-242): the nest's {@link Nest#spawned} when the worm was placed. Beside {@link #TAG}, in the entity's own file.
     */
    public static final String ORDINAL_TAG = "mic_rtsim_nest_ordinal";
    /**
     * Persistent data of a nest's heart (a spawner block entity, saved under NeoForge's "NeoForgeData" in the chunk's
     * own file beside the carved blocks): the nest id (long). A PLACE_NEST again for a nest the Nests lost finds its
     * chamber by it instead of carving a second one (RP-240).
     */
    public static final String HEART_TAG = "mic_rtsim_nest_heart";
    /** Persistent data: the worm is out on a sortie (cleared when it is home). */
    public static final String SORTIE_TAG = "mic_rtsim_nest_sortie";
    public static final ResourceLocation DEATHWORM = ResourceLocation.fromNamespaceAndPath("iceandfire", "deathworm");

    /** The chamber's half width (9 by 9) and its air above the sand. */
    public static final int HALF = 4, AIR = 4;
    /** Spots tried per nest, the first at the asked depth, the rest up to {@link #SHIFT} blocks above or below. */
    public static final int TRIES = 8, SHIFT = 6;
    /** Worms are placed only with no player this near the heart (EQUILIBRIUM.md §2.4.8: "the next unwatched load"). */
    public static final int CLEAR_OF_PLAYERS = 128;
    /** One nest day: the wait before placing worms again when a player was near or the chunk unloaded. */
    public static final int NEST_DAY = 24_000;
    /** The wait when the heart's chunk is loaded but not ticking entities yet. */
    public static final int NOT_TICKING_RETRY = 100;
    /** PLACE_NESTs carved per tick (each reads up to 8 x 847 blocks), PLACEHOLDER. */
    public static final int PLACEMENTS_PER_TICK = 2;
    /** Blocks per tick a gliding worm moves. */
    public static final double GLIDE_SPEED = 0.5;
    /** Blocks of ground a raiding worm keeps under the target column's top before it rises. */
    public static final int RISE_FROM = 24;
    /** A fighting worm is set on the nearest player this near when it surfaces. */
    public static final double FIGHT_REACH = 48;
    /** A spawner spawn of a worm this near a heart is cancelled even if the event names no spawner. */
    public static final int HEART_GUARD = 8;
    /** Where on the floor worms are placed and come home to, around the heart. */
    private static final int[][] SPOTS = {{-2, -2}, {2, 2}, {-2, 2}, {2, -2}, {0, -3}, {0, 3}, {-3, 0}, {3, 0}};
    private static final long SALT = 0x6F696C5F6E657374L;

    /** One PLACE_NEST waiting for its chunk. */
    public record Pending(long nest, String dim, int cx, int cz, int depth) {}

    /** The last NEST_RENEW serial applied under one earlier engine life (RP-14, RP-232). */
    public record LifeSerial(long life, long serial) {}

    /** One placed nest. */
    public static final class Nest {
        /**
         * How many engine lives a nest remembers the last renewal serial of: the current one and the seven before it.
         * A life past that is forgotten oldest first, and a return to a forgotten life counts from 0 again, which is
         * the one way left to owe a renewal twice (RP-232: more than eight lives, and a hand-mixed restore).
         */
        public static final int LIVES = 8;
        /** The most holes a nest keeps (RP-242): far above any brood in use; the lowest is dropped past it. */
        public static final int HOLES_MAX = 32;

        public final long id;
        public final String dim;
        public final BlockPos heart;
        /** Worms owed (NEST_RENEW) and not yet placed, and the size stage they will have. */
        public int owed, stage = 1;
        /**
         * How many worms the nest has ever spawned (RP-242): each worm carries the count it was spawned at as its
         * ordinal ({@link Nests#ORDINAL_TAG}), so a copy of a worm a restore spawned again, or one that died, is told
         * from the worm the nest holds. Saved in the same file as {@link #owed}; a file from before it loads 0.
         */
        public int spawned;
        /**
         * Ordinals below {@link #spawned} that were skipped (RP-242, RP-242): a worm adopted at ordinal k above the count
         * leaves {@code spawned..k-1} here, because the worms of those ordinals may be in entity files that load after it.
         * A hole is a worm the file's brood still owes, so the join rule adopts its copy (once) where it would discard
         * one of an ordinal below the count, and the nest's own spawn takes the lowest hole before it raises the count (the
         * old copy of the ordinal it spawns again is then stale, as before). Held to {@link #HOLES_MAX}: a brood is
         * {@code K} worms (2 by default, a u16 setting), and a hole is one of them or one that died in the lost window;
         * past the cap the lowest is dropped, so that worm's copy is discarded when it loads (a loss, never a double).
         * Saved in the same file as {@link #spawned}.
         */
        public final TreeSet<Integer> holes = new TreeSet<>();
        /**
         * The heart is gone: no worm is placed until a PLACE_NEST (or a NEST_RENEW) re-forms it; {@code reform}:
         * one asked for it, to be done when the heart's chunk is loaded.
         */
        public boolean broken, reform;
        /** The game time the owed worms are tried again. */
        public long dueAt;
        /**
         * NEST_RENEW's dedup (docs/replay-gate.md §7.1, RP-14): the serial of the last renewal applied (the engine's
         * count of this nest's renewals, an unsigned 32 bits), and the engine life it came under
         * ({@code RtsimSession.engineLife}, RP-19): the nest's current life. Both 0 for a nest with none, and for one
         * loaded from a file from before them. Saved with {@link #owed}, in the same file: the two reach disk together.
         */
        public long serial, life;
        /**
         * The last serial of each earlier life, oldest first: never the current life, never a serial 0 (that is the same
         * as no entry), at most {@link #LIVES} - 1 of them. A life the engine comes back to (world/rtsim restored from a
         * backup while the Nests stayed) takes its serial from here, so what it applied is still a repeat (RP-232).
         */
        public final ArrayList<LifeSerial> earlier = new ArrayList<>();
        /** Its worms alive, loaded or not. */
        public final LinkedHashSet<UUID> worms = new LinkedHashSet<>();

        public Nest(long id, String dim, BlockPos heart) {
            this.id = id;
            this.dim = dim;
            this.heart = heart.immutable();
        }

        public ChunkPos chunk() {
            return new ChunkPos(heart);
        }

        /**
         * Whether a NEST_RENEW of {@code renewal} under engine life {@code engineLife} is new, and if so counted:
         * only a serial above the last one applied under the same life is (RP-14). Under a life the nest has not
         * seen its serial starts afresh, since the new engine counts its renewals from 1; what is owed stays (RP-19).
         * Under a life it has seen, within the last {@link #LIVES}, the serial is the one that life reached (RP-232).
         * A repeat changes nothing but the life it is judged under.
         */
        public boolean acceptRenewal(long engineLife, long renewal) {
            if (engineLife != life)
                switchTo(engineLife);
            if (renewal <= serial)
                return false;
            serial = renewal;
            return true;
        }

        /** Makes {@code engineLife} the current life: the current one goes to {@link #earlier}, the asked one comes back from it. */
        private void switchTo(long engineLife) {
            if (serial != 0)
                earlier.add(new LifeSerial(life, serial));
            long back = 0;
            for (Iterator<LifeSerial> it = earlier.iterator(); it.hasNext();) {
                LifeSerial e = it.next();
                if (e.life() == engineLife) {
                    back = e.serial();
                    it.remove();
                    break;
                }
            }
            while (earlier.size() > LIVES - 1)
                earlier.remove(0);
            life = engineLife;
            serial = back;
        }

        /** The last serial applied under {@code engineLife}: 0 for a life the nest does not remember. */
        public long lastUnder(long engineLife) {
            if (engineLife == life)
                return serial;
            for (LifeSerial e : earlier)
                if (e.life() == engineLife)
                    return e.serial();
            return 0;
        }

        /** Loading: an earlier life's serial from a file, leaving out what could not be one (the current life, a 0, a repeat) and the surplus. */
        void remember(long earlierLife, long earlierSerial) {
            if (earlierLife == life || earlierSerial == 0)
                return;
            earlier.removeIf(e -> e.life() == earlierLife);
            earlier.add(new LifeSerial(earlierLife, earlierSerial));
            while (earlier.size() > LIVES - 1)
                earlier.remove(0);
        }

        /**
         * NEST_RENEW applied to the nest: when the serial is new (see {@link #acceptRenewal}), {@code count} more worms
         * are owed at size {@code stage}, tried from {@code now}, and a broken heart is asked to re-form. False for a
         * repeat, which changes none of that.
         */
        public boolean renewed(long engineLife, long renewal, int count, int stage, long now) {
            if (!acceptRenewal(engineLife, renewal))
                return false;
            if (broken)
                reform = true;
            owed += count;
            this.stage = Mth.clamp(stage, 1, 3);
            dueAt = now;
            return true;
        }

        /** The serial a renewal sent next under engine life {@code engineLife} must carry to be applied (GameTests). */
        public long nextSerial(long engineLife) {
            return lastUnder(engineLife) + 1;
        }

        /** What a worm joining a level means to its nest (RP-242). */
        public enum Join {
            /** The nest holds it, or the worm carries no ordinal (a worm from before it): left alone, as it always was. */
            NORMAL,
            /** A copy of a worm the nest spawned again after a rollback, or that has died: cancelled and discarded. */
            STALE,
            /** A worm a later session spawned whose count the nest's file lost: taken into the list. */
            ADOPT
        }

        /**
         * The join rule of a nest worm (RP-242). {@code ordinal} is the worm's {@link Nests#ORDINAL_TAG}, or
         * {@code -1} for a worm that has none. A worm the list holds is normal whatever its ordinal. An unlisted one
         * of a hole's ordinal (RP-242) is adopted: a worm a later session spawned, whose entity file loaded after a
         * higher one's. An unlisted one below {@link #spawned} otherwise is a stale copy: lose, never double (the count
         * it was spawned at was spawned again, or it died, and either way the nest's books no longer owe it). One at or
         * above the count was spawned by a session whose count did not reach this file, and is adopted.
         */
        public Join judge(UUID worm, int ordinal) {
            if (ordinal < 0 || worms.contains(worm))
                return Join.NORMAL;
            if (holes.contains(ordinal))
                return Join.ADOPT;
            return ordinal < spawned ? Join.STALE : Join.ADOPT;
        }

        /**
         * Takes an adopted worm into the nest (RP-242): listed, its ordinal counted as spawned, and one worm
         * less owed (never below 0: {@link #owed} still covers the worms of the ordinals skipped). The ordinals it skips,
         * the count up to it, become {@link #holes} (RP-242), so their worms are adopted when they load, whatever the
         * order; a hole it fills is taken off. Only the {@link #HOLES_MAX} highest are kept.
         */
        public void adopt(UUID worm, int ordinal) {
            worms.add(worm);
            if (!holes.remove(ordinal) && ordinal >= spawned) {
                for (int k = (int) Math.max(spawned, (long) ordinal - HOLES_MAX); k < ordinal; k++)
                    holes.add(k);
                while (holes.size() > HOLES_MAX)
                    holes.pollFirst();
                spawned = ordinal == Integer.MAX_VALUE ? ordinal : ordinal + 1;
            }
            if (owed > 0)
                owed--;
        }

        /** One loaded worm of this nest, for {@link #settle}. */
        public record Loaded(UUID worm, int ordinal) {}

        /** What {@link #settle} decided: the copies to discard, and how many worms it adopted. */
        public record Settled(List<UUID> stale, int adopted) {}

        /**
         * The join rule over worms already loaded when the nest was registered (RP-241): each goes through {@link #judge}
         * in turn, a stale one is returned to be discarded, an adopted one is {@link #adopt adopted}. Order does not
         * matter (the holes see to that), and two worms of one ordinal end as one adopted and one stale.
         */
        public Settled settle(List<Loaded> loaded) {
            List<UUID> stale = new ArrayList<>();
            int adopted = 0;
            for (Loaded l : loaded)
                switch (judge(l.worm(), l.ordinal())) {
                    case STALE -> stale.add(l.worm());
                    case ADOPT -> {
                        adopt(l.worm(), l.ordinal());
                        adopted++;
                    }
                    default -> {
                    }
                }
            return new Settled(stale, adopted);
        }

        /**
         * The ordinal the nest's next worm carries (RP-242): the lowest hole, taken, else the count, raised. The old copy
         * of an ordinal spawned again finds no hole and is stale. {@link #returnOrdinal} undoes it if the worm cannot join.
         */
        public int takeOrdinal() {
            if (!holes.isEmpty())
                return holes.pollFirst();
            return spawned++;
        }

        /** Undoes {@link #takeOrdinal} for a worm that did not join: {@code hole} says whether it was a hole's. */
        public void returnOrdinal(int ordinal, boolean hole) {
            if (hole)
                holes.add(ordinal);
            else
                spawned = ordinal;
        }

        /**
         * Whether the death of {@code worm} is the nest's loss (RP-242): not for an unlisted worm of an ordinal, which
         * the nest never counted (the census would not have held it, and the engine does not either).
         */
        public boolean countsLoss(UUID worm, int ordinal) {
            return ordinal < 0 || worms.contains(worm);
        }
    }

    /** Stamps nest {@code nest}'s id on a heart's persistent data (RP-240). */
    static void markHeart(CompoundTag persistent, long nest) {
        persistent.putLong(HEART_TAG, nest);
    }

    /**
     * Whether a spawner block entity's saved custom data ({@code saveCustomOnly}, which carries the NeoForge persistent
     * data under "NeoForgeData") is nest {@code nest}'s heart (RP-240). A spawner of the world's own, or of another
     * nest, is not.
     */
    static boolean marksNest(CompoundTag spawnerSave, long nest) {
        CompoundTag persistent = spawnerSave.getCompound("NeoForgeData");
        return persistent.contains(HEART_TAG, Tag.TAG_LONG) && persistent.getLong(HEART_TAG) == nest;
    }

    /** How a worm's last sortie went (GameTests and the log). */
    public record Outcome(int kind, long nest, int x, int z, BlockPos at, long tick) {}

    public static final int OUT_STOPPED = 1, OUT_SURFACED = 2, OUT_HOME = 3;

    private static final int GLIDE_OUT = 0, FIGHT = 1, GLIDE_HOME = 2;

    private static final class Glide {
        final UUID worm;
        final long nest;
        final ResourceKey<Level> dim;
        final int tx, tz;
        final long deadline;
        int phase;
        double x, y, z, approachY;
        boolean rising;

        Glide(UUID worm, long nest, ResourceKey<Level> dim, int tx, int tz, long deadline) {
            this.worm = worm;
            this.nest = nest;
            this.dim = dim;
            this.tx = tx;
            this.tz = tz;
            this.deadline = deadline;
        }
    }

    private record Load(ResourceKey<Level> dim, long chunk) {}

    private record Homecoming(ResourceKey<Level> dim, UUID worm) {}

    private final DragonBridge bridge;
    /** Chunk loads of watched chunks (any thread); drained at the end of a tick. Outlives {@link #clear}. */
    private final ConcurrentLinkedQueue<Load> loads = new ConcurrentLinkedQueue<>();
    /** Chunks (any dimension) with a waiting nest or a heart: the cheap filter of {@link #loaded}. */
    private final Set<Long> watched = ConcurrentHashMap.newKeySet();
    /** Worms that joined a level saved mid-glide: they glide home once a session ticks. Outlives {@link #clear}. */
    private final ConcurrentLinkedQueue<Homecoming> homecomings = new ConcurrentLinkedQueue<>();
    /** The saved state the indexes below were built from (null: build them). */
    private Data indexed;
    /** Per dimension: chunk to the nests (waiting or placed) in it. */
    private final Map<String, Map<Long, List<Long>>> byChunk = new HashMap<>();
    /** Per dimension: heart position to its nest. */
    private final Map<String, Map<Long, Nest>> hearts = new HashMap<>();
    /** Waiting nests whose chunk is loaded, in order. */
    private final LinkedHashSet<Long> toPlace = new LinkedHashSet<>();
    private final Map<UUID, Glide> glides = new LinkedHashMap<>();
    /** What each worm's sorties came to, in order, the last 256 worms (GameTests and the log). */
    private final Map<UUID, List<Outcome>> outcomes = new LinkedHashMap<>() {
        @Override
        protected boolean removeEldestEntry(Map.Entry<UUID, List<Outcome>> eldest) {
            return size() > 256;
        }
    };
    /** The last PLACE_NESTs received (GameTests and the log). */
    public final List<Pending> asked = new ArrayList<>();
    /** The game time owed worms are next looked at (every 20 ticks). */
    private long nextSpawnCheck = Long.MIN_VALUE;

    // Counters for logs and the GameTests (this session).
    public long requests, repeats, refused, badRequests, placedCount, failedCount, renews, renewsUnknown, renewsRepeated, reformed, wormsPlaced,
            censuses, losses, heartsBroken, sorties, wormsSent, stoppedUnderArmour, surfaced, homeComings, spawnerCancelled,
            griefCancelled, mended, heartsRecognised, staleWorms, adoptedWorms, lossesSkipped;

    Nests(DragonBridge bridge) {
        this.bridge = bridge;
    }

    /** Session start: the indexes are rebuilt from the saved state at the next tick; glides end. */
    void clear() {
        endGlides(false);
        indexed = null;
        byChunk.clear();
        hearts.clear();
        watched.clear();
        toPlace.clear();
        outcomes.clear();
        asked.clear();
        nextSpawnCheck = Long.MIN_VALUE;
        requests = repeats = refused = badRequests = placedCount = failedCount = renews = renewsUnknown = renewsRepeated = reformed = wormsPlaced = 0;
        censuses = losses = heartsBroken = sorties = wormsSent = stoppedUnderArmour = surfaced = homeComings = spawnerCancelled = 0;
        griefCancelled = mended = 0;
        heartsRecognised = staleWorms = adoptedWorms = lossesSkipped = 0;
    }

    // ---- the saved state and its indexes -----------------------------------------------------------

    /** The saved state, from the overworld's data storage. */
    public Data data() {
        return data(bridge.session().server());
    }

    private static Data data(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(Data.FACTORY, Data.NAME);
    }

    /** The placed nest {@code id}, or null. */
    public Nest nest(long id) {
        return data().placed.get(id);
    }

    /** The placed nest whose heart lies in chunk {@code chunk} of {@code dim}, or null. */
    public Nest placedIn(String dim, ChunkPos chunk) {
        for (Nest n : data().placed.values())
            if (n.dim.equals(dim) && n.chunk().equals(chunk))
                return n;
        return null;
    }

    /** What worm {@code worm}'s sorties came to this session, in order (empty for none). */
    public List<Outcome> outcomes(UUID worm) {
        List<Outcome> l = outcomes.get(worm);
        return l == null ? List.of() : List.copyOf(l);
    }

    private void note(UUID worm, Outcome o) {
        outcomes.computeIfAbsent(worm, k -> new ArrayList<>(2)).add(o);
    }

    /** True while worm {@code worm} is out on a sortie (gliding or fighting). */
    public boolean onSortie(UUID worm) {
        return glides.containsKey(worm);
    }

    private Data index(MinecraftServer server) {
        Data d = data(server);
        if (d == indexed)
            return d;
        indexed = d;
        byChunk.clear();
        hearts.clear();
        watched.clear();
        toPlace.clear();
        for (Pending p : d.pending.values())
            indexChunk(p.dim(), ChunkPos.asLong(p.cx(), p.cz()), p.nest());
        for (Nest n : d.placed.values())
            indexNest(n);
        // Loads before the index existed (the spawn chunks, before the session) were not queued.
        for (Pending p : d.pending.values()) {
            ServerLevel level = level(server, p.dim());
            if (level != null && level.getChunkSource().getChunkNow(p.cx(), p.cz()) != null)
                toPlace.add(p.nest());
        }
        for (Nest n : d.placed.values()) {
            ServerLevel level = level(server, n.dim);
            if (level != null && level.isLoaded(n.heart))
                loads.add(new Load(level.dimension(), n.chunk().toLong()));
        }
        return d;
    }

    private void indexChunk(String dim, long chunk, long nest) {
        List<Long> l = byChunk.computeIfAbsent(dim, k -> new HashMap<>()).computeIfAbsent(chunk, k -> new ArrayList<>(1));
        if (!l.contains(nest))
            l.add(nest);
        watched.add(chunk);
    }

    private void indexNest(Nest n) {
        indexChunk(n.dim, n.chunk().toLong(), n.id);
        hearts.computeIfAbsent(n.dim, k -> new HashMap<>()).put(n.heart.asLong(), n);
    }

    private void unindex(String dim, long chunk, long nest) {
        Map<Long, List<Long>> m = byChunk.get(dim);
        List<Long> l = m == null ? null : m.get(chunk);
        if (l != null) {
            l.remove(nest);
            if (l.isEmpty())
                m.remove(chunk);
        }
    }

    /** The nest whose heart is at {@code pos} in {@code level}, or null (before the first tick: null). */
    private Nest heartAt(Level level, BlockPos pos) {
        Map<Long, Nest> m = hearts.get(dimName(level));
        return m == null ? null : m.get(pos.asLong());
    }

    private static String dimName(Level level) {
        return level.dimension().location().toString();
    }

    private static ServerLevel level(MinecraftServer server, String dim) {
        ResourceLocation loc = dim == null ? null : ResourceLocation.tryParse(dim);
        return loc == null ? null : server.getLevel(ResourceKey.create(Registries.DIMENSION, loc));
    }

    private ServerLevel level(String dim) {
        return level(bridge.session().server(), dim);
    }

    private RtsimSession live() {
        RtsimSession s = bridge.session();
        return s == null || s.failed() || RtsimSession.current() != s ? null : s;
    }

    private static int dimId(RtsimSession s, String dim) {
        ResourceLocation loc = dim == null ? null : ResourceLocation.tryParse(dim);
        return loc == null ? 0 : s.dimId(ResourceKey.create(Registries.DIMENSION, loc));
    }

    // ---- commands -------------------------------------------------------------------------------

    /**
     * PLACE_NEST from the engine. Idempotent: a placed nest answers again with its heart, a waiting one
     * waits. A placed nest whose heart is broken re-forms it (the engine's reform after its wait, OGC79):
     * at once when the heart's chunk is loaded, else at its next load, before that load's census.
     */
    public void place(long nest, String dim, int cx, int cz, int depth, long now) {
        requests++;
        if (asked.size() >= 64)
            asked.remove(0);
        asked.add(new Pending(nest, dim, cx, cz, depth));
        Data d = index(bridge.session().server());
        Nest n = d.placed.get(nest);
        if (n != null) {
            repeats++;
            ServerLevel level = level(n.dim);
            boolean loaded = level != null && level.isLoaded(n.heart);
            if (n.broken || (loaded && !heartStands(level, n))) {
                n.broken = true;
                n.reform = true;
                if (loaded)
                    reform(level, n);
                d.setDirty();
            }
            found(nest, n.dim, n.heart, now);
            return;
        }
        if (d.pending.containsKey(nest)) {
            repeats++;
            return;
        }
        if (!DragonConfig.placeNests()) {
            refused++;
            MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_NEST {} refused: placeNests is off", nest);
            return;
        }
        ServerLevel level = level(dim);
        if (level == null || depth < 1) {
            badRequests++;
            MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] PLACE_NEST {} in {} at depth {}: failed", nest, dim, depth);
            found(nest, dim, null, now);
            return;
        }
        Pending p = new Pending(nest, dim, cx, cz, depth);
        d.pending.put(nest, p);
        d.setDirty();
        indexChunk(dim, ChunkPos.asLong(cx, cz), nest);
        if (level.getChunkSource().getChunkNow(cx, cz) != null)
            toPlace.add(nest);
    }

    /**
     * NEST_RENEW: {@code count} more worms of size {@code stage} for the nest. A broken heart re-forms
     * here too, as a fallback: the engine's own reform is a PLACE_NEST ({@link #place}).
     *
     * <p>Applied once per serial (docs/replay-gate.md §7.1, RP-14): {@code serial} is the engine's count of this
     * nest's renewals with this one in it, and the nest remembers the last it applied and the engine life it came
     * under. A renewal the engine decides again (after a restore from older pages) carries the serial it had
     * and adds nothing. A renewal below the nest's last serial is dropped too:
     * the rule is to lose, never to double. A life the nest has not seen starts its serial afresh (RP-19); one it has seen
     * takes up the serial it had reached (RP-232).
     */
    public void renew(long nest, int stage, int count, long serial, long now) {
        renews++;
        Data d = index(bridge.session().server());
        Nest n = d.placed.get(nest);
        if (n == null || count <= 0) {
            renewsUnknown++;
            MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] NEST_RENEW {} for nest {}: {}", count, nest,
                    n == null ? "no such nest placed" : "nothing to renew");
            return;
        }
        boolean fresh = n.renewed(bridge.session().engineLife(), serial, count, stage, now);
        d.setDirty();
        if (!fresh) {
            renewsRepeated++;
            MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] NEST_RENEW {} for nest {}: serial {} is not above {} (engine life {}),"
                    + " owed once already", count, nest, serial, n.serial, Long.toHexString(n.life));
        }
    }

    /** NEST_SORTIE: up to {@code count} of the nest's loaded worms raid column (x, z) for {@code timeout} ticks. */
    public void sortie(long nest, int x, int z, int timeout, int count, long now) {
        sorties++;
        Data d = index(bridge.session().server());
        Nest n = d.placed.get(nest);
        ServerLevel level = n == null ? null : level(n.dim);
        if (level == null || count <= 0)
            return;
        int sent = 0;
        for (UUID u : n.worms) {
            if (sent >= count)
                break;
            if (glides.containsKey(u) || !(level.getEntity(u) instanceof DeathWormEntity w) || !w.isAlive())
                continue;
            Glide g = new Glide(u, nest, level.dimension(), x, z, now + Math.max(1, timeout));
            g.x = w.getX();
            g.y = w.getY();
            g.z = w.getZ();
            g.approachY = approachY(level, g.y, x, z);
            glides.put(u, g);
            w.getPersistentData().putBoolean(SORTIE_TAG, true);
            stopAi(w);
            sent++;
        }
        wormsSent += sent;
        MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] NEST_SORTIE: {} of nest {}'s worms raid ({}, {}) for {} ticks",
                sent, nest, x, z, timeout);
    }

    // ---- the tick -------------------------------------------------------------------------------

    void tick(long now) {
        MinecraftServer server = bridge.session().server();
        Data d = index(server);
        drainLoads(server, d, now);
        placeWaiting(d, now);
        for (Homecoming h; (h = homecomings.poll()) != null; ) {
            ServerLevel level = server.getLevel(h.dim());
            if (level != null && level.getEntity(h.worm()) instanceof DeathWormEntity w && w.isAlive() && !glides.containsKey(w.getUUID())) {
                Glide g = new Glide(w.getUUID(), w.getPersistentData().getLong(TAG), level.dimension(), w.getBlockX(), w.getBlockZ(), now);
                g.x = w.getX();
                g.y = w.getY();
                g.z = w.getZ();
                glides.put(w.getUUID(), g);
                goHome(g, w);
            }
        }
        if (!glides.isEmpty())
            for (Glide g : List.copyOf(glides.values()))
                glide(server, g, now);
        if (now >= nextSpawnCheck) {
            nextSpawnCheck = now + 20;
            placeOwed(d, now);
        }
    }

    /** A chunk loaded (any thread): queued when a nest waits in it or has its heart there. */
    public void loaded(LevelChunk chunk) {
        if (watched.contains(chunk.getPos().toLong()))
            loads.add(new Load(chunk.getLevel().dimension(), chunk.getPos().toLong()));
    }

    private void drainLoads(MinecraftServer server, Data d, long now) {
        for (Load l; (l = loads.poll()) != null; ) {
            Map<Long, List<Long>> m = byChunk.get(l.dim().location().toString());
            List<Long> ids = m == null ? null : m.get(l.chunk());
            if (ids == null)
                continue;
            ServerLevel level = server.getLevel(l.dim());
            if (level == null)
                continue;
            for (long id : List.copyOf(ids)) {
                if (d.pending.containsKey(id)) {
                    toPlace.add(id);
                    continue;
                }
                Nest n = d.placed.get(id);
                if (n == null || !level.isLoaded(n.heart))
                    continue;
                // A fresh load of the heart's chunk: a heart owed by a PLACE_NEST re-forms first, then the
                // worms are counted, and the owed ones are due now.
                if (n.broken && n.reform)
                    reform(level, n);
                census(n, level, now);
                if (n.owed > 0 && n.dueAt > now) {
                    n.dueAt = now;
                    d.setDirty();
                }
            }
        }
    }

    private void placeWaiting(Data d, long now) {
        if (toPlace.isEmpty() || !DragonConfig.placeNests())
            return;
        int budget = PLACEMENTS_PER_TICK;
        for (Iterator<Long> it = toPlace.iterator(); it.hasNext() && budget > 0; ) {
            long id = it.next();
            Pending p = d.pending.get(id);
            ServerLevel level = p == null ? null : level(p.dim());
            LevelChunk chunk = level == null ? null : level.getChunkSource().getChunkNow(p.cx(), p.cz());
            if (chunk == null) {
                it.remove();
                continue;
            }
            budget--;
            it.remove();
            d.pending.remove(id);
            unindex(p.dim(), ChunkPos.asLong(p.cx(), p.cz()), id);
            // A nest the saved state lost but whose chamber is in the chunk (this file older than the chunk's, RP-240): its
            // heart is found by the nest id it carries, and nothing is carved. A second chamber would be a second heart
            // and a second brood.
            BlockPos heart = findHeart(level, chunk, id);
            boolean standing = heart != null;
            int settled = 0;
            if (!standing)
                heart = carve(level, chunk, id, p.depth());
            if (heart == null) {
                failedCount++;
                MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_NEST {}: no natural ground for a chamber in chunk {} of {}",
                        id, chunk.getPos(), p.dim());
            } else {
                Nest n = new Nest(id, p.dim(), heart);
                d.placed.put(id, n);
                indexNest(n);
                if (standing) {
                    settled = settleLoaded(level, n);
                    heartsRecognised++;
                    MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_NEST {}: its chamber stands, heart at {} in {}: nothing carved",
                            id, heart.toShortString(), p.dim());
                } else {
                    placedCount++;
                    MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] PLACE_NEST {}: chamber carved, heart at {} in {}", id,
                            heart.toShortString(), p.dim());
                }
            }
            d.setDirty();
            found(id, p.dim(), heart, now);
            // Worms the nest holds now that were loaded before it was known: the engine is told after NEST_FOUND.
            if (settled > 0 && heart != null)
                census(d.placed.get(id), level, now);
        }
    }

    /**
     * The worms of nest {@code n} already loaded when its chamber was found standing (RP-241): their joins ran while the
     * file did not know the nest, so {@link #staleCopy} let them through. They go through the join rule now, once, so a
     * chunk that stays loaded until the next renewal cannot hold both an old worm and the one the nest spawns. The scan is
     * the heart's chunk and the ring of chunks round it (48 by 48 blocks, the world's full height), through the level's
     * entity index: one query per registration, never per tick, and it reads the sections of nine columns that hold
     * entities. A stale copy is cancelled and discarded as a join does; the number adopted is returned.
     */
    private int settleLoaded(ServerLevel level, Nest n) {
        ChunkPos c = n.chunk();
        AABB area = new AABB(c.getMinBlockX() - 16, level.getMinBuildHeight(), c.getMinBlockZ() - 16,
                c.getMaxBlockX() + 17, level.getMaxBuildHeight(), c.getMaxBlockZ() + 17);
        Map<UUID, DeathWormEntity> found = new LinkedHashMap<>();
        List<Nest.Loaded> loaded = new ArrayList<>();
        for (DeathWormEntity w : level.getEntitiesOfClass(DeathWormEntity.class, area)) {
            CompoundTag t = w.getPersistentData();
            int ordinal = ordinal(w);
            if (ordinal < 0 || !t.contains(TAG, Tag.TAG_LONG) || t.getLong(TAG) != n.id || w.isRemoved())
                continue;
            found.put(w.getUUID(), w);
            loaded.add(new Nest.Loaded(w.getUUID(), ordinal));
        }
        if (loaded.isEmpty())
            return 0;
        Nest.Settled s = n.settle(loaded);
        for (UUID u : s.stale()) {
            DeathWormEntity w = found.get(u);
            staleWorms++;
            MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}: worm {} was loaded before the nest was found and is a stale copy: discarded", n.id, u);
            w.ejectPassengers();
            w.stopRiding();
            w.discard();
        }
        adoptedWorms += s.adopted();
        if (s.adopted() > 0)
            MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}: {} worm(s) loaded before the nest was found adopted (spawned now {})",
                    n.id, s.adopted(), n.spawned);
        return s.adopted();
    }

    // ---- placement --------------------------------------------------------------------------------

    /**
     * The chamber floor centres tried for nest {@code nest} in {@code chunk}, in order: drawn from the nest
     * id, each far enough inside the chunk for the chamber and its shell, the first {@code depth} below
     * the ocean-floor heightmap (or at {@code forcedFloor}), the rest up to {@link #SHIFT} above or below.
     */
    public static List<BlockPos> spots(long nest, LevelChunk chunk, int depth, Integer forcedFloor) {
        RandomSource r = RandomSource.create(nest ^ SALT);
        ChunkPos cp = chunk.getPos();
        int min = chunk.getMinBuildHeight() + 1, max = chunk.getMaxBuildHeight() - AIR - 2;
        int margin = HALF + 1;
        List<BlockPos> out = new ArrayList<>(TRIES);
        for (int i = 0; i < TRIES; i++) {
            int x = cp.getMinBlockX() + margin + r.nextInt(16 - 2 * margin);
            int z = cp.getMinBlockZ() + margin + r.nextInt(16 - 2 * margin);
            int shift = i == 0 ? 0 : r.nextInt(2 * SHIFT + 1) - SHIFT;
            int floor = forcedFloor != null ? forcedFloor : chunk.getHeight(Heightmap.Types.OCEAN_FLOOR, x, z) - depth;
            out.add(new BlockPos(x, Mth.clamp(floor + shift, min, max), z));
        }
        return out;
    }

    /** Natural ground a nest may be carved in. */
    public static boolean natural(BlockState s) {
        return (s.is(BlockTags.OVERWORLD_CARVER_REPLACEABLES) || s.is(Tags.Blocks.ORES)) && s.getFluidState().isEmpty() && !s.hasBlockEntity();
    }

    /** True when the chamber at {@code centre} (its floor's middle) and its shell are natural ground, in the chunk. */
    public static boolean fits(LevelChunk chunk, BlockPos centre) {
        BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
        ChunkPos cp = chunk.getPos();
        if (((centre.getX() - HALF - 1) >> 4) != cp.x || ((centre.getX() + HALF + 1) >> 4) != cp.x
                || ((centre.getZ() - HALF - 1) >> 4) != cp.z || ((centre.getZ() + HALF + 1) >> 4) != cp.z)
            return false;
        for (int dy = -1; dy <= AIR + 1; dy++)
            for (int dx = -HALF - 1; dx <= HALF + 1; dx++)
                for (int dz = -HALF - 1; dz <= HALF + 1; dz++) {
                    BlockState s = chunk.getBlockState(p.set(centre.getX() + dx, centre.getY() + dy, centre.getZ() + dz));
                    if (!natural(s))
                        return false;
                    if (dy == AIR + 1 && s.getBlock() instanceof FallingBlock)
                        return false; // the roof would fall in
                }
        return true;
    }

    /** Carves the first spot that fits and sets its heart; null when none does. */
    private static BlockPos carve(ServerLevel level, LevelChunk chunk, long nest, int depth) {
        for (BlockPos centre : spots(nest, chunk, depth, DragonConfig.Test.nestFloorY)) {
            if (!fits(chunk, centre))
                continue;
            BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
            for (int dx = -HALF; dx <= HALF; dx++)
                for (int dz = -HALF; dz <= HALF; dz++) {
                    level.setBlock(p.set(centre.getX() + dx, centre.getY(), centre.getZ() + dz), Blocks.SAND.defaultBlockState(), Block.UPDATE_CLIENTS);
                    for (int dy = 1; dy <= AIR; dy++)
                        level.setBlock(p.set(centre.getX() + dx, centre.getY() + dy, centre.getZ() + dz), Blocks.AIR.defaultBlockState(), Block.UPDATE_CLIENTS);
                }
            if (!setHeart(level, centre, nest))
                return null;
            return centre.immutable();
        }
        return null;
    }

    /**
     * The heart of nest {@code nest} in {@code chunk}, or null: a spawner whose NeoForge persistent data carries the nest's
     * id ({@link #HEART_TAG}, RP-240). One pass over the chunk's block entity positions, once per placement, never per tick;
     * only a spawner is read, through {@code saveCustomOnly}, so no foreign block entity gets a persistent-data compound
     * allocated (and saved) by a {@code getPersistentData()} call. The lowest position wins, should two chambers carry it.
     */
    static BlockPos findHeart(ServerLevel level, LevelChunk chunk, long nest) {
        BlockPos found = null;
        for (BlockPos pos : chunk.getBlockEntitiesPos()) {
            if (!chunk.getBlockState(pos).is(Blocks.SPAWNER) || (found != null && found.asLong() <= pos.asLong()))
                continue;
            if (chunk.getBlockEntity(pos) instanceof SpawnerBlockEntity sp && marksNest(sp.saveCustomOnly(level.registryAccess()), nest))
                found = pos.immutable();
        }
        return found;
    }

    /**
     * A spawner at {@code heart}, set to the Death Worm, that never spawns (RequiredPlayerRange 0), and marked with the
     * nest's id in its persistent data (RP-240): saved in the chunk's own file, beside the blocks this carved.
     */
    private static boolean setHeart(ServerLevel level, BlockPos heart, long nest) {
        level.setBlock(heart, Blocks.SPAWNER.defaultBlockState(), Block.UPDATE_CLIENTS);
        EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.getOptional(DEATHWORM).orElse(null);
        if (!(level.getBlockEntity(heart) instanceof SpawnerBlockEntity sp) || type == null)
            return false;
        sp.setEntityId(type, level.random);
        BaseSpawner b = sp.getSpawner();
        CompoundTag t = b.save(new CompoundTag());
        t.putShort("RequiredPlayerRange", (short) 0);
        b.load(level, heart, t);
        markHeart(sp.getPersistentData(), nest);
        sp.setChanged();
        return true;
    }

    /** The heart of a nest whose chunk is loaded is still a spawner. */
    private static boolean heartStands(ServerLevel level, Nest n) {
        return !level.isLoaded(n.heart) || level.getBlockState(n.heart).is(Blocks.SPAWNER);
    }

    // ---- worms ------------------------------------------------------------------------------------

    /** The variant (0 tan, 1 red, 2 white) of nest {@code nest}'s worms. */
    public static int variant(long nest) {
        long h = nest * 0x9E3779B97F4A7C15L;
        return (int) Math.floorMod(h ^ (h >>> 29), 3L);
    }

    private int retry() {
        return DragonConfig.Test.nestRetryTicks != null ? DragonConfig.Test.nestRetryTicks : NEST_DAY;
    }

    private void placeOwed(Data d, long now) {
        for (Nest n : d.placed.values()) {
            if (n.owed <= 0 || n.dueAt > now)
                continue;
            ServerLevel level = level(n.dim);
            if (level == null || !level.isLoaded(n.heart)) {
                n.dueAt = now + retry(); // its chunk's next load makes it due again
                d.setDirty();
                continue;
            }
            if (!RtsimSession.entitiesTickAt(level, n.heart)) {
                n.dueAt = now + Math.min(retry(), NOT_TICKING_RETRY);
                d.setDirty();
                continue;
            }
            if (level.hasNearbyAlivePlayer(n.heart.getX() + 0.5, n.heart.getY() + 0.5, n.heart.getZ() + 0.5, CLEAR_OF_PLAYERS)) {
                n.dueAt = now + retry();
                d.setDirty();
                continue;
            }
            if (n.broken && n.reform)
                reform(level, n);
            if (n.broken || !heartStands(level, n)) {
                // A heart gone with no BreakEvent (an explosion, a machine): the census says so.
                n.broken = true;
                n.reform = false;
                n.owed = 0;
                d.setDirty();
                census(n, level, now);
                continue;
            }
            int placed = 0;
            while (n.owed > 0) {
                DeathWormEntity w = spawnWorm(level, n);
                if (w == null)
                    break;
                n.owed--;
                placed++;
            }
            wormsPlaced += placed;
            if (n.owed > 0)
                n.dueAt = now + retry();
            d.setDirty();
            MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}: {} worm(s) of stage {} placed, {} still owed", n.id, placed,
                    n.stage, n.owed);
            census(n, level, now);
        }
    }

    /** A broken heart re-forms where it was, if the spot is air, sand or natural ground. */
    private void reform(ServerLevel level, Nest n) {
        n.reform = false;
        BlockState s = level.getBlockState(n.heart);
        if (!(s.isAir() || s.is(BlockTags.SAND) || natural(s) || s.is(Blocks.SPAWNER)))
            return;
        if (setHeart(level, n.heart, n.id)) {
            n.broken = false;
            reformed++;
            MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}: its heart re-formed at {}", n.id, n.heart.toShortString());
        }
    }

    private DeathWormEntity spawnWorm(ServerLevel level, Nest n) {
        EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.getOptional(DEATHWORM).orElse(null);
        if (type == null || !(type.create(level) instanceof DeathWormEntity w))
            return null;
        int[] o = SPOTS[n.worms.size() % SPOTS.length]; // the worms already placed pick the next spot
        w.moveTo(n.heart.getX() + 0.5 + o[0], n.heart.getY() + 1, n.heart.getZ() + 0.5 + o[1], level.random.nextFloat() * 360f, 0f);
        // Ice and Fire's own order: the age first, then the scale (which sets the health to the new maximum).
        w.setWormAge(5);
        w.setDeathWormScale(n.stage);
        w.setVariant(variant(n.id));
        w.setWormHome(n.heart);
        w.setPersistenceRequired();
        w.getPersistentData().putLong(TAG, n.id);
        // The worm's ordinal (RP-242), and the worm in the list before it joins: its own join runs the join rule, which
        // must find it listed (it is the normal case). A refused join takes both back.
        boolean hole = !n.holes.isEmpty();
        int ordinal = n.takeOrdinal();
        w.getPersistentData().putInt(ORDINAL_TAG, ordinal);
        n.worms.add(w.getUUID());
        if (level.addFreshEntity(w))
            return w;
        n.worms.remove(w.getUUID());
        n.returnOrdinal(ordinal, hole);
        return null;
    }

    // ---- reports ----------------------------------------------------------------------------------

    private void found(long nest, String dim, BlockPos heart, long now) {
        RtsimSession s = live();
        if (s == null)
            return;
        int d = dimId(s, dim);
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.NEST_FOUND, DragonRecords.NestFound.SIZE,
                heart != null ? DragonRecords.NestFound.PLACED : DragonRecords.NestFound.FAILED, now);
        r.putLong(DragonRecords.NestFound.NEST, nest);
        r.putInt(DragonRecords.NestFound.DIM, d);
        r.putInt(DragonRecords.NestFound.HEART_X, heart == null ? 0 : heart.getX());
        r.putInt(DragonRecords.NestFound.HEART_Y, heart == null ? 0 : heart.getY());
        r.putInt(DragonRecords.NestFound.HEART_Z, heart == null ? 0 : heart.getZ());
        s.boundary().commit(r);
    }

    /** NEST_CENSUS for a nest whose heart's chunk is loaded. A heart found gone breaks the nest here. */
    public void census(Nest n, ServerLevel level, long now) {
        boolean missing = !heartStands(level, n);
        if (missing && !n.broken) {
            n.broken = true;
            n.reform = false;
            n.owed = 0;
            data().setDirty();
        }
        RtsimSession s = live();
        if (s == null)
            return;
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.NEST_CENSUS, DragonRecords.NestCensus.SIZE,
                missing ? DragonRecords.NestCensus.FLAG_HEART_MISSING : 0, now);
        r.putLong(DragonRecords.NestCensus.NEST, n.id);
        r.putInt(DragonRecords.NestCensus.WORMS, n.worms.size() + (n.broken ? 0 : n.owed));
        r.putInt(DragonRecords.NestCensus.RESERVED, 0);
        s.boundary().commit(r);
        censuses++;
    }

    /** A Death Worm died (LivingDeathEvent, not cancelled): a nest worm's death is a NEST_LOSS. */
    void onDeath(DeathWormEntity dead, DamageSource source) {
        if (!dead.getPersistentData().contains(TAG, Tag.TAG_LONG) || dead.getServer() == null)
            return;
        long nest = dead.getPersistentData().getLong(TAG);
        glides.remove(dead.getUUID());
        Nest n = data(dead.getServer()).placed.get(nest);
        int ordinal = ordinal(dead);
        if (n != null && !n.countsLoss(dead.getUUID(), ordinal)) {
            // A worm of an ordinal the nest does not hold (RP-242): its books never counted it, so its death is no loss of
            // theirs. The join rule keeps such a worm out of a level; this is for one that got in some other way.
            lossesSkipped++;
            return;
        }
        if (n != null && n.worms.remove(dead.getUUID()))
            data(dead.getServer()).setDirty();
        RtsimSession s = live();
        if (s == null)
            return; // the next census reports the worm missing
        UUID[] player = {null};
        int cause = cause(dead, source, player);
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.NEST_LOSS, DragonRecords.NestLoss.SIZE, cause, s.gameTime());
        r.putLong(DragonRecords.NestLoss.NEST, nest);
        r.putInt(DragonRecords.NestLoss.COUNT, 1);
        r.putInt(DragonRecords.NestLoss.RESERVED, 0);
        r.putLong(DragonRecords.NestLoss.PLAYER_MOST, player[0] == null ? 0 : player[0].getMostSignificantBits());
        r.putLong(DragonRecords.NestLoss.PLAYER_LEAST, player[0] == null ? 0 : player[0].getLeastSignificantBits());
        s.boundary().commit(r);
        losses++;
        MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {} lost a worm: cause {}, player {}", nest, cause, player[0]);
    }

    /**
     * The boss rule (EQUILIBRIUM.md §2.5, item 3): a player's loss unless a wild mob is credited with the
     * death. The player is the killer, the one who hurt it in the last 100 ticks ({@code getKillCredit}),
     * or the owner of the pet that killed it; a trap, lava or an unknown source counts with no player.
     */
    static int cause(LivingEntity dead, DamageSource source, UUID[] player) {
        Entity direct = source.getEntity();
        LivingEntity credit = dead.getKillCredit();
        if (direct instanceof Player p) {
            player[0] = p.getUUID();
            return DragonRecords.NestLoss.CAUSE_PLAYER;
        }
        if (credit instanceof Player p) {
            player[0] = p.getUUID();
            return DragonRecords.NestLoss.CAUSE_PLAYER;
        }
        for (Entity k : new Entity[] {direct, credit}) {
            if (k instanceof OwnableEntity o && o.getOwnerUUID() != null) {
                player[0] = o.getOwnerUUID();
                return DragonRecords.NestLoss.CAUSE_PLAYER;
            }
            if (k != null && ThreatClasses.guard(k))
                return DragonRecords.NestLoss.CAUSE_PLAYER;
        }
        Entity killer = direct != null ? direct : credit;
        if (killer instanceof LivingEntity && killer != dead)
            return DragonRecords.NestLoss.CAUSE_PREDATOR;
        return DragonRecords.NestLoss.CAUSE_PLAYER;
    }

    /** A player broke a block (BreakEvent, not cancelled): breaking a heart is NEST_HEART. */
    void onBreak(ServerLevel level, BlockPos pos, BlockState state, Player player, long now) {
        if (!state.is(Blocks.SPAWNER))
            return;
        index(level.getServer());
        Nest n = heartAt(level, pos);
        if (n == null || n.broken)
            return;
        n.broken = true;
        n.reform = false;
        n.owed = 0;
        data().setDirty();
        heartsBroken++;
        MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}: heart broken by {}", n.id, player == null ? null : player.getGameProfile().getName());
        RtsimSession s = live();
        if (s == null)
            return;
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.NEST_HEART, DragonRecords.NestHeart.SIZE, 0, now);
        r.putLong(DragonRecords.NestHeart.NEST, n.id);
        r.putLong(DragonRecords.NestHeart.PLAYER_MOST, player == null ? 0 : player.getUUID().getMostSignificantBits());
        r.putLong(DragonRecords.NestHeart.PLAYER_LEAST, player == null ? 0 : player.getUUID().getLeastSignificantBits());
        s.boundary().commit(r);
    }

    /** A worm left a level: one discarded (not killed, not unloaded) is gone from its nest. */
    void left(DeathWormEntity w) {
        if (!w.getPersistentData().contains(TAG, Tag.TAG_LONG) || w.getServer() == null)
            return;
        Entity.RemovalReason why = w.getRemovalReason();
        glides.remove(w.getUUID());
        if (why != Entity.RemovalReason.DISCARDED)
            return;
        Nest n = data(w.getServer()).placed.get(w.getPersistentData().getLong(TAG));
        if (n != null && n.worms.remove(w.getUUID())) {
            data(w.getServer()).setDirty();
            if (live() != null && w.level() instanceof ServerLevel level && level.isLoaded(n.heart))
                census(n, level, live().gameTime());
        }
    }

    // ---- hooks ------------------------------------------------------------------------------------

    /** FinalizeSpawnEvent of a spawner spawn: true when it comes from a nest's heart (to be cancelled). */
    boolean fromHeart(FinalizeSpawnEvent event) {
        if (hearts.isEmpty() || !(event.getLevel().getLevel() instanceof ServerLevel level))
            return false;
        var spawner = event.getSpawner();
        if (spawner != null && spawner.left().isPresent()) {
            BlockEntity be = spawner.left().get();
            if (heartAt(level, be.getBlockPos()) != null) {
                spawnerCancelled++;
                return true;
            }
        }
        if (!(event.getEntity() instanceof DeathWormEntity))
            return false;
        Map<Long, Nest> m = hearts.get(dimName(level));
        if (m == null)
            return false;
        Vec3 at = new Vec3(event.getX(), event.getY(), event.getZ());
        for (Nest n : m.values())
            if (n.heart.getCenter().distanceToSqr(at) <= HEART_GUARD * HEART_GUARD) {
                spawnerCancelled++;
                return true;
            }
        return false;
    }

    /**
     * Ice and Fire's grief hook (a worm's bite launching blocks): cancelled for nest worms. Not for a worm
     * that ate the TNT a player fed it (the event's target is the worm itself): Ice and Fire posts that one
     * every tick until it goes through, and the player lit it.
     */
    void onGrief(GriefBreakBlockEvent event) {
        LivingEntity g = event.getGriefer();
        if (!(g instanceof DeathWormEntity) || !g.getPersistentData().contains(TAG, Tag.TAG_LONG))
            return;
        if (event.getTargetX() == g.getX() && event.getTargetY() == g.getY() && event.getTargetZ() == g.getZ())
            return;
        event.setCanceled(true);
        griefCancelled++;
    }

    /** The worm's {@link #ORDINAL_TAG}, or -1 for a worm without one. */
    private static int ordinal(DeathWormEntity w) {
        CompoundTag t = w.getPersistentData();
        return t.contains(ORDINAL_TAG, Tag.TAG_INT) ? Math.max(0, t.getInt(ORDINAL_TAG)) : -1;
    }

    /**
     * A nest worm is about to join a level (EntityJoinLevelEvent, RP-242): true when it is a stale copy, to be cancelled
     * and discarded by the caller the way core discards a stale puppet. A copy is a worm whose ordinal the nest spawned
     * again after a rollback of its file, or which has died: the entity file holding it is older than the nest's.
     * Journals nothing (it is not a death). A worm no later session spawned the ordinal of is adopted into its nest
     * (one less owed), a worm in the list, a worm with no ordinal and a worm of a nest the file does not know are left to
     * today's rules. Once per worm join, a set lookup: never per tick.
     */
    boolean staleCopy(DeathWormEntity w) {
        CompoundTag t = w.getPersistentData();
        int ordinal = ordinal(w);
        if (ordinal < 0 || !t.contains(TAG, Tag.TAG_LONG) || w.getServer() == null)
            return false;
        Data d = data(w.getServer());
        Nest n = d.placed.get(t.getLong(TAG));
        if (n == null)
            return false;
        switch (n.judge(w.getUUID(), ordinal)) {
            case STALE -> {
                staleWorms++;
                MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}: worm {} of ordinal {} is a stale copy (the nest has spawned {}): discarded",
                        n.id, w.getUUID(), ordinal, n.spawned);
                return true;
            }
            case ADOPT -> {
                boolean wasOwed = n.owed > 0;
                n.adopt(w.getUUID(), ordinal);
                d.setDirty();
                adoptedWorms++;
                MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}: worm {} of ordinal {} adopted (spawned now {}, {} owed)",
                        n.id, w.getUUID(), ordinal, n.spawned, n.owed);
                // With nothing owed to cover it the nest holds one more worm than the engine was told.
                if (!wasOwed && live() != null && w.level() instanceof ServerLevel level && level.isLoaded(n.heart))
                    census(n, level, live().gameTime());
            }
            default -> {
            }
        }
        return false;
    }

    /** A Death Worm joined a level: a nest worm saved mid-glide gets its AI back and glides home. */
    void joined(DeathWormEntity w) {
        if (!w.getPersistentData().contains(TAG, Tag.TAG_LONG) || glides.containsKey(w.getUUID()))
            return;
        boolean sortie = w.getPersistentData().getBoolean(SORTIE_TAG);
        if (!sortie && !w.isNoAi() && !w.noPhysics)
            return;
        w.setNoAi(false);
        w.noPhysics = false;
        mended++;
        if (sortie)
            homecomings.add(new Homecoming(w.level().dimension(), w.getUUID()));
    }

    // ---- sorties ----------------------------------------------------------------------------------

    /**
     * The y a raiding worm crosses to the target column at: its own depth, at least {@link #RISE_FROM}
     * under the column's top, and under the lowest armoured floor or block entity in the column, so that
     * it rises past every floor a machine stands on.
     */
    private static double approachY(ServerLevel level, double startY, int x, int z) {
        int min = level.getMinBuildHeight() + 1;
        BlockPos column = new BlockPos(x, min, z);
        if (!level.isLoaded(column))
            return Math.max(min, startY);
        int top = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z);
        double y = Math.min(startY, top - RISE_FROM);
        BlockPos.MutableBlockPos p = column.mutable();
        for (int yy = min; yy < top; yy++) {
            BlockState s = level.getBlockState(p.setY(yy));
            if (armour(s) || s.hasBlockEntity()) {
                y = Math.min(y, yy - 3);
                break;
            }
        }
        return Math.max(min, y);
    }

    private static boolean armour(BlockState s) {
        return s.is(ThreatClasses.ARMOURED_FLOOR) || s.is(ThreatClasses.BURROW_SPARED);
    }

    private void glide(MinecraftServer server, Glide g, long now) {
        ServerLevel level = server.getLevel(g.dim);
        if (level == null || !(level.getEntity(g.worm) instanceof DeathWormEntity w) || !w.isAlive()) {
            glides.remove(g.worm); // unloaded: saved mid-glide, mended when it joins again
            return;
        }
        switch (g.phase) {
            case GLIDE_OUT -> {
                if (now >= g.deadline) {
                    goHome(g, w);
                    return;
                }
                if (!g.rising) {
                    if (step(level, w, g, new Vec3(g.tx + 0.5, g.approachY, g.tz + 0.5)))
                        g.rising = true;
                    return;
                }
                double next = g.y + GLIDE_SPEED;
                BlockPos head = BlockPos.containing(g.tx + 0.5, next + w.getBbHeight(), g.tz + 0.5);
                if (head.getY() >= level.getMaxBuildHeight() || !level.isLoaded(head)) {
                    goHome(g, w);
                    return;
                }
                if (armour(level.getBlockState(head))) {
                    stoppedUnderArmour++;
                    note(g.worm, new Outcome(OUT_STOPPED, g.nest, g.tx, g.tz, w.blockPosition(), now));
                    MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] nest {}'s worm stopped under {} at {}", g.nest,
                            BuiltInRegistries.BLOCK.getKey(level.getBlockState(head).getBlock()), head.toShortString());
                    goHome(g, w);
                    return;
                }
                move(w, g, g.tx + 0.5, next, g.tz + 0.5);
                AABB box = w.getDimensions(w.getPose()).makeBoundingBox(g.x, g.y, g.z);
                if (level.noBlockCollision(w, box) && !level.containsAnyLiquid(box)) {
                    g.phase = FIGHT;
                    w.noPhysics = false;
                    w.setNoAi(false);
                    Player target = level.getNearestPlayer(TargetingConditions.forCombat().range(FIGHT_REACH), w);
                    if (target != null)
                        w.setTarget(target);
                    surfaced++;
                    note(g.worm, new Outcome(OUT_SURFACED, g.nest, g.tx, g.tz, w.blockPosition(), now));
                }
            }
            case FIGHT -> {
                if (now >= g.deadline) {
                    g.x = w.getX();
                    g.y = w.getY();
                    g.z = w.getZ();
                    goHome(g, w);
                }
            }
            default -> {
                BlockPos h = w.getWormHome();
                int[] o = SPOTS[Math.floorMod(g.worm.hashCode(), SPOTS.length)];
                if (step(level, w, g, new Vec3(h.getX() + 0.5 + o[0], h.getY() + 1, h.getZ() + 0.5 + o[1]))) {
                    land(w);
                    glides.remove(g.worm);
                    homeComings++;
                    note(g.worm, new Outcome(OUT_HOME, g.nest, g.tx, g.tz, w.blockPosition(), now));
                }
            }
        }
    }

    /** One glide step toward {@code dest}; true on arrival. Waits at the edge of what is loaded. */
    private static boolean step(ServerLevel level, DeathWormEntity w, Glide g, Vec3 dest) {
        Vec3 cur = new Vec3(g.x, g.y, g.z);
        Vec3 d = dest.subtract(cur);
        double len = d.length();
        Vec3 next = len <= GLIDE_SPEED ? dest : cur.add(d.scale(GLIDE_SPEED / len));
        if (!level.isLoaded(BlockPos.containing(next)))
            return false;
        if (len > 1e-3)
            w.setYRot((float) (Mth.atan2(d.z, d.x) * Mth.RAD_TO_DEG) - 90f);
        move(w, g, next.x, next.y, next.z);
        return len <= GLIDE_SPEED;
    }

    /** The glide keeps its own position: the worm's tick adds a step of gravity between two of ours. */
    private static void move(DeathWormEntity w, Glide g, double x, double y, double z) {
        g.x = x;
        g.y = y;
        g.z = z;
        w.setDeltaMovement(Vec3.ZERO);
        w.setPos(x, y, z);
        w.setYHeadRot(w.getYRot());
        w.yBodyRot = w.getYRot();
        w.fallDistance = 0;
    }

    private static void stopAi(DeathWormEntity w) {
        w.setNoAi(true);
        w.noPhysics = true;
        w.setTarget(null);
        w.getNavigation().stop();
        w.setDeltaMovement(Vec3.ZERO);
    }

    private static void goHome(Glide g, DeathWormEntity w) {
        g.phase = GLIDE_HOME;
        stopAi(w);
    }

    private static void land(DeathWormEntity w) {
        w.noPhysics = false;
        w.setNoAi(false);
        w.getPersistentData().remove(SORTIE_TAG);
    }

    /**
     * Ends every glide: AI and physics back. {@code home}: the worm is put home at once and its sortie is
     * over (the bridge failed); otherwise it keeps its sortie mark and glides home when it next joins a
     * level (the server stops: the world save after this finds no worm without its AI).
     */
    private void endGlides(boolean home) {
        RtsimSession s = bridge.session();
        MinecraftServer server = s == null ? null : s.server();
        for (Glide g : List.copyOf(glides.values())) {
            ServerLevel level = server == null ? null : server.getLevel(g.dim);
            if (level != null && level.getEntity(g.worm) instanceof DeathWormEntity w) {
                w.noPhysics = false;
                w.setNoAi(false);
                if (home) {
                    BlockPos h = w.getWormHome();
                    if (level.isLoaded(h))
                        w.teleportTo(h.getX() + 0.5, h.getY() + 1, h.getZ() + 2.5);
                    w.getPersistentData().remove(SORTIE_TAG);
                }
            }
        }
        glides.clear();
    }

    /** The server is stopping, before the world save. */
    void stopping() {
        endGlides(false);
    }

    /** The bridge failed: no more ticks to glide in. */
    void failed() {
        endGlides(true);
    }

    /** GameTests: forgets nest {@code id} (the GameTest world is reused). */
    public void forgetForTest(long id) {
        Data d = data();
        Pending p = d.pending.remove(id);
        if (p != null)
            unindex(p.dim(), ChunkPos.asLong(p.cx(), p.cz()), id);
        Nest n = d.placed.remove(id);
        if (n != null) {
            unindex(n.dim, n.chunk().toLong(), id);
            Map<Long, Nest> m = hearts.get(n.dim);
            if (m != null)
                m.remove(n.heart.asLong());
        }
        toPlace.remove(id);
        d.setDirty();
    }

    // ---- the saved state ----------------------------------------------------------------------------

    /** What the plugin keeps with the world: the nests waiting for their chunk and the nests placed. */
    public static final class Data extends SavedData {
        public static final String NAME = "mic_rtsim_iceandfire_oil_nests";
        static final Factory<Data> FACTORY = new Factory<>(Data::new, Data::load);

        public final Map<Long, Pending> pending = new LinkedHashMap<>();
        public final Map<Long, Nest> placed = new LinkedHashMap<>();

        @Override
        public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
            ListTag p = new ListTag();
            for (Pending x : pending.values()) {
                CompoundTag t = new CompoundTag();
                t.putLong("nest", x.nest());
                t.putString("dimension", x.dim());
                t.putInt("cx", x.cx());
                t.putInt("cz", x.cz());
                t.putInt("depth", x.depth());
                p.add(t);
            }
            tag.put("pending", p);
            ListTag l = new ListTag();
            for (Nest n : placed.values()) {
                CompoundTag t = new CompoundTag();
                t.putLong("nest", n.id);
                t.putString("dimension", n.dim);
                t.putLong("heart", n.heart.asLong());
                t.putInt("owed", n.owed);
                t.putInt("spawned", n.spawned);
                if (!n.holes.isEmpty())
                    t.putIntArray("holes", n.holes.stream().mapToInt(Integer::intValue).toArray());
                t.putInt("stage", n.stage);
                t.putBoolean("broken", n.broken);
                t.putBoolean("reform", n.reform);
                t.putLong("dueAt", n.dueAt);
                t.putLong("serial", n.serial);
                t.putLong("life", n.life);
                ListTag lives = new ListTag();
                for (LifeSerial e : n.earlier) {
                    CompoundTag c = new CompoundTag();
                    c.putLong("life", e.life());
                    c.putLong("serial", e.serial());
                    lives.add(c);
                }
                t.put("lives", lives);
                ListTag worms = new ListTag();
                for (UUID u : n.worms)
                    worms.add(NbtUtils.createUUID(u));
                t.put("worms", worms);
                l.add(t);
            }
            tag.put("placed", l);
            return tag;
        }

        public static Data load(CompoundTag tag, HolderLookup.Provider registries) {
            Data d = new Data();
            for (Tag e : tag.getList("pending", Tag.TAG_COMPOUND)) {
                CompoundTag t = (CompoundTag) e;
                Pending p = new Pending(t.getLong("nest"), t.getString("dimension"), t.getInt("cx"), t.getInt("cz"), t.getInt("depth"));
                d.pending.put(p.nest(), p);
            }
            for (Tag e : tag.getList("placed", Tag.TAG_COMPOUND)) {
                CompoundTag t = (CompoundTag) e;
                Nest n = new Nest(t.getLong("nest"), t.getString("dimension"), BlockPos.of(t.getLong("heart")));
                // A file from before the renewal serial (RP-14) has neither serial nor life: its nests load owed 0 and
                // serial 0. Whatever was owed is dropped, never doubled: the engine may decide it again, and with no
                // serial to compare there is no telling whether it was applied.
                boolean serialled = t.contains("serial", Tag.TAG_ANY_NUMERIC) && t.contains("life", Tag.TAG_ANY_NUMERIC);
                n.owed = serialled ? t.getInt("owed") : 0;
                n.serial = serialled ? t.getLong("serial") : 0;
                n.life = serialled ? t.getLong("life") : 0;
                // The earlier lives' serials (RP-232): a file without the list, an old file or one written before it,
                // loads none.
                if (serialled)
                    for (Tag x : t.getList("lives", Tag.TAG_COMPOUND))
                        n.remember(((CompoundTag) x).getLong("life"), ((CompoundTag) x).getLong("serial"));
                // The worms ever spawned (RP-242): a file from before it loads 0, and no worm it holds carries an ordinal.
                n.spawned = Math.max(0, t.getInt("spawned"));
                // The skipped ordinals (RP-242): a file from before them has none. Only ordinals below the count, at most
                // HOLES_MAX of the highest, so a hand-edited file cannot grow the set.
                for (int h : t.getIntArray("holes"))
                    if (h >= 0 && h < n.spawned)
                        n.holes.add(h);
                while (n.holes.size() > Nest.HOLES_MAX)
                    n.holes.pollFirst();
                n.stage = Math.max(1, t.getInt("stage"));
                n.broken = t.getBoolean("broken");
                n.reform = t.getBoolean("reform");
                n.dueAt = t.getLong("dueAt");
                for (Tag u : t.getList("worms", Tag.TAG_INT_ARRAY))
                    n.worms.add(NbtUtils.loadUUID(u));
                d.placed.put(n.id, n);
            }
            return d;
        }
    }
}
