package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
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
