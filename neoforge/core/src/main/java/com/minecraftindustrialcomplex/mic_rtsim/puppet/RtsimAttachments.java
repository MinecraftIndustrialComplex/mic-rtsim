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
     * {@code mic_rtsim:talk_days}: for each player, the last game day a talk with this entity earned a
     * credit (TK25, TALK.md §7). Saved with the entity; at most 16 players.
     */
    public static final Supplier<AttachmentType<com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDays>> TALK_DAYS =
            TYPES.register("talk_days", () -> AttachmentType.builder(() -> com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDays.EMPTY)
                    .serialize(com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDaysCodec.CODEC, d -> !d.isEmpty()).build());

    /**
     * {@code mic_rtsim:player_blocks}: which blocks of a chunk players placed (per section, a 4096-bit
     * set). Village growth never builds over them (PLAN.md §8.8). Saved with the chunk.
     */
    public static final Supplier<AttachmentType<com.minecraftindustrialcomplex.mic_rtsim.growth.PlayerBlocks>> PLAYER_BLOCKS =
            TYPES.register("player_blocks", () -> AttachmentType.builder(com.minecraftindustrialcomplex.mic_rtsim.growth.PlayerBlocks::new)
                    .serialize(com.minecraftindustrialcomplex.mic_rtsim.growth.PlayerBlocks.CODEC, p -> !p.isEmpty()).build());

    /**
     * {@code mic_rtsim:camp_member}: a camp body's camp, slot and generation (MOWZIE.md §4.9). Saved with
     * the body, on purpose: bodies at home are the jar's own entities, saved by vanilla.
     */
    public static final Supplier<AttachmentType<com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember>> CAMP_MEMBER =
            TYPES.register("camp_member", () -> AttachmentType.builder(() -> com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember.NONE)
                    .serialize(com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember.CODEC, m -> !m.isNone()).build());

    /**
     * {@code mic_rtsim:applied_ops} on a level: the engine ops applied to that level's own attachments, kept in the
     * same file as them (docs/choices-journal-barrier.md RP-207; {@code DurableOpRings.SERIALIZER}).
     */
    public static final Supplier<AttachmentType<com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing>> LEVEL_APPLIED_OPS =
            TYPES.register("applied_ops", () -> AttachmentType.builder(() -> new com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing())
                    .serialize(com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRings.SERIALIZER).build());

    /** The entity's camp tag, or null for a body no camp holds. */
    public static com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember campMember(Entity entity) {
        com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember m = entity.getExistingDataOrNull(CAMP_MEMBER);
        return m == null || m.isNone() ? null : m;
    }

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
