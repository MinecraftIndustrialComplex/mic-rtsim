package com.minecraftindustrialcomplex.mic_rtsim_endertransmission.gametest;

import com.forsteri.createendertransmission.blocks.AbstractMatterTransmitterBlockEntity;
import com.forsteri.createendertransmission.blocks.MatterTransmitterNetwork;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_endertransmission.ChannelWatch;
import com.minecraftindustrialcomplex.mic_rtsim_endertransmission.MicRtsimEnderTransmission;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.Map;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_endertransmission on Create Ender Transmission 2.1.1's own jar. */
@GameTestHolder(MicRtsimEnderTransmission.MODID)
@PrefixGameTestTemplate(false)
public final class EnderTransmissionGameTests {
    private EnderTransmissionGameTests() {}

    /**
     * A transmitter in a district (channel 3, a password of its own): ten iron inserted through its item
     * capability and four taken out are one IND_PORT of kind 7 (-6 into the district, counterpart the
     * channel id); IND_CHANNEL_SAMPLE carries the buffer (6 iron, capacity 64); IND_CHANNEL_DELTA +20 is
     * applied to the buffer at once (APPLIED, 26 iron), and the op again is DUPLICATE.
     */
    @GameTest(template = "yard", batch = "endertransmission_1", timeoutTicks = 600)
    public static void aChannelIsSharedStockWithPorts(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ChannelWatch.forget();
        BlockPos chest = inChunk(helper, 3, 3);
        BlockPos tx = inChunk(helper, 8, 8);
        String password = "rtsim-" + Long.toHexString(System.nanoTime());
        long channel = ChannelWatch.channelId(MatterTransmitterNetwork.ITEM, 3, password);
        long[] id = {0};
        long op = engineOp();
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, Blocks.CHEST.defaultBlockState());
                    playerPlaced(helper, chest);
                    set(helper, tx, block("createendertransmission:item_transmitter").defaultBlockState());
                    AbstractMatterTransmitterBlockEntity<?> be = (AbstractMatterTransmitterBlockEntity<?>) helper.getLevel().getBlockEntity(tx);
                    be.getPersistentData().putInt("channel", 3);
                    be.getPersistentData().putString("password", password);
                    playerPlaced(helper, tx);
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null && d.chunks.contains(chunk(tx)));
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    IItemHandler h = helper.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, tx, null);
                    check("the transmitter's item handler", h != null);
                    ItemStack rest = h.insertItem(0, new ItemStack(Items.IRON_INGOT, 10), false);
                    check("ten went in", rest.isEmpty());
                    ItemStack out = h.extractItem(0, 4, false);
                    check("four came out", out.getCount() == 4);
                    ChannelWatch.flush(ind());
                    ChannelWatch.sample(ind(), ind().gameTime());
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> ports = received(IndRecords.IND_PORT, p -> p.getLong(IndRecords.Port.DISTRICT) == id[0]
                            && p.getInt(IndRecords.Port.PORT_KIND) == IndRecords.Port.CHANNEL && p.getLong(IndRecords.Port.COUNTERPART) == channel);
                    check("an IND_PORT of kind 7 on the channel", ports.size() == 1);
                    Map<String, Long> g = entries(ports.get(0), IndRecords.Port.ENTRIES, aux(ports.get(0)));
                    check("-6 iron into the district: " + g, g.equals(Map.of("i/64/minecraft:iron_ingot", -6L)));
                    List<ByteBuffer> samples = received(IndRecords.IND_CHANNEL_SAMPLE, IndRecords.ChannelSample.CHANNEL, channel);
                    check("an IND_CHANNEL_SAMPLE", !samples.isEmpty());
                    ByteBuffer s = samples.get(samples.size() - 1);
                    Map<String, Long> c = entries(s, IndRecords.ChannelSample.ENTRIES, aux(s));
                    record("aChannelIsSharedStockWithPorts", "port and sample (" + engineName() + ")", g + "; buffer " + c
                            + ", capacity " + s.getLong(IndRecords.ChannelSample.CAPACITY));
                    check("6 iron in the buffer", c.equals(Map.of("i/64/minecraft:iron_ingot", 6L)));
                    check("capacity 64 (one slot of iron)", s.getLong(IndRecords.ChannelSample.CAPACITY) == 64);
                    channelDelta(op, channel, Map.of("i/64/minecraft:iron_ingot", 20L));
                })
                .thenWaitUntil(() -> check("applied", commandResults(op).contains(Obs.CommandResult.APPLIED)))
                .thenExecute(() -> {
                    IItemHandler h = helper.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, tx, null);
                    check("26 iron now: " + h.getStackInSlot(0), h.getStackInSlot(0).is(Items.IRON_INGOT) && h.getStackInSlot(0).getCount() == 26);
                    channelDelta(op, channel, Map.of("i/64/minecraft:iron_ingot", 20L));
                })
                .thenWaitUntil(() -> check("the op again is a duplicate", commandResults(op).contains(Obs.CommandResult.DUPLICATE_OP)))
                .thenExecute(() -> {
                    IItemHandler h = helper.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, tx, null);
                    check("still 26", h.getStackInSlot(0).getCount() == 26);
                    h.extractItem(0, 26, false); // leave the shared static buffer empty for the next run
                    ChannelWatch.forget();
                })
                .thenSucceed();
    }
}
