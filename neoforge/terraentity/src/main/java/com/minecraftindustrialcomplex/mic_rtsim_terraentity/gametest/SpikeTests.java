package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.neoforged.fml.ModList;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

@GameTestHolder(MicRtsimTerraEntity.MODID)
@PrefixGameTestTemplate(false)
public final class SpikeTests {
    private SpikeTests() {}

    @GameTest(template = "arena", timeoutTicks = 100, batch = "boot")
    public static void boots(GameTestHelper helper) {
        MicRtsimTerraEntity.LOGGER.info("[spike] boots: terra_entity loaded = {}, geckolib = {}", ModList.get().isLoaded("terra_entity"),
                ModList.get().getModContainerById("geckolib").map(c -> c.getModInfo().getVersion().toString()).orElse("none"));
        helper.succeed();
    }
}
