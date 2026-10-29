package com.minecraftindustrialcomplex.mic_rtsim.sites;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;

/**
 * Follows a site's footprint chunks in and out (EQUILIBRIUM.md S5, EQC128): a site is <b>in</b> while every chunk of
 * its footprint is at ticket level FULL (33) or lower with its entities loaded, and <b>out</b> otherwise. Mod-agnostic:
 * a plugin declares an {@link Area} (a key and a disc round a point) and a {@link Listener}, and is told when the area
 * comes in (it mints the epoch) and when it goes out.
 *
 * <p>The camps' proven rules ({@code Camps}, MZB-290, MZB-249, MZB-660), copied rather than moved so that M12's code is
 * not touched: a poll every {@link #POLL_PERIOD} ticks is the authority, each area in its own slot of them, so a tick
 * reads a twentieth of the areas; {@code ChunkTicketLevelUpdatedEvent} (forwarded by the plugin to {@link #onTicket}) is
 * an early hint only, since C2ME replaces the method that posts it. An area whose chunks moved is looked at once at the
 * end of the tick, from the final levels. A chunk counts as loaded only when its entities are visible to the level's
 * queries, not merely loaded (MZB-660). After it goes out, an area waits {@code gap} ticks before it can come in again,
 * so a player standing on the boundary makes no record storm. Server thread only.
 */
public final class ChunkWatch {
    /** The ticket level of a FULL chunk; above it a chunk's entities are out. */
    public static final int FULL = 33;
    /** The level a chunk with no holder is read as. */
    public static final int NO_HOLDER = 46;
    /** Every this many ticks an area's levels are read, the area in the slot of its declaration order. */
    public static final int POLL_PERIOD = 20;

    /** Where the levels and the entities' presence come from: the game, or a test's fake. */
    public interface Source {
        /** The ticket level of a chunk, or {@link #NO_HOLDER} (every level above the maximum reads so). */
        int level(Object dimension, long chunk);

        /** Whether the chunk's entities are loaded and visible to the level's queries. */
        boolean entitiesIn(Object dimension, long chunk);
    }

    /** What happens to an area. */
    public interface Listener {
        /** The area came in. Returns the epoch minted for this stay in (never 0); the area is in from now. */
        long in(Area area, long now);

        /** The area went out; its epoch ends. */
        void out(Area area, long now);
    }

    /** A footprint and where it stands. */
    public static final class Area {
        public final long key;
        public final Object dimension;
        final long[] chunks;
        final int[] levels;
        int pollSlot;
        long epoch;
        boolean waiting;
        boolean recount;
        long outAt = Long.MIN_VALUE;
        /** What the declaring plugin keeps with the area. */
        public Object tag;

        Area(long key, Object dimension, long[] chunks) {
            this.key = key;
            this.dimension = dimension;
            this.chunks = chunks;
            this.levels = new int[chunks.length];
        }

        /** The epoch of the stay in; 0 while out. */
        public long epoch() {
            return epoch;
        }

        public boolean in() {
            return epoch != 0;
        }

        /** The footprint's chunks (packed as {@link #pack}). */
        public long[] chunks() {
            return chunks.clone();
        }
    }

    private final Source source;
    private final Listener listener;
    private final long gap;
    private final java.util.Map<Long, Area> areas = new java.util.LinkedHashMap<>();
    private final List<List<Area>> pollSlots = new ArrayList<>(POLL_PERIOD);
    private final java.util.Map<Object, HashMap<Long, List<Area>>> byChunk = new java.util.HashMap<>();
    private final Set<Area> attention = new LinkedHashSet<>();
    private int declared;
    /** Polls made, and levels they found moved that the event had not told (tests and logs). */
    public long polls, pollChanges;

    public ChunkWatch(Source source, Listener listener, long gapTicks) {
        this.source = source;
        this.listener = listener;
        this.gap = gapTicks;
        for (int i = 0; i < POLL_PERIOD; i++)
            pollSlots.add(new ArrayList<>(2));
    }

    /** A chunk's packed coordinates, as vanilla's {@code ChunkPos.asLong}. */
    public static long pack(int cx, int cz) {
        return (long) cx & 0xFFFFFFFFL | ((long) cz & 0xFFFFFFFFL) << 32;
    }

    /** The chunks the disc of {@code radius} blocks round (x, z) touches. */
    public static long[] footprint(double x, double z, double radius) {
        int x0 = (int) Math.floor((x - radius) / 16.0), x1 = (int) Math.floor((x + radius) / 16.0);
        int z0 = (int) Math.floor((z - radius) / 16.0), z1 = (int) Math.floor((z + radius) / 16.0);
        List<Long> out = new ArrayList<>();
        for (int cx = x0; cx <= x1; cx++) {
            for (int cz = z0; cz <= z1; cz++) {
                // The nearest point of the chunk's square to the centre.
                double nx = Math.max(cx * 16.0, Math.min(x, cx * 16.0 + 16.0));
                double nz = Math.max(cz * 16.0, Math.min(z, cz * 16.0 + 16.0));
                if ((nx - x) * (nx - x) + (nz - z) * (nz - z) <= radius * radius)
                    out.add(pack(cx, cz));
            }
        }
        long[] a = new long[out.size()];
        for (int i = 0; i < a.length; i++)
            a[i] = out.get(i);
        return a;
    }

    // ---- areas ----------------------------------------------------------------------------------

    /**
     * Declares an area (idempotent by key): its footprint's levels are read once now and followed from then on.
     * The area starts out and is looked at at the end of the tick.
     */
    public Area declare(long key, Object dimension, double x, double z, double radius) {
        Area a = areas.get(key);
        if (a != null)
            return a;
        a = new Area(key, dimension, footprint(x, z, radius));
        areas.put(key, a);
        HashMap<Long, List<Area>> m = byChunk.computeIfAbsent(dimension, k -> new HashMap<>());
        for (int i = 0; i < a.chunks.length; i++) {
            a.levels[i] = source.level(dimension, a.chunks[i]);
            final Area area = a;
            m.computeIfAbsent(a.chunks[i], k -> new ArrayList<>(1)).add(area);
        }
        a.pollSlot = declared++ % POLL_PERIOD;
        pollSlots.get(a.pollSlot).add(a);
        attention.add(a);
        return a;
    }

    public Area get(long key) {
        return areas.get(key);
    }

    public int size() {
        return areas.size();
    }

    /** Forgets an area; one that is in goes out first. */
    public void remove(long key, long now) {
        Area a = areas.remove(key);
        if (a == null)
            return;
        if (a.in())
            out(a, now);
        pollSlots.get(a.pollSlot).remove(a);
        attention.remove(a);
        HashMap<Long, List<Area>> m = byChunk.get(a.dimension);
        if (m != null) {
            for (long c : a.chunks) {
                List<Area> l = m.get(c);
                if (l != null) {
                    l.remove(a);
                    if (l.isEmpty())
                        m.remove(c);
                }
            }
        }
    }

    /** A session started: every area is out with no listener call (the engine voids its lines at its start). */
    public void reset() {
        for (Area a : areas.values()) {
            a.epoch = 0;
            a.waiting = false;
            a.recount = false;
            a.outAt = Long.MIN_VALUE;
            attention.add(a);
        }
    }

    /** An area that is in asks for a new census at the end of the tick (a body read from disk outside it). */
    public void recount(Area a) {
        if (a.in()) {
            a.recount = true;
            attention.add(a);
        }
    }

    /** ChunkTicketLevelUpdatedEvent: a footprint chunk's level moved; an early hint only. */
    public void onTicket(Object dimension, long chunk, int newLevel) {
        newLevel = Math.min(newLevel, NO_HOLDER);
        HashMap<Long, List<Area>> m = byChunk.get(dimension);
        List<Area> l = m == null ? null : m.get(chunk);
        if (l == null)
            return;
        for (Area a : l) {
            for (int i = 0; i < a.chunks.length; i++)
                if (a.chunks[i] == chunk)
                    a.levels[i] = newLevel;
            attention.add(a);
        }
    }

    // ---- the tick -------------------------------------------------------------------------------

    /** End of tick: this tick's slot of areas is polled, then every area whose chunks moved is looked at. */
    public void tick(long now) {
        List<Area> slot = pollSlots.get((int) Math.floorMod(now, (long) POLL_PERIOD));
        for (Area a : slot) {
            polls++;
            if (refresh(a))
                attention.add(a);
        }
        if (attention.isEmpty())
            return;
        List<Area> again = new ArrayList<>();
        Iterator<Area> it = List.copyOf(attention).iterator();
        attention.clear();
        while (it.hasNext()) {
            Area a = it.next();
            if (areas.get(a.key) == a && look(a, now))
                again.add(a);
        }
        attention.addAll(again);
    }

    private boolean refresh(Area a) {
        boolean moved = false;
        for (int i = 0; i < a.chunks.length; i++) {
            int l = source.level(a.dimension, a.chunks[i]);
            if (l != a.levels[i]) {
                a.levels[i] = l;
                moved = true;
                pollChanges++;
            }
        }
        return moved;
    }

    /** One area: out, waiting, or in. True when it is to be looked at again next tick. */
    private boolean look(Area a, long now) {
        for (int l : a.levels) {
            if (l > FULL) {
                if (a.in())
                    out(a, now);
                a.waiting = false;
                a.recount = false;
                return false;
            }
        }
        for (long ch : a.chunks) {
            if (!source.entitiesIn(a.dimension, ch)) {
                if (a.in())
                    out(a, now);
                a.waiting = true;
                return true;
            }
        }
        if (!a.in() && a.outAt != Long.MIN_VALUE && now - a.outAt < gap) {
            a.waiting = true;
            return true;
        }
        a.waiting = false;
        if (!a.in() || a.recount) {
            a.recount = false;
            long e = listener.in(a, now);
            a.epoch = e == 0 ? 1 : e;
        }
        return false;
    }

    private void out(Area a, long now) {
        listener.out(a, now);
        a.epoch = 0;
        a.outAt = now;
    }

    /** True while every chunk of an area is FULL with its entities in, read now (a line played at once). */
    public boolean playable(Area a) {
        if (refresh(a))
            attention.add(a);
        for (int l : a.levels)
            if (l > FULL)
                return false;
        for (long ch : a.chunks)
            if (!source.entitiesIn(a.dimension, ch))
                return false;
        return true;
    }
}
