package com.minecraftindustrialcomplex.mic_rtsim_sandworms;

import com.mojang.logging.LogUtils;
import net.minecraft.network.chat.Component;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PackSelectionConfig;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.PathPackResources;
import net.minecraft.server.packs.repository.Pack;
import net.minecraft.server.packs.repository.PackSource;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModList;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.event.AddPackFindersEvent;
import org.slf4j.Logger;

import java.nio.file.Path;
import java.util.Optional;

/**
 * mic_rtsim_sandworms: a spike. It owns no logic; the GameTests are in {@code gametest/}. At load it offers one datapack
 * ({@code biome_pack/}) unless the run says {@code -PswBiome=default}: it replaces the tag {@code sandworm_mod:can_spawn_sandworms}
 * (the mod's "desert" test, which gates every target and every spawn) with every overworld biome, because the GameTest world is plains.
 */
@Mod(MicRtsimSandworms.MODID)
public final class MicRtsimSandworms {
    public static final String MODID = "mic_rtsim_sandworms";
    public static final Logger LOGGER = LogUtils.getLogger();

    public MicRtsimSandworms(IEventBus modBus) {
        modBus.addListener(MicRtsimSandworms::onPackFinders);
        Probes.register();
    }

    private static void onPackFinders(AddPackFindersEvent event) {
        if (event.getPackType() != PackType.SERVER_DATA || "default".equals(System.getProperty("mic_rtsim_sandworms.biome")))
            return;
        Path root = ModList.get().getModFileById(MODID).getFile().findResource("biome_pack");
        PackLocationInfo info = new PackLocationInfo("mic_rtsim_sandworms_biomes", Component.literal("Sandworms: every overworld biome"), PackSource.BUILT_IN, Optional.empty());
        Pack pack = Pack.readMetaAndCreate(info, new PathPackResources.PathResourcesSupplier(root), PackType.SERVER_DATA,
                new PackSelectionConfig(true, Pack.Position.TOP, false));
        event.addRepositorySource(consumer -> consumer.accept(pack));
        LOGGER.info("[spike] offering the biome datapack from {}", root);
    }
}
