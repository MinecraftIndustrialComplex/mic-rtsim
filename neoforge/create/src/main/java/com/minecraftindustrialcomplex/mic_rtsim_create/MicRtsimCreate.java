package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.mojang.logging.LogUtils;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import org.slf4j.Logger;

/**
 * mic_rtsim_create: the Create plugin of the industry workstream (INDUSTRY.md §12, layer 2; A19).
 * Built around Create 6.0.10 (D4 allows a plugin to depend on its mod). It adds no block, item or entity
 * (D3). It registers nothing a player can see: what a distant factory is doing is not shown to players (I18).
 * The one thing it leaves in the world is Create's own: a repair schematic where an unwatched attack broke
 * blocks ({@link RepairSchematics}, I1).
 */
@Mod(MicRtsimCreate.MODID)
public final class MicRtsimCreate {
    public static final String MODID = "mic_rtsim_create";
    public static final Logger LOGGER = LogUtils.getLogger();

    public MicRtsimCreate(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new CreatePlugin());
        IndustryApi.register(new CreateDefencePlugin());
        IndustryApi.register(new RepairSchematics());
        IndustryApi.registerCommand(IndRecords.IND_CARRIAGE_DELTA, TrainWatch::carriageDelta);
        IndustryApi.registerCommand(IndRecords.IND_WRITE_PACKAGE, WritePackages::accept);
    }
}
