package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
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
 *   <li><b>links</b>: chain conveyor connections, which span gaps;</li>
 *   <li><b>canonical inventories</b>: a vault's or tank's controller ({@code IMultiBlockEntityContainer});</li>
 *   <li><b>boundary inventories</b>: the inventory behind each packager ({@code targetInventory}, I3), and
 *       postboxes and frogports, whose packages are counted by their contents;</li>
 *   <li><b>stations</b>: a district is named after its first station ({@code GlobalStation.name});</li>
 *   <li><b>crossings</b>: rotation or a belt, chute or pipe across a district boundary (§6 case 1);</li>
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
     * DS-10: what starts a district. Machines: kinetic generators (water wheels, windmill bearings, hand
     * cranks, motors, steam engines) and every kinetic block that does work (presses, mixers, millstones,
     * crushing wheels, saws, drills, deployers, arms, fans, pumps, crafters, bearings and pulleys), and
     * packagers. Not seeds, though industrial and so members and links: <b>transmission</b> (a bare shaft, cogwheel,
     * gearbox, clutch, gearshift, chain drive, speed controller, gauge: rotation that goes nowhere is no
     * factory, and a player's rotary decoration is not either), <b>transport</b> (belts, chain conveyors,
     * chutes, funnels, tunnels, pipes, valves), <b>storage</b> (vaults and tanks by the tags, depots, basins,
     * crates), the logistics controls (stock tickers and links, factory panels, switches, observers, package
     * ports) and stations (trains are another module's), decoration (flywheels, clocks, flap displays,
     * backtanks). Create block entities this plugin does not list (a spout, an item drain, a portable
     * storage interface) have no opinion here: the core's rule takes them as machines when they tick and
     * hold items or fluids.
     */
    @Override
    public Boolean seed(BlockEntity be) {
        if (!isIndustrial(be))
            return null;
        if (be instanceof KineticBlockEntity)
            return !(be instanceof SimpleKineticBlockEntity || be instanceof DirectionalShaftHalvesBlockEntity
                    || be instanceof ChainGearshiftBlockEntity || be instanceof SpeedControllerBlockEntity
                    || be instanceof GaugeBlockEntity || be instanceof BeltBlockEntity || be instanceof ChainConveyorBlockEntity
                    || be instanceof FluidValveBlockEntity || be instanceof GantryShaftBlockEntity
                    || be instanceof FlywheelBlockEntity || be instanceof CuckooClockBlockEntity
                    || be instanceof FlapDisplayBlockEntity || be instanceof BacktankBlockEntity);
        return be instanceof PackagerBlockEntity;
    }

    @Override
    public void links(BlockEntity be, Consumer<BlockPos> out) {
        if (be instanceof ChainConveyorBlockEntity chain)
            for (BlockPos rel : chain.connections)
                out.accept(be.getBlockPos().offset(rel)); // connections are relative (addConnectionTo subtracts)
    }

    @Override
    public BlockPos canonicalInventory(BlockEntity be) {
        if (be instanceof IMultiBlockEntityContainer multi) {
            BlockPos c = multi.getController();
            return c == null ? be.getBlockPos() : c;
        }
        return null;
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
    public int crossing(BlockEntity a, BlockEntity b) {
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
    }

    @Override
    public String perfLine() {
        return TrainWatch.perfLine();
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
