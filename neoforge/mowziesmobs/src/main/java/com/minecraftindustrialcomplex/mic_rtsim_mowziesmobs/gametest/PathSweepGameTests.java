package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.ai.MMPathNavigateGround;
import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.PathSweep;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.PathSweepRunaway;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Mth;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.SplittableRandom;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The jar's own {@code MMPathNavigateGround.sweep} with the mixin on (MZB-951), called on a real Frostmaw's navigation by
 * reflection (the method is private): what it answers for a stone floor with a wall or a hole across it, near the origin and
 * 3.8 million blocks out, where the jar's float walk ran away (MZB-950). The far test makes the sweep that is known to run away
 * and 120 random ones; it never lets a runaway run, three ways: it asks first whether the jar's class carries the
 * mixin's handler and fails before the first call if it does not, every call is counted against a {@link PathSweep.Budget} of
 * block reads and of time that stops the walk, and the place is a 96-block square of forced chunks that no sweep of the test
 * leaves (a walk that did would load a chunk). A sweep that ends in the budget fails the test.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class PathSweepGameTests {
    private PathSweepGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;

    /** The place: 6 x 6 chunks. Chunk-aligned and holding the known runaway's coordinates (x -3,797,619.272 is 44.7 blocks in, z 4,715,190.381 is 22.4). */
    private static final int SIDE = 96;
    private static final int FAR_X = -3_797_664, FAR_Z = 4_715_168;

    /** What one sweep may read (a 32-block sweep of a 5 wide, 5 tall box reads about 1,500 cells) and how long it may take. */
    private static final long CELL_BUDGET = 12_000, MILLIS = 5_000;

    // ---- the two tests ---------------------------------------------------------------------------

    /**
     * The jar's real sweep, near the origin (inside {@link com.minecraftindustrialcomplex.mic_rtsim.gametest.PathSafeBase}'s limit,
     * where its float walk was exact), on a stone floor: clear across it, blocked by a wall of stone, blocked by a missing strip
     * of floor, clear again with both put back, and clear for 60 random sweeps.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_path_sweep_near", skyAccess = true)
    public static void theJarsPathSweepAnswersAStoneFloorNearTheOrigin(GameTestHelper helper) {
        int[] base = ConflictKit.nearBase();
        place(helper, "theJarsPathSweepAnswersAStoneFloorNearTheOrigin", base[0], base[1], false);
    }

    /**
     * The same floor at x -3,797,664, z 4,715,168 (the jar's float walk ran away on 1 sweep in 25 there): the same answers, the
     * sweep of {@link PathSweepRunaway} that is known to run away in a copy of it, and 120 random sweeps, each inside its
     * budget.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_path_sweep_far", skyAccess = true)
    public static void theJarsPathSweepEndsOnAStoneFloorWhereItsFloatWalkRanAway(GameTestHelper helper) {
        place(helper, "theJarsPathSweepEndsOnAStoneFloorWhereItsFloatWalkRanAway", FAR_X, FAR_Z, true);
    }

    // ---- one place -------------------------------------------------------------------------------

    private static void place(GameTestHelper helper, String test, int bx, int bz, boolean far) {
        ChunkWait waits = ChunkWait.of(helper, 3000);
        Far place = new Far(helper, test, bx, bz);
        ServerLevel level = place.level;
        EntityFrostmaw[] f = new EntityFrostmaw[1];

        MowzieKit.seq(helper)
                .thenExecute(() -> place.forceRect(place.k, place.z0, place.k + SIDE / 16 - 1, place.z0 + SIDE / 16 - 1))
                .thenWaitUntil(waits.chunks(place.ready()))
                .thenExecute(() -> {
                    // before any call to the jar's sweep: is the mixin on it? Without it a far sweep may count through the int range.
                    String handler = mixinHandler();
                    check("the jar's MMPathNavigateGround carries the mixin's handler for sweep (none: " + Arrays.toString(
                            Arrays.stream(MMPathNavigateGround.class.getDeclaredMethods()).map(Method::getName).filter(n -> n.contains("sweep")).toArray()) + ")",
                            handler != null);
                    record(test, "mixin", handler);

                    int top = MowzieKit.platform(level, bx, bx + SIDE - 1, bz, bz + SIDE - 1);
                    double ex = bx + 48.5, ez = bz + 48.5, y = top + 1;
                    f[0] = MowzieKit.stray(level, EntityHandler.FROSTMAW.get(), ex, y, ez, true, false);
                    ConflictKit.onEnd(helper, () -> f[0].discard());
                    check("the Frostmaw walks on the jar's MMPathNavigateGround", f[0].getNavigation() instanceof MMPathNavigateGround);
                    MMPathNavigateGround nav = (MMPathNavigateGround) f[0].getNavigation();
                    Method sweep = sweepMethod();
                    int loaded = level.getChunkSource().getLoadedChunksCount();
                    long cells = 0;
                    int sweeps = 0;

                    // 1: across the floor, in the four directions the walls and holes below are not in the way of
                    double w = PathSweepRunaway.WIDTH, h = PathSweepRunaway.HEIGHT;
                    for (Vec3 v : new Vec3[] {new Vec3(30, 0, 0), new Vec3(0, 0, -30), new Vec3(-25, 0, 15), new Vec3(20, 0, 20)}) {
                        Answer a = sweep(nav, sweep, v, ex, y, ez, w, h);
                        check("a clear stone floor, vector " + v + ": clear (" + a + ")", a.clear && !a.budget.overran());
                        cells += a.budget.cells();
                        sweeps++;
                    }

                    // 2: a wall of stone 6 high and 9 wide across the way, 12 and 13 blocks along x
                    int wx = Mth.floor(ex) + 12, wz = Mth.floor(ez);
                    fill(level, wx, wx + 1, wz - 4, wz + 4, top + 1, top + 6, true);
                    Answer wall = sweep(nav, sweep, new Vec3(30, 0, 0), ex, y, ez, w, h);
                    check("a wall of stone across the way: blocked (" + wall + ")", !wall.clear && !wall.budget.overran());
                    fill(level, wx, wx + 1, wz - 4, wz + 4, top + 1, top + 6, false);

                    // 3: the floor missing in a strip 2 blocks wide and 2 deep: open air under the way. (One block deep is a step down
                    // the jar's sweep lets a mob take: the cell under its feet is then walkable, standing on the ground below.)
                    fill(level, wx, wx + 1, wz - 4, wz + 4, top - 1, top, false);
                    Answer hole = sweep(nav, sweep, new Vec3(30, 0, 0), ex, y, ez, w, h);
                    check("the floor missing across the way: blocked (" + hole + ")", !hole.clear && !hole.budget.overran());
                    fill(level, wx, wx + 1, wz - 4, wz + 4, top - 1, top, true);

                    // 4: both put back
                    Answer again = sweep(nav, sweep, new Vec3(30, 0, 0), ex, y, ez, w, h);
                    check("wall and hole put back: clear (" + again + ")", again.clear && !again.budget.overran());

                    // 5: far from the origin, the sweep that runs away in the jar's float walk, then random ones
                    if (far) {
                        double kx = PathSweepRunaway.X, kz = PathSweepRunaway.Z;
                        Answer known = sweep(nav, sweep, new Vec3(PathSweepRunaway.VX, PathSweepRunaway.VY, PathSweepRunaway.VZ), kx, y, kz,
                                PathSweepRunaway.WIDTH, PathSweepRunaway.HEIGHT);
                        check("the sweep that runs away in the jar's float walk: clear, in budget (" + known + ")", known.clear && !known.budget.overran());
                        record(test, "known runaway", known.budget.cells() + " cells in " + known.budget.columns() + " columns");
                    }
                    SplittableRandom r = new SplittableRandom(951);
                    long worst = 0;
                    for (int k = 0; k < (far ? 120 : 60); k++) {
                        double rw = 0.6 + r.nextDouble() * 4.4, rh = 1 + r.nextDouble() * 4;
                        double rx = bx + 48 + (r.nextDouble() * 12 - 6), rz = bz + 48 + (r.nextDouble() * 12 - 6);
                        double len = 1 + r.nextDouble() * 31, angle = r.nextDouble() * Math.PI * 2;
                        Answer a = sweep(nav, sweep, new Vec3(Math.cos(angle) * len, 0, Math.sin(angle) * len), rx, y, rz, rw, rh);
                        check("random sweep " + k + " (box " + rw + " by " + rh + " at " + rx + ", " + rz + ", " + len + " blocks at " + angle
                                + " rad): clear and in budget (" + a + ")", a.clear && !a.budget.overran());
                        cells += a.budget.cells();
                        worst = Math.max(worst, a.budget.cells());
                        sweeps++;
                    }
                    int delta = level.getChunkSource().getLoadedChunksCount() - loaded;
                    record(test, "sweeps", sweeps + " sweeps, " + cells + " block reads, the largest " + worst + "; " + delta + " more chunks loaded");
                    check("no sweep loaded chunks beyond the place (" + delta + " more; a runaway loads tens of thousands)", delta <= 300);
                    f[0].discard();
                })
                .thenSucceed();
    }

    // ---- the call --------------------------------------------------------------------------------

    private record Answer(boolean clear, PathSweep.Budget budget) {
        @Override
        public String toString() {
            return "answer " + clear + ", " + budget.cells() + " cells in " + budget.columns() + " columns" + (budget.overran() ? ", OVERRAN" : "");
        }
    }

    /**
     * The name the mixin's handler has in the jar's class once Mixin has merged it, or null: {@code <prefix>$<ids>$<source id>$sweep}
     * ({@code MethodMapper.getHandlerName} of Mixin 0.8.7 puts the handler's own name, here {@code mic_rtsim_mowziesmobs$sweep}, last,
     * and the jar's own method is plain {@code sweep}, so a name ending in {@code $sweep} is the handler).
     */
    private static String mixinHandler() {
        for (Method m : MMPathNavigateGround.class.getDeclaredMethods())
            if (m.getName().endsWith("$sweep"))
                return m.getName();
        return null;
    }

    private static Method sweepMethod() {
        try {
            Method m = MMPathNavigateGround.class.getDeclaredMethod("sweep", Vec3.class, Vec3.class, Vec3.class);
            m.setAccessible(true);
            return m;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("the jar's MMPathNavigateGround has no sweep(Vec3, Vec3, Vec3): " + e, e);
        }
    }

    /** The jar's sweep for a box standing at (x, y, z) (feet), {@code w} wide and {@code h} tall, along {@code vec}, as {@code followThePath} makes its arguments. */
    private static Answer sweep(MMPathNavigateGround nav, Method sweep, Vec3 vec, double x, double y, double z, double w, double h) {
        Vec3 base = new Vec3(x, y, z).add(-w * 0.5, 0.0, -w * 0.5);
        Vec3 max = base.add(w, h, w);
        PathSweep.Budget budget = new PathSweep.Budget(CELL_BUDGET, MILLIS);
        PathSweep.budget(budget);
        try {
            return new Answer((boolean) sweep.invoke(nav, vec, base, max), budget);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("the jar's sweep threw " + e.getCause(), e);
        } finally {
            PathSweep.budget(null);
        }
    }

    private static void fill(ServerLevel level, int x0, int x1, int z0, int z1, int y0, int y1, boolean stone) {
        for (int x = x0; x <= x1; x++)
            for (int z = z0; z <= z1; z++)
                for (int y = y0; y <= y1; y++)
                    level.setBlock(new BlockPos(x, y, z), (stone ? Blocks.STONE : Blocks.AIR).defaultBlockState(), 2);
    }
}
