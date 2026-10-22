package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.logistics.BigItemStack;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import com.simibubi.create.content.logistics.packager.PackagerBlock;
import com.simibubi.create.content.logistics.packager.PackagerBlockEntity;
import com.simibubi.create.content.logistics.packagerLink.PackagerLinkBlock;
import com.simibubi.create.content.logistics.packagerLink.PackagerLinkBlockEntity;
import com.simibubi.create.content.logistics.redstoneRequester.RedstoneRequesterBlockEntity;
import com.simibubi.create.content.logistics.stockTicker.PackageOrderWithCrafts;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.scrollValue.ScrollValueBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.HorizontalDirectionalBlock;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 11: a packager, chain conveyor and frogport loop. A pulse timer clocks a redstone requester that
 * orders {@value #PER_PACKAGE} clay from a stock link's packager every {@value #CLOCK} ticks, addressed to
 * {@value #ADDRESS}; the packager packs it out of the source chest, the frogport on top hangs the package
 * on chain conveyor 1, the chain carries it to conveyor 2, where the frogport whose filter is
 * {@value #ADDRESS} catches it and hands it down to a second packager, which unpacks it into a millstone;
 * a chute under the millstone empties it into the output chest.
 *
 * <pre>
 *   source (z 6)                               destination (z 14)
 *   y4             conveyor 1 (7,4,6)  ======  conveyor 2 (7,4,14)   frogport B (9,4,14), filter "mill"
 *   y3             motor up, 64 RPM            packager Q (9,3,14) facing W, millstone (10,3,14), cog (11,3,14)
 *   y2  frogport A (5,2,6)                     chute (10,2,14), motor up 256 RPM (11,2,14)
 *   y1  chest (4,1,6), packager P (5,1,6) facing E, stock link (5,1,7)      chest (10,1,14)
 *   y2  requester (2,2,10), pulse timer (2,2,11) facing S, period 40 (both on the floor)
 * </pre>
 * The yard's polished andesite floor is at relative y 1 (the structure block is at relative y 0, the
 * template one block above it; relative y 0 is air, y -1 and below stone): the y 1 blocks above replace
 * floor blocks, which only matters for blocks that need support (the diode stands on the floor at y 2).
 * The two conveyors are joined in code the way {@code ChainConveyorConnectionPacket} joins them
 * ({@code addConnectionTo} on each, after {@code prepareStats}, since {@code addConnectionTo} writes into
 * {@code connectionStats}, which is null before the first tick). The frogports' targets are
 * {@code PackagePortTarget.ChainConveyorFrogportTarget}s on each conveyor's loop (no connection), as
 * {@code ChainConveyorInteractionHandler.onUse} builds them for a player; the requester and the stock link
 * share one frequency id ({@code LogisticallyLinkedBehaviour.freqId}), as a player's tuning gives them.
 *
 * <p><b>Why packages are requested and not packed on a redstone signal.</b> A packager packs on a redstone
 * signal ({@code lazyTick} while powered, {@code activate} on a rising edge) with no amount: one package
 * takes up to 9 full stacks out of the source ({@code attemptToSend} with no requests: 64 per slot, 9
 * slots), 576 clay per 22 ticks, which no processing step keeps up with. A request packs exactly what it
 * asks for ({@code PackagingRequest}), so the requester's order sets the package.
 *
 * <p><b>Rate</b>, one package of {@value #PER_PACKAGE} clay per {@value #CLOCK} ticks:
 * <ul>
 *   <li>Pulse timer ({@code PulseTimerBlockEntity.updateState}): its state counts 0 to period - 1 and it
 *       powers while the state is at most 1: one rising edge every {@value #CLOCK} ticks.</li>
 *   <li>Requester ({@code RedstoneRequesterBlockEntity.onRedstonePowerChanged}, {@code triggerRequest}):
 *       each rising edge broadcasts the order ({@code LogisticsManager.broadcastPackageRequest}, type
 *       REDSTONE) if the network holds it; the packager takes it unless its queue holds 20
 *       ({@code isTooBusyFor}).</li>
 *   <li>Packager ({@code attemptToSend} with requests): idle, so the package is its held box at once, with
 *       a 20-tick animation ({@code CYCLE}); at its end {@code wakeTheFrogs} has the frogport above pull
 *       it. A queued package would wait for the next tick with no box and no animation: 21 ticks per
 *       package at most, against the clock's 40.</li>
 *   <li>Frogport A: a 10-tick animation ({@code animationProgress} chasing 1 at 0.1), hanging the package
 *       on the conveyor's loop past half way ({@code ChainConveyorFrogportTarget.export}).</li>
 *   <li>Chain: packages move {@code speed / 360} blocks per tick on the straight and
 *       {@code speed / (1.5 pi)} degrees per tick on a loop (64 RPM: 0.178 blocks, 13.6 degrees); there is
 *       no spacing between packages, only {@code chainConveyorCapacity} (20) per conveyor, against 1 or 2
 *       in flight here. The loop leaves for conveyor 2 where the routing table
 *       ({@code ChainConveyorRoutingTable}, filled from frogport B's filter) sends {@value #ADDRESS}.</li>
 *   <li>Frogport B catches it when it crosses B's angle ({@code exportToPort}: not animating, not backed
 *       up), 10 ticks, then pushes it into packager Q on its lazy tick (every 11 ticks,
 *       {@code SmartBlockEntity}: the counter runs 10 to 0), and Q unpacks it
 *       ({@code unwrapBox}, {@code DefaultUnpackingHandler}: only when all of it fits) with a 20-tick
 *       animation: at most one package per 22 ticks.</li>
 *   <li>Millstone at 256 RPM: s = 16, clay ({@code milling/clay}: processing_time 50, 4 clay balls, no
 *       chance outputs) takes ceil(50 / 16) + 1 = 5 ticks: 10 ticks per package.</li>
 * </ul>
 * So the clock sets the rate and every stage after it has room: packager 21 of 40 ticks, frogports 10,
 * unpacking 22, millstone 10. Clay: -{@value #PER_PACKAGE} x 1200 / {@value #CLOCK} = -60 per minute;
 * clay balls: 4 per clay, +240 per minute. Both exact (the clock is deterministic, the recipe has no
 * chance outputs, and package routing is an address match).
 *
 * <p><b>Stores.</b> 640 clay last 640 / 2 x 40 = 12,800 ticks; the output chest's 27 stacks hold 1,728
 * clay balls, 7,200 ticks of output plus the warm-up's.
 *
 * <p><b>Measured</b> (stub engine, 3,600-tick window): clay -59.993 per minute, clay balls +240.022. The
 * first package leaves the packager 1 tick after build and its clay balls reach the chest about 112 ticks
 * after build (packager 20, frogport 10, loop and chain about 60, catch 10, lazy push, millstone 10).
 */
public final class PackageLoopFactory implements CorpusFactory {
    static final int CLOCK = 40;
    static final int PER_PACKAGE = 2;
    static final int CLAY_BALLS_PER_CLAY = 4;
    static final String ADDRESS = "mill";
    static final int CHAIN_RPM = 64;
    static final int MILL_RPM = 256;
    static final double CLAY_PER_MIN = PER_PACKAGE * (double) TICKS_PER_MINUTE / CLOCK;

    /** Store totals at the window's first tick, for the per-store rates logged at its end. */
    private final Map<BlockPos, Map<String, Long>> atStart = new HashMap<>();

    @Override
    public String name() {
        return "package_loop";
    }

    @Override
    public int warmupTicks() {
        return 400;
    }

    @Override
    public Built build(GameTestHelper h) {
        UUID freq = UUID.randomUUID();

        // Source: chest, packager (its inventory is behind it: facing east, the chest to the west),
        // frogport on top, stock link on its south face.
        BlockPos src = at(h, 4, 1, 6), packager = at(h, 5, 1, 6), frogA = at(h, 5, 2, 6), link = at(h, 5, 1, 7);
        chest(h, src);
        fill(h, src, Items.CLAY, 640);
        set(h, packager, AllBlocks.PACKAGER.getDefaultState().setValue(PackagerBlock.FACING, Direction.EAST));
        set(h, link, AllBlocks.STOCK_LINK.getDefaultState()
                .setValue(PackagerLinkBlock.FACE, AttachFace.WALL)
                .setValue(HorizontalDirectionalBlock.FACING, Direction.SOUTH));
        be(h, link, PackagerLinkBlockEntity.class).behaviour.freqId = freq;
        set(h, frogA, AllBlocks.PACKAGE_FROGPORT.getDefaultState());

        // Chain: two conveyors joined, the first turned by a motor under it.
        BlockPos c1 = at(h, 7, 4, 6), c2 = at(h, 7, 4, 14);
        motor(h, at(h, 7, 3, 6), Direction.UP, CHAIN_RPM);
        set(h, c1, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
        set(h, c2, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
        ChainConveyorBlockEntity cc1 = be(h, c1, ChainConveyorBlockEntity.class), cc2 = be(h, c2, ChainConveyorBlockEntity.class);
        cc1.prepareStats();
        cc2.prepareStats();
        if (!cc2.addConnectionTo(c1) || !cc1.addConnectionTo(c2))
            throw new GameTestAssertException("could not join the chain conveyors");

        // Destination: frogport B on conveyor 2, packager Q under it unpacking into the millstone, a chute
        // under the millstone into the output chest.
        BlockPos frogB = at(h, 9, 4, 14), unpacker = at(h, 9, 3, 14), mill = at(h, 10, 3, 14), chute = at(h, 10, 2, 14), out = at(h, 10, 1, 14);
        chest(h, out);
        set(h, chute, AllBlocks.CHUTE.getDefaultState());
        set(h, mill, AllBlocks.MILLSTONE.getDefaultState());
        cog(h, at(h, 11, 3, 14), Direction.Axis.Y);
        motor(h, at(h, 11, 2, 14), Direction.UP, MILL_RPM);
        set(h, unpacker, AllBlocks.PACKAGER.getDefaultState().setValue(PackagerBlock.FACING, Direction.WEST));
        set(h, frogB, AllBlocks.PACKAGE_FROGPORT.getDefaultState());

        FrogportBlockEntity a = be(h, frogA, FrogportBlockEntity.class);
        a.target = new PackagePortTarget.ChainConveyorFrogportTarget(c1.subtract(frogA), loopAngle(c1, frogA), (BlockPos) null, false);
        a.notifyUpdate();
        FrogportBlockEntity b = be(h, frogB, FrogportBlockEntity.class);
        b.target = new PackagePortTarget.ChainConveyorFrogportTarget(c2.subtract(frogB), loopAngle(c2, frogB), (BlockPos) null, false);
        b.addressFilter = ADDRESS;
        b.filterChanged();
        b.notifyUpdate();

        // Trigger: a requester on the stock link's frequency, ordering 2 clay to "mill", clocked by a
        // pulse timer south of it (a diode's FACING is the side it is read from: it powers the block on
        // the opposite side). Both stand on the yard's floor (relative y 1): a diode needs a sturdy block
        // under it (DiodeBlock.canSurvive), and relative y 0 is air.
        BlockPos requester = at(h, 2, 2, 10), timer = at(h, 2, 2, 11);
        set(h, requester, AllBlocks.REDSTONE_REQUESTER.getDefaultState());
        RedstoneRequesterBlockEntity r = be(h, requester, RedstoneRequesterBlockEntity.class);
        r.behaviour.freqId = freq;
        r.encodedRequest = PackageOrderWithCrafts.simple(List.of(new BigItemStack(new ItemStack(Items.CLAY), PER_PACKAGE)));
        r.encodedTargetAdress = ADDRESS;
        r.allowPartialRequests = false;
        r.notifyUpdate();
        set(h, timer, AllBlocks.PULSE_TIMER.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.SOUTH));
        ScrollValueBehaviour period = BlockEntityBehaviour.get(h.getLevel(), timer, ScrollValueBehaviour.TYPE);
        if (period == null)
            throw new GameTestAssertException("no pulse timer setting at " + timer.toShortString());
        period.setValue(CLOCK);
        double c = CLAY_PER_MIN;
        return new Built(packager, List.of(src), List.of(out), List.of(
                Expect.exact("minecraft:clay", -c,
                        "pulse timer 40 ticks -> requester orders 2 clay -> packager (21 ticks at most) -> frogport -> chain -> frogport -> unpacker (22 at most): 2 per 40 ticks"),
                Expect.exact("minecraft:clay_ball", CLAY_BALLS_PER_CLAY * c,
                        "milling/clay: 4 clay balls per clay, no chance outputs; millstone 5 ticks per clay at 256 RPM")),
                List.of(), 3600,
                String.format(Locale.ROOT, "requests of %d clay every %d ticks to '%s'; chain %d RPM, millstone %d RPM; frequency %s (drawn per build)",
                        PER_PACKAGE, CLOCK, ADDRESS, CHAIN_RPM, MILL_RPM, freq));
    }

    /**
     * The loop angle a player would pick for a frogport beside a conveyor: the direction from the
     * conveyor to the port, in the convention of {@code ChainConveyorBlockEntity.calculateConnectionStats}
     * (degrees of atan2(x, z)).
     */
    static float loopAngle(BlockPos conveyor, BlockPos port) {
        BlockPos d = port.subtract(conveyor);
        float deg = (float) Math.toDegrees(Math.atan2(d.getX(), d.getZ()));
        return deg < 0 ? deg + 360 : deg;
    }

    @Override
    public void perturb(GameTestHelper h, Built built, int windowTick) {
        if (windowTick == 0) {
            atStart.clear();
            for (BlockPos p : built.stores())
                atStart.put(p, totals(h.getLevel(), List.of(p)));
            log(name(), "packager at the window start", describe(h, built.anchor()));
        } else if (windowTick == built.window() - 1) {
            for (BlockPos p : built.stores()) {
                Map<String, Long> end = totals(h.getLevel(), List.of(p)), start = atStart.getOrDefault(p, Map.of());
                StringBuilder sb = new StringBuilder();
                java.util.TreeSet<String> goods = new java.util.TreeSet<>(end.keySet());
                goods.addAll(start.keySet());
                for (String g : goods)
                    sb.append(String.format(Locale.ROOT, " %s %.3f/min", g,
                            perMinute((end.getOrDefault(g, 0L) - start.getOrDefault(g, 0L)) / (double) windowTick)));
                log(name(), "store " + p.toShortString() + " ends rate", sb.toString().trim());
            }
        }
    }

    private static String describe(GameTestHelper h, BlockPos packager) {
        PackagerBlockEntity p = be(h, packager, PackagerBlockEntity.class);
        return "held " + p.heldBox + ", animation " + p.animationTicks + ", queued " + p.queuedExitingPackages.size()
                + ", linked " + p.getBlockState().getValue(PackagerBlock.LINKED);
    }
}
