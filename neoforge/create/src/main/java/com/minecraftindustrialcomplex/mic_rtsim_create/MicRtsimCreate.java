package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim_create.bench.FarTrainBench;
import com.mojang.logging.LogUtils;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import org.slf4j.Logger;

/**
 * mic_rtsim_create: the Create plugin of the industry workstream (INDUSTRY.md §12, layer 2; A19).
 * Built around Create 6.0.10 (D4 allows a plugin to depend on its mod). It adds no block, item or entity
 * (D3). What a distant factory is doing is not shown to players (I18). The one thing it leaves in the world is
 * Create's own: a repair schematic where an unwatched attack broke blocks ({@link RepairSchematics}, I1).
 *
 * <p>On clients it is also the far view's train source ({@code client.MicRtsimCreateClient}): Create trains
 * beyond the vanilla view go into the core's far registry for a renderer such as mic_rtsim_dh to draw, as the
 * player's eyes would see them (DECISIONS.md DH-1, DH-2). With the core's far feed on the server, the trains
 * come from {@link TrainFeedSource} once a second; without it, the client reads Create's own train map sync.
 * {@link FarTrainBench} lays out synthetic trains for the far-view bench, and only with its property.
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
        IndustryApi.registerCommand(IndRecords.IND_WRITE_CHAIN, WriteChains::accept);
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(ContraptionAnchors::joined);
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(ContraptionAnchors::left);
        StaleKinetics.register();
        // The far feed's trains (docs/boundary.md §12.3): read on the server, sent to subscribers only
        com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi.registerFarSource(new TrainFeedSource());
        // Last, and only if no one cancelled the join: a REAL for a carriage that never joins would hide it
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(net.neoforged.bus.api.EventPriority.LOWEST, false,
                net.neoforged.neoforge.event.entity.EntityJoinLevelEvent.class, TrainFeedSource::joining);
        // A stopped server's carriages are not kept (single player starts another server in the same JVM)
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener(
                (net.neoforged.neoforge.event.server.ServerStoppedEvent e) -> CarriageShapes.clear());
        if (FarTrainBench.ENABLED) {
            FarTrainBench.register();
            com.minecraftindustrialcomplex.mic_rtsim_create.bench.BenchCarriages.register();
        }
    }
}
