package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.mojang.logging.LogUtils;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import org.slf4j.Logger;

/**
 * mic_rtsim_mowziesmobs: Mowzie's Mobs 1.8.2 for the conflict module (MOWZIE.md; slices from M5). The
 * Frostmaw's den and the Umvuthana's grove are camps whose members at home are the jar's own bodies,
 * kept by the core's camp reconciler (§4.9): this plugin finds the camps ({@link MowzieCamps}) and tells
 * the core what their bodies are ({@link DenKind}, {@link GroveKind}); the core does the books, the tags,
 * the markers and the records (D4). The two cultures' bodies are embodied as the jar's own entities.
 *
 * <p>Every hook is a NeoForge event; nothing here reaches into Mowzie's Mobs but its public API (D3: no
 * bespoke entities).
 */
@Mod(MicRtsimMowziesMobs.MODID)
public final class MicRtsimMowziesMobs {
    public static final String MODID = "mic_rtsim_mowziesmobs";
    public static final Logger LOGGER = LogUtils.getLogger();

    /** The cultures' bodies (crates/rtsim-conflict/data/cultures/frostmaw.json, umvuthana.json). */
    public static final String FROSTMAW = "mic_rtsim:conflict/frostmaw/frostmaw";
    public static final String WARRIOR = "mic_rtsim:conflict/umvuthana/warrior";
    public static final String RAPTOR = "mic_rtsim:conflict/umvuthana/raptor";
    public static final String ENVOY = "mic_rtsim:conflict/umvuthana/envoy";

    /** The jar's camp structures (its worldgen/structure ids). */
    public static final String DEN_STRUCTURE = "mowziesmobs:frostmaw_spawn";
    public static final String GROVE_STRUCTURE = "mowziesmobs:umvuthana_grove";

    public MicRtsimMowziesMobs(IEventBus modBus, ModContainer container) {
        // Each culture body is the jar's own entity (MZB-216): the grove's warrior and envoy are its
        // grove warrior (mowziesmobs:umvuthana, EntityUmvuthanaMinion), a raptor pack its raptor.
        BodyKinds.registerStandIn(FROSTMAW, "mowziesmobs:frostmaw");
        BodyKinds.registerStandIn(WARRIOR, "mowziesmobs:umvuthana");
        BodyKinds.registerStandIn(RAPTOR, "mowziesmobs:umvuthana_raptor");
        BodyKinds.registerStandIn(ENVOY, "mowziesmobs:umvuthana");
        RtsimApi.registerCampKind(DenKind.INSTANCE);
        RtsimApi.registerCampKind(GroveKind.INSTANCE);
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(ChunkEvent.Load.class, MowzieCamps::onChunkLoad);
        bus.addListener(ServerTickEvent.Post.class, e -> MowzieCamps.drain());
        bus.addListener(ServerStoppedEvent.class, e -> MowzieCamps.clear());
    }
}
