package com.minecraftindustrialcomplex.mic_rtsim_powergrid.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_powergrid.MicRtsimPowerGrid;
import com.minecraftindustrialcomplex.mic_rtsim_powergrid.PowerGridPlugin;
import com.minecraftindustrialcomplex.mic_rtsim_powergrid.PowerLinks;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import org.patryk3211.powergrid.electricity.battery.BatteryBlockEntity;
import org.patryk3211.powergrid.electricity.creative.CreativeSourceBlockEntity;
import org.patryk3211.powergrid.electricity.wire.BlockWireEndpoint;
import org.patryk3211.powergrid.electricity.wire.HangingWireEntity;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_powergrid on Create Power Grid 0.6.2's own jar (with Create 6.0.10 and Architectury). */
@GameTestHolder(MicRtsimPowerGrid.MODID)
@PrefixGameTestTemplate(false)
public final class PowerGridGameTests {
    private PowerGridGameTests() {}

    /**
     * A battery a player placed is boundary stock: an inventory of kind plugin and energy whose energy
     * capacity is the battery's, read as whole joules in the sweep; a reconcile's -5,000 J is applied through
     * {@code setEnergy}.
     */
    @GameTest(template = "yard", batch = "powergrid_1", timeoutTicks = 600)
    public static void aBatteryIsBoundaryStock(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos bat = inChunk(helper, 6, 6);
        long[] id = {0};
        long[] joules = {0};
        long op = engineOp();
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, bat, block("powergrid:battery").defaultBlockState());
                    playerPlaced(helper, bat);
                    // A battery is storage and never seeds: a machine beside it makes the district.
                    BlockPos machine = inChunk(helper, 3, 3);
                    set(helper, machine, com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, machine);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, bat);
                    check("the battery's district", d != null && d.inventoryAt(bat) != null);
                    District.Inventory inv = d.inventoryAt(bat);
                    BatteryBlockEntity b = (BatteryBlockEntity) helper.getLevel().getBlockEntity(bat);
                    check("a plugin store with energy: kind " + inv.kind(), (inv.kind() & IndRecords.Inventory.KIND_PLUGIN) != 0
                            && (inv.kind() & IndRecords.Inventory.KIND_ENERGY) != 0);
                    check("its capacity, " + inv.energyCapacity() + " J", inv.energyCapacity() == (long) Math.floor(b.getCapacity()) && inv.energyCapacity() > 0);
                    id[0] = d.id;
                })
                .thenWaitUntil(() -> {
                    BatteryBlockEntity b = (BatteryBlockEntity) helper.getLevel().getBlockEntity(bat);
                    long expect = (long) Math.floor(b.getEnergy());
                    boolean seen = false;
                    for (ByteBuffer s : received(IndRecords.IND_STOCK_SAMPLE, IndRecords.StockSample.DISTRICT, id[0])) {
                        Map<String, Long> g = entries(s, IndRecords.StockSample.ENTRIES, aux(s));
                        if (g.getOrDefault(PowerGridPlugin.JOULE, -1L) == expect)
                            seen = true;
                    }
                    check("a sweep reads the battery's " + expect + " J", seen);
                    joules[0] = expect;
                    record("aBatteryIsBoundaryStock", "battery (" + engineName() + ")", expect + " J of " + (long) b.getCapacity());
                })
                .thenExecute(() -> reconcile(op, id[0], 1, helper.getLevel(), bat.getX() >> 4, bat.getZ() >> 4,
                        List.of(new Delta(ind().district(id[0]).inventoryAt(bat).index(), PowerGridPlugin.JOULE, bat, -5000))))
                .thenWaitUntil(() -> {
                    List<ByteBuffer> res = received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.OP_ID, op);
                    check("IND_RECONCILE_RESULT", res.size() == 1);
                    long applied = res.get(0).getLong(IndRecords.ReconcileResult.ENTRIES + 16);
                    BatteryBlockEntity b = (BatteryBlockEntity) helper.getLevel().getBlockEntity(bat);
                    record("aBatteryIsBoundaryStock", "after -5000", "applied " + applied + ", energy " + b.getEnergy());
                    check("-5000 applied", applied == -5000);
                    check("the battery holds 5,000 J less", Math.abs(b.getEnergy() - (joules[0] - 5000)) < 2);
                })
                .thenSucceed();
    }

    /**
     * I13: a creative voltage source in one district feeds a creative resistor in another chunk's district
     * over two hanging wires. The power measured across the lines (current times the end voltage, summed
     * over both conductors) flows from the source's district, so IND_POWER_LINK names the resistor's district
     * the sink and the source's the source.
     */
    @GameTest(template = "yard", batch = "powergrid_2", timeoutTicks = 1200)
    public static void aLineBetweenDistrictsIsAPowerLink(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        PowerLinks.forget(ind());
        // Both inside the structure (whose chunks tick), 29 blocks apart: two chunks, each its own district
        // (made directly: two chunks this close would be one detected district).
        BlockPos src = helper.absolutePos(new BlockPos(2, 1, 5));
        BlockPos res = helper.absolutePos(new BlockPos(31, 1, 5));
        long[] ids = {0, 0};
        List<Entity> wires = new ArrayList<>();
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, src, block("powergrid:creative_voltage_source").defaultBlockState());
                    set(helper, res, block("powergrid:creative_resistor").defaultBlockState());
                    playerPlaced(helper, src);
                    playerPlaced(helper, res);
                    check("two chunks", chunk(src) != chunk(res));
                    ids[0] = ind().districts().createForTest(helper.getLevel(), chunk(src)).id;
                    ids[1] = ind().districts().createForTest(helper.getLevel(), chunk(res)).id;
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    ((CreativeSourceBlockEntity) helper.getLevel().getBlockEntity(src)).setValue(5f); // low: a hot wire burns and is discarded
                    ItemStack wire = new ItemStack(BuiltInRegistries.ITEM.get(ResourceLocation.parse("powergrid:wire")), 32);
                    for (int t = 0; t < 2; t++) {
                        BlockWireEndpoint a = new BlockWireEndpoint(src, t), b = new BlockWireEndpoint(res, t);
                        record("aLineBetweenDistrictsIsAPowerLink", "terminal " + t, "valid " + a.isValid(helper.getLevel()) + "/" + b.isValid(helper.getLevel())
                                + ", nodes " + a.getNode(helper.getLevel()) + "/" + b.getNode(helper.getLevel()));
                        HangingWireEntity e = HangingWireEntity.create(helper.getLevel(), new BlockWireEndpoint(src, t), new BlockWireEndpoint(res, t), wire, null);
                        check("the wire entity joined the level", helper.getLevel().tryAddFreshEntityWithPassengers(e));
                        wires.add(e);
                    }
                })
                .thenIdle(40)
                .thenWaitUntil(() -> {
                    for (int i = 0; i < 5; i++)
                        PowerLinks.sample(ind());
                    PowerLinks.decide(ind());
                    List<ByteBuffer> links = received(IndRecords.IND_POWER_LINK, IndRecords.PowerLink.SINK, ids[1]);
                    double[] last = PowerLinks.last(ids[1]);
                    if (links.isEmpty()) {
                        StringBuilder sb = new StringBuilder();
                        var beh = ((org.patryk3211.powergrid.electricity.base.ElectricBlockEntity) helper.getLevel().getBlockEntity(src)).getElectricBehaviour();
                        sb.append("connections ").append(beh.getConnections().size()).append(", parts ").append(beh.wires().size());
                        for (var part : beh.wires())
                            sb.append(" [line ").append(part.getLine()).append(", I ").append(part.current()).append("]");
                        for (Entity e : wires)
                            sb.append(" entity alive ").append(e.isAlive()).append(" removed ").append(e.getRemovalReason());
                        record("aLineBetweenDistrictsIsAPowerLink", "lines", PowerLinks.describe(ind(), helper.getLevel()) + "; " + sb);
                    }
                    record("aLineBetweenDistrictsIsAPowerLink", "link (" + engineName() + ")", links.size() + " records, last decision "
                            + (last == null ? "none" : "source " + (long) last[0] + ", " + String.format("%.2f", last[1]) + " W"));
                    check("IND_POWER_LINK for the resistor's district", !links.isEmpty());
                    check("fed by the source's district", links.get(links.size() - 1).getLong(IndRecords.PowerLink.SOURCE) == ids[0]);
                    check("the source's district is no sink", received(IndRecords.IND_POWER_LINK, IndRecords.PowerLink.SINK, ids[0]).isEmpty());
                })
                .thenExecute(() -> {
                    for (Entity e : wires)
                        e.discard();
                    set(helper, res, Blocks.AIR.defaultBlockState());
                    set(helper, src, Blocks.AIR.defaultBlockState());
                    PowerLinks.forget(ind());
                })
                .thenSucceed();
    }
}
