package com.minecraftindustrialcomplex.mic_rtsim.body.vanilla;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberSpawn;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.phys.Vec3;

/**
 * {@code minecraft:wandering_trader}, the caravan's merchant and leader (PLAN.md §8.3).
 *
 * <p><b>The despawn trap</b> (PLAN.md §6.1 item 7). {@code WanderingTrader.maybeDespawn} counts
 * {@code despawnDelay} down while it is above 0 and discards the trader when it reaches 0; a delay of
 * 0 stops the countdown. But each trader llama leashed to it copies the trader's delay minus 1 on
 * every tick and discards itself at 0 or below ({@code TraderLlama.maybeDespawn}). So a trader at 0
 * would never leave, and its llamas would vanish on their next tick. The bridge keeps the delay
 * positive instead and refreshes it on every pass, so neither ever reaches 0 while embodied.
 */
public final class WanderingTraderAdapter implements BodyAdapter {

    /** The delay {@code WanderingTraderSpawner.spawn} gives a trader it spawns (vanilla's own value). */
    public static final int VANILLA_DESPAWN_DELAY = 48000;

    @Override
    public String name() {
        return "minecraft:wandering_trader";
    }

    @Override
    public Entity create(ServerLevel level, Vec3 pos, MemberSpawn spawn) {
        WanderingTrader trader = EntityType.WANDERING_TRADER.create(level, null, BlockPos.containing(pos), MobSpawnType.EVENT, false, false);
        if (trader == null)
            return null;
        trader.moveTo(pos.x, pos.y, pos.z, trader.getYRot(), 0f);
        trader.setDespawnDelay(VANILLA_DESPAWN_DELAY);
        if (spawn.health() > 0f)
            trader.setHealth(Math.min(spawn.health(), trader.getMaxHealth()));
        return trader;
    }

    @Override
    public void capture(Entity entity, PuppetGroup group, MemberCapture out) {
        WanderingTrader trader = (WanderingTrader) entity;
        out.health = trader.getHealth();
        out.maxHealth = trader.getMaxHealth();
        out.x = trader.getX();
        out.y = trader.getY();
        out.z = trader.getZ();
    }

    @Override
    public void steer(Entity entity, Vec3 target, float speed) {
        // The trader's own WanderToPositionGoal walks there in 10-block hops at its fixed speed.
        ((WanderingTrader) entity).setWanderTarget(BlockPos.containing(target));
    }

    @Override
    public void maintain(Entity entity, PuppetGroup group) {
        WanderingTrader trader = (WanderingTrader) entity;
        if (trader.getDespawnDelay() != VANILLA_DESPAWN_DELAY)
            trader.setDespawnDelay(VANILLA_DESPAWN_DELAY);
    }
}
