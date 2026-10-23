package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.stats.Stats;
import net.minecraft.world.entity.Entity;

/** Wandering traders (TALK.md §4): a caravan's merchant speaks as the road's voice; no credit (TK6). */
public final class TraderTalker implements Talker {
    @Override
    public String voice(Entity entity, ServerPlayer player) {
        return "trader";
    }

    /**
     * A caravan's merchant speaks for the caravan (TK49): the holder is the puppet group's agent, and the
     * engine answers from the caravan's own board, with its home village counted as "us" (TK33). A trader
     * vanilla spawned (no live puppet group) has no holder and speaks at once.
     */
    @Override
    public TalkHolder holder(Entity entity) {
        RtsimSession session = RtsimSession.current();
        AgentTag tag = RtsimAttachments.tag(entity);
        if (session == null || tag == null)
            return null;
        PuppetGroup group = session.registry().live(tag, entity.getUUID());
        if (group == null || !BodyKinds.HUMAN_CARAVAN.equals(group.kind.name()))
            return null;
        return new TalkHolder(TalkHolder.CARAVAN, group.agent);
    }

    @Override
    public Component speaker(Entity entity, ServerPlayer player) {
        if (entity.hasCustomName())
            return Talk.tr("mic_rtsim.talk.speaker.named", entity.getCustomName());
        return Talk.tr("mic_rtsim.talk.speaker.trader");
    }

    @Override
    public void react(Entity entity, ServerPlayer player) {
        Talker.super.react(entity, player);
        player.awardStat(Stats.TALKED_TO_VILLAGER);
    }
}
