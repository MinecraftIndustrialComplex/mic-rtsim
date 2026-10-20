package com.minecraftindustrialcomplex.mic_rtsim_ribbits;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberSpawn;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.market.CargoAccess;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.mixin.RibbitEntityAccessor;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.module.RibbitProfessionModule;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.phys.Vec3;

import java.util.ArrayList;
import java.util.List;

/**
 * {@code mic_rtsim_ribbits:ribbit_merchant}: a ribbit merchant travelling with its village's goods
 * (C7 merchant caravans, PLAN.md §8.8). A group of one. Its offers come from the engine
 * (MERCHANT_OFFERS) and sell out of the pack it carries ({@link RibbitCargo}); Ribbits' own random
 * merchant trades are dropped at spawn. Body bit A is the umbrella (0 to 2).
 */
public final class RibbitMerchantAdapter implements BodyAdapter {
    public static final String NAME = "mic_rtsim_ribbits:ribbit_merchant";

    @Override
    public String name() {
        return NAME;
    }

    @Override
    public Entity create(ServerLevel level, Vec3 pos, MemberSpawn spawn) {
        if (spawn.items().size() > RibbitCargo.SLOTS)
            throw new SpawnRefused(Obs.SpawnRejected.BAD_CARGO, spawn.items().size() + " stacks for a pack of " + RibbitCargo.SLOTS);
        RibbitEntity r = Ribbits.create(level, pos, RibbitProfessionModule.MERCHANT, Ribbits.umbrella(spawn.bodyA()));
        if (r == null)
            return null;
        r.getOffers().clear(); // Ribbits rolls merchant trades on first access; the caravan's come from its cargo
        List<ItemStack> pack = new ArrayList<>(RibbitCargo.SLOTS);
        for (int i = 0; i < RibbitCargo.SLOTS; i++)
            pack.add(ItemStack.EMPTY);
        for (MemberCapture.Stack s : spawn.items()) {
            if (s.count() > s.item().getDefaultMaxStackSize())
                throw new SpawnRefused(Obs.SpawnRejected.BAD_CARGO, s.count() + " x " + s.item() + " is more than one stack");
            int slot = s.slot() >= 0 && s.slot() < RibbitCargo.SLOTS && pack.get(s.slot()).isEmpty() ? s.slot() : pack.indexOf(ItemStack.EMPTY);
            for (int k = 0; slot < 0 && k < pack.size(); k++)
                if (pack.get(k).isEmpty())
                    slot = k;
            pack.set(slot, new ItemStack(s.item(), s.count()));
        }
        r.setData(RibbitCargo.PACK, List.copyOf(pack));
        if (spawn.health() > 0f)
            r.setHealth(Math.min(spawn.health(), r.getMaxHealth()));
        return r;
    }

    @Override
    public void capture(Entity entity, PuppetGroup group, MemberCapture out) {
        RibbitEntity r = (RibbitEntity) entity;
        out.health = r.getHealth();
        out.maxHealth = r.getMaxHealth();
        out.x = r.getX();
        out.y = r.getY();
        out.z = r.getZ();
        if (r.getLeashHolder() != null)
            out.flags |= Obs.MemberState.FLAG_LEASHED_ELSEWHERE;
        out.flags |= Obs.MemberState.FLAG_HAS_CHEST;
        out.bodyA = Ribbits.umbrellaIndex(r.getRibbitData().getUmbrellaType());
        List<ItemStack> pack = r.getData(RibbitCargo.PACK);
        for (int k = 0; k < pack.size(); k++)
            if (!pack.get(k).isEmpty())
                out.items.add(new MemberCapture.Stack(k, pack.get(k).getItem(), pack.get(k).getCount()));
    }

    @Override
    public CargoAccess ownCargo(Entity entity) {
        return RibbitCargo.of(entity);
    }

    /**
     * Walk to the target, and make it home: Ribbits' GoHome goal would otherwise pull a travelling
     * ribbit back to where it was spawned (PATCHES.md: the {@code homePosition} accessor).
     */
    @Override
    public void steer(Entity entity, Vec3 target, float speed) {
        RibbitEntity r = (RibbitEntity) entity;
        ((RibbitEntityAccessor) r).mic_rtsim_ribbits$setHomePosition(BlockPos.containing(target));
        r.getNavigation().moveTo(target.x, target.y, target.z, speed > 0 ? speed : 1.0);
    }

    /** A lead held by a player takes it (A10); Ribbits' merchants can be leashed like any mob. */
    @Override
    public int takenBy(Entity entity, PuppetGroup group) {
        RibbitEntity r = (RibbitEntity) entity;
        return r.getLeashHolder() instanceof Player p && p.isAlive() ? Obs.Taken.LEAD : 0;
    }
}
