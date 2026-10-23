package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.players.PlayerList;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

import java.util.List;

/**
 * The player list's backing list ({@code getPlayers()} is unmodifiable in NeoForge). TALK's broadcast
 * test adds its hearing players for one click, so a line sent to everyone would reach them.
 */
@Mixin(PlayerList.class)
public interface PlayerListAccessor {
    @Accessor("players")
    List<ServerPlayer> mic_rtsim$players();
}
