package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.neoforged.fml.ModList;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.lang.management.ManagementFactory;

/** Start-up numbers: the JVM's uptime when the first test runs, and which mods and versions are loaded. */
@GameTestHolder(MicRtsimTerraEntity.MODID)
@PrefixGameTestTemplate(false)
public final class BootTests {
    private BootTests() {}

    @GameTest(template = "arena", timeoutTicks = 100, batch = "00_boot")
    public static void boot(GameTestHelper helper) {
        if (!Kit.run(helper, "boot", false)) return;
        Kit.say("boot", "jvm uptime at first test (ms)", ManagementFactory.getRuntimeMXBean().getUptime());
        Kit.say("boot", "terra_entity loaded", Kit.terraLoaded());
        ModList.get().getModContainerById("terra_entity").ifPresent(c -> Kit.say("boot", "terra_entity version", c.getModInfo().getVersion()));
        ModList.get().getModContainerById("geckolib").ifPresent(c -> Kit.say("boot", "geckolib version in use", c.getModInfo().getVersion()));
        Kit.say("boot", "mods loaded", ModList.get().size());
        helper.succeed();
    }
}
