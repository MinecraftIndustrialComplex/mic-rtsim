package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Traveller;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin.EntityUmvuthiAccessor;
import net.minecraft.core.BlockPos;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;

/**
 * How the grove's chief travels with its Invasion (M11, MZB-807 to MZB-814): the Umvuthi's puppet floats. The rules
 * (heights, pace, the stuck machine, settling, the re-pair gate) are core's {@link AloftRules}, tested in JUnit; this
 * is the glue that reads the world and moves the jar's body. One instance serves every puppet; what it keeps for one
 * puppet is {@link Aloft}, in the core's {@link Traveller.Memo} on the puppet's slot.
 *
 * <p>The core calls {@link #step} once a tick from the end of its tick, after every level ticked: the move here
 * is the last word on the puppet's motion for the tick (a push that landed before it is wiped, MZB-810).
 * Nothing here loads a chunk: a column is read with {@code getChunkNow}, and one that is not at hand is skipped.
 */
public final class UmvuthiAloft implements Traveller {
    public static final UmvuthiAloft INSTANCE = new UmvuthiAloft();

    /**
     * Columns read for the height: every column under its box (at most 3 by 3 for a box up to 2 wide) at each block of
     * the way, out to {@link AloftRules#AHEAD}.
     */
    private static final int COLUMNS = 9 * (AloftRules.AHEAD + 1);
    /** Within this far (blocks, horizontally) of its home spot it is there. */
    private static final double ARRIVED = 0.25;
    /** A place nearer than this has no way worth turning to. */
    private static final double WAY_MIN = 1.0;
    private static final long NEVER = Long.MIN_VALUE / 4;

    /** D1 diagnostics (MZB-808, MZB-811): columns a height look skipped for want of a chunk, and stuck puppets moved. */
    public long heightColumnsSkipped, stuckMoves;

    private UmvuthiAloft() {}

    /** What one puppet keeps (MZB-807): its height, its stuck machine, its re-pair gate and its settle. */
    private static final class Aloft {
        AloftRules.Stuck stuck = new AloftRules.Stuck();
        final AloftRules.Repair repair = new AloftRules.Repair();
        final int[] tops = new int[COLUMNS];
        /** The foot's height the rule gives, and with the stuck rule's raise. */
        double base, target;
        long lookedAt = NEVER, trailAt = NEVER, playerTargetAt = NEVER;
        /** It stays where it is (down on the ground, or hovering over a fluid), until it rises. */
        boolean settled;
        /** It is down on the ground: gravity is back. */
        boolean grounded;
        /** Ticks it has held still for a turn at home. */
        int homeHeld;
    }

    // ---- the hook -------------------------------------------------------------------------------

    @Override
    public void made(ServerLevel level, Mob body, CampKind.ChiefBody memory, Memo memo) {
        if (!(body instanceof EntityUmvuthi u))
            return;
        of(memo, u);
        if (memory != null)
            GroveKind.remember(u, memory);
        u.setNoGravity(true);
        u.restrictTo(u.blockPosition(), -1);
    }

    @Override
    public double lift(Memo memo) {
        Aloft s = memo == null ? null : memo.get(Aloft.class);
        return s != null && s.grounded ? 0.0 : AloftRules.CLEARANCE;
    }

    @Override
    public void step(ServerLevel level, Mob body, Way way, Memo memo) {
        if (!(body instanceof EntityUmvuthi u) || u.isDeadOrDying())
            return;
        Aloft s = of(memo, u);
        long now = way.now();
        LivingEntity target = u.getTarget();
        boolean player = target instanceof Player;
        if (player)
            s.playerTargetAt = now;
        boolean attack = way.attack() && !way.recalled();
        if (s.settled) {
            if (!AloftRules.rises(attack, player, now, s.playerTargetAt))
                return;
            rise(u, s);
        }
        double tdx = way.targetX() - u.getX(), tdz = way.targetZ() - u.getZ();
        boolean settles = AloftRules.settles(attack, tdx, tdz, player && u.getSensing().hasLineOfSight(target));
        AloftRules.Landing landing = AloftRules.landing(settles, settles && overFluid(level, u), tdx, tdz);
        switch (landing) {
            case SINK -> {
                sink(u, s);
                return;
            }
            case HOVER -> {
                hold(u);
                s.settled = true;
                return;
            }
            case AIR -> {
            }
        }
        double dx = way.placeX() - u.getX(), dz = way.placeZ() - u.getZ();
        march(level, u, s, way.placeX(), way.placeZ(), dx, dz, now, true);
        trail(level, u, s, now);
    }

    @Override
    public boolean home(ServerLevel level, Mob body, Vec3 spot, int direction, Memo memo) {
        if (!(body instanceof EntityUmvuthi u) || u.isDeadOrDying())
            return false;
        Aloft s = of(memo, u);
        long now = level.getGameTime();
        if (s.settled)
            rise(u, s);
        double dx = spot.x - u.getX(), dz = spot.z - u.getZ();
        if (Math.sqrt(dx * dx + dz * dz) > ARRIVED) {
            s.homeHeld = 0;
            march(level, u, s, spot.x, spot.z, dx, dz, now, false);
            trail(level, u, s, now);
            return false;
        }
        EntityUmvuthiAccessor facing = (EntityUmvuthiAccessor) u;
        if (facing.mic_rtsim_mowziesmobs$direction() != direction) {
            // Turn it where it floats: hold still for the tracker's base to catch up, then turn and send it again.
            if (u.getActiveAbility() != null) {
                s.homeHeld = 0;
                return false;
            }
            if (s.homeHeld < AloftRules.REPAIR_HOLD) {
                s.homeHeld++;
                hold(u);
                return false;
            }
            s.homeHeld = 0;
            UmvuthiMaker.face(u, direction);
            RtsimApi.rePair(u);
        }
        double down = Math.max(spot.y - u.getY(), -AloftRules.SINK);
        u.setDeltaMovement(Vec3.ZERO);
        u.resetFallDistance();
        u.move(MoverType.SELF, new Vec3(0.0, down, 0.0));
        u.setDeltaMovement(Vec3.ZERO);
        u.restrictTo(u.blockPosition(), -1);
        if (u.getY() - spot.y <= 1.0e-3 || u.onGround() || u.verticalCollisionBelow) {
            land(u, s);
            return true;
        }
        return false;
    }

    @Override
    public void release(Mob body, Memo memo) {
        body.setNoGravity(false);
        memo.set(null);
    }

    @Override
    public void died(Mob body, Memo memo) {
        body.setNoGravity(false);
    }

    // ---- test reads (the memo's, MZB-811) -------------------------------------------------------

    /** Blocks the stuck rule has raised this puppet's target height. */
    public int raised(Memo memo) {
        Aloft s = memo.get(Aloft.class);
        return s == null ? 0 : s.stuck.raised();
    }

    /** The foot height it is going to (with the stuck rule's raise); NaN with none yet. */
    public double target(Memo memo) {
        Aloft s = memo.get(Aloft.class);
        return s == null ? Double.NaN : s.target;
    }

    /** Whether it has settled (on the ground, or hovering over a fluid). */
    public boolean settled(Memo memo) {
        Aloft s = memo.get(Aloft.class);
        return s != null && s.settled;
    }

    // ---- the march ------------------------------------------------------------------------------

    private static Aloft of(Memo memo, EntityUmvuthi u) {
        Aloft s = memo.get(Aloft.class);
        if (s == null) {
            s = new Aloft();
            s.base = s.target = u.getY();
            memo.set(s);
        }
        return s;
    }

    /**
     * One tick of floating toward (placeX, placeZ), {@code (dx, dz)} away: the height look, the stuck machine, the
     * turn (when {@code turnToWay}: the way is more than 55 degrees off its facing), then the move (MZB-808, MZB-809,
     * MZB-811, MZB-813).
     */
    private void march(ServerLevel level, EntityUmvuthi u, Aloft s, double placeX, double placeZ, double dx, double dz, long now,
                       boolean turnToWay) {
        double dist = Math.sqrt(dx * dx + dz * dz);
        if (now - s.lookedAt >= AloftRules.LOOK)
            look(level, u, s, dx, dz, dist, now);
        if (s.stuck.tick(now, dist) == AloftRules.Verdict.RAISED)
            s.target = s.base + s.stuck.raised();
        if (s.stuck.holding())
            unstick(level, u, s, placeX, placeZ, now);
        dx = placeX - u.getX();
        dz = placeZ - u.getZ();
        dist = Math.sqrt(dx * dx + dz * dz);
        AloftRules.Turn turn = AloftRules.Turn.NO;
        if (turnToWay) {
            boolean busy = dist <= WAY_MIN || u.getActiveAbility() != null;
            turn = s.repair.gate(now, u.getYRot(), AloftRules.wayYaw(dx, dz), busy, false);
        }
        if (turn == AloftRules.Turn.HOLD) {
            hold(u);
            return;
        }
        if (turn == AloftRules.Turn.GO) {
            UmvuthiMaker.face(u, AloftRules.quarter(AloftRules.wayYaw(dx, dz)));
            RtsimApi.rePair(u);
        }
        double y = u.getY();
        double vy = AloftRules.verticalStep(y, s.target);
        double hx = 0.0, hz = 0.0;
        if (dist > 1.0e-6 && AloftRules.mayStepAcross(y, s.target)) {
            double a = AloftRules.across(dist) / dist;
            hx = dx * a;
            hz = dz * a;
        }
        u.setDeltaMovement(Vec3.ZERO);
        u.resetFallDistance();
        u.move(MoverType.SELF, new Vec3(hx, vy, hz));
        u.setDeltaMovement(Vec3.ZERO);
        u.restrictTo(u.blockPosition(), -1);
    }

    /** Holds the body still this tick: no step, no push, no fall. */
    private static void hold(EntityUmvuthi u) {
        u.setDeltaMovement(Vec3.ZERO);
        u.resetFallDistance();
        u.restrictTo(u.blockPosition(), -1);
    }

    /** The height rule (MZB-808): the foot's target from the columns under its box and ahead on its way. */
    private void look(ServerLevel level, EntityUmvuthi u, Aloft s, double dx, double dz, double dist, long now) {
        s.lookedAt = now;
        double ux = dist > 1.0e-6 ? dx / dist : 0.0, uz = dist > 1.0e-6 ? dz / dist : 0.0;
        int n = read(level, u.getX(), u.getZ(), u.getBbWidth() * 0.5, ux, uz, AloftRules.AHEAD, s.tops);
        double foot = AloftRules.foot(s.tops, n, Double.NaN);
        if (!Double.isNaN(foot)) {
            s.base = foot;
            s.target = foot + s.stuck.raised();
        }
    }

    /**
     * Reads the tops of the columns under a box of half-width {@code half} at (x, z) and, along the unit way, out to
     * {@code ahead} blocks (the box's columns at each block). A column whose chunk is not at hand is
     * {@link AloftRules#UNREAD} and counted, never loaded. Returns how many entries are filled.
     */
    private int read(ServerLevel level, double x, double z, double half, double ux, double uz, int ahead, int[] tops) {
        int n = 0;
        for (int k = 0; k <= ahead; k++) {
            double cx = x + ux * k, cz = z + uz * k;
            int x0 = (int) Math.floor(cx - half), x1 = (int) Math.floor(cx + half);
            int z0 = (int) Math.floor(cz - half), z1 = (int) Math.floor(cz + half);
            for (int bx = x0; bx <= x1; bx++) {
                for (int bz = z0; bz <= z1; bz++) {
                    int top = top(level, bx, bz);
                    if (top == AloftRules.UNREAD)
                        heightColumnsSkipped++;
                    tops[n++] = top;
                }
            }
        }
        return n;
    }

    /** The top block's y of a column ({@code ChunkAccess.getHeight}, MOTION_BLOCKING), or UNREAD with no chunk at hand. */
    private static int top(ServerLevel level, int bx, int bz) {
        LevelChunk chunk = level.getChunkSource().getChunkNow(bx >> 4, bz >> 4);
        if (chunk == null)
            return AloftRules.UNREAD;
        return chunk.getHeight(Heightmap.Types.MOTION_BLOCKING, bx & 15, bz & 15);
    }

    /** Whether the top block under its box holds a fluid (a fluid is not ground, MZB-812). */
    private static boolean overFluid(ServerLevel level, EntityUmvuthi u) {
        double half = u.getBbWidth() * 0.5;
        int x0 = (int) Math.floor(u.getX() - half), x1 = (int) Math.floor(u.getX() + half);
        int z0 = (int) Math.floor(u.getZ() - half), z1 = (int) Math.floor(u.getZ() + half);
        for (int bx = x0; bx <= x1; bx++) {
            for (int bz = z0; bz <= z1; bz++) {
                LevelChunk chunk = level.getChunkSource().getChunkNow(bx >> 4, bz >> 4);
                if (chunk == null)
                    continue;
                int top = chunk.getHeight(Heightmap.Types.MOTION_BLOCKING, bx & 15, bz & 15);
                if (!chunk.getBlockState(new BlockPos(bx, top, bz)).getFluidState().isEmpty())
                    return true;
            }
        }
        return false;
    }

    // ---- stuck ----------------------------------------------------------------------------------

    /**
     * A held puppet (MZB-811): every {@code LOOK} ticks it looks whether any player has a line to it; the first look
     * with none, or after {@code HOLD_MAX} ticks held, it is moved to its place at its target height there. A place
     * whose columns are not at hand waits for the next look.
     */
    private void unstick(ServerLevel level, EntityUmvuthi u, Aloft s, double placeX, double placeZ, long now) {
        if (!s.stuck.lookDue(now))
            return;
        s.stuck.looked(now);
        if (!s.stuck.moveDue(now, seen(level, u)))
            return;
        int n = read(level, placeX, placeZ, u.getBbWidth() * 0.5, 0.0, 0.0, 0, s.tops);
        double foot = AloftRules.foot(s.tops, n, Double.NaN);
        if (Double.isNaN(foot))
            return;
        u.moveTo(placeX, foot, placeZ, u.getYRot(), u.getXRot());
        u.setDeltaMovement(Vec3.ZERO);
        u.resetFallDistance();
        s.base = s.target = foot;
        s.lookedAt = now;
        s.stuck.moved(now, 0.0);
        stuckMoves++;
    }

    /** Whether any player within 160 blocks (horizontally) has a clear line to the body's middle (the plugin's own clip). */
    private static boolean seen(ServerLevel level, EntityUmvuthi u) {
        double bx = u.getX(), by = u.getY() + u.getBbHeight() * 0.5, bz = u.getZ();
        for (ServerPlayer p : level.players()) {
            if (!AloftRules.near(bx, by, bz, p.getX(), p.getY(), p.getZ(), AloftRules.SIGHT))
                continue;
            Vec3 eye = p.getEyePosition();
            if (AloftRules.clear(eye.x, eye.y, eye.z, bx, by, bz, (x, y, z) -> cell(level, x, y, z)))
                return true;
        }
        return false;
    }

    private static int cell(ServerLevel level, int x, int y, int z) {
        LevelChunk chunk = level.getChunkSource().getChunkNow(x >> 4, z >> 4);
        if (chunk == null)
            return AloftRules.Cells.ABSENT;
        BlockPos pos = new BlockPos(x, y, z);
        BlockState state = chunk.getBlockState(pos);
        return state.getCollisionShape(chunk, pos).isEmpty() ? AloftRules.Cells.OPEN : AloftRules.Cells.BLOCKS;
    }

    // ---- settle and rise ------------------------------------------------------------------------

    /** Sinks at {@code SINK} with collision, and settles once it is down (MZB-812). */
    private static void sink(EntityUmvuthi u, Aloft s) {
        u.setDeltaMovement(Vec3.ZERO);
        u.resetFallDistance();
        u.move(MoverType.SELF, new Vec3(0.0, -AloftRules.SINK, 0.0));
        u.setDeltaMovement(Vec3.ZERO);
        u.restrictTo(u.blockPosition(), -1);
        if (u.onGround() || u.verticalCollisionBelow)
            land(u, s);
    }

    /** Down: gravity is the jar's again, and it is restricted where it stands (radius -1: no pull anywhere). */
    private static void land(EntityUmvuthi u, Aloft s) {
        u.setNoGravity(false);
        u.restrictTo(u.blockPosition(), -1);
        s.settled = true;
        s.grounded = true;
    }

    /** Floats again: the height rule takes it, with the stuck machine started over. */
    private static void rise(EntityUmvuthi u, Aloft s) {
        u.setNoGravity(true);
        s.settled = false;
        s.grounded = false;
        s.lookedAt = NEVER;
        s.stuck = new AloftRules.Stuck();
        s.base = s.target = u.getY();
    }

    // ---- the trail ------------------------------------------------------------------------------

    /** A few small flames under the body every 10 ticks, for each player within 160 blocks (MZB-814; a PLACEHOLDER). */
    private static void trail(ServerLevel level, EntityUmvuthi u, Aloft s, long now) {
        if (s.settled || now - s.trailAt < AloftRules.LOOK)
            return;
        s.trailAt = now;
        for (ServerPlayer p : level.players())
            if (AloftRules.near(u.getX(), u.getY(), u.getZ(), p.getX(), p.getY(), p.getZ(), AloftRules.SIGHT))
                level.sendParticles(p, ParticleTypes.SMALL_FLAME, true, u.getX(), u.getY() + 0.2, u.getZ(), 3, 0.4, 0.1, 0.4, 0.01);
    }
}
