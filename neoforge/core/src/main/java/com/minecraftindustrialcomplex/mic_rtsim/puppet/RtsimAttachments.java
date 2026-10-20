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
