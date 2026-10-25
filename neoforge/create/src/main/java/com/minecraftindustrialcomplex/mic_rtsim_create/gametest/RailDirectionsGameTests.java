package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.conflict.LurkSpots;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.RailDirections;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.trains.track.TrackBlock;
import com.simibubi.create.content.trains.track.TrackShape;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.record;

/**
 * Which way the rail runs at a point, read from Create's track blocks (M8, MZB-360, the re-check's P2): the create
 * plugin's {@link RailDirections}, which the core's lurk placement asks for the direction it stands a body square to.
 * Real track blocks of every shape that matters laid in the yard, each a few blocks from the next, the answer read
 * after Create has ticked them in (the track graph knows them, as on a real line) and the track taken out again.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class RailDirectionsGameTests {
    private RailDirectionsGameTests() {}

    private static final String TEST = "theRailDirectionIsReadFromTheTrackBlocksAtThePoint";

    private static void lay(GameTestHelper helper, List<BlockPos> laid, int x, int y, int z, TrackShape shape) {
        BlockPos at = new BlockPos(x, y, z);
        helper.getLevel().setBlock(at, AllBlocks.TRACK.getDefaultState().setValue(TrackBlock.SHAPE, shape), 3);
        laid.add(at);
    }

    /** The answer must run along (ex, ez) in either sense, horizontal, of length 1. */
    private static void along(String what, Vec3 got, double ex, double ez) {
        check(what + ": a direction is known", got != null);
        double len = Math.hypot(ex, ez), ux = ex / len, uz = ez / len;
        check(what + ": horizontal and of length 1 (" + got + ")", got.y == 0.0 && Math.abs(Math.hypot(got.x, got.z) - 1.0) < 1e-9);
        check(what + ": along (" + ux + ", " + uz + ") (" + got + ")", Math.abs(got.x * uz - got.z * ux) < 1e-9 && Math.abs(got.x * ux + got.z * uz) > 1.0 - 1e-9);
    }

    private static void none(String what, Vec3 got) {
        check(what + ": no direction is known (" + got + ")", got == null);
    }

    /**
     * XO, ZO, a diagonal each way, a slope, a crossing, two lines of different directions as near as each other, a
     * point a block off the line and a curve's fake track block: the straight pieces answer with their line (in either
     * sense), the slope with its horizontal direction, and the rest with nothing, so the core keeps the engine's spot
     * unless {@code trackNear} sees a rail near it (the crossing, the corner and the fake block do; bare ground does
     * not). The registered provider is the same object's answer. Not run.
     */
    @GameTest(template = IndustryKit.YARD, batch = "rail_directions", timeoutTicks = 400)
    public static void theRailDirectionIsReadFromTheTrackBlocksAtThePoint(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        int y = o.getY() + 1, z = o.getZ() + 10;
        int[] cx = new int[8];
        for (int i = 0; i < cx.length; i++)
            cx[i] = o.getX() + 2 + 3 * i;
        List<BlockPos> laid = new ArrayList<>();
        helper.startSequence()
                .thenExecute(() -> {
                    lay(helper, laid, cx[0], y, z, TrackShape.XO);
                    lay(helper, laid, cx[1], y, z, TrackShape.ZO);
                    lay(helper, laid, cx[2], y, z, TrackShape.PD);
                    lay(helper, laid, cx[3], y, z, TrackShape.ND);
                    lay(helper, laid, cx[4], y, z, TrackShape.AE);
                    lay(helper, laid, cx[5], y, z, TrackShape.CR_O);
                    // two lines of different directions: an east-west piece west of the point's block, a north-south one south of it
                    lay(helper, laid, cx[6] - 1, y, z, TrackShape.XO);
                    lay(helper, laid, cx[6], y, z + 1, TrackShape.ZO);
                    // the middle of a curve: Create fills a bezier piece's span with fake track blocks, which are no ITrackBlock
                    BlockPos fake = new BlockPos(cx[7], y, z);
                    level.setBlock(fake, AllBlocks.FAKE_TRACK.getDefaultState(), 3);
                    laid.add(fake);
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    try {
                        Vec3 p0 = new Vec3(cx[0] + 0.5, y, z + 0.5);
                        along("XO", RailDirections.axisAt(level, p0), 1, 0);
                        along("XO, a third of a block off the line", RailDirections.axisAt(level, p0.add(0, 0, 0.3)), 1, 0);
                        none("XO, a block off the line", RailDirections.axisAt(level, p0.add(0, 0, 1.0)));
                        check("the plugin registered its rail axes (" + LurkSpots.axes() + ")", LurkSpots.axes() instanceof RailDirections);
                        along("XO by the registered provider", LurkSpots.axes().at(level, p0), 1, 0);
                        along("ZO", RailDirections.axisAt(level, new Vec3(cx[1] + 0.5, y, z + 0.5)), 0, 1);
                        along("PD", RailDirections.axisAt(level, new Vec3(cx[2] + 0.5, y, z + 0.5)), 1, 1);
                        along("ND", RailDirections.axisAt(level, new Vec3(cx[3] + 0.5, y, z + 0.5)), -1, 1);
                        along("a slope (AE)", RailDirections.axisAt(level, new Vec3(cx[4] + 0.5, y + 0.5, z + 0.5)), 1, 0);
                        none("a crossing (CR_O)", RailDirections.axisAt(level, new Vec3(cx[5] + 0.5, y, z + 0.5)));
                        // equally near an east-west and a north-south line: ambiguous; nearer one of them: that one
                        none("the corner of two lines of different directions", RailDirections.axisAt(level, new Vec3(cx[6] + 0.9, y, z + 0.9)));
                        along("the nearer of two lines", RailDirections.axisAt(level, new Vec3(cx[6] + 0.6, y, z + 0.9)), 0, 1);
                        none("the middle of a curve (a fake track block)", RailDirections.axisAt(level, new Vec3(cx[7] + 0.5, y, z + 0.5)));
                        // trackNear (the second re-check's P3): where no direction is known the core asks whether a rail is near
                        RailDirections rails = new RailDirections();
                        double r = 3.5;
                        check("a crossing has a rail near", rails.trackNear(level, new Vec3(cx[5] + 0.5, y, z + 0.5), r));
                        check("the corner of two lines has a rail near", rails.trackNear(level, new Vec3(cx[6] + 0.9, y, z + 0.9), r));
                        check("a point a block off an XO line has a rail near within 1.5", rails.trackNear(level, p0.add(0, 0, 1.0), 1.5));
                        check("and not within 0.9", !rails.trackNear(level, p0.add(0, 0, 1.0), 0.9));
                        check("the middle of a curve (a fake track block) has a rail near", rails.trackNear(level, new Vec3(cx[7] + 0.5, y, z + 0.5), 0.5));
                        check("a point 2.5 off the fake block has one within 3.5", rails.trackNear(level, new Vec3(cx[7] + 0.5, y, z + 3.0), r));
                        check("a few blocks of height off does not hide it (3 above)", rails.trackNear(level, new Vec3(cx[7] + 0.5, y + 3.0, z + 0.5), 0.5));
                        check("but 5 above is another level", !rails.trackNear(level, new Vec3(cx[7] + 0.5, y + 5.0, z + 0.5), 0.5));
                        check("bare ground 6 blocks from the line has none within 3.5", !rails.trackNear(level, new Vec3(cx[0] + 0.5, y, z - 5.5), r));
                        check("the registered provider is the same object's answer", LurkSpots.axes().trackNear(level, p0, 0.5));
                        record(TEST, "axes", "XO, ZO, PD, ND and a slope answer with their line; a crossing, the corner of two lines, a point a block off the line, a curve's fake block and bare ground with none; trackNear sees the crossing, the corner, the fake block, 3 blocks of height, and not bare ground");
                    } finally {
                        for (BlockPos p : laid)
                            level.setBlock(p, Blocks.AIR.defaultBlockState(), 3);
                    }
                })
                .thenSucceed();
    }
}
