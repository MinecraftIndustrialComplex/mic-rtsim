package com.minecraftindustrialcomplex.mic_rtsim.base;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.saveddata.SavedData;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * Player bases as market nodes (C26). What a base is, is Proposal P30; this takes its recommendation
 * (choice G14): a base is a place a player spends much time, by a decaying dwell counter fed from the
 * position samples the bridge already takes (P26's counter). Caravans then trade there through their
 * own offers with the players present, and the trades' prices are the base's news (TRADE carries the
 * base as its node). A base has no treasury or stock of its own.
 *
 * <p>Space is cut into {@link #CELL}-block cells (PLACEHOLDER). Each sample adds 1 to the player's
 * counter for the cell they stand in; every counter halves every {@link #HALF_LIFE} ticks
 * (PLACEHOLDER), lazily. A counter reaching {@link #MARK} marks a base there (BASE MARKED); one that
 * decays below {@link #UNMARK} unmarks it (BASE UNMARKED). Each player keeps at most {@link #CELLS}
 * counters; the weakest goes. Saved in {@code data/mic_rtsim_bases.dat}.
 */
public final class BaseTracker extends SavedData {
    public static final String FILE = "mic_rtsim_bases";

    /** Cell size in blocks (PLACEHOLDER). */
    public static final int CELL = 128;
    /** Ticks for a dwell counter to halve (PLACEHOLDER: three in-game days). */
    public static final double HALF_LIFE = 72_000;
    /** Samples (one per 20 ticks while present) a counter needs to mark a base (PLACEHOLDER: about 30 minutes, decayed). */
    public static final double MARK = 1_800;
    /** Below this a marked base is unmarked (PLACEHOLDER: a quarter of MARK). */
    public static final double UNMARK = MARK / 4;
    /** Counters kept per player (PLACEHOLDER). */
    public static final int CELLS = 16;

    /** One player's presence in one cell. */
    public static final class Dwell {
        public ResourceKey<Level> dimension;
        public int cellX, cellZ;
        public double value;
        public long at;
        public double sx, sy, sz, weight;
        public boolean marked;

        double decayed(long now) {
            return value * Math.pow(0.5, Math.max(0, now - at) / HALF_LIFE);
        }

        void decayTo(long now) {
            double f = Math.pow(0.5, Math.max(0, now - at) / HALF_LIFE);
            value *= f;
            sx *= f;
            sy *= f;
            sz *= f;
            weight *= f;
            at = now;
        }
    }

    private final Map<UUID, List<Dwell>> byPlayer = new LinkedHashMap<>();
    long marks;
    long unmarks;

    public static final Factory<BaseTracker> FACTORY = new Factory<>(BaseTracker::new, BaseTracker::load);

    public static BaseTracker get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
    }

    public long marks() {
        return marks;
    }

    /** A position sample (every PLAYER observation, PLAN.md §6.3). */
    public void sample(RtsimSession session, ServerPlayer p, long now) {
        sample(session, p.getUUID(), p.level().dimension(), p.getX(), p.getY(), p.getZ(), 1.0, now);
    }

    /** A sample of {@code weight} (the GameTests use large weights to stand for long stays). */
    public void sample(RtsimSession session, UUID player, ResourceKey<Level> dim, double x, double y, double z, double weight, long now) {
        int cx = Math.floorDiv((int) Math.floor(x), CELL), cz = Math.floorDiv((int) Math.floor(z), CELL);
        List<Dwell> list = byPlayer.computeIfAbsent(player, k -> new ArrayList<>(4));
        Dwell d = null;
        for (Dwell e : list)
            if (e.cellX == cx && e.cellZ == cz && e.dimension.equals(dim))
                d = e;
        if (d == null) {
            if (list.size() >= CELLS) {
                Dwell weakest = list.get(0);
                for (Dwell e : list)
                    if (e.decayed(now) < weakest.decayed(now))
                        weakest = e;
                if (weakest.marked)
                    unmark(session, player, weakest, now);
                list.remove(weakest);
            }
            d = new Dwell();
            d.dimension = dim;
            d.cellX = cx;
            d.cellZ = cz;
            d.at = now;
            list.add(d);
        }
        d.decayTo(now);
        d.value += weight;
        d.sx += x * weight;
        d.sy += y * weight;
        d.sz += z * weight;
        d.weight += weight;
        if (!d.marked && d.value >= MARK) {
            d.marked = true;
            journal(session, player, d, Obs.Base.MARKED, now);
            marks++;
        }
        setDirty();
    }

    /** Unmarks bases whose counters decayed away (every few hundred ticks). */
    public void sweep(RtsimSession session, long now) {
        for (Map.Entry<UUID, List<Dwell>> e : byPlayer.entrySet())
            for (Dwell d : e.getValue())
                if (d.marked && d.decayed(now) < UNMARK)
                    unmark(session, e.getKey(), d, now);
    }

    private void unmark(RtsimSession session, UUID player, Dwell d, long now) {
        d.marked = false;
        journal(session, player, d, Obs.Base.UNMARKED, now);
        unmarks++;
        setDirty();
    }

    /** The marked base at a position, or 0: a trade there is the base's news (C26). */
    public long baseAt(RtsimSession session, ResourceKey<Level> dim, double x, double z) {
        int cx = Math.floorDiv((int) Math.floor(x), CELL), cz = Math.floorDiv((int) Math.floor(z), CELL);
        for (List<Dwell> list : byPlayer.values())
            for (Dwell d : list)
                if (d.marked && d.cellX == cx && d.cellZ == cz && d.dimension.equals(dim))
                    return Obs.baseId(session.dimId(dim), cx, cz);
        return 0;
    }

    private void journal(RtsimSession session, UUID player, Dwell d, int what, long now) {
        int dim = session.dimId(d.dimension);
        ByteBuffer r = session.boundary().beginJournaled(Obs.BASE, Obs.Base.SIZE, what, now);
        r.putLong(Obs.Base.BASE, Obs.baseId(dim, d.cellX, d.cellZ));
        r.putInt(Obs.Base.DIM, dim);
        double w = Math.max(d.weight, 1e-9);
        r.putDouble(Obs.Base.X, d.sx / w);
        r.putDouble(Obs.Base.Y, d.sy / w);
        r.putDouble(Obs.Base.Z, d.sz / w);
        r.putLong(Obs.Base.PLAYER_MOST, player.getMostSignificantBits());
        r.putLong(Obs.Base.PLAYER_LEAST, player.getLeastSignificantBits());
        r.putFloat(Obs.Base.DWELL, (float) d.decayed(now));
        session.boundary().commit(r);
    }

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        ListTag players = new ListTag();
        for (Map.Entry<UUID, List<Dwell>> e : byPlayer.entrySet()) {
            CompoundTag p = new CompoundTag();
            p.putUUID("player", e.getKey());
            ListTag cells = new ListTag();
            for (Dwell d : e.getValue()) {
                CompoundTag c = new CompoundTag();
                c.putString("dim", d.dimension.location().toString());
                c.putInt("x", d.cellX);
                c.putInt("z", d.cellZ);
                c.putDouble("v", d.value);
                c.putLong("at", d.at);
                c.putDouble("sx", d.sx);
                c.putDouble("sy", d.sy);
                c.putDouble("sz", d.sz);
                c.putDouble("w", d.weight);
                c.putBoolean("marked", d.marked);
                cells.add(c);
            }
            p.put("cells", cells);
            players.add(p);
        }
        tag.put("players", players);
        return tag;
    }

    private static BaseTracker load(CompoundTag tag, HolderLookup.Provider registries) {
        BaseTracker b = new BaseTracker();
        for (Tag t : tag.getList("players", Tag.TAG_COMPOUND)) {
            CompoundTag p = (CompoundTag) t;
            List<Dwell> list = new ArrayList<>();
            for (Tag ct : p.getList("cells", Tag.TAG_COMPOUND)) {
                CompoundTag c = (CompoundTag) ct;
                Dwell d = new Dwell();
                d.dimension = ResourceKey.create(Registries.DIMENSION, ResourceLocation.parse(c.getString("dim")));
                d.cellX = c.getInt("x");
                d.cellZ = c.getInt("z");
                d.value = c.getDouble("v");
                d.at = c.getLong("at");
                d.sx = c.getDouble("sx");
                d.sy = c.getDouble("sy");
                d.sz = c.getDouble("sz");
                d.weight = c.getDouble("w");
                d.marked = c.getBoolean("marked");
                list.add(d);
            }
            b.byPlayer.put(p.getUUID("player"), list);
        }
        return b;
    }

    /** For tests: the counters of a player. */
    public List<Dwell> dwell(UUID player) {
        return byPlayer.getOrDefault(player, List.of());
    }

    static Map<UUID, List<Dwell>> copy(Map<UUID, List<Dwell>> m) {
        return new HashMap<>(m);
    }
}
