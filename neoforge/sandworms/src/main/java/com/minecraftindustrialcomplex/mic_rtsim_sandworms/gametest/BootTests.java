package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.neoforged.fml.ModList;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.lang.management.ManagementFactory;

/** Start-up numbers: the JVM's uptime when the first test runs, and which mods and versions are loaded. */
@GameTestHolder(MicRtsimSandworms.MODID)
@PrefixGameTestTemplate(false)
public final class BootTests {
    private BootTests() {}

    @GameTest(template = "arena", timeoutTicks = 100, batch = "00_boot")
    public static void boot(GameTestHelper helper) {
        if (!Kit.run(helper, "boot", false)) return;
        Kit.say("boot", "jvm uptime at first test (ms)", ManagementFactory.getRuntimeMXBean().getUptime());
        for (String id : new String[] {"sandworm_mod", "lodestone", "aaa_particles", "geckolib", "architectury", "create", "createdieselgenerators"})
            ModList.get().getModContainerById(id).ifPresentOrElse(c -> Kit.say("boot", id + " version", c.getModInfo().getVersion()), () -> Kit.say("boot", id, "not loaded"));
        Kit.say("boot", "mods loaded", ModList.get().size());
        helper.succeed();
    }
}
