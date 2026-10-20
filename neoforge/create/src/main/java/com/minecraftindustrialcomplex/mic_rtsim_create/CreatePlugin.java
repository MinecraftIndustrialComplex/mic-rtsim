package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
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
import com.simibubi.create.foundation.blockEntity.SmartBlockEntity;
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
 *   <li><b>Industrial</b>: every Create {@code SmartBlockEntity} and every kinetic one, so shafts, cogs,
 *       belts, pipes and chutes join the chunks they touch into one district (I2, §8.5);</li>
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

    @Override
    public boolean isIndustrial(BlockEntity be) {
        return be instanceof SmartBlockEntity || be instanceof KineticBlockEntity;
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
            if (target != null)
                out.add(canonicalAt(be, target.getConnectedPos()), IndRecords.Inventory.KIND_PACKAGER_TARGET);
        }
        if (be instanceof PackagePortBlockEntity)
            out.add(be.getBlockPos(), IndRecords.Inventory.KIND_POSTBOX);
    }

    private static BlockPos canonicalAt(BlockEntity from, BlockPos pos) {
        BlockEntity t = from.getLevel() == null ? null : from.getLevel().getBlockEntity(pos);
        if (t instanceof IMultiBlockEntityContainer multi && multi.getController() != null)
            return multi.getController();
        return pos;
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

    @Override
    public StockAccess access(ServerLevel level, District.Inventory inventory) {
        if ((inventory.kind() & IndRecords.Inventory.KIND_POSTBOX) == 0)
            return null;
        return new PostboxAccess(level, inventory.pos());
    }
}
