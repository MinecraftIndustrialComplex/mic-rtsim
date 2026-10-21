package com.minecraftindustrialcomplex.mic_rtsim_radar.gametest;

import com.happysg.radar.block.behavior.networks.NetworkData;
import com.happysg.radar.block.behavior.networks.config.TargetingConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim_radar.MicRtsimRadar;
import com.minecraftindustrialcomplex.mic_rtsim_radar.RadarPlugin;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_radar on Create Radars 0.4.9.4 and Create Big Cannons 5.11.7. */
@GameTestHolder(MicRtsimRadar.MODID)
@PrefixGameTestTemplate(false)
public final class RadarGameTests {
    private RadarGameTests() {}

    /**
     * A Create Radars network (a stationary radar, the mount among its weapon mounts, targeting mobs with
     * auto-target and auto-fire) is read from its saved data: networked, configured, radar running. With no
     * cannon assembled on the mount it is not a turret, and the district's survey has turret strength 0;
     * auto-fire off, it is not configured. (An assembled, loaded autocannon is not built here.)
     */
    @GameTest(template = "yard", batch = "radar_1", timeoutTicks = 600)
    public static void aRadarNetworkIsReadAndAnUnloadedCannonIsNoTurret(GameTestHelper helper) {
        fastConfig();
        forgetAll(helper);
        BlockPos chest = inChunk(helper, 2, 2);
        BlockPos mount = inChunk(helper, 6, 6);
        BlockPos radar = inChunk(helper, 9, 6);
        BlockPos filterer = inChunk(helper, 11, 6);
        long[] id = {0};
        NetworkData.Group[] group = {null};
        helper.startSequence()
                .thenExecute(() -> {
                    set(helper, chest, com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MACHINE.get().defaultBlockState());
                    playerPlaced(helper, chest);
                    set(helper, mount, block("createbigcannons:cannon_mount").defaultBlockState());
                    set(helper, radar, block("create_radar:plane_radar").defaultBlockState());
                    NetworkData nd = NetworkData.get(helper.getLevel());
                    group[0] = nd.getOrCreateGroup(helper.getLevel().dimension(), filterer);
                    nd.attachRadar(group[0], radar, NetworkData.RadarKind.STATIONARY);
                    group[0].usedWeaponMounts.add(mount);
                    nd.setTargetingConfig(group[0], new TargetingConfig(false, false, true, false, false, true, true, false));
                })
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the district", d != null && d.chunks.contains(chunk(mount)));
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    RadarPlugin.Facts f = RadarPlugin.facts(helper.getLevel(), mount);
                    record("aRadarNetworkIsReadAndAnUnloadedCannonIsNoTurret", "facts (" + engineName() + ")", f);
                    check("driven by the network", f.networked());
                    check("mob, auto-target, auto-fire", f.configured());
                    check("the radar runs", f.radarRunning());
                    check("no cannon assembled", !f.assembled() && !f.counts());
                    ind().threats().surveySoon(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    long[] s = ind().threats().lastSurvey(id[0]);
                    check("a survey", s != null);
                    check("no turret strength: " + s[10], s[10] == 0 && s[11] == 0 && s[12] == 0 && s[13] == 0);
                    NetworkData.get(helper.getLevel()).setTargetingConfig(group[0], new TargetingConfig(false, false, true, false, false, true, false, false));
                    check("auto-fire off: not configured", !RadarPlugin.facts(helper.getLevel(), mount).configured());
                    NetworkData.get(helper.getLevel()).dissolveNetworkForBrokenController(helper.getLevel(), filterer);
                })
                .thenSucceed();
    }
}
