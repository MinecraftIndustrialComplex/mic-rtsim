package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.mojang.logging.LogUtils;
import com.simibubi.create.api.behaviour.display.DisplaySource;
import com.simibubi.create.api.registry.CreateRegistries;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredRegister;
import org.slf4j.Logger;

/**
 * mic_rtsim_create: the Create plugin of the industry workstream (INDUSTRY.md §12, layer 2; A19).
 * Built around Create 6.0.10 (D4 allows a plugin to depend on its mod). It adds no block, item or entity
 * (D3); the one registration is the "District ledger" display source.
 */
@Mod(MicRtsimCreate.MODID)
public final class MicRtsimCreate {
    public static final String MODID = "mic_rtsim_create";
    public static final Logger LOGGER = LogUtils.getLogger();

    private static final DeferredRegister<DisplaySource> SOURCES = DeferredRegister.create(CreateRegistries.DISPLAY_SOURCE, MODID);
    public static final DeferredHolder<DisplaySource, DistrictLedgerSource> DISTRICT_LEDGER =
            SOURCES.register("district_ledger", DistrictLedgerSource::new);

    public MicRtsimCreate(IEventBus modBus, ModContainer container) {
        SOURCES.register(modBus);
        IndustryApi.register(new CreatePlugin());
        IndustryApi.registerCommand(IndRecords.IND_CARRIAGE_DELTA, TrainWatch::carriageDelta);
        modBus.addListener(MicRtsimCreate::setup);
    }

    /** Binds the display source to Stock Links and Train Stations (I18): DisplaySource.BY_BLOCK. */
    private static void setup(FMLCommonSetupEvent event) {
        event.enqueueWork(() -> {
            for (String id : new String[] {"create:stock_link", "create:track_station"}) {
                Block block = BuiltInRegistries.BLOCK.get(ResourceLocation.parse(id));
                if (block == Blocks.AIR) {
                    LOGGER.warn("[mic_rtsim_create] no block {}: the District ledger display source is not bound to it", id);
                    continue;
                }
                DisplaySource.BY_BLOCK.add(block, DISTRICT_LEDGER.get());
            }
        });
    }
}
