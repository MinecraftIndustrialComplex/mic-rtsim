package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassGraphCommand;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.StaleKinetics;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorPackage;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.SteamEngineBlock;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.TicketType;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.function.Supplier;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * SEC-1 slice S1, no chunk loads (SECTIONS.md §2.8, §9 and §10.1; PATCHES.md rows 50 to 56): Create's chain
 * conveyors, its network recount and the compile never load a chunk of a district that is away, and a part
 * of a network that comes back stale is re-synced instead of broken.
 *
 * <p><b>The away state.</b> Far chunks, each test its own batch. A district A and a district B two chunks
 * apart (an empty chunk M between) are built with both chunks forced, then both are unforced until both
 * have unloaded, and A alone gets a radius-1 region ticket back: A is at level 32 (it ticks), M at 33 (FULL,
 * not ticking) and B at 34, which keeps a chunk holder and never makes it FULL. That is what a player's
 * own ticket does to the chunks two away, so it is the away district a live server has: B has no block
 * entity, {@code getChunkNow} answers null, and any lookup that loads (vanilla's {@code getBlockEntity}, a
 * capability, Create's chain and network code without the S1 patches) promotes it to FULL, which fires
 * {@code ChunkEvent.Load} and so B's {@code IND_CHUNK_LOADED}. A watch on every tick checks that B is not
 * FULL, that B's district counts no loaded chunk, and that no {@code IND_CHUNK_LOADED} for B was
 * journaled. (A chunk demoted from FULL to 34 and promoted again fires no event: the tests let B unload
 * fully first, so a promotion is the load the watch sees.)
 *
 * <p>Away chunks are read only through {@link Loaded}; {@code GlassCorpusKit.be} and {@code speed} load.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class SectionsChunkLoadGameTests {
    private SectionsChunkLoadGameTests() {}

    /** A's radius-1 region ticket: level 32 at A (block ticking), 33 one chunk away, 34 two away. */
    private static final TicketType<ChunkPos> SEC1 = TicketType.create("mic_rtsim_create_sec1", Comparator.comparingLong(ChunkPos::toLong));
    /** A ticket that keeps a chunk FULL (level 33) and not block-ticking. */
    private static final TicketType<ChunkPos> FULL_ONLY = TicketType.create("mic_rtsim_create_sec1_full_only", Comparator.comparingLong(ChunkPos::toLong));

    /** The fixtures' height, above any terrain of the far chunks. */
    private static final int Y = 180;
    private static final String ADDRESS = "mill";

    // ---- shared helpers --------------------------------------------------------------------------

    /** A wait on the world's IO; while the check fails the tick sleeps 5 ms (ReconcileAheadGameTests.io). */
    private static Runnable io(Runnable check) {
        return () -> {
            try {
                check.run();
            } catch (RuntimeException e) {
                try {
                    Thread.sleep(5);
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                }
                throw e;
            }
        };
    }

    private static int farChunk(GameTestHelper helper, boolean x, int offset) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        return ((x ? o.getX() : o.getZ()) >> 4) + offset;
    }

    /** The journaled IND_CHUNK_LOADED records of a district for one chunk. */
    private static int loadedCount(long district, int cx, int cz) {
        int n = 0;
        for (ScriptedEngine.Received r : engine().received(IndRecords.IND_CHUNK_LOADED)) {
            if (r.skipped() || r.bytes().length < IndRecords.ChunkLoaded.DISTRICT + 8 || r.view().getLong(IndRecords.ChunkLoaded.DISTRICT) != district)
                continue;
            if (r.view().getInt(IndRecords.ChunkLoaded.X) == cx && r.view().getInt(IndRecords.ChunkLoaded.Z) == cz)
                n++;
        }
        return n;
    }

    private static boolean unloaded(ServerLevel level, int cx, int cz, long district) {
        District d = ind().district(district);
        return level.getChunkSource().getChunkNow(cx, cz) == null && d != null && ind().districts().loadedChunks(d) == 0;
    }

    private static int loadedChunks(long district) {
        District d = ind().district(district);
        return d == null ? -1 : ind().districts().loadedChunks(d);
    }

    /** The loop angle a player picks for a frogport beside a conveyor (PackageLoopFactory.loopAngle). */
    private static float loopAngle(BlockPos conveyor, BlockPos port) {
        BlockPos d = port.subtract(conveyor);
        float deg = (float) Math.toDegrees(Math.atan2(d.getX(), d.getZ()));
        return deg < 0 ? deg + 360 : deg;
    }

    private static ItemStack box() {
        ItemStack box = PackageItem.containing(List.of(new ItemStack(Items.IRON_INGOT)));
        PackageItem.addAddress(box, ADDRESS);
        return box;
    }

    private static ChainConveyorBlockEntity conveyor(ServerLevel level, BlockPos p) {
        return Loaded.blockEntity(level, p) instanceof ChainConveyorBlockEntity c ? c : null;
    }

    private static KineticBlockEntity kinetic(ServerLevel level, BlockPos p) {
        return Loaded.blockEntity(level, p) instanceof KineticBlockEntity k ? k : null;
    }

    /** The speed of a kinetic block entity in a loaded chunk; NaN where none is loaded. */
    private static float speedOf(ServerLevel level, BlockPos p) {
        KineticBlockEntity k = kinetic(level, p);
        return k == null ? Float.NaN : k.getSpeed();
    }

    private static int looping(ServerLevel level, BlockPos p) {
        ChainConveyorBlockEntity c = conveyor(level, p);
        return c == null ? -1 : c.getLoopingPackages().size();
    }

    private static int travelling(ServerLevel level, BlockPos p) {
        ChainConveyorBlockEntity c = conveyor(level, p);
        if (c == null)
            return -1;
        int n = 0;
        for (List<ChainConveyorPackage> l : c.getTravellingPackages().values())
            n += l.size();
        return n;
    }

    /** Packages a frogport holds (one per slot); -1 where it is not loaded. */
    private static int held(ServerLevel level, BlockPos frog) {
        if (!(Loaded.blockEntity(level, frog) instanceof FrogportBlockEntity f))
            return -1;
        int n = 0;
        for (int i = 0; i < f.inventory.getSlots(); i++)
            if (!f.inventory.getStackInSlot(i).isEmpty())
                n++;
        return n;
    }

    /** Joins two chain conveyors the way ChainConveyorConnectionPacket does (PackageLoopFactory). */
    private static void join(ServerLevel level, BlockPos a, BlockPos b) {
        ChainConveyorBlockEntity ca = conveyor(level, a), cb = conveyor(level, b);
        check("both conveyors are built", ca != null && cb != null);
        ca.prepareStats();
        cb.prepareStats();
        check("the conveyors join", cb.addConnectionTo(a) && ca.addConnectionTo(b));
    }

    /**
     * Removes the block entities from {@code Y - 3} to {@code Y + 3} in the loaded chunks of a box of chunks
     * (an earlier run's fixture in a reused world), then forgets the box's industry.
     */
    private static void clear(ServerLevel level, int minCx, int minCz, int maxCx, int maxCz) {
        for (int cx = minCx; cx <= maxCx; cx++)
            for (int cz = minCz; cz <= maxCz; cz++) {
                LevelChunk c = level.getChunkSource().getChunkNow(cx, cz);
                if (c == null)
                    continue;
                for (BlockEntity be : new ArrayList<>(c.getBlockEntities().values()))
                    if (be.getBlockPos().getY() >= Y - 3 && be.getBlockPos().getY() <= Y + 3)
                        level.removeBlock(be.getBlockPos(), false);
            }
        ind().districts().forgetArea(level, minCx, minCz, maxCx, maxCz);
    }

    private static void force(ServerLevel level, int minCx, int maxCx, int cz, boolean on) {
        for (int cx = minCx; cx <= maxCx; cx++)
            level.setChunkForced(cx, cz, on);
    }

    private static boolean allLoaded(ServerLevel level, int minCx, int maxCx, int cz) {
        for (int cx = minCx; cx <= maxCx; cx++)
            if (level.getChunkSource().getChunkNow(cx, cz) == null)
                return false;
        return true;
    }

    /**
     * A check on every tick while {@link #on}: the first time the probe answers a reason, it is kept (with
     * the game time) and {@link #check} fails with it.
     */
    private static final class Watch {
        boolean on;
        String breach;

        Watch(GameTestHelper helper, Supplier<String> probe) {
            helper.onEachTick(() -> {
                if (!on || breach != null)
                    return;
                String why = probe.get();
                if (why != null)
                    breach = "tick " + helper.getLevel().getGameTime() + ": " + why;
            });
        }

        void check(String what) {
            IndustryKit.check(what + ": " + breach, breach == null);
        }
    }

    // ---- the adjacent fixture: A, an empty M, B ----------------------------------------------------

    /**
     * Conveyor CA in A and CB in B, 23 blocks apart, joined; a cog over each and a millstone meshing with
     * it (the seeds of districts A and B); a frogport FB on CB's loop that keeps packages addressed
     * {@value #ADDRESS} (nothing under it, so it holds them); a creative motor under CA or CB.
     */
    private record Adj(int ax, int az, int bx, BlockPos ca, BlockPos cb, BlockPos millA, BlockPos millB, BlockPos frogB, BlockPos motor,
                       long[] ids) {
        static Adj at(GameTestHelper h, int offset, boolean motorInA) {
            int ax = farChunk(h, true, offset), az = farChunk(h, false, offset), bx = ax + 2, z = az * 16 + 8;
            BlockPos ca = new BlockPos(ax * 16 + 12, Y, z), cb = new BlockPos(bx * 16 + 3, Y, z);
            return new Adj(ax, az, bx, ca, cb, ca.above().south(), cb.above().south(), new BlockPos(bx * 16 + 5, Y, z),
                    (motorInA ? ca : cb).below(), new long[2]);
        }

        long da() {
            return ids[0];
        }

        long db() {
            return ids[1];
        }

        ChunkPos a() {
            return new ChunkPos(ax, az);
        }

        /** What the watch checks while B is away. */
        String probe(ServerLevel level, int loadsOfB) {
            if (level.getChunkSource().getChunkNow(bx, az) != null)
                return "B became FULL";
            int n = loadedChunks(db());
            if (n != 0)
                return "B's district counts " + n + " loaded chunks";
            int loads = loadedCount(db(), bx, az);
            if (loads != loadsOfB)
                return "IND_CHUNK_LOADED for B: " + loads + " after " + loadsOfB;
            return null;
        }
    }

    /** Forces A and B, clears an earlier run, builds the fixture and waits for both districts. */
    private static GameTestSequence build(GameTestHelper h, Adj f, int rpm) {
        ServerLevel level = h.getLevel();
        return h.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(f.ax, f.az, true);
                    level.setChunkForced(f.bx, f.az, true);
                })
                .thenWaitUntil(io(() -> check("A, M and B load", allLoaded(level, f.ax, f.bx, f.az))))
                .thenExecute(() -> {
                    clear(level, f.ax - 1, f.az - 1, f.bx + 1, f.az + 1);
                    GlassCorpusKit.motor(h, f.motor, Direction.UP, rpm);
                    GlassCorpusKit.set(h, f.ca, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    GlassCorpusKit.set(h, f.cb, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    for (BlockPos c : List.of(f.ca, f.cb)) {
                        GlassCorpusKit.cog(h, c.above(), Direction.Axis.Y);
                        GlassCorpusKit.set(h, c.above().south(), AllBlocks.MILLSTONE.getDefaultState());
                    }
                    join(level, f.ca, f.cb);
                    GlassCorpusKit.set(h, f.frogB, AllBlocks.PACKAGE_FROGPORT.getDefaultState());
                    FrogportBlockEntity fb = GlassCorpusKit.be(h, f.frogB, FrogportBlockEntity.class);
                    fb.target = new PackagePortTarget.ChainConveyorFrogportTarget(f.cb.subtract(f.frogB), loopAngle(f.cb, f.frogB), (BlockPos) null, false);
                    fb.addressFilter = ADDRESS;
                    fb.filterChanged();
                    fb.notifyUpdate();
                    playerPlaced(h, f.millA);
                    playerPlaced(h, f.millB);
                })
                .thenWaitUntil(() -> {
                    District a = districtAt(h, f.millA), b = districtAt(h, f.millB);
                    check("both districts exist", a != null && b != null);
                    check("A and B are two districts: " + a.id, a.id != b.id);
                    f.ids[0] = a.id;
                    f.ids[1] = b.id;
                })
                .thenWaitUntil(() -> check("the chain turns both millstones: A " + speedOf(level, f.millA) + ", B " + speedOf(level, f.millB),
                        speedOf(level, f.millA) != 0 && speedOf(level, f.millB) != 0 && speedOf(level, f.ca) == speedOf(level, f.cb)));
    }

    /** Unforces A and B, waits until both have unloaded, and gives A its radius-1 ticket: B is away. */
    private static GameTestSequence away(GameTestSequence s, ServerLevel level, Adj f) {
        return s.thenExecute(() -> {
                    level.setChunkForced(f.ax, f.az, false);
                    level.setChunkForced(f.bx, f.az, false);
                })
                .thenWaitUntil(io(() -> check("A and B unload: A " + loadedChunks(f.da()) + ", B " + loadedChunks(f.db()),
                        unloaded(level, f.ax, f.az, f.da()) && unloaded(level, f.bx, f.az, f.db()))))
                .thenExecute(() -> level.getChunkSource().addRegionTicket(SEC1, f.a(), 1, f.a()))
                .thenWaitUntil(io(() -> check("A loads and ticks", Loaded.ticks(level, f.ca) && conveyor(level, f.ca) != null)))
                .thenExecute(() -> check("B stays away (level 34)", level.getChunkSource().getChunkNow(f.bx, f.az) == null && loadedChunks(f.db()) == 0));
    }

    /** Forces A and B again, clears the fixture and lets go of every ticket. */
    private static GameTestSequence restore(GameTestSequence s, ServerLevel level, Adj f) {
        return s.thenExecute(() -> {
                    level.setChunkForced(f.ax, f.az, true);
                    level.setChunkForced(f.bx, f.az, true);
                })
                .thenWaitUntil(io(() -> check("A, M and B load for the clean-up", allLoaded(level, f.ax, f.bx, f.az))))
                .thenExecute(() -> {
                    clear(level, f.ax - 1, f.az - 1, f.bx + 1, f.az + 1);
                    level.getChunkSource().removeRegionTicket(SEC1, f.a(), 1, f.a());
                    ChunkPos b = new ChunkPos(f.bx, f.az);
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, b, 0, b);
                    level.setChunkForced(f.ax, f.az, false);
                    level.setChunkForced(f.bx, f.az, false);
                });
    }

    // ---- the tests ---------------------------------------------------------------------------------

    /**
     * Packages on a chain into an away district stay on the loaded side (PATCHES.md rows 50 and 51). A
     * first package reaches FB while both ends are loaded (the routes are learned). With B away, three
     * more are put on CA's loop over 300 ticks: they circle CA, nothing looks into B (without the patch,
     * CA's adverts, hand-offs and arrivals looked CB up and loaded B about every 80 ticks), and when B is
     * forced again all three reach FB.
     */
    @GameTest(template = YARD, batch = "sec1_1", timeoutTicks = 12000)
    public static void anAwayDistrictOnAChainStaysAway(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 4400, true);
        int[] mark = {0};
        Watch watch = new Watch(helper, () -> f.probe(level, mark[0]));
        GameTestSequence s = build(helper, f, 64)
                .thenExecute(() -> conveyor(level, f.ca).addLoopingPackage(new ChainConveyorPackage(0, box())))
                .thenWaitUntil(() -> check("the first package reaches FB: " + held(level, f.frogB), held(level, f.frogB) == 1));
        s = away(s, level, f)
                .thenExecute(() -> {
                    mark[0] = loadedCount(f.db(), f.bx, f.az);
                    watch.on = true;
                    conveyor(level, f.ca).addLoopingPackage(new ChainConveyorPackage(0, box()));
                })
                .thenExecuteAfter(100, () -> conveyor(level, f.ca).addLoopingPackage(new ChainConveyorPackage(90, box())))
                .thenExecuteAfter(100, () -> conveyor(level, f.ca).addLoopingPackage(new ChainConveyorPackage(180, box())))
                .thenIdle(100)
                .thenExecute(() -> {
                    watch.check("nothing loaded B");
                    record("anAwayDistrictOnAChainStaysAway", "CA looping, travelling", looping(level, f.ca) + ", " + travelling(level, f.ca));
                    check("the three packages circle CA: looping " + looping(level, f.ca) + ", travelling " + travelling(level, f.ca),
                            looping(level, f.ca) == 3 && travelling(level, f.ca) == 0);
                    watch.on = false;
                    level.setChunkForced(f.bx, f.az, true);
                })
                .thenWaitUntil(() -> check("all four reach FB once B ticks: FB " + held(level, f.frogB) + ", CA " + looping(level, f.ca) + "/" + travelling(level, f.ca),
                        held(level, f.frogB) == 4 && looping(level, f.ca) == 0 && travelling(level, f.ca) == 0));
        restore(s, level, f).thenSucceed();
    }

    /**
     * Rotation never propagates into an away chunk, and the part it left stale is re-synced when it loads
     * (PATCHES.md rows 52 and 56). With B away, the motor in A is set to 128, 32, -64 and -32 RPM; CA follows
     * each change and B is never loaded (without the patch, every change walked the chain into B and loaded
     * it). CB keeps 64 RPM in its saved state, which no longer matches CA: Create's first attach at the load
     * would break CB (opposite signs). With the re-sync, CB loads, takes CA's -32 RPM and turns its millstone
     * at 32 RPM.
     */
    @GameTest(template = YARD, batch = "sec1_2", timeoutTicks = 12000)
    public static void anAwayDistrictOnASharedNetworkStaysAway(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 4500, true);
        int[] mark = {0, 0};
        Watch watch = new Watch(helper, () -> f.probe(level, mark[0]));
        String t = "anAwayDistrictOnASharedNetworkStaysAway";
        GameTestSequence s = away(build(helper, f, 64), level, f)
                .thenExecute(() -> {
                    mark[0] = loadedCount(f.db(), f.bx, f.az);
                    watch.on = true;
                });
        for (int rpm : new int[] {128, 32, -64, -32}) {
            s = s.thenExecute(() -> ((CreativeMotorBlockEntity) Loaded.blockEntity(level, f.motor)).generatedSpeed.setValue(rpm))
                    .thenIdle(60)
                    .thenExecute(() -> {
                        KineticBlockEntity m = kinetic(level, f.motor);
                        record(t, "motor " + rpm + ": CA, millstone A", speedOf(level, f.ca) + ", " + speedOf(level, f.millA));
                        check("CA follows the motor to " + rpm + ": " + speedOf(level, f.ca),
                                m != null && speedOf(level, f.ca) == m.getGeneratedSpeed() && Math.abs(speedOf(level, f.ca)) == Math.abs(rpm));
                        check("millstone A turns at " + Math.abs(rpm) + ": " + speedOf(level, f.millA), Math.abs(speedOf(level, f.millA)) == Math.abs(rpm));
                    });
        }
        s = s.thenExecute(() -> {
                    watch.check("nothing loaded B");
                    watch.on = false;
                    mark[1] = StaleKinetics.resyncs();
                    level.setChunkForced(f.bx, f.az, true);
                })
                .thenWaitUntil(io(() -> check("B loads", Loaded.ticks(level, f.cb))))
                .thenIdle(40)
                .thenExecute(() -> {
                    record(t, "after the load: CB, millstone B, re-syncs", speedOf(level, f.cb) + ", " + speedOf(level, f.millB) + ", "
                            + (StaleKinetics.resyncs() - mark[1]));
                    check("CB is not broken: " + level.getBlockState(f.cb), conveyor(level, f.cb) != null);
                    check("CB turns with CA: CB " + speedOf(level, f.cb) + ", CA " + speedOf(level, f.ca), speedOf(level, f.cb) == speedOf(level, f.ca));
                    check("millstone B turns at 32 RPM: " + speedOf(level, f.millB), Math.abs(speedOf(level, f.millB)) == 32);
                    check("the stale part was re-synced", StaleKinetics.resyncs() > mark[1]);
                    KineticBlockEntity ca = kinetic(level, f.ca), cb = kinetic(level, f.cb);
                    check("one network: " + ca.network + ", " + cb.network, ca.network != null && ca.network.equals(cb.network));
                });
        restore(s, level, f).thenSucceed();
    }

    /**
     * A network's recount keeps an away source (PATCHES.md rows 53 and 54). Eight conveyors 30 blocks apart
     * reach from A over 13 chunks to B, where a creative motor turns them; both ends have a cog and a
     * millstone. With only A's radius-1 ticket left, B (13 chunks away) unloads and the conveyors between
     * are held below FULL. The recount (Create's {@code calculateCapacity}) keeps the motor as it last was,
     * without loading B: the capacity is unchanged and the motor still listed. A millstone added in A
     * makes the network recount its stress, with the same answer for the capacity. When B is forced again,
     * the fresh motor joins and the next recount drops the stale one: one source, the same capacity.
     */
    @GameTest(template = YARD, batch = "sec1_3", timeoutTicks = 16000)
    public static void aNetworkRecountKeepsAnUnloadedSource(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aNetworkRecountKeepsAnUnloadedSource";
        int ax = farChunk(helper, true, 4600), az = farChunk(helper, false, 4600), bx = ax + 13, z = az * 16 + 8;
        ChunkPos a = new ChunkPos(ax, az);
        List<BlockPos> cs = new ArrayList<>();
        for (int k = 0; k < 8; k++)
            cs.add(new BlockPos(ax * 16 + 8 + 30 * k, Y, z));
        BlockPos c0 = cs.get(0), c7 = cs.get(7), motor = c7.below(), millA = c0.above().south(), millB = c7.above().south();
        BlockPos added = c0.above().north();
        long[] ids = new long[2];
        float[] cap = new float[3], stress = new float[2];
        KineticBlockEntity[] oldMotor = {null};
        int[] mark = {0};
        Watch watch = new Watch(helper, () -> {
            for (int cx = ax + 2; cx <= bx; cx++)
                if (level.getChunkSource().getChunkNow(cx, az) != null)
                    return "chunk " + (cx - ax) + " from A became FULL";
            int n = loadedChunks(ids[1]);
            if (n != 0)
                return "B's district counts " + n + " loaded chunks";
            int loads = loadedCount(ids[1], bx, az);
            return loads != mark[0] ? "IND_CHUNK_LOADED for B: " + loads + " after " + mark[0] : null;
        });
        Supplier<KineticNetwork> net = () -> kinetic(level, c0).getOrCreateNetwork();
        helper.startSequence()
                .thenExecute(() -> force(level, ax, bx, az, true))
                .thenWaitUntil(io(() -> check("the fourteen chunks load", allLoaded(level, ax, bx, az))))
                .thenExecute(() -> {
                    clear(level, ax - 1, az - 1, bx + 1, az + 1);
                    GlassCorpusKit.motor(helper, motor, Direction.UP, 64);
                    for (BlockPos c : cs)
                        GlassCorpusKit.set(helper, c, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    for (int k = 0; k + 1 < cs.size(); k++)
                        join(level, cs.get(k), cs.get(k + 1));
                    for (BlockPos c : List.of(c0, c7)) {
                        GlassCorpusKit.cog(helper, c.above(), Direction.Axis.Y);
                        GlassCorpusKit.set(helper, c.above().south(), AllBlocks.MILLSTONE.getDefaultState());
                    }
                    playerPlaced(helper, millA);
                    playerPlaced(helper, millB);
                })
                .thenWaitUntil(() -> {
                    District da = districtAt(helper, millA), db = districtAt(helper, millB);
                    check("both districts exist", da != null && db != null && da.id != db.id);
                    ids[0] = da.id;
                    ids[1] = db.id;
                })
                .thenWaitUntil(() -> check("the motor in B turns millstone A: " + speedOf(level, millA), Math.abs(speedOf(level, millA)) == 64))
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    cap[0] = n.calculateCapacity();
                    stress[0] = n.calculateStress();
                    oldMotor[0] = kinetic(level, motor);
                    record(t, "loaded: capacity, stress, sources, members", cap[0] + ", " + stress[0] + ", " + n.sources.size() + ", " + n.members.size());
                    check("one source, the motor", n.sources.size() == 1 && n.sources.containsKey(oldMotor[0]));
                    level.getChunkSource().addRegionTicket(SEC1, a, 1, a);
                    force(level, ax, bx, az, false);
                })
                .thenWaitUntil(io(() -> check("B unloads: " + loadedChunks(ids[1]), unloaded(level, bx, az, ids[1]))))
                .thenWaitUntil(io(() -> check("the chunks two or more from A are below FULL", !anyLoaded(level, ax + 2, bx, az))))
                .thenExecute(() -> {
                    mark[0] = loadedCount(ids[1], bx, az);
                    watch.on = true;
                    KineticNetwork n = net.get();
                    cap[1] = n.calculateCapacity();
                    record(t, "B away: capacity, sources, members, size", cap[1] + ", " + n.sources.size() + ", " + n.members.size() + ", " + n.getSize());
                    check("the recount keeps the away motor's capacity: " + cap[1] + " vs " + cap[0], cap[1] == cap[0]);
                    check("the away motor is still listed", n.sources.containsKey(oldMotor[0]));
                    GlassCorpusKit.set(helper, added, AllBlocks.MILLSTONE.getDefaultState());
                })
                .thenWaitUntil(() -> check("the added millstone turns: " + speedOf(level, added), Math.abs(speedOf(level, added)) == 64))
                .thenIdle(20)
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    cap[2] = n.calculateCapacity();
                    stress[1] = n.calculateStress();
                    record(t, "after the added millstone: capacity, stress", cap[2] + ", " + stress[1]);
                    check("the capacity is unchanged: " + cap[2], cap[2] == cap[0]);
                    check("the stress grew by the added millstone: " + stress[0] + " to " + stress[1], stress[1] > stress[0]);
                    check("the away motor is still listed", n.sources.containsKey(oldMotor[0]));
                    watch.check("no chunk two or more from A loaded");
                    watch.on = false;
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(io(() -> {
                    KineticBlockEntity fresh = kinetic(level, motor);
                    check("the fresh motor joins the network", fresh != null && fresh != oldMotor[0] && net.get().sources.containsKey(fresh));
                }))
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    float c = n.calculateCapacity();
                    record(t, "B loaded again: capacity, sources", c + ", " + n.sources.size());
                    check("one source again: " + n.sources.size(), n.sources.size() == 1 && !n.sources.containsKey(oldMotor[0]));
                    check("the same capacity: " + c + " vs " + cap[0], c == cap[0]);
                })
                .thenExecute(() -> force(level, ax, bx, az, true))
                .thenWaitUntil(io(() -> check("the fourteen chunks load for the clean-up", allLoaded(level, ax, bx, az))))
                .thenExecute(() -> {
                    clear(level, ax - 1, az - 1, bx + 1, az + 1);
                    level.getChunkSource().removeRegionTicket(SEC1, a, 1, a);
                    force(level, ax, bx, az, false);
                })
                .thenSucceed();
    }

    private static boolean anyLoaded(ServerLevel level, int minCx, int maxCx, int cz) {
        for (int cx = minCx; cx <= maxCx; cx++)
            if (level.getChunkSource().getChunkNow(cx, cz) != null)
                return true;
        return false;
    }

    /**
     * A compile of a district whose network reaches into an away district loads nothing (PATCHES.md row
     * 55, SECTIONS.md §2.8). The motor is in B; with B away, A's compile reads CA's chain into B as nothing
     * (counted as stops at unloaded chunks), and the network, which has members outside A, is a crossing
     * one: millstone A's domain has one level at the speed the network last had, no driver, and its members
     * are bounded. {@code /rtsim district graph} writes the compile without loading anything either.
     */
    @GameTest(template = YARD, batch = "sec1_4", timeoutTicks = 12000)
    public static void aCompileLoadsNoChunk(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 4700, false);
        int[] mark = {0, -1};
        Watch watch = new Watch(helper, () -> f.probe(level, mark[0]));
        String t = "aCompileLoadsNoChunk";
        GameTestSequence s = away(build(helper, f, 64), level, f)
                .thenExecute(() -> {
                    mark[0] = loadedCount(f.db(), f.bx, f.az);
                    watch.on = true;
                    GlassKit.Graph g = GlassKit.latest(f.da());
                    mark[1] = g == null ? -1 : g.end().compile();
                    ind().glass().requestCompile(ind().district(f.da()));
                })
                .thenWaitUntil(() -> {
                    GlassKit.Graph g = GlassKit.latest(f.da());
                    check("a new complete compile of A", g != null && g.complete() && g.end().compile() > mark[1]);
                })
                .thenExecute(() -> {
                    watch.check("the compile loaded B");
                    GlassKit.Graph g = GlassKit.latest(f.da());
                    GraphBuilder java = ind().glass().lastGraph(f.da());
                    record(t, "graph", g.describe() + "; stops at unloaded chunks " + (java == null ? "?" : java.unloadedStops));
                    check("the Java side kept the compile", java != null);
                    check("the reads stopped at B: " + java.unloadedStops, java.unloadedStops > 0);
                    GlassKit.Domain d = millDomain(g);
                    record(t, "millstone A's domain", d);
                    check("one level: " + d.levels(), d.levels().size() == 1);
                    check("at the network's last speed: " + d.levels().get(0).rpm(), Math.abs(d.levels().get(0).rpm()) == 64);
                    check("no driver: " + d.driver(), d.driver() == GlassRecords.NONE);
                    checkBounded(g, d);
                    try {
                        Path p = GlassGraphCommand.write(ind(), level.getServer(), ind().district(f.da()));
                        record(t, "graph dump", p);
                        check("the dump is written", Files.size(p) > 0);
                    } catch (java.io.IOException e) {
                        throw new net.minecraft.gametest.framework.GameTestAssertException("the dump failed: " + e);
                    }
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    watch.check("the dump loaded B");
                    watch.on = false;
                });
        restore(s, level, f).thenSucceed();
    }

    /** The domain of the graph's millstone (the one machine of a district here). */
    private static GlassKit.Domain millDomain(GlassKit.Graph g) {
        var mills = g.nodes(GlassRecords.Node.MACHINE, GlassRecords.Model.MILLSTONE);
        check("one millstone: " + mills.size(), mills.size() == 1);
        GlassKit.Node m = mills.get(0);
        check("the millstone is in a domain", m.domain() != GlassRecords.NONE);
        for (GlassKit.Domain d : g.domains())
            if (d.index() == m.domain())
                return d;
        throw new net.minecraft.gametest.framework.GameTestAssertException("no domain " + m.domain());
    }

    private static void checkBounded(GlassKit.Graph g, GlassKit.Domain d) {
        int n = 0;
        for (GlassKit.Member m : d.members()) {
            if (m.node() == GlassRecords.NONE)
                continue;
            for (GlassKit.Node node : g.nodes())
                if (node.index() == m.node()) {
                    check("member node " + node.index() + " (kind " + node.kind() + ", model " + node.model() + ") is bounded",
                            node.has(GlassRecords.Node.BOUNDED));
                    n++;
                }
        }
        check("the domain has member nodes", n > 0);
    }

    /**
     * A package into a chunk that is FULL and does not tick (slice S1's half of §9's test; S4 adds the
     * package store). B is held FULL by its own ticket and does not tick; A ticks. Two packages on CA's
     * loop and one already on the chain to CB stay on A's side: the looping ones circle CA (its routes
     * time out, and the hand-off to a conveyor that does not tick is refused), the travelling one waits at
     * the chain's end. CB and FB never change. The motor is then reversed: the travelling package stays on
     * CA's side at the end (Create would move it to CB's side, a write into a chunk that does not tick).
     * When B ticks again, all three reach FB.
     */
    @GameTest(template = YARD, batch = "sec1_5", timeoutTicks = 12000)
    public static void aPackageIntoAChunkThatDoesNotTick(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 4800, true);
        String t = "aPackageIntoAChunkThatDoesNotTick";
        ChunkPos b = new ChunkPos(f.bx, f.az);
        int[] base = {-1, -1, -1};
        long[] routesGone = {-1, -1};
        float[] before = {0};
        boolean[] on = {false};
        ItemStack probeBox = box();
        Watch watch = new Watch(helper, () -> {
            if (level.shouldTickBlocksAt(b.toLong()))
                return "B ticks";
            int lb = looping(level, f.cb), tb = travelling(level, f.cb), hb = held(level, f.frogB);
            if (lb != base[0] || tb != base[1] || hb != base[2])
                return "B changed: CB " + lb + "/" + tb + ", FB " + hb + " from " + base[0] + "/" + base[1] + ", " + base[2];
            return null;
        });
        helper.onEachTick(() -> {
            if (!on[0] || routesGone[0] >= 0)
                return;
            ChainConveyorBlockEntity ca = conveyor(level, f.ca);
            if (ca != null && ca.routingTable.getExitFor(probeBox).equals(BlockPos.ZERO))
                routesGone[0] = level.getGameTime() - routesGone[1];
        });
        GameTestSequence s = build(helper, f, 64)
                .thenExecute(() -> conveyor(level, f.ca).addLoopingPackage(new ChainConveyorPackage(0, box())))
                .thenWaitUntil(() -> check("the first package reaches FB: " + held(level, f.frogB), held(level, f.frogB) == 1))
                .thenExecute(() -> {
                    level.getChunkSource().addRegionTicket(FULL_ONLY, b, 0, b);
                    level.getChunkSource().addRegionTicket(SEC1, f.a(), 1, f.a());
                    level.setChunkForced(f.ax, f.az, false);
                    level.setChunkForced(f.bx, f.az, false);
                })
                .thenWaitUntil(io(() -> check("B is FULL and does not tick", level.getChunkSource().getChunkNow(f.bx, f.az) != null
                        && !level.shouldTickBlocksAt(b.toLong()) && Loaded.ticks(level, f.ca))))
                .thenExecute(() -> {
                    base[0] = looping(level, f.cb);
                    base[1] = travelling(level, f.cb);
                    base[2] = held(level, f.frogB);
                    record(t, "B at rest: CB looping, travelling, FB", base[0] + ", " + base[1] + ", " + base[2]);
                    watch.on = true;
                    on[0] = true;
                    routesGone[1] = level.getGameTime();
                    ChainConveyorBlockEntity ca = conveyor(level, f.ca);
                    ca.addLoopingPackage(new ChainConveyorPackage(0, box()));
                    ca.addLoopingPackage(new ChainConveyorPackage(180, box()));
                    check("a package on the chain to CB", ca.addTravellingPackage(new ChainConveyorPackage(0, box()), f.cb.subtract(f.ca)));
                })
                .thenIdle(200)
                .thenExecute(() -> {
                    record(t, "routes to B gone after (ticks)", routesGone[0]);
                    record(t, "CA looping, travelling, the travelling one's position", looping(level, f.ca) + ", " + travelling(level, f.ca) + ", " + position(level, f));
                    check("two circle CA, one waits on the chain: " + looping(level, f.ca) + "/" + travelling(level, f.ca),
                            looping(level, f.ca) == 2 && travelling(level, f.ca) == 1);
                    before[0] = speedOf(level, f.ca);
                    ((CreativeMotorBlockEntity) Loaded.blockEntity(level, f.motor)).generatedSpeed.setValue(-64);
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    record(t, "reversed: CA speed, CB speed, CA looping, travelling, position", speedOf(level, f.ca) + ", " + speedOf(level, f.cb) + ", "
                            + looping(level, f.ca) + ", " + travelling(level, f.ca) + ", " + position(level, f));
                    check("CA turns the other way: " + before[0] + " to " + speedOf(level, f.ca), speedOf(level, f.ca) == -before[0] && before[0] != 0);
                    check("the travelling package stays on CA's side: " + travelling(level, f.ca), travelling(level, f.ca) == 1 && looping(level, f.ca) == 2);
                    watch.check("B did not change");
                    watch.on = false;
                    on[0] = false;
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, b, 0, b);
                    level.setChunkForced(f.bx, f.az, true);
                })
                .thenWaitUntil(() -> check("all four reach FB once B ticks: FB " + held(level, f.frogB) + ", CA " + looping(level, f.ca) + "/" + travelling(level, f.ca),
                        held(level, f.frogB) == 4 && looping(level, f.ca) == 0 && travelling(level, f.ca) == 0));
        restore(s, level, f).thenSucceed();
    }

    /** The chain position of the package travelling from CA to CB, and the chain's length; "none" if there is none. */
    private static String position(ServerLevel level, Adj f) {
        ChainConveyorBlockEntity ca = conveyor(level, f.ca);
        if (ca == null)
            return "none";
        BlockPos rel = f.cb.subtract(f.ca);
        List<ChainConveyorPackage> l = ca.getTravellingPackages().get(rel);
        if (l == null || l.isEmpty())
            return "none";
        ChainConveyorBlockEntity.ConnectionStats st = ca.connectionStats == null ? null : ca.connectionStats.get(rel);
        return l.get(0).chainPosition + " of " + (st == null ? "?" : st.chainLength());
    }

    /**
     * A boiler on a chain drives the next district, the driver part (SECTIONS.md §2.6, choice SC11): a
     * network whose members lie in more than one district is unstable in every district on it, the
     * boiler's district included. In A, a 2 x 2 x 1 fluid tank over one kindled blaze burner, filled with
     * 20 mB of water a tick, and a steam engine on it turn a shaft, a chain conveyor over it, a cog and a
     * millstone; the chain reaches CB in B, two chunks away, with a cog and a millstone. Both chunks are
     * forced. In both districts' compiles the millstone's domain has one level, no driver (not the
     * boiler's ladder of levels) and bounded members.
     */
    @GameTest(template = YARD, batch = "sec1_6", timeoutTicks = 12000)
    public static void aBoilerOnAChainDrivesTheNextDistrict(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aBoilerOnAChainDrivesTheNextDistrict";
        int ax = farChunk(helper, true, 4900), az = farChunk(helper, false, 4900), bx = ax + 2, z = az * 16 + 8;
        BlockPos burner = new BlockPos(ax * 16 + 2, Y - 1, z), tank = new BlockPos(ax * 16 + 2, Y, z);
        BlockPos engine = new BlockPos(ax * 16 + 4, Y, z), shaft = engine.east(2), ca = shaft.above(), cb = new BlockPos(bx * 16 + 3, Y + 1, z);
        BlockPos millA = ca.above().south(), millB = cb.above().south();
        long[] ids = new long[2];
        int[] compiles = {-1, -1};
        boolean[] water = {false};
        helper.onEachTick(() -> {
            if (!water[0])
                return;
            IFluidHandler h = Loaded.capability(level, Capabilities.FluidHandler.BLOCK, tank, null);
            if (h != null)
                h.fill(new FluidStack(Fluids.WATER, 20), IFluidHandler.FluidAction.EXECUTE);
        });
        helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(ax, az, true);
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(io(() -> check("A, M and B load", allLoaded(level, ax, bx, az))))
                .thenExecute(() -> {
                    clear(level, ax - 1, az - 1, bx + 1, az + 1);
                    SteamHelpers.burner(helper, burner);
                    SteamHelpers.topUpAsPlayer(helper, burner, Items.COAL, 4);
                    for (int dx = 0; dx <= 1; dx++)
                        for (int dz = 0; dz <= 1; dz++)
                            GlassCorpusKit.set(helper, tank.offset(dx, 0, dz), AllBlocks.FLUID_TANK.getDefaultState());
                    GlassCorpusKit.shaft(helper, shaft, Direction.Axis.Y);
                    GlassCorpusKit.set(helper, engine, AllBlocks.STEAM_ENGINE.getDefaultState()
                            .setValue(SteamEngineBlock.FACE, AttachFace.WALL).setValue(SteamEngineBlock.FACING, Direction.EAST));
                    GlassCorpusKit.set(helper, ca, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    GlassCorpusKit.set(helper, cb, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    for (BlockPos c : List.of(ca, cb)) {
                        GlassCorpusKit.cog(helper, c.above(), Direction.Axis.Y);
                        GlassCorpusKit.set(helper, c.above().south(), AllBlocks.MILLSTONE.getDefaultState());
                    }
                    join(level, ca, cb);
                    water[0] = true;
                    playerPlaced(helper, millA);
                    playerPlaced(helper, millB);
                })
                .thenWaitUntil(() -> {
                    District da = districtAt(helper, millA), db = districtAt(helper, millB);
                    check("both districts exist", da != null && db != null);
                    check("A and B are two districts", da.id != db.id);
                    ids[0] = da.id;
                    ids[1] = db.id;
                })
                .thenWaitUntil(() -> check("the boiler turns both millstones: A " + speedOf(level, millA) + ", B " + speedOf(level, millB),
                        speedOf(level, millA) != 0 && speedOf(level, millB) != 0))
                .thenExecute(() -> {
                    record(t, "engine shaft, CA, CB, millstones", speedOf(level, shaft) + ", " + speedOf(level, ca) + ", " + speedOf(level, cb) + ", "
                            + speedOf(level, millA) + ", " + speedOf(level, millB));
                    for (int i = 0; i < 2; i++) {
                        GlassKit.Graph g = GlassKit.latest(ids[i]);
                        compiles[i] = g == null ? -1 : g.end().compile();
                        ind().glass().requestCompile(ind().district(ids[i]));
                    }
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < 2; i++) {
                        GlassKit.Graph g = GlassKit.latest(ids[i]);
                        check("a new complete compile of " + (i == 0 ? "A" : "B"), g != null && g.complete() && g.end().compile() > compiles[i]);
                    }
                })
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        String which = i == 0 ? "A (the boiler's)" : "B";
                        GlassKit.Graph g = GlassKit.latest(ids[i]);
                        GlassKit.Domain d = millDomain(g);
                        record(t, "district " + which + ": graph, domain", g.describe() + "; " + d);
                        check(which + ": one level: " + d.levels(), d.levels().size() == 1);
                        check(which + ": no driver: " + d.driver(), d.driver() == GlassRecords.NONE);
                        checkBounded(g, d);
                    }
                    water[0] = false;
                    clear(level, ax - 1, az - 1, bx + 1, az + 1);
                    level.setChunkForced(ax, az, false);
                    level.setChunkForced(bx, az, false);
                })
                .thenSucceed();
    }
}
