package com.minecraftindustrialcomplex.mic_rtsim.conflict;

/**
 * The rules of a body that travels aloft with its party (M11, MZB-806 to MZB-814): the Umvuthi's puppet floats over
 * the ground at the column's pace and sets down where the fight is. Pure (no Minecraft state), so core's JUnit tests
 * hold every threshold; the Mowzie plugin's glue ({@code UmvuthiAloft}) reads the world and calls these.
 *
 * <p>Every number is a PLACEHOLDER (MZB-841), named once here; test 31's client run (MZB-836) is where they are
 * judged. Every "within N blocks" is horizontal, as sight is ({@link MarchTrim#withinSight}, MZB-753, MZB-831): a
 * player high above a band sees it as one at its own height would.
 */
public final class AloftRules {
    private AloftRules() {}

    // ---- height ---------------------------------------------------------------------------------

    /** Blocks between the highest top and the foot (MZB-808). */
    public static final int CLEARANCE = 2;
    /** How far ahead of its box (blocks) the height rule reads columns. */
    public static final int AHEAD = 8;
    /** A column whose chunk is not at hand: skipped, never loaded. */
    public static final int UNREAD = Integer.MIN_VALUE;
    /** Blocks up a tick, blocks down a tick, blocks across a tick (MZB-808, MZB-809). */
    public static final double RISE = 0.1, SINK = 0.05, PACE = 0.1;
    /** It takes no step across while its target is more than this far above its foot. */
    public static final double LOW = 1.0;
    /** Ticks between height looks and trail puffs. */
    public static final int LOOK = 10;

    // ---- stuck ----------------------------------------------------------------------------------

    /** Ticks between headway checks, the headway that counts (blocks), and the distance within which it is never stuck. */
    public static final int STUCK_PERIOD = 100;
    public static final double HEADWAY = 0.5, STUCK_FAR = 8.0;
    /** Blocks the target rises for each period without headway, and the most it rises. */
    public static final int RAISE = 8, RAISE_MAX = 24;
    /** The most ticks it is held in a player's sight before it is moved. */
    public static final int HOLD_MAX = 1200;
    /** The players whose sight holds it, and the trail's audience (blocks, horizontal). */
    public static final double SIGHT = 160.0;

    // ---- settle and rise ------------------------------------------------------------------------

    /** With the attack flag it settles within this many blocks of the target (MZB-812). */
    public static final double SETTLE = 32.0;
    /** Over a fluid, a settling body hovers within this many blocks of its target. */
    public static final double HOVER_NEAR = 8.0;
    /** Ticks with no flag and no player target before it rises again. */
    public static final int RISE_AFTER = 200;

    // ---- the re-pair ----------------------------------------------------------------------------

    /** Degrees between its way and its facing past which it is turned for the clients (MZB-813). */
    public static final float TURN = 55f;
    /** Ticks between re-pairs, and ticks it holds still before one. */
    public static final int REPAIR_COOLDOWN = 200, REPAIR_HOLD = 2;

    private static final long NEVER = Long.MIN_VALUE / 4;

    // ---- facing ---------------------------------------------------------------------------------

    /**
     * The jar's {@code direction} (1 to 4) for a yaw: inverts {@code EntityUmvuthi.tick}'s
     * {@code setYRot((direction - 1) * 90)} (MZB-806). A yaw halfway between two quarters takes the later one. Never
     * 0: that is the jar's "unset" (its constructor rolls {@code nextInt(4) + 1}) and {@code checkBlocksByFeet}
     * matches no branch for it (MZB-870).
     */
    public static int quarter(float yaw) {
        return Math.floorMod(Math.round(yaw / 90f), 4) + 1;
    }

    /** The yaw the jar turns a {@code direction} to. */
    public static float quarterYaw(int quarter) {
        return (quarter - 1) * 90f;
    }

    /** The yaw (Minecraft's: 0 faces +z, -90 faces +x) of the way (dx, dz). */
    public static float wayYaw(double dx, double dz) {
        return (float) Math.toDegrees(Math.atan2(-dx, dz));
    }

    /** Degrees, 0 to 180, between two yaws. */
    public static float turn(float a, float b) {
        float d = (a - b) % 360f;
        if (d >= 180f)
            d -= 360f;
        else if (d < -180f)
            d += 360f;
        return Math.abs(d);
    }

    // ---- height and steps -----------------------------------------------------------------------

    /**
     * The foot's target height: the highest of the first {@code count} column tops (each a
     * {@code ChunkAccess.getHeight}, the top block's y, so one less than {@code Level.getHeight}) plus 1 plus
     * {@link #CLEARANCE}; an {@link #UNREAD} column is skipped. With none read it stays {@code previous} (MZB-808).
     */
    public static double foot(int[] tops, int count, double previous) {
        int best = UNREAD;
        for (int i = 0; i < count; i++)
            if (tops[i] > best)
                best = tops[i];
        return best == UNREAD ? previous : best + 1 + CLEARANCE;
    }

    /** The vertical step toward {@code target}: up at most {@link #RISE}, down at most {@link #SINK} (signed). */
    public static double verticalStep(double y, double target) {
        double d = target - y;
        return d > 0 ? Math.min(d, RISE) : Math.max(d, -SINK);
    }

    /** Whether it may step across: not while its target is more than {@link #LOW} above its foot. */
    public static boolean mayStepAcross(double y, double target) {
        return target - y <= LOW;
    }

    /** The length of the step across toward a place {@code distance} blocks off. */
    public static double across(double distance) {
        return Math.min(Math.max(distance, 0.0), PACE);
    }

    /** Whether b is within {@code range} blocks of a, horizontally: the height is no part of the distance. */
    public static boolean near(double ax, double ay, double az, double bx, double by, double bz, double range) {
        return MarchTrim.withinSight(bx - ax, bz - az, range);
    }

    // ---- settle and rise ------------------------------------------------------------------------

    /** How a settling body lands. */
    public enum Landing {
        /** Not settling, or over a fluid and far from the target: it stays up and goes on toward its place. */
        AIR,
        /** It sinks onto the ground. */
        SINK,
        /** Over a fluid within {@link #HOVER_NEAR} of its target: it stays where it floats. */
        HOVER
    }

    /**
     * Whether it comes down (MZB-812): the attack flag and within {@link #SETTLE} blocks of the target ((dx, dz) from
     * the body), or a player target it has in sight.
     */
    public static boolean settles(boolean attack, double dx, double dz, boolean seesPlayer) {
        return seesPlayer || (attack && MarchTrim.withinSight(dx, dz, SETTLE));
    }

    /** What a body that {@code settles} does over what it stands on (a fluid is not ground). */
    public static Landing landing(boolean settles, boolean overFluid, double dx, double dz) {
        if (!settles)
            return Landing.AIR;
        if (!overFluid)
            return Landing.SINK;
        return MarchTrim.withinSight(dx, dz, HOVER_NEAR) ? Landing.HOVER : Landing.AIR;
    }

    /**
     * Whether a settled body rises again: no attack flag, no player target now, and none for {@link #RISE_AFTER}
     * ticks ({@code lastPlayerTarget} is the last tick it had one).
     */
    public static boolean rises(boolean attack, boolean playerTarget, long now, long lastPlayerTarget) {
        return !attack && !playerTarget && now - lastPlayerTarget >= RISE_AFTER;
    }

    // ---- stuck ----------------------------------------------------------------------------------

    /** What a stuck check found. */
    public enum Verdict {
        NONE,
        /** No headway for a period: the target height rose another {@link #RAISE}. */
        RAISED,
        /** No headway for a period after the most it rises: it is held, to be moved. */
        HELD
    }

    /**
     * The stuck machine (MZB-811), one per body. No headway is under {@link #HEADWAY} blocks nearer its place in
     * {@link #STUCK_PERIOD} ticks while more than {@link #STUCK_FAR} off. Each such period raises the target height
     * by {@link #RAISE}, to {@link #RAISE_MAX}; one more is the hold. A held body is moved to its place the first
     * check no player has a line to it, or after {@link #HOLD_MAX} ticks held. Headway ends it all.
     */
    public static final class Stuck {
        private int raised;
        private boolean started;
        private long windowAt, heldAt = NEVER, lookedAt = NEVER;
        private double windowFrom;

        /** Called each tick (a compare unless a period ends) with the horizontal distance to its place. */
        public Verdict tick(long now, double distance) {
            if (!started) {
                begin(now, distance);
                return Verdict.NONE;
            }
            if (now - windowAt < STUCK_PERIOD)
                return Verdict.NONE;
            double from = windowFrom;
            windowAt = now;
            windowFrom = distance;
            if (distance <= STUCK_FAR || from - distance >= HEADWAY) {
                raised = 0;
                heldAt = NEVER;
                lookedAt = NEVER;
                return Verdict.NONE;
            }
            if (raised < RAISE_MAX) {
                raised += RAISE;
                return Verdict.RAISED;
            }
            if (heldAt == NEVER)
                heldAt = now;
            return Verdict.HELD;
        }

        private void begin(long now, double distance) {
            started = true;
            raised = 0;
            windowAt = now;
            windowFrom = distance;
            heldAt = NEVER;
            lookedAt = NEVER;
        }

        /** Blocks the target height is raised now. */
        public int raised() {
            return raised;
        }

        public boolean holding() {
            return heldAt != NEVER;
        }

        /** Whether a held body is to be moved now: no player has a line to it ({@code !seen}), or it has been held long enough. */
        public boolean moveDue(long now, boolean seen) {
            return heldAt != NEVER && (!seen || now - heldAt >= HOLD_MAX);
        }

        /** Whether a held body looks for a player's line now (every {@link #LOOK} ticks). */
        public boolean lookDue(long now) {
            return heldAt != NEVER && now - lookedAt >= LOOK;
        }

        public void looked(long now) {
            lookedAt = now;
        }

        /** It was moved to its place: everything starts over there. */
        public void moved(long now, double distance) {
            begin(now, distance);
        }
    }

    // ---- the re-pair gate -----------------------------------------------------------------------

    /** What the re-pair gate says this tick. */
    public enum Turn {
        /** Nothing to do (or not allowed now). */
        NO,
        /** The body holds still this tick (its tracker's base must catch up). */
        HOLD,
        /** Turn it and re-pair now. */
        GO
    }

    /**
     * The re-pair gate (MZB-813), one per body: its way more than {@link #TURN} degrees from its facing, no ability
     * active, not dying, and {@link #REPAIR_COOLDOWN} ticks since the last; then {@link #REPAIR_HOLD} ticks held still,
     * then go. Any condition failing during the hold starts it over.
     */
    public static final class Repair {
        private long last = NEVER;
        private int held;

        public Turn gate(long now, float facing, float way, boolean ability, boolean dying) {
            if (dying || ability || now - last < REPAIR_COOLDOWN || AloftRules.turn(facing, way) <= TURN) {
                held = 0;
                return Turn.NO;
            }
            if (held < REPAIR_HOLD) {
                held++;
                return Turn.HOLD;
            }
            held = 0;
            last = now;
            return Turn.GO;
        }
    }

    // ---- the line to a player -------------------------------------------------------------------

    /** What is in a cell, as the line sees it. */
    @FunctionalInterface
    public interface Cells {
        /** Nothing that blocks motion. */
        int OPEN = 0;
        /** A block that blocks motion (a glass pane does). */
        int BLOCKS = 1;
        /** The cell's chunk is not at hand: it counts as blocking, and is never loaded. */
        int ABSENT = 2;

        int at(int x, int y, int z);
    }

    /**
     * Whether the line from (ex, ey, ez) to (tx, ty, tz) is clear (MZB-811): an integer DDA over the cells between,
     * the eye's cell and the target's left out. {@code LivingEntity.hasLineOfSight} gives up past 128 blocks, so the
     * stuck rule's 160 has this. It visits at most |dx| + |dy| + |dz| cells (about 400 for 160 blocks across).
     */
    public static boolean clear(double ex, double ey, double ez, double tx, double ty, double tz, Cells cells) {
        int x = (int) Math.floor(ex), y = (int) Math.floor(ey), z = (int) Math.floor(ez);
        int rx = Math.abs((int) Math.floor(tx) - x), ry = Math.abs((int) Math.floor(ty) - y), rz = Math.abs((int) Math.floor(tz) - z);
        int steps = rx + ry + rz;
        double dx = tx - ex, dy = ty - ey, dz = tz - ez;
        int sx = dx > 0 ? 1 : -1, sy = dy > 0 ? 1 : -1, sz = dz > 0 ? 1 : -1;
        double dtx = dx == 0 ? Double.POSITIVE_INFINITY : 1.0 / Math.abs(dx);
        double dty = dy == 0 ? Double.POSITIVE_INFINITY : 1.0 / Math.abs(dy);
        double dtz = dz == 0 ? Double.POSITIVE_INFINITY : 1.0 / Math.abs(dz);
        double mx = dx == 0 ? Double.POSITIVE_INFINITY : (sx > 0 ? x + 1 - ex : ex - x) * dtx;
        double my = dy == 0 ? Double.POSITIVE_INFINITY : (sy > 0 ? y + 1 - ey : ey - y) * dty;
        double mz = dz == 0 ? Double.POSITIVE_INFINITY : (sz > 0 ? z + 1 - ez : ez - z) * dtz;
        for (int i = 0; i < steps; i++) {
            // The axis whose cell edge comes first, among those that still have cells to cross (so rounding at a
            // corner can never walk past the target's cell).
            if (rx > 0 && (ry == 0 || mx <= my) && (rz == 0 || mx <= mz)) {
                x += sx;
                mx += dtx;
                rx--;
            } else if (ry > 0 && (rz == 0 || my <= mz)) {
                y += sy;
                my += dty;
                ry--;
            } else {
                z += sz;
                mz += dtz;
                rz--;
            }
            if (i < steps - 1 && cells.at(x, y, z) != Cells.OPEN)
                return false;
        }
        return true;
    }
}
