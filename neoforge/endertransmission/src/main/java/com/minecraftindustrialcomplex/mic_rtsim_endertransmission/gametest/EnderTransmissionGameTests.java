package com.minecraftindustrialcomplex.mic_rtsim_endertransmission.gametest;

import com.forsteri.createendertransmission.transmitUtil.ITransmitter;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_endertransmission.ChannelLinks;
import com.minecraftindustrialcomplex.mic_rtsim_endertransmission.MicRtsimEnderTransmission;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_endertransmission on Create Ender Transmission 2.1.1's own jar. */
@GameTestHolder(MicRtsimEnderTransmission.MODID)
@PrefixGameTestTemplate(false)
public final class EnderTransmissionGameTests {
    private EnderTransmissionGameTests() {}

    /**
     * I13 as corrected: a channel's endpoints are one district. Four transmitters at the corners of a 3 by
     * 3 chunk square, so no two are in neighbouring chunks (a district is a group of 8-neighbour industrial
     * chunks) and nothing else joins them: two item transmitters on channel 3 with a password of their
     * own, and two rotation transmitters on channel 4 with another. Each pair is one district holding both
     * chunks and not the chunk between; the two pairs are two districts. Ten iron
     * through the item channel (in at one end, out at the other) is no port. An item transmitter on the
     * same channel in the Nether puts a warning on the district's ledger, cleared when it goes. A new
     * password at one end splits the pair.
     */
    @GameTest(template = "yard", batch = "endertransmission_1", timeoutTicks = 600)
    public static void aChannelsEndpointsAreOneDistrict(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        ChannelLinks.forget();
        // The yard covers the first corner chunk; the others are past it, loaded for the test.
        BlockPos itemA = inChunk(helper, 3, 3), itemB = inChunk(helper, 35, 3);
        BlockPos rotA = inChunk(helper, 3, 35), rotB = inChunk(helper, 35, 35);
        List<BlockPos> all = List.of(itemA, itemB, rotA, rotB);
        String tag = Long.toHexString(System.nanoTime());
        ServerLevel nether = helper.getLevel().getServer().getLevel(Level.NETHER);
        BlockPos far = new BlockPos(itemA.getX() >> 3, 100, itemA.getZ() >> 3);
        long[] ids = {0, 0};
        helper.startSequence()
                .thenExecute(() -> {
                    for (BlockPos p : all)
                        helper.getLevel().setChunkForced(p.getX() >> 4, p.getZ() >> 4, true);
                    place(helper.getLevel(), itemA, "item_transmitter", 3, "items-" + tag);
                    place(helper.getLevel(), itemB, "item_transmitter", 3, "items-" + tag);
                    place(helper.getLevel(), rotA, "energy_transmitter", 4, "rotation-" + tag);
                    place(helper.getLevel(), rotB, "energy_transmitter", 4, "rotation-" + tag);
                    for (BlockPos p : all)
                        playerPlaced(helper, p);
                })
                .thenWaitUntil(() -> {
                    District items = districtAt(helper, itemA), rotation = districtAt(helper, rotA);
                    check("the item pair's district", items != null && districtAt(helper, itemB) == items);
                    check("it holds both chunks: " + items.chunks.size(), items.chunks.contains(chunk(itemA)) && items.chunks.contains(chunk(itemB)));
                    check("the rotation pair's district", rotation != null && districtAt(helper, rotB) == rotation);
                    check("it holds both chunks: " + rotation.chunks.size(), rotation.chunks.contains(chunk(rotA)) && rotation.chunks.contains(chunk(rotB)));
                    check("two districts", items.id != rotation.id);
                    check("not the chunk between", !items.chunks.contains(chunk(inChunk(helper, 19, 3))));
                    ids[0] = items.id;
                    ids[1] = rotation.id;
                    record("aChannelsEndpointsAreOneDistrict", "districts (" + engineName() + ")", "items " + items.id + " " + items.chunks.size()
                            + " chunks, rotation " + rotation.id + " " + rotation.chunks.size() + " chunks");
                })
                .thenExecute(() -> {
                    IItemHandler in = helper.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, itemA, null);
                    IItemHandler out = helper.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, itemB, null);
                    check("both item handlers", in != null && out != null);
                    check("ten in at one end", in.insertItem(0, new ItemStack(Items.IRON_INGOT, 10), false).isEmpty());
                    check("ten out at the other", out.extractItem(0, 10, false).getCount() == 10);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    long channelPorts = received(IndRecords.IND_PORT, p -> p.getLong(IndRecords.Port.DISTRICT) == ids[0]
                            && p.getInt(IndRecords.Port.PORT_KIND) == IndRecords.Port.CHANNEL).size();
                    check("no channel port: " + channelPorts, channelPorts == 0);
                    nether.getChunk(far);
                    place(nether, far, "item_transmitter", 3, "items-" + tag);
                    ChannelLinks.warnNow(ind());
                    List<String> w = ind().ledgers().warnings(ids[0]);
                    check("the ledger warns of the Nether: " + w, w.size() == 1 && w.get(0).contains("minecraft:the_nether"));
                    check("not the other district", ind().ledgers().warnings(ids[1]).isEmpty());
                    record("aChannelsEndpointsAreOneDistrict", "warning", w.get(0));
                    nether.setBlock(far, Blocks.AIR.defaultBlockState(), 3);
                    ChannelLinks.warnNow(ind());
                    check("cleared when it goes", ind().ledgers().warnings(ids[0]).isEmpty());
                    // A new password at one end, as its settings screen sets it: the next check marks
                    // its chunk, and detection splits the pair.
                    ITransmitter t = (ITransmitter) helper.getLevel().getBlockEntity(itemB);
                    t.reloadSettings();
                    helper.getLevel().getBlockEntity(itemB).getPersistentData().putString("password", "other-" + tag);
                    t.afterReload();
                    ChannelLinks.warnNow(ind());
                })
                .thenWaitUntil(() -> {
                    District a = districtAt(helper, itemA), b = districtAt(helper, itemB);
                    check("a new password splits the pair", a != null && b != null && a != b);
                    record("aChannelsEndpointsAreOneDistrict", "after a new password", "districts " + a.id + " and " + b.id);
                })
                .thenExecute(() -> {
                    for (BlockPos p : all) {
                        helper.getLevel().setBlock(p, Blocks.AIR.defaultBlockState(), 3);
                        helper.getLevel().setChunkForced(p.getX() >> 4, p.getZ() >> 4, false);
                    }
                })
                .thenSucceed();
    }

    /** Places a transmitter and sets its channel and password as its settings screen does. */
    private static void place(ServerLevel level, BlockPos pos, String id, int channel, String password) {
        level.setBlock(pos, block("createendertransmission:" + id).defaultBlockState(), 3);
        BlockEntity be = level.getBlockEntity(pos);
        check(id + " at " + pos, be instanceof ITransmitter);
        ITransmitter t = (ITransmitter) be;
        t.reloadSettings();
        be.getPersistentData().putInt("channel", channel);
        be.getPersistentData().putString("password", password);
        t.afterReload();
    }
}
