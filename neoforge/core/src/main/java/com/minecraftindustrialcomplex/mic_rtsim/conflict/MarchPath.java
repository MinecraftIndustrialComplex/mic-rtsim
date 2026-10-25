package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin.EngineNames;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology;

import java.nio.ByteBuffer;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * A party's march as the engine last said it (BAND_MARCH, docs/boundary.md §9.3, MZB-468): who marches, in which
 * phase, how many units of each body, and the route as waypoints with the game tick the band reaches each. It is
 * immutable; a newer record replaces it whole, and nothing in it is saved (UR-1).
 *
 * <p>{@link #chartAt} is the one position rule the engine's {@code BandMarch::chart_pos} shares (MZB-454); the
 * result is in the route's chart, and {@link #at} folds it into the level's canonical frame the way the engine's
 * {@code party_pos} does, from the block centre (MZB-477).
 */
public final class MarchPath {
    private final long agent;
    private final int revision;
    private final String dimension;
    private final long faction;
    private final String culture;
    private final int kind;
    private final int phase;
    private final int flags;
    private final long partner;
    private final long musterEnd;
    private final Map<String, Integer> units;
    private final int[] xs;
    private final int[] zs;
    private final long[] ticks;

    MarchPath(long agent, int revision, String dimension, long faction, String culture, int kind, int phase, int flags,
            long partner, long musterEnd, Map<String, Integer> units, int[] xs, int[] zs, long[] ticks) {
        this.agent = agent;
        this.revision = revision;
        this.dimension = dimension;
        this.faction = faction;
        this.culture = culture;
        this.kind = kind;
        this.phase = phase;
        this.flags = flags;
        this.partner = partner;
        this.musterEnd = musterEnd;
        this.units = Collections.unmodifiableMap(units);
        this.xs = xs;
        this.zs = zs;
        this.ticks = ticks;
    }

    /**
     * Reads a BAND_MARCH record from {@code at}. Null when it is malformed: shorter than its own counts say, or
     * with a phase outside 1 to 5. A unit entry of a body the dictionary does not name is left out (no held body
     * can carry that name), and so is a dimension or culture it does not name (null in the result).
     */
    static MarchPath decode(ByteBuffer b, int at, int size, int waypoints, EngineNames names) {
        if (size < ConflictRecords.March.UNIT_LIST)
            return null;
        long k = Integer.toUnsignedLong(b.getInt(at + ConflictRecords.March.UNITS));
        long w = Integer.toUnsignedLong(waypoints);
        long need = ConflictRecords.March.UNIT_LIST + (long) ConflictRecords.March.UNIT * k
                + (long) ConflictRecords.March.WAYPOINT * w;
        if (need > size)
            return null;
        int phase = b.get(at + ConflictRecords.March.PHASE) & 0xFF;
        if (phase < ConflictRecords.March.MUSTERING || phase > ConflictRecords.March.HOME)
            return null;
        Map<String, Integer> units = new LinkedHashMap<>();
        int u = at + ConflictRecords.March.UNIT_LIST;
        for (int i = 0; i < (int) k; i++, u += ConflictRecords.March.UNIT) {
            String body = names.name(Obs.DICT_BODY, b.getInt(u + ConflictRecords.March.U_BODY));
            if (body != null)
                units.merge(body, b.getInt(u + ConflictRecords.March.U_COUNT), Integer::sum);
        }
        int n = (int) w;
        int[] xs = new int[n];
        int[] zs = new int[n];
        long[] ticks = new long[n];
        int p = u;
        for (int i = 0; i < n; i++, p += ConflictRecords.March.WAYPOINT) {
            xs[i] = b.getInt(p + ConflictRecords.March.W_X);
            zs[i] = b.getInt(p + ConflictRecords.March.W_Z);
            ticks[i] = b.getLong(p + ConflictRecords.March.W_TICK);
        }
        int culture = b.getInt(at + ConflictRecords.March.CULTURE);
        return new MarchPath(b.getLong(at + ConflictRecords.March.AGENT), b.getInt(at + ConflictRecords.March.REVISION),
                names.name(Obs.DICT_DIMENSION, b.getInt(at + ConflictRecords.March.DIM)),
                b.getLong(at + ConflictRecords.March.FACTION), culture == 0 ? null : names.name(Obs.DICT_CULTURE, culture),
                b.get(at + ConflictRecords.March.KIND) & 0xFF, phase, b.get(at + ConflictRecords.March.FLAGS) & 0xFF,
                b.getLong(at + ConflictRecords.March.PARTNER), b.getLong(at + ConflictRecords.March.MUSTER_END), units, xs,
                zs, ticks);
    }

    public long agent() {
        return agent;
    }

    /** The record's per-session revision (an unsigned 32-bit count; compare with {@link #newerThan}). */
    public int revision() {
        return revision;
    }

    /** True when this record is a later one than {@code other}'s (unsigned order). */
    public boolean newerThan(MarchPath other) {
        return Integer.compareUnsigned(revision, other.revision) > 0;
    }

    /** The dimension's name (dictionary 1), or null when the dictionary did not name it. */
    public String dimension() {
        return dimension;
    }

    public long faction() {
        return faction;
    }

    /** The culture's name (dictionary 5), or null for none. */
    public String culture() {
        return culture;
    }

    /** BAND_MATERIALIZE's purposes, 1 envoy to 6 plunder. */
    public int kind() {
        return kind;
    }

    /** {@link ConflictRecords.March#MUSTERING} to {@link ConflictRecords.March#HOME}. */
    public int phase() {
        return phase;
    }

    public int flags() {
        return flags;
    }

    /** The partner contingent's agent for a joint attack; 0 in this build. */
    public long partner() {
        return partner;
    }

    public long musterEnd() {
        return musterEnd;
    }

    /** Units by body name; a body the record does not list counts zero. */
    public Map<String, Integer> units() {
        return units;
    }

    public int count(String body) {
        return units.getOrDefault(body, 0);
    }

    /** The war chief marches with the band. */
    public boolean chief() {
        return (flags & ConflictRecords.March.FLAG_CHIEF) != 0;
    }

    /** The march is over: the agent is dropped and later records of it are ignored. */
    public boolean ended() {
        return (flags & ConflictRecords.March.FLAG_ENDED) != 0;
    }

    public boolean joint() {
        return (flags & ConflictRecords.March.FLAG_JOINT) != 0;
    }

    /** The band waits or fights where it stands (phases 3 and 4): the stop of MZB-472. */
    public boolean stopped() {
        return phase == ConflictRecords.March.WAITING || phase == ConflictRecords.March.FIGHTING;
    }

    public int waypoints() {
        return xs.length;
    }

    public int waypointX(int i) {
        return xs[i];
    }

    public int waypointZ(int i) {
        return zs[i];
    }

    public long waypointTick(int i) {
        return ticks[i];
    }

    /**
     * Where the band is at game tick {@code t}, in the route's chart (MZB-454), or null with no waypoint. The
     * first waypoint while mustering; the last while waiting or fighting. Out and home: the segment {@code s} is
     * the largest index at most {@code w - 2} whose tick is at most {@code t} (0 when none); the position is that
     * segment's start when {@code t1 <= t0} or {@code t <= t0}, its end when {@code t >= t1}, else each axis
     * {@code (int) (a + (b - a) * fr)}, which truncates toward zero as the engine's {@code as i32} does.
     */
    public int[] chartAt(long t) {
        int w = xs.length;
        if (w == 0)
            return null;
        switch (phase) {
            case ConflictRecords.March.MUSTERING:
                return new int[] {xs[0], zs[0]};
            case ConflictRecords.March.WAITING:
            case ConflictRecords.March.FIGHTING:
                return new int[] {xs[w - 1], zs[w - 1]};
            default:
                break;
        }
        if (w < 2)
            return new int[] {xs[0], zs[0]};
        int s = 0;
        for (int i = w - 2; i >= 0; i--) {
            if (ticks[i] <= t) {
                s = i;
                break;
            }
        }
        long t0 = ticks[s];
        long t1 = ticks[s + 1];
        if (t1 <= t0 || t <= t0)
            return new int[] {xs[s], zs[s]};
        if (t >= t1)
            return new int[] {xs[s + 1], zs[s + 1]};
        double fr = (double) (t - t0) / (double) (t1 - t0);
        return new int[] {(int) ((double) xs[s] + ((double) xs[s + 1] - (double) xs[s]) * fr),
                (int) ((double) zs[s] + ((double) zs[s + 1] - (double) zs[s]) * fr)};
    }

    /**
     * Where the band is at game tick {@code t} in the level's own frame, or null with no waypoint: the chart
     * position folded from the block centre (the engine folds the cell the block is in, MZB-477) and floored to a
     * block.
     */
    public int[] at(long t, LevelTopology topo) {
        int[] c = chartAt(t);
        if (c == null)
            return null;
        LevelTopology.Folded f = topo.fold(c[0] + 0.5, c[1] + 0.5);
        return new int[] {(int) Math.floor(f.x()), (int) Math.floor(f.z())};
    }
}
