package com.minecraftindustrialcomplex.mic_rtsim.body.vanilla;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberSpawn;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.LlamaInvoker;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.animal.horse.Llama;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.phys.Vec3;

/**
 * {@code minecraft:trader_llama}, the caravan's pack animal. Its cargo rides in its chest (C21):
 * three rows of {@code strength} columns ({@code Llama.getInventoryColumns}), reached through vanilla's
 * {@code getSlot(500 + k)} (the same slots {@code /item} uses). Variant and strength are captured as
 * the body bits, so a llama comes back as the same llama.
 */
public final class TraderLlamaAdapter implements BodyAdapter {

    /** {@code TraderLlama.despawnDelay}'s initial value (vanilla's own). */
    public static final int VANILLA_DESPAWN_DELAY = 47999;
    /** {@code AbstractChestedHorse.getSlot}: 499 is the chest itself, 500 + k is chest slot k. */
    private static final int SLOT_CHEST = 499;
    private static final int SLOT_CARGO = 500;

    @Override
    public String name() {
        return "minecraft:trader_llama";
    }

    @Override
    public Entity create(ServerLevel level, Vec3 pos, MemberSpawn spawn) {
        TraderLlama llama = EntityType.TRADER_LLAMA.create(level, null, BlockPos.containing(pos), MobSpawnType.EVENT, false, false);
        if (llama == null)
            return null;
        llama.moveTo(pos.x, pos.y, pos.z, llama.getYRot(), 0f);
        if (spawn.bodyA() >= 0)
            llama.setVariant(Llama.Variant.byId(spawn.bodyA()));
        if (spawn.bodyB() >= 1)
            ((LlamaInvoker) llama).mic_rtsim$setStrength(spawn.bodyB());
        if (!spawn.items().isEmpty())
            load(llama, spawn);
        if (spawn.health() > 0f)
            llama.setHealth(Math.min(spawn.health(), llama.getMaxHealth()));
        return llama;
    }

    private static void load(TraderLlama llama, MemberSpawn spawn) {
        llama.getSlot(SLOT_CHEST).set(new ItemStack(Items.CHEST));
        int slots = llama.getInventoryColumns() * 3;
        for (MemberCapture.Stack s : spawn.items()) {
            ItemStack stack = new ItemStack(s.item(), Math.min(s.count(), s.item().getDefaultMaxStackSize()));
            if (stack.getCount() < s.count())
                MicRtsim.LOGGER.warn("[mic_rtsim] cargo stack of {} x {} clamped to {}", s.count(), s.item(), stack.getCount());
            int slot = s.slot() >= 0 && s.slot() < slots && llama.getSlot(SLOT_CARGO + s.slot()).get().isEmpty() ? s.slot() : -1;
            for (int k = 0; slot < 0 && k < slots; k++)
                if (llama.getSlot(SLOT_CARGO + k).get().isEmpty())
                    slot = k;
            if (slot < 0) {
                MicRtsim.LOGGER.warn("[mic_rtsim] llama chest full: {} x {} not loaded", stack.getCount(), s.item());
                continue;
            }
            llama.getSlot(SLOT_CARGO + slot).set(stack);
        }
    }

    @Override
    public void afterGroupSpawned(Entity self, PuppetGroup group) {
        Entity leader = group.leader();
        if (leader != null && leader != self)
            ((TraderLlama) self).setLeashedTo(leader, true);
    }

    @Override
    public void capture(Entity entity, PuppetGroup group, MemberCapture out) {
        TraderLlama llama = (TraderLlama) entity;
        out.health = llama.getHealth();
        out.maxHealth = llama.getMaxHealth();
        out.x = llama.getX();
        out.y = llama.getY();
        out.z = llama.getZ();
        Entity holder = llama.getLeashHolder();
        Entity leader = group == null ? null : group.members[0].entity;
        if (holder != null && holder == leader)
            out.flags |= Obs.MemberState.FLAG_LEASHED_TO_LEADER;
        else if (holder != null)
            out.flags |= Obs.MemberState.FLAG_LEASHED_ELSEWHERE;
        if (llama.isTamed())
            out.flags |= Obs.MemberState.FLAG_TAMED;
        out.bodyA = llama.getVariant().getId();
        out.bodyB = llama.getStrength();
        if (llama.hasChest()) {
            out.flags |= Obs.MemberState.FLAG_HAS_CHEST;
            int slots = llama.getInventoryColumns() * 3;
            for (int k = 0; k < slots; k++) {
                ItemStack stack = llama.getSlot(SLOT_CARGO + k).get();
                if (!stack.isEmpty())
                    out.items.add(new MemberCapture.Stack(k, stack.getItem(), stack.getCount()));
            }
        }
    }

    @Override
    public void maintain(Entity entity, PuppetGroup group) {
        // Leashed to the trader, the llama copies the trader's delay (kept positive by the trader's
        // adapter). Off the leash it counts its own down; keep that one from reaching 0 too.
        ((TraderLlama) entity).setDespawnDelay(VANILLA_DESPAWN_DELAY);
    }

    @Override
    public int takenBy(Entity entity, PuppetGroup group) {
        TraderLlama llama = (TraderLlama) entity;
        Entity holder = llama.getLeashHolder();
        // Still on the group's trader, even a trader that just died: the llama drops that leash on
        // its own next tick (Leashable.tickLeash), which is not a player taking it.
        if (holder != null && holder != group.members[0].entity && holder.isAlive())
            return Obs.Taken.LEAD;
        if (llama.isTamed())
            return Obs.Taken.TAME;
        return 0;
    }
}
