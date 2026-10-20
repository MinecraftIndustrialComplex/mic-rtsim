package com.minecraftindustrialcomplex.mic_rtsim.puppet;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

/**
 * The payload of the {@code mic_rtsim:agent} attachment (PLAN.md §6.1): which engine agent a puppet
 * embodies, and in which materialization.
 *
 * <p>The attachment is serialized with the entity, on purpose. Vanilla never saves a puppet (the
 * {@code EntitySaveMixin}), but any copy made by another path (a structure template, another mod's
 * save, a dimension change) carries the tag with it, so the join check can tell a copy from the
 * puppet itself.
 *
 * @param agent the engine id
 * @param epoch the materialization epoch, new for every spawn of the agent
 */
public record AgentTag(long agent, int epoch) {
    public static final Codec<AgentTag> CODEC = RecordCodecBuilder.create(i -> i.group(
            Codec.LONG.fieldOf("agent").forGetter(AgentTag::agent),
            Codec.INT.fieldOf("epoch").forGetter(AgentTag::epoch)
    ).apply(i, AgentTag::new));
}
