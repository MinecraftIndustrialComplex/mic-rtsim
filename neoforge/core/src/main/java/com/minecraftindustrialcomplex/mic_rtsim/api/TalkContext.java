package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;

/**
 * What a talker's {@link Talker#candidates} sees: who talks to whom, when, and the state and attitude
 * the core already worked out.
 *
 * @param home  the name of the settlement the talker calls home, or null
 * @param facts what the engine answered about the talker's holder (news, prices, port, sentiment), or null
 *              when there is no answer (see {@link TalkFacts})
 */
public record TalkContext(Entity entity, ServerPlayer player, ServerLevel level, long gameTime,
                          Talker.State state, Talker.Attitude attitude, String home, TalkFacts facts) {}
