package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkCandidates;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkContext;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.stats.Stats;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.entity.ai.memory.MemoryModuleType;
import net.minecraft.world.entity.ai.behavior.EntityTracker;
import net.minecraft.world.entity.npc.Villager;

/**
 * Vanilla villagers (TALK.md §4): adults by profession, babies as children. The relationship is
 * vanilla's own gossip (§7, TK1).
 */
public final class VillagerTalker implements Talker {
    /** The gossip a day's first talk adds (TK1): 3 survives vanilla's decay of 1 a day. */
    public static final int GOSSIP = 3;

    private static ResourceLocation professionKey(Villager v) {
        return BuiltInRegistries.VILLAGER_PROFESSION.getKey(v.getVillagerData().getProfession());
    }

    @Override
    public String voice(Entity entity, ServerPlayer player) {
        Villager v = (Villager) entity;
        if (v.isBaby())
            return "child";
        return "villager." + professionKey(v).getPath();
    }

    @Override
    public String home(Entity entity) {
        Village village = RtsimApi.villageOf(entity);
        return village == null ? null : Names.village(village.id);
    }

    @Override
    public Component speaker(Entity entity, ServerPlayer player) {
        Villager v = (Villager) entity;
        if (v.hasCustomName())
            return Talk.tr("mic_rtsim.talk.speaker.named", v.getCustomName());
        String home = home(v);
        if (v.isBaby())
            return home == null ? Talk.tr("mic_rtsim.talk.speaker.child.unplaced") : Talk.tr("mic_rtsim.talk.speaker.child", home);
        ResourceLocation prof = professionKey(v);
        Component name;
        if (prof.getPath().equals("none"))
            name = Talk.tr("mic_rtsim.talk.profession.none");
        else
            name = Component.translatable("entity.minecraft.villager."
                    + (!"minecraft".equals(prof.getNamespace()) ? prof.getNamespace() + '.' : "") + prof.getPath());
        return home == null ? Talk.tr("mic_rtsim.talk.speaker.villager.unplaced", name)
                : Talk.tr("mic_rtsim.talk.speaker.villager", home, name);
    }

    @Override
    public Attitude attitude(Entity entity, ServerPlayer player) {
        return ((Villager) entity).getPlayerReputation(player) < -25 ? Attitude.WARY : Attitude.FRIENDLY;
    }

    @Override
    public void candidates(TalkContext ctx, TalkCandidates out) {
        int rep = ((Villager) ctx.entity()).getPlayerReputation(ctx.player());
        if (rep >= 25)
            out.add("mood.liked", 1.5);
        else if (rep <= -100)
            out.add("mood.hated", 3);
        else if (rep <= -25)
            out.add("mood.disliked", 1.5);
    }

    /** TK16: babies, sleeping and busy villagers give no credit. */
    @Override
    public boolean credits(Entity entity, ServerPlayer player) {
        State s = state(entity, player);
        return !((Villager) entity).isBaby() && (s == State.NONE || s == State.HURT);
    }

    @Override
    public void credit(Entity entity, ServerPlayer player) {
        ((Villager) entity).getGossips().add(player.getUUID(), GossipType.MINOR_POSITIVE, GOSSIP);
    }

    @Override
    public void react(Entity entity, ServerPlayer player) {
        Villager v = (Villager) entity;
        if (!v.isSleeping())
            v.getBrain().setMemoryWithExpiry(MemoryModuleType.LOOK_TARGET, new EntityTracker(player, true), 40L);
        v.playAmbientSound();
        player.awardStat(Stats.TALKED_TO_VILLAGER);
    }
}
