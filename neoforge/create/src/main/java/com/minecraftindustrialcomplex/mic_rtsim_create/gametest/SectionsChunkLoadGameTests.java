package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassGraphCommand;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder;
import com.minecraftindustrialcomplex.mic_rtsim_create.KineticJoin;
import com.minecraftindustrialcomplex.mic_rtsim_create.KineticSyncPending;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.StaleKinetics;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.base.RotatedPillarKineticBlock;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorPackage;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.SteamEngineBlock;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
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
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * SEC-1 slice S1, no chunk loads (SECTIONS.md §2.8, §9 and §10.1; PATCHES.md rows 50 to 59): Create's chain
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
 * fully first, so a promotion is the load the watch sees; {@link #aDemotedChunkComesBackInStep} keeps B in
 * memory instead.)
 *
 * <p>Away chunks are read only through {@link Loaded}; {@code GlassCorpusKit.be} and {@code speed} load.
 *
 * <p>{@link #anEditOutsideEveryDistrictReachesTheDistrictOnItsNetwork} is slice S2d's (choice SC77), on the
 * same fixture with B in no district.
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

    /** A wait on the world's IO or the engine; while the check fails the tick sleeps 5 ms ({@link IndustryKit#paced}). */
    private static Runnable io(Runnable check) {
        return IndustryKit.paced(check);
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

    /** Whether the chunks from {@code minCx} to {@code maxCx} in row {@code cz} tick (at their middle, at {@code Y}). */
    private static boolean allTick(ServerLevel level, int minCx, int maxCx, int cz) {
        for (int cx = minCx; cx <= maxCx; cx++)
            if (!Loaded.ticks(level, new BlockPos(cx * 16 + 8, Y, cz * 16 + 8)))
                return false;
        return true;
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
        return away(s, level, f, "", () -> true);
    }

    /**
     * As {@link #away(GameTestSequence, ServerLevel, Adj)}, and waits for {@code also} with A and B before A's
     * ticket: a chunk pending unload within reach of the ticket (level 44) comes back instead.
     */
    private static GameTestSequence away(GameTestSequence s, ServerLevel level, Adj f, String alsoName, BooleanSupplier also) {
        return s.thenExecute(() -> {
                    level.setChunkForced(f.ax, f.az, false);
                    level.setChunkForced(f.bx, f.az, false);
                })
                .thenWaitUntil(io(() -> check("A and B unload: A " + loadedChunks(f.da()) + ", B " + loadedChunks(f.db()) + alsoName,
                        unloaded(level, f.ax, f.az, f.da()) && unloaded(level, f.bx, f.az, f.db()) && also.getAsBoolean())))
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
     * (PATCHES.md rows 55 and 56). With B away, the motor in A is set to 128, 32, -64 and -32 RPM; CA follows
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
     * A network's recount keeps an away source (PATCHES.md rows 53, 54 and 57). Eight conveyors 30 blocks
     * apart reach from A over 13 chunks to B, where a creative motor turns them; both ends have a cog and a
     * millstone. With only A's radius-1 ticket left, B (13 chunks away) unloads and the conveyors between
     * are held below FULL. The recount (Create's {@code calculateCapacity}) keeps the motor as it last was,
     * without loading B: the capacity is unchanged and the motor still listed, and the loaded half keeps
     * turning at the motor's speed, not overstressed, for 600 ticks and after the recount below (slice
     * S5-1, SECTIONS.md §2.6). A millstone added in A
     * makes the network recount its stress, with the same answer for the capacity. B then loads FULL and
     * does not tick: its fresh motor exists and has not joined, and a recount still keeps the kept motor.
     * When B is forced, the fresh motor takes the kept one's place as it joins: one source, with no recount
     * in between, the network's unloaded share (set by hand, as a network loaded from a save has one) left
     * alone, and the network's current stress, not the one B saved at the unload. B then unloads a second
     * time and another millstone in A makes the network recount: one motor kept, the same capacity (without
     * row 57, the motors of both visits were listed and counted). On a third visit B is FULL and does not
     * tick, and the fresh motor's generator update ({@code updateGeneratedRotation}, as a neighbour change
     * runs it) lists it before its first tick: it takes the kept one's place, with the same capacity
     * (without row 57's update hooks, both were counted until the next recount). On a fourth visit the next
     * fresh motor joins at a position that joined before: the unloaded share is left alone (without the
     * joined positions, the recount after the update forgot the position and Create took the motor's share
     * out a second time).
     */
    @GameTest(template = YARD, batch = "sec1_3", timeoutTicks = 17000)
    public static void aNetworkRecountKeepsAnUnloadedSource(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aNetworkRecountKeepsAnUnloadedSource";
        int ax = farChunk(helper, true, 4600), az = farChunk(helper, false, 4600), bx = ax + 13, z = az * 16 + 8;
        ChunkPos a = new ChunkPos(ax, az), b = new ChunkPos(bx, az);
        List<BlockPos> cs = new ArrayList<>();
        for (int k = 0; k < 8; k++)
            cs.add(new BlockPos(ax * 16 + 8 + 30 * k, Y, z));
        BlockPos c0 = cs.get(0), c7 = cs.get(7), motor = c7.below(), millA = c0.above().south(), millB = c7.above().south();
        BlockPos added = c0.above().north(), added2 = c0.above().east();
        long[] ids = new long[2];
        float[] cap = new float[3], stress = new float[2], joined = new float[2];
        KineticBlockEntity[] oldMotor = {null, null};
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
                // forced chunks come FULL before they tick, under load by many ticks: a part placed in one that
                // does not tick yet attaches at its first tick
                .thenWaitUntil(io(() -> check("the fourteen chunks tick", allTick(level, ax, bx, az))))
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
                .thenWaitUntil(io(() -> {
                    District da = districtAt(helper, millA), db = districtAt(helper, millB);
                    check("both districts exist", da != null && db != null && da.id != db.id);
                    ids[0] = da.id;
                    ids[1] = db.id;
                }))
                .thenWaitUntil(io(() -> {
                    KineticNetwork n = net.get();
                    check("the motor in B turns millstone A: " + speedOf(level, millA) + " (motor " + speedOf(level, motor)
                            + ", capacity " + n.calculateCapacity() + ", stress " + n.calculateStress() + ", sources "
                            + n.sources.size() + ", members " + n.members.size() + ", A and B tick " + Loaded.ticks(level, millA)
                            + ", " + Loaded.ticks(level, motor) + ", test tick " + helper.getTick() + ")",
                            Math.abs(speedOf(level, millA)) == 64);
                }))
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
                    loadedHalfTurns(level, millA, "with B away");
                })
                // the loaded half keeps its speed and the away motor's capacity for as long as B stays away (slice S5-1,
                // SECTIONS.md §2.6)
                .thenIdle(600)
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    float c = n.calculateCapacity();
                    record(t, "B away 600 ticks: millstone A's speed, capacity, stress, overstressed",
                            speedOf(level, millA) + ", " + c + ", " + n.calculateStress() + ", " + kinetic(level, millA).isOverStressed());
                    loadedHalfTurns(level, millA, "600 ticks after B went away");
                    check("the same capacity 600 ticks on: " + c + " vs " + cap[0], c == cap[0]);
                    check("the away motor is still listed 600 ticks on", n.sources.containsKey(oldMotor[0]));
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
                    loadedHalfTurns(level, millA, "after the recount");
                    watch.check("no chunk two or more from A loaded");
                    watch.on = false;
                    // B's chunk loads FULL and does not tick: its block entities exist and have not joined
                    level.getChunkSource().addRegionTicket(FULL_ONLY, b, 0, b);
                })
                .thenWaitUntil(io(() -> {
                    KineticBlockEntity fresh = kinetic(level, motor);
                    check("the fresh motor loads", fresh != null && fresh != oldMotor[0]);
                }))
                .thenExecute(() -> {
                    KineticBlockEntity fresh = kinetic(level, motor);
                    KineticNetwork n = net.get();
                    check("B's chunk does not tick", !Loaded.ticks(level, motor));
                    check("the fresh motor has not joined", !n.members.containsKey(fresh));
                    // a recount before the fresh motor's first tick keeps the kept one, the motor's capacity with it
                    float c = n.calculateCapacity();
                    n.calculateStress();
                    record(t, "B FULL, not ticking: capacity, sources", c + ", " + n.sources.size());
                    check("the recount keeps the kept motor: " + c + " vs " + cap[0], c == cap[0] && n.sources.containsKey(oldMotor[0]));
                    // as a network loaded from a save with parts still unloaded: the join must leave that share alone
                    unloadedShare(n, new float[] {100f, 50f, 3f});
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, b, 0, b);
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(io(() -> {
                    KineticBlockEntity fresh = kinetic(level, motor);
                    KineticNetwork n = net.get();
                    check("the fresh motor joins the network", fresh != null && fresh != oldMotor[0] && n.sources.containsKey(fresh));
                    // in the tick it is first seen joined, before a later recount or sync could set it
                    joined[0] = field(fresh, KineticBlockEntity.class, "stress");
                    joined[1] = field(n, KineticNetwork.class, "currentStress");
                }))
                .thenExecute(() -> {
                    // no recount here: the kept motor left the network as the fresh one joined
                    KineticNetwork n = net.get();
                    oldMotor[1] = kinetic(level, motor);
                    float[] u = unloadedShare(n, new float[] {0f, 0f, 0f});
                    record(t, "the unloaded share after the join (set 100, 50, 3)", u[0] + ", " + u[1] + ", " + u[2]);
                    check("the join leaves the unloaded share: " + u[0] + ", " + u[1] + ", " + u[2], u[0] == 100f && u[1] == 50f && u[2] == 3f);
                    float have = joined[0], want = joined[1];
                    record(t, "the fresh motor's stress as it joined, the network's (and at the unload)", have + ", " + want + " (" + stress[0] + ")");
                    check("the network's stress moved while the motor was away: " + want + " vs " + stress[0], want != stress[0]);
                    check("the fresh motor takes the network's current stress: " + have + " vs " + want, have == want);
                    record(t, "B loaded again: sources, members, distinct positions", n.sources.size() + ", " + n.members.size() + ", " + positions(n));
                    check("one source again: " + n.sources.size(), n.sources.size() == 1 && !n.sources.containsKey(oldMotor[0]));
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
                    level.setChunkForced(bx, az, false);
                })
                .thenWaitUntil(io(() -> check("B unloads a second time: " + loadedChunks(ids[1]), unloaded(level, bx, az, ids[1]))))
                .thenWaitUntil(io(() -> check("the chunks two or more from A are below FULL", !anyLoaded(level, ax + 2, bx, az))))
                .thenExecute(() -> {
                    mark[0] = loadedCount(ids[1], bx, az);
                    watch.on = true;
                    GlassCorpusKit.set(helper, added2, AllBlocks.MILLSTONE.getDefaultState());
                })
                .thenWaitUntil(() -> check("the second added millstone turns: " + speedOf(level, added2), Math.abs(speedOf(level, added2)) == 64))
                .thenIdle(20)
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    float c = n.calculateCapacity();
                    record(t, "B away again: capacity, sources, members, distinct positions",
                            c + ", " + n.sources.size() + ", " + n.members.size() + ", " + positions(n));
                    check("one motor kept, the second visit's: " + n.sources.size(),
                            n.sources.size() == 1 && n.sources.containsKey(oldMotor[1]));
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
                    check("the same capacity: " + c + " vs " + cap[0], c == cap[0]);
                    watch.check("no chunk two or more from A loaded the second time");
                    watch.on = false;
                    level.getChunkSource().addRegionTicket(FULL_ONLY, b, 0, b);
                })
                .thenWaitUntil(io(() -> {
                    KineticBlockEntity fresh = kinetic(level, motor);
                    check("the third motor loads", fresh != null && fresh != oldMotor[1]);
                }))
                .thenExecute(() -> {
                    KineticBlockEntity fresh = kinetic(level, motor);
                    KineticNetwork n = net.get();
                    check("B's chunk does not tick", !Loaded.ticks(level, motor));
                    check("the third motor has not joined", !n.members.containsKey(fresh));
                    // a neighbour change runs this on a generator in a chunk that is FULL and does not tick
                    ((CreativeMotorBlockEntity) fresh).updateGeneratedRotation();
                    float c = n.calculateCapacity();
                    record(t, "a generator update before the first tick: capacity, sources, members, distinct positions",
                            c + ", " + n.sources.size() + ", " + n.members.size() + ", " + positions(n));
                    check("the third motor takes the kept one's place: " + n.sources.size() + " sources",
                            n.sources.size() == 1 && n.sources.containsKey(fresh) && n.members.containsKey(fresh) && !n.members.containsKey(oldMotor[1]));
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
                    check("the same capacity: " + c + " vs " + cap[0], c == cap[0]);
                    oldMotor[1] = fresh;
                    unloadedShare(n, new float[] {100f, 50f, 3f});
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, b, 0, b);
                    level.setChunkForced(bx, az, true);
                })
                .thenWaitUntil(io(() -> check("B ticks", Loaded.ticks(level, motor))))
                .thenIdle(5)
                .thenExecute(() -> {
                    // its first tick: Create's addSilently finds it a member and returns
                    float[] u = unloadedShare(net.get(), new float[] {100f, 50f, 3f});
                    check("the third motor's join leaves the unloaded share: " + u[0] + ", " + u[1] + ", " + u[2], u[0] == 100f && u[1] == 50f && u[2] == 3f);
                    level.setChunkForced(bx, az, false);
                })
                .thenWaitUntil(io(() -> check("B unloads a third time: " + loadedChunks(ids[1]), unloaded(level, bx, az, ids[1]))))
                .thenWaitUntil(io(() -> check("the chunks two or more from A are below FULL", !anyLoaded(level, ax + 2, bx, az))))
                .thenExecute(() -> level.setChunkForced(bx, az, true))
                .thenWaitUntil(io(() -> {
                    KineticBlockEntity fresh = kinetic(level, motor);
                    check("the fourth motor joins", fresh != null && fresh != oldMotor[1] && net.get().sources.containsKey(fresh));
                }))
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    float[] u = unloadedShare(n, new float[] {0f, 0f, 0f});
                    record(t, "the unloaded share after the fourth join (set 100, 50, 3)", u[0] + ", " + u[1] + ", " + u[2]);
                    check("a join where one joined before leaves the unloaded share: " + u[0] + ", " + u[1] + ", " + u[2],
                            u[0] == 100f && u[1] == 50f && u[2] == 3f);
                    check("one source: " + n.sources.size(), n.sources.size() == 1);
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
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

    /** A member of the loaded half turns at the away motor's 64 RPM and is not overstressed. */
    private static void loadedHalfTurns(ServerLevel level, BlockPos member, String when) {
        KineticBlockEntity k = kinetic(level, member);
        check("the loaded half turns " + when + ": " + speedOf(level, member), k != null && Math.abs(speedOf(level, member)) == 64);
        check("the loaded half is not overstressed " + when, !k.isOverStressed());
    }

    /**
     * A network's unloaded capacity, stress and member count (private in Create), set to {@code set}; the
     * values before.
     */
    private static float[] unloadedShare(KineticNetwork n, float[] set) {
        float[] was = {field(n, KineticNetwork.class, "unloadedCapacity"), field(n, KineticNetwork.class, "unloadedStress"), 0};
        try {
            var m = KineticNetwork.class.getDeclaredField("unloadedMembers");
            m.setAccessible(true);
            was[2] = m.getInt(n);
            m.setInt(n, (int) set[2]);
            for (String f : List.of("unloadedCapacity", "unloadedStress")) {
                var x = KineticNetwork.class.getDeclaredField(f);
                x.setAccessible(true);
                x.setFloat(n, f.equals("unloadedCapacity") ? set[0] : set[1]);
            }
        } catch (ReflectiveOperationException e) {
            throw new net.minecraft.gametest.framework.GameTestAssertException("Create moved a network field: " + e);
        }
        return was;
    }

    /** A private float field of Create's (the test only). */
    private static float field(Object o, Class<?> c, String name) {
        try {
            var f = c.getDeclaredField(name);
            f.setAccessible(true);
            return f.getFloat(o);
        } catch (ReflectiveOperationException e) {
            throw new net.minecraft.gametest.framework.GameTestAssertException("Create moved " + name + ": " + e);
        }
    }

    /** The distinct positions a network lists. */
    private static int positions(KineticNetwork n) {
        java.util.Set<BlockPos> ps = new java.util.HashSet<>();
        for (KineticBlockEntity m : n.members.keySet())
            ps.add(m.getBlockPos());
        return ps.size();
    }

    private static boolean anyLoaded(ServerLevel level, int minCx, int maxCx, int cz) {
        for (int cx = minCx; cx <= maxCx; cx++)
            if (level.getChunkSource().getChunkNow(cx, cz) != null)
                return true;
        return false;
    }

    /**
     * A compile of a district whose network reaches into an away district loads nothing (SECTIONS.md §2.8,
     * choice SC10; PATCHES.md row 53's kept members). The motor is in B; with B away, A's compile reads CA's
     * chain into B as nothing (counted as stops at unloaded chunks), and the network, which has members
     * outside A, is a crossing one, so a shared part (SEC-1 S5-2, choice SC153): millstone A's domain is
     * SHARED with B as its only peer, the motor a foreign source (B's chunks hold it: no rows, no driver, its
     * ratio known), one speed row at 64 RPM (the motor's candidate), and the members are not bounded.
     * {@code /rtsim district graph} writes the compile without loading anything either. Then B's block
     * entities are taken out of the network, as a restart with B away leaves it (the network lists only what
     * loaded this session): the next compile still finds the network crossing (choice SC44: CA's chain reaches
     * B, which is not loaded), but no listed source has a known ratio, so the domain is unstable as before: one
     * row at the speed of A's members, no driver, bounded, not shared.
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
                .thenWaitUntil(io(() -> {
                    GlassKit.Graph g = GlassKit.latest(f.da());
                    check("a new complete compile of A", g != null && g.complete() && g.end().compile() > mark[1]);
                }))
                .thenExecute(() -> {
                    watch.check("the compile loaded B");
                    GlassKit.Graph g = GlassKit.latest(f.da());
                    GraphBuilder java = ind().glass().lastGraph(f.da());
                    record(t, "graph", g.describe() + "; stops at unloaded chunks " + (java == null ? "?" : java.unloadedStops));
                    check("the Java side kept the compile", java != null);
                    check("the reads stopped at B: " + java.unloadedStops, java.unloadedStops > 0);
                    GlassKit.Domain d = millDomain(g);
                    record(t, "millstone A's domain", d);
                    GlassKit.Source motor = d.source(f.motor.asLong());
                    record(t, "the motor's source, peers, current", motor + ", " + d.peers() + ", " + d.current());
                    check("a shared part: " + d, d.shared());
                    check("its peer is B: " + d.peers() + " for " + f.db(), d.peers().equals(List.of(f.db())));
                    check("one level: " + d.levels(), d.levels().size() == 1);
                    check("at the motor's speed: " + d.levels().get(0).rpm(), Math.abs(d.levels().get(0).rpm()) == 64);
                    check("and turning at it now: " + d.current(), d.current() != GlassRecords.NONE);
                    check("no driver: " + d.sources(), d.driven().isEmpty());
                    check("the motor is a foreign source: " + motor, motor != null && motor.rows().isEmpty() && motor.driver() == GlassRecords.NONE
                            && motor.current() == GlassRecords.NONE && motor.ratioKnown());
                    check("no tail: " + d.foreign(), d.foreign() == 0);
                    checkNotBounded(g, d);
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
                    KineticNetwork n = kinetic(level, f.ca).getOrCreateNetwork();
                    int bx = f.bx;
                    n.members.keySet().removeIf(m -> m.getBlockPos().getX() >> 4 == bx);
                    n.sources.keySet().removeIf(m -> m.getBlockPos().getX() >> 4 == bx);
                    // and the mixin's record of who joined where (PATCHES.md row 57), as a restart's new network has none
                    try {
                        var byPos = KineticNetwork.class.getDeclaredField("mic_rtsim$byPos");
                        byPos.setAccessible(true);
                        byPos.set(n, null);
                    } catch (ReflectiveOperationException e) {
                        throw new net.minecraft.gametest.framework.GameTestAssertException("no per-position record: " + e);
                    }
                    record(t, "as after a restart: sources, members, size", n.sources.size() + ", " + n.members.size() + ", " + n.getSize());
                    check("nothing of B is listed", n.sources.isEmpty() && n.getSize() == n.members.size());
                    GlassKit.Graph g = GlassKit.latest(f.da());
                    mark[1] = g == null ? -1 : g.end().compile();
                    ind().glass().requestCompile(ind().district(f.da()));
                })
                .thenWaitUntil(io(() -> {
                    GlassKit.Graph g = GlassKit.latest(f.da());
                    check("a new complete compile of A", g != null && g.complete() && g.end().compile() > mark[1]);
                }))
                .thenExecute(() -> {
                    watch.check("the second compile loaded B");
                    watch.on = false;
                    GlassKit.Domain d = millDomain(GlassKit.latest(f.da()));
                    record(t, "millstone A's domain, B unlisted", d);
                    check("no source is listed: " + d.sources(), d.sources().isEmpty());
                    check("not a shared part: " + d, !d.shared() && d.peers().isEmpty() && d.foreign() == 0);
                    check("still one level: " + d.levels(), d.levels().size() == 1);
                    check("at the speed of A's members: " + d.levels().get(0).rpm(), Math.abs(d.levels().get(0).rpm()) == 64);
                    check("still no driver: " + d.sources(), d.driven().isEmpty());
                    checkBounded(GlassKit.latest(f.da()), d);
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
        checkMembers(g, d, true);
    }

    /** The domain's member nodes are modelled, not bounded (a shared part: its network is modelled with its peers). */
    private static void checkNotBounded(GlassKit.Graph g, GlassKit.Domain d) {
        checkMembers(g, d, false);
    }

    private static void checkMembers(GlassKit.Graph g, GlassKit.Domain d, boolean bounded) {
        int n = 0;
        for (GlassKit.Member m : d.members()) {
            if (m.node() == GlassRecords.NONE)
                continue;
            for (GlassKit.Node node : g.nodes())
                if (node.index() == m.node()) {
                    check("member node " + node.index() + " (kind " + node.kind() + ", model " + node.model() + ") is " + (bounded ? "" : "not ") + "bounded",
                            node.has(GlassRecords.Node.BOUNDED) == bounded);
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
     * A boiler on a chain drives the next district, the loaded half (SECTIONS.md §2.6, SEC-1 S5-2, choice
     * SC153): a network whose members lie in more than one district is a shared part in every district on it,
     * the boiler's district included. In A, a 2 x 2 x 1 fluid tank over one kindled blaze burner, filled with
     * 20 mB of water a tick, and a steam engine on it turn a shaft, a chain conveyor over it, a cog and a
     * millstone; the chain reaches CB in B, two chunks away, with a cog and a millstone. Both chunks are
     * forced. In both districts' compiles the millstone's domain is SHARED and names the other district as its
     * only peer. A's part tabulates the engine's shaft as a driven source (19 boiler levels and the boiler as
     * its driver) and has more than one speed row; B's lists the same shaft as a foreign source (no rows, no
     * driver, its ratio known), has a speed row for each speed an engine gives, and turns at one of them. Each
     * lists only the members in its own chunks (the middle chunk holds no kinetic block, so the tail is
     * empty), and the millstones' nodes are not bounded.
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
                .thenWaitUntil(io(() -> {
                    for (int i = 0; i < 2; i++) {
                        GlassKit.Graph g = GlassKit.latest(ids[i]);
                        check("a new complete compile of " + (i == 0 ? "A" : "B"), g != null && g.complete() && g.end().compile() > compiles[i]);
                    }
                }))
                .thenExecute(() -> {
                    // the members Create's network holds in each district's chunk, by position
                    var network = kinetic(level, ca).getOrCreateNetwork();
                    LongOpenHashSet inA = new LongOpenHashSet(), inB = new LongOpenHashSet(), elsewhere = new LongOpenHashSet();
                    for (KineticBlockEntity m : network.members.keySet()) {
                        int cx = m.getBlockPos().getX() >> 4;
                        (cx == ax ? inA : cx == bx ? inB : elsewhere).add(m.getBlockPos().asLong());
                    }
                    record(t, "network members in A, in B, elsewhere", inA.size() + ", " + inB.size() + ", " + elsewhere.size());
                    check("the network has members in A and B and none between: " + inA.size() + "/" + inB.size() + "/" + elsewhere.size(),
                            !inA.isEmpty() && !inB.isEmpty() && elsewhere.isEmpty());
                    for (int i = 0; i < 2; i++) {
                        String which = i == 0 ? "A (the boiler's)" : "B";
                        GlassKit.Graph g = GlassKit.latest(ids[i]);
                        GlassKit.Domain d = millDomain(g);
                        record(t, "district " + which + ": graph, domain", g.describe() + "; " + d);
                        check(which + ": a shared part: " + d, d.shared());
                        check(which + ": the other district is its one peer: " + d.peers(), d.peers().equals(List.of(ids[1 - i])));
                        GlassKit.Source engineShaft = d.source(shaft.asLong());
                        record(t, which + ": the engine shaft's source, speed rows, current", engineShaft + "; " + d.levels() + "; " + d.current());
                        check(which + ": the engine's shaft is a source: " + d.sources(), engineShaft != null);
                        if (i == 0) {
                            check(which + ": the shaft has a row for each boiler level: " + engineShaft.rows().size(), engineShaft.rows().size() == 19);
                            check(which + ": and the boiler as its driver: " + d.sources(), !d.driven().isEmpty() && engineShaft.driver() != GlassRecords.NONE);
                            check(which + ": more than one speed row: " + d.levels(), d.levels().size() > 1);
                        } else {
                            check(which + ": the shaft is a foreign source: " + engineShaft, engineShaft.rows().isEmpty()
                                    && engineShaft.driver() == GlassRecords.NONE && engineShaft.current() == GlassRecords.NONE && engineShaft.ratioKnown());
                            check(which + ": no driver of its own: " + d.sources(), d.driven().isEmpty());
                            check(which + ": a speed row at least: " + d.levels(), !d.levels().isEmpty());
                            check(which + ": turning at one of them: " + d.current(), d.current() != GlassRecords.NONE);
                        }
                        // the members are the district's own: all of those in its chunks, none in the other's, no tail
                        int listed = 0;
                        for (GlassKit.Member m : d.ownMembers())
                            listed += m.count();
                        record(t, which + ": members listed, tail", listed + ", " + d.foreign());
                        check(which + ": every member in its chunks and no other: " + listed + " of " + (i == 0 ? inA : inB).size(),
                                listed == (i == 0 ? inA : inB).size());
                        check(which + ": no tail: " + d.tail(), d.foreign() == 0 && d.tail().isEmpty());
                        checkNotBounded(g, d);
                    }
                    water[0] = false;
                    clear(level, ax - 1, az - 1, bx + 1, az + 1);
                    level.setChunkForced(ax, az, false);
                    level.setChunkForced(bx, az, false);
                })
                .thenSucceed();
    }

    /**
     * A chunk that drops below FULL and comes back without loading is re-synced (PATCHES.md rows 54 and 58;
     * SECTIONS.md §2.8, choices SC37 and SC45). With A's radius-1 ticket and a FULL-only ticket on B, both
     * unforced, B is FULL and does not tick. The FULL-only ticket goes: B drops to level 34 and stays in
     * memory with its block entities. The motor in A is set to 128 RPM: CA follows, CB keeps 64 (rotation
     * stops at B), and a network sync skips CB and marks it. The ticket comes back: B is FULL again with the
     * same block entities (no load: no IND_CHUNK_LOADED), and CB gets the held-back update and CA's speed
     * (without row 58 nothing attaches, and CB stays at 64 on CA's 128). Reversed to -64 RPM while B is FULL,
     * CB follows and nothing breaks; forced, B ticks and turns millstone B on CA's network.
     */
    @GameTest(template = YARD, batch = "sec1_7", timeoutTicks = 12000)
    public static void aDemotedChunkComesBackInStep(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 5000, true);
        ChunkPos b = new ChunkPos(f.bx, f.az);
        String t = "aDemotedChunkComesBackInStep";
        KineticBlockEntity[] cb = {null};
        float[] before = {0};
        int[] mark = {0, 0};
        Watch watch = new Watch(helper, () -> {
            int loads = loadedCount(f.db(), f.bx, f.az);
            return loads != mark[0] ? "IND_CHUNK_LOADED for B: " + loads + " after " + mark[0] : null;
        });
        Supplier<Float> motorSpeed = () -> kinetic(level, f.motor).getGeneratedSpeed();
        GameTestSequence s = build(helper, f, 64)
                .thenExecute(() -> {
                    level.getChunkSource().addRegionTicket(SEC1, f.a(), 1, f.a());
                    level.getChunkSource().addRegionTicket(FULL_ONLY, b, 0, b);
                    level.setChunkForced(f.ax, f.az, false);
                    level.setChunkForced(f.bx, f.az, false);
                })
                .thenWaitUntil(io(() -> check("A ticks, B is FULL and does not tick",
                        Loaded.ticks(level, f.ca) && Loaded.at(level, f.cb) && !Loaded.ticks(level, f.cb))))
                .thenExecute(() -> {
                    cb[0] = kinetic(level, f.cb);
                    check("CB turns with CA", cb[0] != null && cb[0].getSpeed() != 0 && cb[0].getSpeed() == speedOf(level, f.ca));
                    before[0] = cb[0].getSpeed();
                    mark[0] = loadedCount(f.db(), f.bx, f.az);
                    watch.on = true;
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, b, 0, b);
                })
                .thenWaitUntil(io(() -> check("B drops below FULL", level.getChunkSource().getChunkNow(f.bx, f.az) == null)))
                .thenExecute(() -> ((CreativeMotorBlockEntity) Loaded.blockEntity(level, f.motor)).generatedSpeed.setValue(128))
                .thenIdle(60)
                .thenExecute(() -> {
                    record(t, "B below FULL, motor 128: CA, CB", speedOf(level, f.ca) + ", " + cb[0].getSpeed());
                    check("CA follows the motor: " + speedOf(level, f.ca), speedOf(level, f.ca) == motorSpeed.get() && Math.abs(motorSpeed.get()) == 128);
                    check("CB keeps its speed: " + cb[0].getSpeed(), cb[0].getSpeed() == before[0]);
                    check("B is still below FULL", level.getChunkSource().getChunkNow(f.bx, f.az) == null);
                    kinetic(level, f.ca).getOrCreateNetwork().sync();
                    check("the sync skipped CB and marked it", ((KineticSyncPending) cb[0]).mic_rtsim$syncPending());
                    mark[1] = StaleKinetics.resyncs();
                    level.getChunkSource().addRegionTicket(FULL_ONLY, b, 0, b);
                })
                .thenWaitUntil(io(() -> check("B is FULL again and CB re-synced: CB " + cb[0].getSpeed() + ", CA " + speedOf(level, f.ca),
                        Loaded.at(level, f.cb) && StaleKinetics.resyncs() > mark[1])))
                .thenExecute(() -> {
                    KineticBlockEntity now = kinetic(level, f.cb), ca = kinetic(level, f.ca);
                    record(t, "B FULL again: CB, CA, re-syncs, same object", cb[0].getSpeed() + ", " + ca.getSpeed() + ", "
                            + (StaleKinetics.resyncs() - mark[1]) + ", " + (now == cb[0]));
                    check("the same block entity: B did not load", now == cb[0]);
                    check("B does not tick", !Loaded.ticks(level, f.cb));
                    check("CB has CA's speed: CB " + cb[0].getSpeed() + ", CA " + ca.getSpeed(), cb[0].getSpeed() == ca.getSpeed());
                    check("the held-back update was given", !((KineticSyncPending) cb[0]).mic_rtsim$syncPending());
                    check("one network: " + ca.network + ", " + cb[0].network, ca.network != null && ca.network.equals(cb[0].network));
                    ((CreativeMotorBlockEntity) Loaded.blockEntity(level, f.motor)).generatedSpeed.setValue(-64);
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    record(t, "reversed: CA, CB", speedOf(level, f.ca) + ", " + speedOf(level, f.cb));
                    check("CB is not broken: " + level.getBlockState(f.cb), kinetic(level, f.cb) == cb[0] && !cb[0].isRemoved());
                    check("CA and CB follow the motor: " + speedOf(level, f.ca) + ", " + speedOf(level, f.cb),
                            speedOf(level, f.ca) == motorSpeed.get() && speedOf(level, f.cb) == speedOf(level, f.ca) && Math.abs(motorSpeed.get()) == 64);
                    watch.check("B did not load");
                    watch.on = false;
                    level.setChunkForced(f.bx, f.az, true);
                })
                .thenWaitUntil(io(() -> check("B ticks", Loaded.ticks(level, f.cb))))
                .thenIdle(20)
                .thenExecute(() -> {
                    KineticBlockEntity ca = kinetic(level, f.ca), now = kinetic(level, f.cb);
                    record(t, "B forced: CB, millstone B", speedOf(level, f.cb) + ", " + speedOf(level, f.millB));
                    check("millstone B turns at 64 RPM: " + speedOf(level, f.millB), Math.abs(speedOf(level, f.millB)) == 64);
                    check("CB on CA's network", now != null && ca.network != null && ca.network.equals(now.network));
                });
        restore(s, level, f).thenSucceed();
    }

    /**
     * A chunk that loads anew FULL and does not tick is re-synced at the load (PATCHES.md rows 56 and 58;
     * SECTIONS.md §2.8, choice SC73). The chain goes on from CB to CC in C, the chunk past B, with a cog and a
     * millstone on CC. With B and C away, the motor in A is reversed to -64 RPM: CA follows, and CB and CC keep
     * 64 in their saved state. B's and C's block entities are taken out of the network and their share put in
     * its unloaded share, as a restart with them away leaves it. A FULL-only ticket loads C: C is FULL, does
     * not tick, and nothing there joins (CC's source, CB, is not loaded). A FULL-only ticket loads B: nothing
     * there attaches, so row 56's re-sync at the first attach never runs; row 58 re-syncs B at its FULL report.
     * CB, and CC past it, take CA's -64 RPM before either has attached, and every block entity of B and C
     * joins CA's network once: the unloaded share drops by exactly their share (without the join where Create
     * lists them again, C's block entities, which the re-sync reaches from B, stayed in the unloaded share as
     * well as in the members). Forced, B and C tick: the first attaches find nothing stale, nothing breaks,
     * and both millstones turn.
     */
    @GameTest(template = YARD, batch = "sec1_8", timeoutTicks = 16000)
    public static void aChunkLoadedFullComesBackInStep(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 5100, true);
        int cx = f.bx + 1;
        ChunkPos b = new ChunkPos(f.bx, f.az), c = new ChunkPos(cx, f.az);
        // C's conveyor north of the line CB to CA, so its chain passes clear of the frogport on CB's loop
        BlockPos cc = new BlockPos(cx * 16 + 3, Y, f.az * 16), millC = cc.above().south();
        List<BlockPos> parts = List.of(f.cb, f.cb.above(), f.millB, cc, cc.above(), millC);
        String t = "aChunkLoadedFullComesBackInStep";
        KineticBlockEntity[] old = {null}, fresh = new KineticBlockEntity[parts.size()];
        float[] share = {0, 0, 0};
        int[] mark = {0};
        Supplier<Float> motorSpeed = () -> kinetic(level, f.motor).getGeneratedSpeed();
        Supplier<KineticNetwork> net = () -> kinetic(level, f.ca).getOrCreateNetwork();
        GameTestSequence built = build(helper, f, 64)
                .thenExecute(() -> {
                    level.setChunkForced(cx, f.az, true);
                    GlassCorpusKit.set(helper, cc, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    GlassCorpusKit.cog(helper, cc.above(), Direction.Axis.Y);
                    GlassCorpusKit.set(helper, millC, AllBlocks.MILLSTONE.getDefaultState());
                    join(level, f.cb, cc);
                })
                .thenWaitUntil(() -> check("the chain turns millstone C: " + speedOf(level, millC),
                        Math.abs(speedOf(level, millC)) == 64 && speedOf(level, cc) == speedOf(level, f.cb)))
                .thenExecute(() -> {
                    old[0] = kinetic(level, cc);
                    level.setChunkForced(cx, f.az, false);
                });
        GameTestSequence s = away(built, level, f, ", and C",
                        () -> old[0].isRemoved() && level.getChunkSource().getChunkNow(cx, f.az) == null)
                .thenExecute(() -> ((CreativeMotorBlockEntity) Loaded.blockEntity(level, f.motor)).generatedSpeed.setValue(-64))
                .thenIdle(60)
                .thenExecute(() -> {
                    record(t, "B and C away, motor -64: CA", speedOf(level, f.ca));
                    check("CA follows the motor: " + speedOf(level, f.ca), speedOf(level, f.ca) == motorSpeed.get() && motorSpeed.get() == -64);
                    check("B and C are still away", level.getChunkSource().getChunkNow(f.bx, f.az) == null && level.getChunkSource().getChunkNow(cx, f.az) == null);
                    KineticNetwork n = net.get();
                    System.arraycopy(forgetAway(n, f.bx, cx, f.az), 0, share, 0, 3);
                    record(t, "as after a restart: B's and C's share (capacity, stress, members)", share[0] + ", " + share[1] + ", " + share[2]);
                    check("B and C hold six members with stress: " + share[2] + ", " + share[1], share[2] == 6 && share[1] > 0 && share[0] == 0);
                    unloadedShare(n, new float[] {100f, 50f + share[1], 3f + share[2]});
                    level.getChunkSource().addRegionTicket(FULL_ONLY, c, 0, c);
                })
                .thenWaitUntil(io(() -> check("C loads FULL", Loaded.at(level, cc) && kinetic(level, cc) != null && kinetic(level, cc) != old[0])))
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    old[0] = kinetic(level, cc);
                    check("C does not tick", !Loaded.ticks(level, cc));
                    check("CC keeps its saved speed: " + old[0].getSpeed() + ", CA " + speedOf(level, f.ca),
                            Math.abs(old[0].getSpeed()) == 64 && old[0].getSpeed() != speedOf(level, f.ca));
                    for (BlockPos p : parts.subList(3, 6))
                        check("nothing of C joined: " + p, kinetic(level, p) != null && !n.members.containsKey(kinetic(level, p)));
                    float[] u = unloadedShare(n, new float[] {100f, 50f + share[1], 3f + share[2]});
                    check("the unloaded share is unchanged: " + u[0] + ", " + u[1] + ", " + u[2],
                            u[0] == 100f && u[1] == 50f + share[1] && u[2] == 3f + share[2]);
                    mark[0] = StaleKinetics.resyncs();
                    level.getChunkSource().addRegionTicket(FULL_ONLY, b, 0, b);
                })
                .thenWaitUntil(io(() -> {
                    KineticBlockEntity cb = kinetic(level, f.cb), ca = kinetic(level, f.ca);
                    check("B loads FULL and CB takes CA's speed and network: CB " + (cb == null ? "none" : cb.getSpeed() + " on " + cb.network),
                            Loaded.at(level, f.cb) && cb != null && cb.getSpeed() == -64 && ca.network != null && ca.network.equals(cb.network));
                }))
                .thenExecute(() -> {
                    KineticBlockEntity ca = kinetic(level, f.ca);
                    KineticNetwork n = net.get();
                    for (int i = 0; i < parts.size(); i++)
                        fresh[i] = kinetic(level, parts.get(i));
                    float[] u = unloadedShare(n, new float[] {100f, 50f, 3f});
                    record(t, "B loaded FULL: CA, CB, CC, re-syncs, members, distinct positions", ca.getSpeed() + ", " + fresh[0].getSpeed() + ", "
                            + fresh[3].getSpeed() + ", " + (StaleKinetics.resyncs() - mark[0]) + ", " + n.members.size() + ", " + positions(n));
                    record(t, "the unloaded share after the joins (set 100, 50 + share, 3 + share)", u[0] + ", " + u[1] + ", " + u[2]);
                    check("B and C do not tick", !Loaded.ticks(level, f.cb) && !Loaded.ticks(level, cc));
                    check("CB was re-synced", StaleKinetics.resyncs() > mark[0]);
                    check("CB and CC have not attached", fresh[0].updateSpeed && fresh[3].updateSpeed);
                    check("CC is the block entity C loaded", fresh[3] == old[0]);
                    check("CC has CA's speed: CC " + fresh[3].getSpeed() + ", CA " + ca.getSpeed(), fresh[3].getSpeed() == ca.getSpeed());
                    for (int i = 0; i < parts.size(); i++)
                        check("joined CA's network: " + parts.get(i), fresh[i] != null && n.members.containsKey(fresh[i]));
                    check("each counted out of the unloaded share once: " + u[0] + ", " + u[1] + ", " + u[2],
                            u[0] == 100f && Math.abs(u[1] - 50f) < 1e-3f && u[2] == 3f);
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
                    mark[0] = StaleKinetics.resyncs();
                    level.setChunkForced(f.bx, f.az, true);
                    level.setChunkForced(cx, f.az, true);
                })
                .thenWaitUntil(io(() -> check("B and C tick", Loaded.ticks(level, f.cb) && Loaded.ticks(level, cc))))
                .thenIdle(40)
                .thenExecute(() -> {
                    KineticBlockEntity ca = kinetic(level, f.ca);
                    KineticNetwork n = net.get();
                    record(t, "B and C forced: CB, CC, millstones B and C, re-syncs at the attach", speedOf(level, f.cb) + ", " + speedOf(level, cc) + ", "
                            + speedOf(level, f.millB) + ", " + speedOf(level, millC) + ", " + (StaleKinetics.resyncs() - mark[0]));
                    for (int i = 0; i < parts.size(); i++)
                        check("the same block entity, not broken: " + parts.get(i) + " " + level.getBlockState(parts.get(i)),
                                kinetic(level, parts.get(i)) == fresh[i] && !fresh[i].isRemoved());
                    check("CB and CC attached", !fresh[0].updateSpeed && !fresh[3].updateSpeed);
                    check("the first attaches found nothing stale: " + (StaleKinetics.resyncs() - mark[0]), StaleKinetics.resyncs() == mark[0]);
                    check("CB and CC turn with CA: " + fresh[0].getSpeed() + ", " + fresh[3].getSpeed() + ", CA " + ca.getSpeed(),
                            fresh[0].getSpeed() == ca.getSpeed() && fresh[3].getSpeed() == ca.getSpeed());
                    check("millstones B and C turn at 64 RPM: " + speedOf(level, f.millB) + ", " + speedOf(level, millC),
                            Math.abs(speedOf(level, f.millB)) == 64 && Math.abs(speedOf(level, millC)) == 64);
                    float[] u = unloadedShare(n, new float[] {100f, 50f, 3f});
                    check("the first ticks leave the unloaded share: " + u[0] + ", " + u[1] + ", " + u[2],
                            u[0] == 100f && Math.abs(u[1] - 50f) < 1e-3f && u[2] == 3f);
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, c, 0, c);
                    level.setChunkForced(cx, f.az, false);
                });
        restore(s, level, f).thenSucceed();
    }

    /** IND_EDIT records journaled for district {@code d}. */
    private static List<ByteBuffer> edits(long d) {
        return received(IndRecords.IND_EDIT, IndRecords.Edit.DISTRICT, d);
    }

    /**
     * A player's edit outside every district, on a network a district's compile reaches, is that district's
     * edit too (SECTIONS.md §3.6, choice SC77). The chain of the adjacent fixture joins A to B, but only A's
     * millstone is placed by a player: B is in no district and its millstone turns on A's motor. A's compile
     * keeps B's chunk as one its network reaches (and not M's, which the chain only spans). A cog a player
     * places in B, meshing with B's, is an IND_EDIT (placed) of A at the cog, and A recompiles. Breaking it
     * is A's edit (broken) too. A chest placed and broken in B is no part of the network and no edit of A
     * (review M2). A cog line a player runs in A out to its chunk's east border leaves M, empty there, as a
     * chunk A's compile reaches across the last cog's face (review M3; the compile after the line turns, as
     * the edits' own may run before its last cog joins): a cog placed in M meshing with it is an edit of A,
     * with no compile asked for in between. B stays in no district.
     */
    @GameTest(template = YARD, batch = "sec1_9", timeoutTicks = 12000)
    public static void anEditOutsideEveryDistrictReachesTheDistrictOnItsNetwork(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 5200, true);
        BlockPos cogB = f.cb.above().east(), chestB = f.cb.north(3);
        // A's cog over CA is at local x 12: three more out to x 15, then M's first column
        List<BlockPos> line = List.of(f.ca.above().east(), f.ca.above().east(2), f.ca.above().east(3));
        BlockPos cogM = f.ca.above().east(4);
        long chunkB = ChunkPos.asLong(f.bx, f.az), chunkM = ChunkPos.asLong(f.ax + 1, f.az);
        long[] mark = {0, 0};
        String t = "anEditOutsideEveryDistrictReachesTheDistrictOnItsNetwork";
        GameTestSequence s = helper.startSequence()
                .thenExecute(() -> {
                    level.setChunkForced(f.ax, f.az, true);
                    level.setChunkForced(f.bx, f.az, true);
                })
                .thenWaitUntil(io(() -> check("A, M and B load", allLoaded(level, f.ax, f.bx, f.az))))
                .thenExecute(() -> {
                    clear(level, f.ax - 1, f.az - 1, f.bx + 1, f.az + 1);
                    GlassCorpusKit.motor(helper, f.motor, Direction.UP, 64);
                    GlassCorpusKit.set(helper, f.ca, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    GlassCorpusKit.set(helper, f.cb, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    for (BlockPos c : List.of(f.ca, f.cb)) {
                        GlassCorpusKit.cog(helper, c.above(), Direction.Axis.Y);
                        GlassCorpusKit.set(helper, c.above().south(), AllBlocks.MILLSTONE.getDefaultState());
                    }
                    join(level, f.ca, f.cb);
                    playerPlaced(helper, f.millA);
                })
                .thenWaitUntil(io(() -> {
                    District a = districtAt(helper, f.millA);
                    check("district A exists", a != null);
                    f.ids[0] = a.id;
                    check("B is in no district: " + districtAt(helper, f.millB), districtAt(helper, f.millB) == null);
                    check("the chain turns B's millstone: " + speedOf(level, f.millB), speedOf(level, f.millB) != 0);
                }))
                .thenExecute(() -> {
                    // A's first compile may have run before the chain joined B to A's network: a chunk a
                    // network reaches only after a compile is indexed at the next one (SC77)
                    mark[1] = ind().glass().compileNo(f.da());
                    ind().glass().requestCompile(ind().district(f.da()));
                })
                .thenWaitUntil(io(() -> check("A recompiled once B turns: " + ind().glass().compileNo(f.da()) + " after " + mark[1],
                        ind().glass().compileNo(f.da()) > mark[1] && !ind().glass().pending(f.da()))))
                .thenWaitUntil(io(() -> {
                    long[] by = ind().glass().reachedBy(level.dimension(), chunkB);
                    var lg = ind().glass().lastGraph(f.da());
                    check("A's compile reaches B: " + java.util.Arrays.toString(by) + " (compile " + ind().glass().compileNo(f.da())
                            + ", pending " + ind().glass().pending(f.da()) + ", reached " + (lg == null ? "no graph" : lg.reachedChunks)
                            + ", B " + chunkB + ", A's chunks " + ind().district(f.da()).chunks + ")", by.length == 1 && by[0] == f.da());
                    check("and not M, which the chain only spans", ind().glass().reachedBy(level.dimension(), chunkM).length == 0);
                    check("A's compile is not pending", !ind().glass().pending(f.da()));
                }))
                .thenExecute(() -> {
                    mark[0] = edits(f.da()).size();
                    mark[1] = ind().glass().compileNo(f.da());
                    GlassCorpusKit.cog(helper, cogB, Direction.Axis.Y);
                    playerPlacedBare(helper, cogB);
                })
                .thenWaitUntil(io(() -> {
                    List<ByteBuffer> e = edits(f.da());
                    check("one more IND_EDIT of A: " + (e.size() - mark[0]), e.size() == mark[0] + 1);
                    ByteBuffer last = e.get(e.size() - 1);
                    check("placed, at the cog in B: " + aux(last) + " at " + last.getInt(IndRecords.Edit.X) + ", " + last.getInt(IndRecords.Edit.Z),
                            aux(last) == IndRecords.Edit.PLACED && last.getInt(IndRecords.Edit.X) == cogB.getX()
                                    && last.getInt(IndRecords.Edit.Y) == cogB.getY() && last.getInt(IndRecords.Edit.Z) == cogB.getZ());
                    check("the cog in B turns with the chain: " + speedOf(level, cogB), speedOf(level, cogB) != 0);
                    check("A recompiled: " + ind().glass().compileNo(f.da()) + " after " + mark[1],
                            ind().glass().compileNo(f.da()) > mark[1] && !ind().glass().pending(f.da()));
                }))
                .thenIdle(40)
                .thenExecute(() -> {
                    check("one edit of A for the cog: " + (edits(f.da()).size() - mark[0]), edits(f.da()).size() == mark[0] + 1);
                    record(t, "A's edits, compiles after the placement", (edits(f.da()).size() - mark[0]) + ", " + (ind().glass().compileNo(f.da()) - mark[1]));
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, cogB, level.getBlockState(cogB),
                            com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.editor(level)));
                    level.removeBlock(cogB, false);
                })
                .thenWaitUntil(io(() -> {
                    List<ByteBuffer> e = edits(f.da());
                    check("one more IND_EDIT of A: " + (e.size() - mark[0]), e.size() == mark[0] + 2);
                    ByteBuffer last = e.get(e.size() - 1);
                    check("broken, at the cog in B", aux(last) == IndRecords.Edit.BROKEN && last.getInt(IndRecords.Edit.X) == cogB.getX());
                }))
                .thenIdle(40)
                .thenExecute(() -> {
                    check("B's chunk is a member chunk of no district", ind().districts().at(level.dimension(), chunkB) == null);
                    mark[0] = edits(f.da()).size();
                    GlassCorpusKit.set(helper, chestB, net.minecraft.world.level.block.Blocks.CHEST.defaultBlockState());
                    playerPlacedBare(helper, chestB);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("a chest placed in B is no edit of A: " + (edits(f.da()).size() - mark[0]), edits(f.da()).size() == mark[0]);
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, chestB, level.getBlockState(chestB),
                            com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.editor(level)));
                    level.removeBlock(chestB, false);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("nor is breaking it: " + (edits(f.da()).size() - mark[0]), edits(f.da()).size() == mark[0]);
                    mark[1] = ind().glass().compileNo(f.da());
                    for (BlockPos c : line) {
                        GlassCorpusKit.cog(helper, c, Direction.Axis.Y);
                        playerPlacedBare(helper, c);
                    }
                })
                .thenWaitUntil(io(() -> {
                    check("the cog line turns: " + speedOf(level, line.get(2)), Math.abs(speedOf(level, line.get(2))) == 64);
                    check("A recompiled after the cog line: " + ind().glass().compileNo(f.da()) + " after " + mark[1],
                            ind().glass().compileNo(f.da()) > mark[1] && !ind().glass().pending(f.da()));
                }))
                .thenExecute(() -> {
                    // the edits' recompile may have run before the last cog joined A's network (a cog placed
                    // this tick turns from its first tick on): a part the network takes only after a compile
                    // is read at the next one (SC77), as for B above
                    mark[1] = ind().glass().compileNo(f.da());
                    ind().glass().requestCompile(ind().district(f.da()));
                })
                .thenWaitUntil(io(() -> {
                    check("A recompiled once the cog line turns: " + ind().glass().compileNo(f.da()) + " after " + mark[1],
                            ind().glass().compileNo(f.da()) > mark[1] && !ind().glass().pending(f.da()));
                    long[] by = ind().glass().reachedBy(level.dimension(), chunkM);
                    check("A's compile reaches M across the line's last face: " + java.util.Arrays.toString(by), by.length == 1 && by[0] == f.da());
                }))
                .thenExecute(() -> {
                    mark[0] = edits(f.da()).size();
                    mark[1] = ind().glass().compileNo(f.da());
                    GlassCorpusKit.cog(helper, cogM, Direction.Axis.Y);
                    playerPlacedBare(helper, cogM);
                })
                .thenWaitUntil(io(() -> {
                    List<ByteBuffer> e = edits(f.da());
                    check("an IND_EDIT of A for the cog in M: " + (e.size() - mark[0]), e.size() == mark[0] + 1);
                    ByteBuffer last = e.get(e.size() - 1);
                    check("placed, at the cog in M", aux(last) == IndRecords.Edit.PLACED && last.getInt(IndRecords.Edit.X) == cogM.getX()
                            && last.getInt(IndRecords.Edit.Z) == cogM.getZ());
                    check("the cog in M turns: " + speedOf(level, cogM), speedOf(level, cogM) != 0);
                    check("A recompiled: " + ind().glass().compileNo(f.da()) + " after " + mark[1],
                            ind().glass().compileNo(f.da()) > mark[1] && !ind().glass().pending(f.da()));
                }))
                .thenExecute(() -> {
                    record(t, "A's edits for the cog in M, compiles after it", (edits(f.da()).size() - mark[0]) + ", " + (ind().glass().compileNo(f.da()) - mark[1]));
                    for (BlockPos c : List.of(cogM, line.get(2), line.get(1), line.get(0))) {
                        NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, c, level.getBlockState(c),
                                com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.editor(level)));
                        level.removeBlock(c, false);
                    }
                })
                .thenIdle(40)
                .thenExecute(() -> check("B is still in no district: " + districtAt(helper, f.millB), districtAt(helper, f.millB) == null));
        restore(s, level, f).thenSucceed();
    }

    /**
     * Joins that the first tick does not make (PATCHES.md row 58; SECTIONS.md choice SC73), on the fixture of
     * {@link #aChunkLoadedFullComesBackInStep}: with B away, C loads FULL and does not tick, and nothing there
     * has joined. A setBlock turns C's cog to another axis (as {@code /setblock} does): Create clears it and
     * forgets its network, and the cog joins and leaves at once, its share out of the unloaded share once and
     * not listed. A read of millstone C's own saved tag (as a {@code /data} merge) clears it and reads its
     * network back: it joins and stays listed. CC joins while a sync of the network is held open (the
     * network's sync depth set by hand, as an {@code onSpeedChanged} inside a sync would join it): nothing
     * changes until the sync ends; then its share leaves the unloaded share, it is listed, and the network's
     * totals are its recount's. The cog goes back to its axis; forced, B and C tick, nothing breaks, both
     * millstones turn, and every share is out of the unloaded share once.
     */
    @GameTest(template = YARD, batch = "sec1_10", timeoutTicks = 16000)
    public static void aPartClearedBeforeItsFirstTickLeavesItsNetwork(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        Adj f = Adj.at(helper, 5300, true);
        int cx = f.bx + 1;
        ChunkPos c = new ChunkPos(cx, f.az);
        BlockPos cc = new BlockPos(cx * 16 + 3, Y, f.az * 16), cog = cc.above(), millC = cog.south();
        String t = "aPartClearedBeforeItsFirstTickLeavesItsNetwork";
        KineticBlockEntity[] old = {null}, be = new KineticBlockEntity[3];
        float[] share = {0, 0, 0}, own = {0, 0, 0}, u0 = {0, 0, 0};
        Supplier<KineticNetwork> net = () -> kinetic(level, f.ca).getOrCreateNetwork();
        GameTestSequence built = build(helper, f, 64)
                .thenExecute(() -> {
                    level.setChunkForced(cx, f.az, true);
                    GlassCorpusKit.set(helper, cc, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    GlassCorpusKit.cog(helper, cog, Direction.Axis.Y);
                    GlassCorpusKit.set(helper, millC, AllBlocks.MILLSTONE.getDefaultState());
                    join(level, f.cb, cc);
                })
                .thenWaitUntil(() -> check("the chain turns millstone C: " + speedOf(level, millC),
                        Math.abs(speedOf(level, millC)) == 64 && speedOf(level, cc) == speedOf(level, f.cb)))
                .thenExecute(() -> {
                    old[0] = kinetic(level, cc);
                    level.setChunkForced(cx, f.az, false);
                });
        GameTestSequence s = away(built, level, f, ", and C",
                        () -> old[0].isRemoved() && level.getChunkSource().getChunkNow(cx, f.az) == null)
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    System.arraycopy(forgetAway(n, f.bx, cx, f.az), 0, share, 0, 3);
                    record(t, "as after a restart: B's and C's share (capacity, stress, members)", share[0] + ", " + share[1] + ", " + share[2]);
                    check("B and C hold six members with stress: " + share[2] + ", " + share[1], share[2] == 6 && share[1] > 0 && share[0] == 0);
                    unloadedShare(n, new float[] {100f, 50f + share[1], 3f + share[2]});
                    level.getChunkSource().addRegionTicket(FULL_ONLY, c, 0, c);
                })
                .thenWaitUntil(io(() -> check("C loads FULL", Loaded.at(level, cc) && kinetic(level, cc) != null && kinetic(level, cc) != old[0])))
                .thenExecute(() -> {
                    KineticNetwork n = net.get();
                    check("C does not tick", !Loaded.ticks(level, cc));
                    List<BlockPos> ps = List.of(cc, cog, millC);
                    for (int i = 0; i < 3; i++) {
                        be[i] = kinetic(level, ps.get(i));
                        check("nothing of C joined: " + ps.get(i), be[i] != null && !n.members.containsKey(be[i]) && n.id.equals(be[i].network));
                        own[i] = field(be[i], KineticBlockEntity.class, "lastStressApplied") * Math.abs(be[i].getTheoreticalSpeed());
                    }
                    record(t, "C's own stress: CC, cog, millstone", own[0] + ", " + own[1] + ", " + own[2]);
                    System.arraycopy(unloadedNow(n), 0, u0, 0, 3);
                    // the cog to another axis: the same block entity, cleared
                    level.setBlock(cog, level.getBlockState(cog).setValue(RotatedPillarKineticBlock.AXIS, Direction.Axis.X), 2);
                    float[] u = unloadedNow(n);
                    record(t, "the cog cleared: its network, listed, the unloaded share", be[1].network + ", " + n.members.containsKey(be[1]) + ", "
                            + u[0] + ", " + u[1] + ", " + u[2]);
                    check("the same block entity", kinetic(level, cog) == be[1]);
                    check("the cog was cleared: " + be[1].network + ", " + be[1].getSpeed(), be[1].network == null && be[1].getSpeed() == 0);
                    check("the cog is not listed", !n.members.containsKey(be[1]) && !n.sources.containsKey(be[1]));
                    check("the cog's share left the unloaded share once: " + u[1] + ", " + u[2],
                            u[0] == u0[0] && Math.abs(u[1] - (u0[1] - own[1])) < 1e-3f && u[2] == u0[2] - 1);
                    // millstone C reads its own saved tag
                    be[2].loadWithComponents(be[2].saveWithFullMetadata(level.registryAccess()), level.registryAccess());
                    u = unloadedNow(n);
                    record(t, "the millstone read: its network, listed, the unloaded share", be[2].network + ", " + n.members.containsKey(be[2]) + ", "
                            + u[0] + ", " + u[1] + ", " + u[2]);
                    check("the millstone keeps its network: " + be[2].network, n.id.equals(be[2].network) && Math.abs(be[2].getSpeed()) == 64);
                    check("the millstone is listed", n.members.containsKey(be[2]));
                    check("the millstone's share left the unloaded share once: " + u[1] + ", " + u[2],
                            Math.abs(u[1] - (u0[1] - own[1] - own[2])) < 1e-3f && u[2] == u0[2] - 2);
                    // CC joins inside a sync
                    syncing(n, 1);
                    ((KineticJoin) be[0]).mic_rtsim$join();
                    u = unloadedNow(n);
                    check("inside the sync CC is not listed: " + n.members.containsKey(be[0]), !n.members.containsKey(be[0]));
                    check("inside the sync the unloaded share keeps CC's: " + u[1] + ", " + u[2],
                            Math.abs(u[1] - (u0[1] - own[1] - own[2])) < 1e-3f && u[2] == u0[2] - 2);
                    syncing(n, 0);
                    n.sync();
                    u = unloadedNow(n);
                    float stress = field(n, KineticNetwork.class, "currentStress"), capacity = field(n, KineticNetwork.class, "currentCapacity");
                    record(t, "the sync ended: CC listed, the unloaded share, current stress and capacity, their recount", n.members.containsKey(be[0])
                            + ", " + u[0] + ", " + u[1] + ", " + u[2] + ", " + stress + ", " + capacity + ", " + n.calculateStress() + ", " + n.calculateCapacity());
                    check("CC is listed once the sync ends", n.members.containsKey(be[0]));
                    check("CC's share left the unloaded share once: " + u[1] + ", " + u[2],
                            Math.abs(u[1] - (u0[1] - own[0] - own[1] - own[2])) < 1e-3f && u[2] == u0[2] - 3);
                    check("the totals are the recount's: " + stress + " of " + n.calculateStress() + ", " + capacity + " of " + n.calculateCapacity(),
                            stress == n.calculateStress() && capacity == n.calculateCapacity());
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
                    level.setBlock(cog, level.getBlockState(cog).setValue(RotatedPillarKineticBlock.AXIS, Direction.Axis.Y), 2);
                    check("the cog back on its axis is still not listed", kinetic(level, cog) == be[1] && !n.members.containsKey(be[1]));
                    level.setChunkForced(f.bx, f.az, true);
                    level.setChunkForced(cx, f.az, true);
                })
                .thenWaitUntil(io(() -> check("B and C tick", Loaded.ticks(level, f.cb) && Loaded.ticks(level, cc))))
                .thenIdle(40)
                .thenExecute(() -> {
                    KineticBlockEntity ca = kinetic(level, f.ca);
                    KineticNetwork n = net.get();
                    float[] u = unloadedNow(n);
                    record(t, "B and C forced: CC, cog, millstones B and C, the unloaded share", speedOf(level, cc) + ", " + speedOf(level, cog) + ", "
                            + speedOf(level, f.millB) + ", " + speedOf(level, millC) + ", " + u[0] + ", " + u[1] + ", " + u[2]);
                    for (int i = 0; i < 3; i++)
                        check("the same block entity, not broken, listed: " + be[i].getBlockPos(),
                                kinetic(level, be[i].getBlockPos()) == be[i] && !be[i].isRemoved() && n.members.containsKey(be[i]));
                    check("C turns with CA: " + speedOf(level, cc) + ", " + speedOf(level, cog) + ", CA " + ca.getSpeed(),
                            speedOf(level, cc) == ca.getSpeed() && Math.abs(speedOf(level, cog)) == 64);
                    check("millstones B and C turn at 64 RPM: " + speedOf(level, f.millB) + ", " + speedOf(level, millC),
                            Math.abs(speedOf(level, f.millB)) == 64 && Math.abs(speedOf(level, millC)) == 64);
                    check("every share left the unloaded share once: " + u[0] + ", " + u[1] + ", " + u[2],
                            u[0] == 100f && Math.abs(u[1] - 50f) < 1e-3f && u[2] == 3f);
                    check("one member per position: " + n.members.size() + " members, " + positions(n) + " positions", positions(n) == n.members.size());
                    level.getChunkSource().removeRegionTicket(FULL_ONLY, c, 0, c);
                    level.setChunkForced(cx, f.az, false);
                });
        restore(s, level, f).thenSucceed();
    }

    /** A network's unloaded capacity, stress and member count, unchanged. */
    private static float[] unloadedNow(KineticNetwork n) {
        try {
            var m = KineticNetwork.class.getDeclaredField("unloadedMembers");
            m.setAccessible(true);
            return new float[] {field(n, KineticNetwork.class, "unloadedCapacity"), field(n, KineticNetwork.class, "unloadedStress"), m.getInt(n)};
        } catch (ReflectiveOperationException e) {
            throw new net.minecraft.gametest.framework.GameTestAssertException("Create moved a network field: " + e);
        }
    }

    /** Sets how deep in {@code sync} the mixin counts a network (PATCHES.md row 54's depth; the test only). */
    private static void syncing(KineticNetwork n, int depth) {
        try {
            var d = KineticNetwork.class.getDeclaredField("mic_rtsim$syncing");
            d.setAccessible(true);
            d.setInt(n, depth);
        } catch (ReflectiveOperationException e) {
            throw new net.minecraft.gametest.framework.GameTestAssertException("no sync depth: " + e);
        }
    }

    /**
     * Takes the block entities of chunks {@code minCx} to {@code maxCx} in row {@code cz} out of a network,
     * with the mixin's record of who joined there (PATCHES.md row 57), as a restart with those chunks away
     * leaves it (the network lists only what loaded, and their share is in its unloaded share); their share:
     * capacity, stress and members, as Create's {@code addSilently} counts it out.
     */
    private static float[] forgetAway(KineticNetwork n, int minCx, int maxCx, int cz) {
        float[] share = {0, 0, 0};
        List<KineticBlockEntity> away = new ArrayList<>();
        for (KineticBlockEntity m : n.members.keySet()) {
            int x = m.getBlockPos().getX() >> 4;
            if (x >= minCx && x <= maxCx && m.getBlockPos().getZ() >> 4 == cz)
                away.add(m);
        }
        for (KineticBlockEntity m : away) {
            if (n.sources.containsKey(m))
                share[0] += field(m, KineticBlockEntity.class, "lastCapacityProvided") * Math.abs(m.getGeneratedSpeed());
            share[1] += field(m, KineticBlockEntity.class, "lastStressApplied") * Math.abs(m.getTheoreticalSpeed());
            share[2]++;
            n.sources.remove(m);
            n.members.remove(m);
        }
        try {
            var byPos = KineticNetwork.class.getDeclaredField("mic_rtsim$byPos");
            var joined = KineticNetwork.class.getDeclaredField("mic_rtsim$joined");
            byPos.setAccessible(true);
            joined.setAccessible(true);
            for (KineticBlockEntity m : away) {
                long k = m.getBlockPos().asLong();
                if (byPos.get(n) instanceof Long2ObjectOpenHashMap<?> map)
                    map.remove(k);
                if (joined.get(n) instanceof LongOpenHashSet set)
                    set.remove(k);
            }
        } catch (ReflectiveOperationException e) {
            throw new net.minecraft.gametest.framework.GameTestAssertException("no per-position record: " + e);
        }
        return share;
    }
}
