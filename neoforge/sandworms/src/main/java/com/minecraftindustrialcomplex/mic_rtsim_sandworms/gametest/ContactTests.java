package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import com.minecraftindustrialcomplex.mic_rtsim_sandworms.Probes;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.monster.Husk;
import net.minecraft.world.entity.vehicle.Minecart;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.HashMap;
import java.util.Map;

/**
 * Contact damage: what a worm does to a villager, a mob, a villager riding a minecart (the stand-in for a Create contraption rider) and a
 * survival player, each with 1000 health in a stone room on the worm's way. (A husk, because a zombie burns in the slab, which has no light.)
 */
@GameTestHolder(MicRtsimSandworms.MODID)
@PrefixGameTestTemplate(false)
public final class ContactTests {
    private ContactTests() {}

    @GameTest(template = "arena", timeoutTicks = 1500, batch = "contact")
    public static void contactVictims(GameTestHelper helper) {
        String test = "contactVictims";
        if (!Kit.run(helper, test, true)) return;
        Arena a = DepthTests.slab(helper);
        // a straight dive: the worm's head starts 3 below the surface at x=0 and the target is at x=60, 20 below the surface. Rooms at x=15..45.
        Vec3 target = a.at(60.5, DepthTests.SURFACE - 20, 0.5);
        a.room(target, 2);
        Vec3 pVillager = a.at(18.5, DepthTests.SURFACE - 8, 0.5), pHusk = a.at(26.5, DepthTests.SURFACE - 11, 0.5), pCart = a.at(34.5, DepthTests.SURFACE - 14, 0.5), pPlayer = a.at(44.5, DepthTests.SURFACE - 17, 0.5);
        for (Vec3 p : new Vec3[] {pVillager, pHusk, pCart, pPlayer}) a.room(p.add(0, -0.0, 0), 4);
        LivingEntity marker = Kit.victim(a.level, target, 1000);
        LivingEntity villager = Kit.victim(a.level, pVillager, 1000);
        Husk husk = EntityType.HUSK.create(a.level);
        husk.moveTo(pHusk.x, pHusk.y, pHusk.z);
        husk.setNoAi(true);
        husk.setPersistenceRequired();
        husk.getAttribute(Attributes.MAX_HEALTH).setBaseValue(1000);
        husk.setHealth(1000);
        a.level.addFreshEntity(husk);
        Minecart cart = EntityType.MINECART.create(a.level);
        cart.moveTo(pCart.x, pCart.y, pCart.z);
        a.level.addFreshEntity(cart);
        LivingEntity rider = Kit.victim(a.level, pCart, 1000);
        rider.startRiding(cart, true);
        ServerPlayer player = Kit.trackedPlayer(a.level, pPlayer, "worm-victim");
        player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(1000);
        player.setHealth(1000);
        Entity worm = Sw.create(a.level, a.at(0.5, DepthTests.SURFACE - 3, 0.5));
        Sw.aggro(worm, marker);
        Probes.HITS.clear();
        Probes.recordHits = true;
        Kit.say(test, "setup", "villager, husk, minecart rider and a survival player, 1000 health each, in rooms 18..45 blocks east of the worm; target villager at 60 east, 20 down");
        Sw.add(a.level, worm);
        int[] t = {0};
        helper.onEachTick(() -> {
            if (t[0] > 600) return;
            if (t[0] == 600) {
                Probes.recordHits = false;
                Map<String, Integer> events = new HashMap<>();
                Map<String, Double> dmg = new HashMap<>();
                for (String h : Probes.HITS) {
                    String key = h.substring(0, h.indexOf('#')) + (h.contains("by -") ? " (non-worm: " + h.substring(h.indexOf("<- ") + 3, h.indexOf(" by")) + ")" : "");
                    events.merge(key, 1, Integer::sum);
                    dmg.merge(key, Double.parseDouble(h.substring(h.lastIndexOf(": ") + 2)), Double::sum);
                }
                Kit.say(test, "damage events by victim type", events);
                Kit.say(test, "damage dealt by victim type", dmg);
                Kit.say(test, "sources", Probes.HITS.stream().map(h -> h.substring(h.indexOf("<- ") + 3, h.indexOf(" @"))).distinct().toList());
                Kit.say(test, "first hits", Probes.HITS.stream().limit(10).toList());
                Kit.say(test, "villager (still) health", villager.getHealth());
                Kit.say(test, "husk health", husk.getHealth());
                Kit.say(test, "minecart rider health / still riding / cart alive", rider.getHealth() + " / " + rider.isPassenger() + " / " + cart.isAlive());
                Kit.say(test, "survival player health", player.getHealth());
                worm.discard();
                for (Entity e : new Entity[] {marker, villager, husk, rider, cart}) e.discard();
                Kit.removeTrackedPlayer(player);
                a.close();
                helper.succeed();
            }
            t[0]++;
        });
    }
}
