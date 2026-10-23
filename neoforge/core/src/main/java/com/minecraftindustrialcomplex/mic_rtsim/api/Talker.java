package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.item.trading.Merchant;

/**
 * An entity that talks when a player crouches and right-clicks it with an empty hand (TALK.md §2, §4).
 * A plugin registers one per entity type through {@link RtsimApi#registerTalker}; the core does the
 * rest: the gesture, the rate limits, picking one line, and sending it to that player only.
 *
 * <p>All methods run on the server thread.
 */
public interface Talker {
    /** What the talker feels toward the player. */
    enum Attitude { FRIENDLY, WARY, HOSTILE }

    /** What the talker is doing. A state other than NONE changes what it says (TALK.md §5.1). */
    enum State { NONE, ASLEEP, BUSY, HURT }

    /** Whether this entity talks to this player now; false lets the click go on untouched. */
    default boolean talks(Entity entity, ServerPlayer player) {
        return true;
    }

    /**
     * Its voice, a dotted path ("villager.farmer"). Every level of the chain that has lines for a topic
     * contributes (TK31): the voice itself, then each parent made by dropping the last part
     * ("villager"), each parent at 0.6 of the weight of the level below it. {@code any} is used only
     * when no level has lines for the topic.
     */
    String voice(Entity entity, ServerPlayer player);

    /** The speaker as chat shows it before the line. */
    Component speaker(Entity entity, ServerPlayer player);

    /** The name of the settlement it calls home, for fluff.home (%1$s), or null. */
    default String home(Entity entity) {
        return null;
    }

    /** Whose board and market it speaks from (slice 2 uses it), or null. */
    default TalkHolder holder(Entity entity) {
        return null;
    }

    default Attitude attitude(Entity entity, ServerPlayer player) {
        return Attitude.FRIENDLY;
    }

    /** ASLEEP if a sleeping LivingEntity, BUSY if a Merchant trading with another player, HURT under half health, else NONE. */
    default State state(Entity entity, ServerPlayer player) {
        if (entity instanceof LivingEntity living && living.isSleeping())
            return State.ASLEEP;
        if (entity instanceof Merchant merchant && merchant.getTradingPlayer() != null && merchant.getTradingPlayer() != player)
            return State.BUSY;
        if (entity instanceof LivingEntity living && living.getHealth() < living.getMaxHealth() / 2f)
            return State.HURT;
        return State.NONE;
    }

    /**
     * Whether it makes small talk. When false the core adds no fluff and no mood (and no state lines),
     * only the talker's own candidates; with none, the click is still cancelled and nothing is said.
     * For hostile band members and envoys.
     */
    default boolean chats(Entity entity, ServerPlayer player) {
        return attitude(entity, player) != Attitude.HOSTILE;
    }

    /** Adds its own candidates (mood from its own data) to the core's fluff and mood. */
    default void candidates(TalkContext ctx, TalkCandidates out) {}

    /** Whether this talk earns a credit; the core then caps it at once a game day per player and entity. */
    default boolean credits(Entity entity, ServerPlayer player) {
        return false;
    }

    /** Applies the credit; called at most once a game day per player and entity, after the line. */
    default void credit(Entity entity, ServerPlayer player) {}

    /** After the line. Default: a Mob looks at the player for 40 ticks and plays its ambient sound. */
    default void react(Entity entity, ServerPlayer player) {
        if (entity instanceof Mob mob) {
            if (!mob.isSleeping())
                com.minecraftindustrialcomplex.mic_rtsim.talk.Talk.lookAt(mob, player, 40);
            mob.playAmbientSound();
        }
    }
}
