package com.minecraftindustrialcomplex.mic_rtsim_terraentity;

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
 * mic_rtsim_terraentity: a spike. It owns no logic; the GameTests are in {@code gametest/}. The one thing
 * it does at load is offer the silencing datapack ({@code silence_pack/}) when the run asks for it
 * ({@code -PteSilence=datapack}); it is a real server datapack, enabled and put on top like a pack's own.
 */
@Mod(MicRtsimTerraEntity.MODID)
public final class MicRtsimTerraEntity {
    public static final String MODID = "mic_rtsim_terraentity";
    public static final Logger LOGGER = LogUtils.getLogger();

    public MicRtsimTerraEntity(IEventBus modBus) {
        modBus.addListener(MicRtsimTerraEntity::onPackFinders);
        Probes.register();
    }

    private static void onPackFinders(AddPackFindersEvent event) {
        if (event.getPackType() != PackType.SERVER_DATA || !"datapack".equals(System.getProperty("mic_rtsim_terraentity.silence")))
            return;
        Path root = ModList.get().getModFileById(MODID).getFile().findResource("silence_pack");
        PackLocationInfo info = new PackLocationInfo("mic_rtsim_terraentity_silence", Component.literal("Terra Entity silenced"), PackSource.BUILT_IN, Optional.empty());
        Pack pack = Pack.readMetaAndCreate(info, new PathPackResources.PathResourcesSupplier(root), PackType.SERVER_DATA,
                new PackSelectionConfig(true, Pack.Position.TOP, false));
        event.addRepositorySource(consumer -> consumer.accept(pack));
        LOGGER.info("[spike] offering the silencing datapack from {}", root);
    }
}
