package com.minecraftindustrialcomplex.mic_rtsim.puppet;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.world.entity.Entity;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

import java.util.function.Supplier;

/** The bridge's data attachments. */
public final class RtsimAttachments {
    private RtsimAttachments() {}

    private static final DeferredRegister<AttachmentType<?>> TYPES =
            DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, MicRtsim.MODID);

    /**
     * {@code mic_rtsim:agent}: marks an entity as a puppet. The default value is never used: the bridge
     * only ever sets it, and reads it with {@code getExistingDataOrNull}.
     */
    public static final Supplier<AttachmentType<AgentTag>> AGENT = TYPES.register("agent",
            () -> AttachmentType.builder(() -> new AgentTag(0L, 0)).serialize(AgentTag.CODEC).build());

    /**
     * {@code mic_rtsim:pending}: journal records a chunk or an entity carries until the engine has
     * acknowledged them ({@link PendingRecords}). Saved with its holder, on purpose.
     */
    public static final Supplier<AttachmentType<PendingRecords>> PENDING = TYPES.register("pending",
            () -> AttachmentType.builder(() -> PendingRecords.EMPTY).serialize(PendingRecords.CODEC, p -> !p.isEmpty()).build());

    /**
     * {@code mic_rtsim:gossip_ops}: the gossip op ids a villager was written with (C20), so it takes
     * each at most once. Saved with the villager, on purpose: it is the villager's own record.
     */
    public static final Supplier<AttachmentType<com.minecraftindustrialcomplex.mic_rtsim.village.GossipWriter.GossipOps>> GOSSIP_OPS =
            TYPES.register("gossip_ops", () -> AttachmentType.builder(() -> com.minecraftindustrialcomplex.mic_rtsim.village.GossipWriter.GossipOps.EMPTY)
                    .serialize(com.minecraftindustrialcomplex.mic_rtsim.village.GossipWriter.GossipOps.CODEC, g -> !g.isEmpty()).build());

    /**
     * {@code mic_rtsim:player_blocks}: which blocks of a chunk players placed (per section, a 4096-bit
     * set). Village growth never builds over them (PLAN.md §8.8). Saved with the chunk.
     */
    public static final Supplier<AttachmentType<com.minecraftindustrialcomplex.mic_rtsim.growth.PlayerBlocks>> PLAYER_BLOCKS =
            TYPES.register("player_blocks", () -> AttachmentType.builder(com.minecraftindustrialcomplex.mic_rtsim.growth.PlayerBlocks::new)
                    .serialize(com.minecraftindustrialcomplex.mic_rtsim.growth.PlayerBlocks.CODEC, p -> !p.isEmpty()).build());

    public static void register(IEventBus modBus) {
        TYPES.register(modBus);
    }

    /** The entity's tag, or null for an ordinary entity. Cheap: one map lookup, null map when untouched. */
    public static AgentTag tag(Entity entity) {
        return entity.getExistingDataOrNull(AGENT);
    }

    public static boolean isTagged(Entity entity) {
        return entity.hasData(AGENT);
    }
}
