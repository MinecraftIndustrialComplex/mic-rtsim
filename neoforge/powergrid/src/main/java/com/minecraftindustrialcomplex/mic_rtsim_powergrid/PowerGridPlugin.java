package com.minecraftindustrialcomplex.mic_rtsim_powergrid;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Goods;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.industry.StockAccess;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.patryk3211.powergrid.electricity.base.IElectricEntity;
import org.patryk3211.powergrid.electricity.battery.BatteryBlockEntity;
import org.patryk3211.powergrid.electricity.deviceconnector.DeviceConnectorBlockEntity;
import org.patryk3211.powergrid.electricity.socket.SocketBlockEntity;
import org.patryk3211.powergrid.electricity.wireconnector.ConnectorBlockEntity;
import org.patryk3211.powergrid.electricity.wireconnector.CordJunctionBlockEntity;

/**
 * Power Grid for the district sampler.
 *
 * <ul>
 *   <li><b>Industrial:</b> every electric block entity ({@code IElectricEntity}: generators, motors,
 *       batteries, connectors, switches, transformers). Wires do not merge districts (I13: lines couple
 *       districts, they do not join them).</li>
 *   <li><b>Seeds (DS-10):</b> the machines among them: generators, motors, heaters, fans, pumps, lamps,
 *       transformers, switches. Not a battery (storage, below) and not the wiring (wire connectors, cord
 *       junctions, device connectors, sockets): a bank of batteries or a pylon on its own is not a factory.</li>
 *   <li><b>Batteries</b> are boundary stock (INDUSTRY.md §7): a plugin inventory at the battery (a
 *       multiblock's controller, which Create's plugin finds through {@code IMultiBlockEntityContainer}),
 *       holding the good {@code e/0/powergrid:joule}, the battery's energy in whole joules (Power Grid keeps
 *       it in joules, a double); its energy capacity is the battery's capacity (choice Q10). Written with
 *       {@code setEnergy}, clamped to [0, capacity].</li>
 *   <li><b>Power links</b> ({@link PowerLinks}).</li>
 * </ul>
 */
public final class PowerGridPlugin implements IndustryPlugin {

    public static final String JOULE = "e/0/powergrid:joule";

    @Override
    public boolean isIndustrial(BlockEntity be) {
        return be instanceof IElectricEntity;
    }

    @Override
    public Boolean seed(BlockEntity be) {
        if (!(be instanceof IElectricEntity))
            return null;
        return !(be instanceof BatteryBlockEntity || be instanceof ConnectorBlockEntity || be instanceof CordJunctionBlockEntity
                || be instanceof DeviceConnectorBlockEntity || be instanceof SocketBlockEntity);
    }

    @Override
    public void inventories(BlockEntity be, InventorySink out) {
        if (be instanceof BatteryBlockEntity)
            out.add(be.getBlockPos(), IndRecords.Inventory.KIND_PLUGIN);
    }

    @Override
    public long[] describe(ServerLevel level, BlockPos pos) {
        if (!(level.getBlockEntity(pos) instanceof BatteryBlockEntity b))
            return null;
        return new long[] {IndRecords.Inventory.KIND_ENERGY, 0, 0, (long) Math.floor(b.getCapacity())};
    }

    @Override
    public StockAccess access(ServerLevel level, District.Inventory inventory) {
        if ((inventory.kind() & IndRecords.Inventory.KIND_PLUGIN) == 0 || !(level.getBlockEntity(inventory.pos()) instanceof BatteryBlockEntity))
            return null;
        BlockPos pos = inventory.pos();
        return new StockAccess() {
            @Override
            public void read(Industry.GoodsCounter out) {
                if (level.getBlockEntity(pos) instanceof BatteryBlockEntity b) {
                    long j = (long) Math.floor(b.getEnergy());
                    if (j > 0)
                        out.add(JOULE, j);
                }
            }

            @Override
            public long apply(Goods.Name good, long delta) {
                if (good == null || good.cls() != 'e' || !"powergrid:joule".equals(good.id().toString()))
                    return 0;
                if (!(level.getBlockEntity(pos) instanceof BatteryBlockEntity b))
                    return 0;
                double before = b.getEnergy();
                double after = Math.max(0, Math.min(b.getCapacity(), before + delta));
                b.setEnergy(after);
                return (long) Math.floor(b.getEnergy()) - (long) Math.floor(before);
            }
        };
    }

    @Override
    public void tick(Industry industry, long now) {
        PowerLinks.tick(industry, now);
    }

    @Override
    public void announce(Industry industry) {
        PowerLinks.announce(industry);
    }
}
