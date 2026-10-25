package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import com.simibubi.create.content.contraptions.bearing.ClockworkBearingBlockEntity;
import com.simibubi.create.content.contraptions.bearing.MechanicalBearingBlockEntity;
import com.simibubi.create.content.contraptions.gantry.GantryCarriageBlockEntity;
import com.simibubi.create.content.contraptions.piston.LinearActuatorBlockEntity;
import com.simibubi.create.content.fluids.hosePulley.HosePulleyBlockEntity;
import com.simibubi.create.content.fluids.pump.PumpBlockEntity;
import com.simibubi.create.content.kinetics.crank.HandCrankBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.PoweredShaftBlockEntity;
import com.simibubi.create.content.kinetics.turntable.TurntableBlockEntity;
import com.simibubi.create.content.kinetics.waterwheel.WaterWheelBlockEntity;
import com.simibubi.create.content.equipment.armor.BacktankBlockEntity;
import com.simibubi.create.content.fluids.pipes.valve.FluidValveBlockEntity;
import com.simibubi.create.content.kinetics.base.DirectionalShaftHalvesBlockEntity;
import com.simibubi.create.content.kinetics.chainDrive.ChainGearshiftBlockEntity;
import com.simibubi.create.content.kinetics.clock.CuckooClockBlockEntity;
import com.simibubi.create.content.kinetics.flywheel.FlywheelBlockEntity;
import com.simibubi.create.content.kinetics.gantry.GantryShaftBlockEntity;
import com.simibubi.create.content.kinetics.gauge.GaugeBlockEntity;
import com.simibubi.create.content.kinetics.simpleRelays.SimpleKineticBlockEntity;
import com.simibubi.create.content.kinetics.speedController.SpeedControllerBlockEntity;
import com.simibubi.create.content.trains.display.FlapDisplayBlockEntity;
import com.simibubi.create.content.fluids.pipes.FluidPipeBlockEntity;
import com.simibubi.create.content.fluids.pipes.StraightPipeBlockEntity;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.chute.ChuteBlockEntity;
import com.simibubi.create.content.logistics.crate.CreativeCrateBlockEntity;
import com.simibubi.create.content.logistics.packagePort.PackagePortBlockEntity;
import com.simibubi.create.content.logistics.packager.PackagerBlockEntity;
import com.simibubi.create.content.logistics.vault.ItemVaultBlockEntity;
import com.simibubi.create.infrastructure.config.AllConfigs;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import com.simibubi.create.foundation.blockEntity.IMultiBlockEntityContainer;
import com.simibubi.create.content.logistics.crate.CrateBlockEntity;
import com.simibubi.create.content.logistics.depot.DepotBlockEntity;
import com.simibubi.create.content.logistics.factoryBoard.FactoryPanelBlockEntity;
import com.simibubi.create.content.logistics.funnel.FunnelBlockEntity;
import com.simibubi.create.content.logistics.packagerLink.PackagerLinkBlockEntity;
import com.simibubi.create.content.logistics.stockTicker.StockTickerBlockEntity;
import com.simibubi.create.content.logistics.tunnel.BeltTunnelBlockEntity;
import com.simibubi.create.content.processing.basin.BasinBlockEntity;
import com.simibubi.create.content.redstone.smartObserver.SmartObserverBlockEntity;
import com.simibubi.create.content.redstone.thresholdSwitch.ThresholdSwitchBlockEntity;
import net.createmod.catnip.math.BlockFace;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.items.ItemStackHandler;

import java.util.function.Consumer;

/**
 * Create 6.0.10 for the district sampler (INDUSTRY.md §12, layer 2):
 * <ul>
 *   <li><b>Industrial</b>: kinetics, pipes, chutes, funnels, tunnels and the logistics blocks, so shafts,
 *       cogs, belts, pipes and chutes join the chunks they touch into one district (I2, §8.5);
 *       decoration (copycats, clipboards, signals, curved tracks) is not;</li>
 *   <li><b>links</b>: chain conveyor connections, which span gaps, and a fan's air current, from the fan to
 *       the belts and depots it reaches and from those to the fans that reach them ({@link FanLinks}, G2-73);</li>
 *   <li><b>canonical inventories</b>: a vault's or tank's controller ({@code IMultiBlockEntityContainer});</li>
 *   <li><b>boundary inventories</b>: the inventory behind each packager ({@code targetInventory}, I3), and
 *       postboxes and frogports, whose packages are counted by their contents;</li>
 *   <li><b>stations</b>: a district is named after its first station ({@code GlobalStation.name});</li>
 *   <li><b>crossings</b>: rotation or a belt, chute or pipe across a district boundary (§6 case 1), and a
 *       fan's current across it (a conveyor's code, G2-75);</li>
 *   <li><b>sweep flags</b>: a creative motor or crate (creative), an overstressed network.</li>
 * </ul>
 * Every member read here is in PATCHES.md (P7).
 */
public final class CreatePlugin implements IndustryPlugin {

    /**
     * Industrial without a capability of its own: kinetics (shafts, cogs, belts, arms, deployers, pumps),
     * pipes, chutes, funnels, tunnels and the logistics blocks (packagers, stock links and tickers,
     * factory gauges, threshold switches, smart observers, stations, postboxes and frogports), and
     * multiblock storage. Not every SmartBlockEntity: copycats, clipboards, signals, curved tracks and
     * display boards are decoration and must not make a district (I2 names "Create kinetics" and the
     * like). Anything with an item, fluid or energy capability is industrial in the core anyway.
     */
    @Override
    public boolean isIndustrial(BlockEntity be) {
        return be instanceof KineticBlockEntity || be instanceof FluidPipeBlockEntity || be instanceof StraightPipeBlockEntity
                || be instanceof ChuteBlockEntity || be instanceof FunnelBlockEntity || be instanceof BeltTunnelBlockEntity
                || be instanceof PackagerBlockEntity || be instanceof PackagerLinkBlockEntity || be instanceof StockTickerBlockEntity
                || be instanceof FactoryPanelBlockEntity || be instanceof ThresholdSwitchBlockEntity || be instanceof SmartObserverBlockEntity
                || be instanceof StationBlockEntity || be instanceof PackagePortBlockEntity || be instanceof IMultiBlockEntityContainer
                || be instanceof DepotBlockEntity || be instanceof BasinBlockEntity || be instanceof CrateBlockEntity;
    }

    /**
     * DS-10: what starts a district is where goods are turned into other goods or used up. Machines: kinetic
     * blocks that do work (presses, mixers, millstones, crushing wheels, saws, drills, deployers, arms, fans,
     * crafters, motors' consumers) and packagers. Not seeds, though industrial and so members: <b>transmission</b>
     * (a bare shaft, cogwheel, gearbox, clutch, gearshift, chain drive, speed controller, gauge), <b>sources</b> (water
     * wheels, hand cranks, creative motors, bearings, a steam engine's shaft: rotation made from nothing, which
     * joins the district of what it drives), <b>movers</b> (mechanical pumps, hose pulleys, pistons, rope and elevator
     * pulleys, gantry carriages, clockwork bearings, turntables), <b>transport</b> (belts, chain conveyors, chutes,
     * funnels, tunnels, pipes, valves), <b>storage</b> (vaults and tanks by the tags, depots, basins, crates), the
     * logistics controls (stock tickers and links, factory panels, switches, observers, package ports) and
     * stations (trains are another module's), decoration (flywheels, clocks, flap displays, backtanks). Create
     * block entities this plugin does not list (a spout, an item drain, a portable storage interface) have no
     * opinion here: the core's rule takes them as machines when they tick and hold items or fluids.
     */
    @Override
    public Boolean seed(BlockEntity be) {
        if (!isIndustrial(be))
            return null;
        Boolean anchor = ContraptionAnchors.works(be);
        if (anchor != null)
            return anchor;
        if (be instanceof KineticBlockEntity)
            return !(be instanceof SimpleKineticBlockEntity || be instanceof DirectionalShaftHalvesBlockEntity
                    || be instanceof ChainGearshiftBlockEntity || be instanceof SpeedControllerBlockEntity
                    || be instanceof GaugeBlockEntity || be instanceof BeltBlockEntity || be instanceof ChainConveyorBlockEntity
                    || be instanceof FluidValveBlockEntity || be instanceof GantryShaftBlockEntity
                    || be instanceof FlywheelBlockEntity || be instanceof CuckooClockBlockEntity
                    || be instanceof FlapDisplayBlockEntity || be instanceof BacktankBlockEntity
                    || isSource(be) || isMover(be));
        return be instanceof PackagerBlockEntity;
    }

    /** A kinetic part (a chain conveyor is one): a compile follows its network out of the district (SC77). */
    @Override
    public boolean networkPart(BlockEntity be) {
        return be instanceof KineticBlockEntity;
    }

    /** The anchor of a contraption (bearing, piston, pulley, gantry shaft) can come to seed: see {@link ContraptionAnchors}. */
    @Override
    public boolean mayBecomeSeed(BlockEntity be) {
        return ContraptionAnchors.isAnchor(be);
    }

    /**
     * Links between chunks: a fan's blown stream (FanLinks). A chain conveyor is not one (SEC-1): it carries
     * packages between districts, it does not join them.
     */
    @Override
    public void links(BlockEntity be, Consumer<BlockPos> out) {
        FanLinks.links(be, out);
    }

    /**
     * Rotation made from nothing: water wheels (large ones too), hand cranks and valve handles, creative motors,
     * mechanical and windmill bearings (a windmill turns a contraption's sails), and the shaft a steam engine
     * powers. A source alone is a fountain, not a factory; what it drives is the machine that seeds, and
     * the source is in that machine's district by the shaft between them. (A steam engine's burner is what
     * eats fuel, and the blaze burner is no block entity this plugin lists, so it is the boiler's own
     * machine that decides.)
     */
    private static boolean isSource(BlockEntity be) {
        return be instanceof WaterWheelBlockEntity || be instanceof HandCrankBlockEntity || be instanceof CreativeMotorBlockEntity
                || be instanceof MechanicalBearingBlockEntity || be instanceof PoweredShaftBlockEntity;
    }

    /**
     * Kinetic blocks that only move things: mechanical pumps and hose pulleys (fluid transport, like pipes),
     * pistons, rope and elevator pulleys, gantry carriages and clockwork bearings (they carry a contraption
     * about, and what it carries is not a block entity in the world), turntables (rotating decoration).
     */
    private static boolean isMover(BlockEntity be) {
        return be instanceof PumpBlockEntity || be instanceof HosePulleyBlockEntity || be instanceof LinearActuatorBlockEntity
                || be instanceof GantryCarriageBlockEntity || be instanceof ClockworkBearingBlockEntity || be instanceof TurntableBlockEntity;
    }

    // No links(): a chain conveyor is a link between districts, not a reason to join them (SEC-1, DS-10). The
    // packages on it are counted at the ports (packagers, frogports, postboxes), the exports at one district's
    // edge and the imports at the other's.

    @Override
    public BlockPos canonicalInventory(BlockEntity be) {
        if (be instanceof IMultiBlockEntityContainer multi) {
            BlockPos c = multi.getController();
            return c == null ? be.getBlockPos() : c;
        }
        return null;
    }

    /** A multiblock with a chunk of its box not loaded reads that chunk through its capability ({@link Loaded#partsLoaded}, R2-11). */
    @Override
    public boolean partsLoaded(ServerLevel level, BlockEntity be) {
        return Loaded.partsLoaded(level, be);
    }

    /**
     * A vault's shape from its controller, without reading its parts: the core asks while a part's chunk is not
     * loaded (R2-11). As its handler counts it ({@code ItemVaultBlockEntity.initCapability}): one inventory of
     * {@code vaultCapacity} slots per block of the box. Null for anything else (a tank is never asked: its
     * controller's handler reads only itself).
     */
    @Override
    public long[] describe(ServerLevel level, BlockPos pos) {
        if (!(Loaded.blockEntity(level, pos) instanceof ItemVaultBlockEntity vault) || !vault.isController())
            return null;
        long slots = (long) vault.getHeight() * vault.getWidth() * vault.getWidth() * AllConfigs.server().logistics.vaultCapacity.get();
        return new long[] {IndRecords.Inventory.KIND_ITEMS, slots, 0, 0};
    }

    @Override
    public void inventories(BlockEntity be, InventorySink out) {
        if (be instanceof PackagerBlockEntity packager && packager.targetInventory != null) {
            BlockFace target = packager.targetInventory.getTarget();
            // The core canonicalizes the position (a vault's controller, a double chest's lower half) and
            // never loads a chunk to do it.
            if (target != null)
                out.add(target.getConnectedPos(), IndRecords.Inventory.KIND_PACKAGER_TARGET);
        }
        if (be instanceof PackagePortBlockEntity)
            out.add(be.getBlockPos(), IndRecords.Inventory.KIND_POSTBOX);
    }

    @Override
    public StationInfo station(BlockEntity be) {
        if (!(be instanceof StationBlockEntity sbe))
            return null;
        GlobalStation st = sbe.getStation();
        if (st == null)
            return null;
        return new StationInfo(CreateIds.of(st.id), be.getBlockPos(), st.name);
    }

    @Override
    public boolean isStation(BlockEntity be) {
        return be instanceof StationBlockEntity;
    }

    @Override
    public int crossing(BlockEntity a, BlockEntity b) {
        // a fan's current works items on the other side, as a belt carries them; a fan and a belt are both
        // kinetic, but no rotation crosses there
        if (FanLinks.blows(a, b) || FanLinks.blows(b, a))
            return IndRecords.NotLearnable.CONVEYOR_CROSSES;
        if (a instanceof KineticBlockEntity && b instanceof KineticBlockEntity)
            return IndRecords.NotLearnable.ROTATION_CROSSES;
        if (conveyor(a) && conveyor(b))
            return IndRecords.NotLearnable.CONVEYOR_CROSSES;
        return 0;
    }

    private static boolean conveyor(BlockEntity be) {
        return be instanceof BeltBlockEntity || be instanceof ChuteBlockEntity || be instanceof FluidPipeBlockEntity
                || be instanceof StraightPipeBlockEntity;
    }

    @Override
    public void scanned(BlockEntity be, ScanNotes notes) {
        if (be instanceof CreativeMotorBlockEntity || be instanceof CreativeCrateBlockEntity)
            notes.creative();
        // A kinetic network's id is its source's position (KineticBlockEntity.network): watching the
        // source watches the network, once.
        if (be instanceof KineticBlockEntity kbe && kbe.network != null && kbe.network == be.getBlockPos().asLong())
            notes.watch(be.getBlockPos());
    }

    @Override
    public int sweepFlags(ServerLevel level, District district) {
        for (BlockPos p : district.watch) {
            if (level.getChunkSource().getChunkNow(p.getX() >> 4, p.getZ() >> 4) == null)
                continue;
            if (level.getBlockEntity(p) instanceof KineticBlockEntity kbe && kbe.isOverStressed())
                return IndRecords.SweepEnd.FLAG_OVERSTRESSED;
        }
        return 0;
    }

    @Override
    public void tick(com.minecraftindustrialcomplex.mic_rtsim.industry.Industry industry, long now) {
        TrainWatch.tick(industry, now);
        PackageStores.of(industry).tick(now);
    }

    @Override
    public String perfLine() {
        com.minecraftindustrialcomplex.mic_rtsim.industry.Industry ind = com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi.current();
        return ind == null ? TrainWatch.perfLine() : TrainWatch.perfLine() + ", " + PackageStores.of(ind).perfLine();
    }

    /** The engine forgot what it was told: every package store is declared again (SECTIONS.md §2.5). */
    @Override
    public void announce(com.minecraftindustrialcomplex.mic_rtsim.industry.Industry industry) {
        PackageStores.of(industry).announce();
    }

    @Override
    public boolean expand(ItemStack stack, ItemSink out) {
        if (!PackageItem.isPackage(stack))
            return false;
        ItemStackHandler contents = PackageItem.getContents(stack);
        for (int s = 0; s < contents.getSlots(); s++) {
            ItemStack inner = contents.getStackInSlot(s);
            if (!inner.isEmpty())
                out.add(inner, inner.getCount());
        }
        return true;
    }

    /** The glass box's extractor (GLASSBOX.md §7.1): Create's machines, logistics and kinetics as a graph. */
    @Override
    public com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassExtractor glass() {
        return com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlass.INSTANCE;
    }

    @Override
    public StockAccess access(ServerLevel level, District.Inventory inventory) {
        if ((inventory.kind() & IndRecords.Inventory.KIND_POSTBOX) == 0)
            return null;
        return new PostboxAccess(level, inventory.pos());
    }
}
