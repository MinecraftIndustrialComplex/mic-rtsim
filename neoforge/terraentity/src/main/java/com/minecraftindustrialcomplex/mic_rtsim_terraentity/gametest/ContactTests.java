package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import com.minecraftindustrialcomplex.mic_rtsim_terraentity.Probes;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.monster.Husk;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.vehicle.Minecart;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Test 4: contact damage. A worm dashes at a target room holding a villager, a husk (no AI), a survival
 * player and a villager sitting in a minecart (the same code path as an entity riding a Create contraption: a
 * passenger is an entity in the level like any other). Victims have 1000 health so they survive and the hits
 * can be counted; the kill run uses their ordinary 20.
 */
@GameTestHolder(MicRtsimTerraEntity.MODID)
@PrefixGameTestTemplate(false)
public final class ContactTests {
    private ContactTests() {}

    @GameTest(template = "arena", timeoutTicks = 1000, batch = "contact_giant_worm")
    public static void contactGiantWorm(GameTestHelper helper) {
        contact(helper, "contactGiantWorm", "giant_worm", true);
    }

    @GameTest(template = "arena", timeoutTicks = 1000, batch = "contact_bone_serpent")
    public static void contactBoneSerpent(GameTestHelper helper) {
        contact(helper, "contactBoneSerpent", "bone_serpent", true);
    }

    @GameTest(template = "arena", timeoutTicks = 1000, batch = "contact_devourer")
    public static void contactDevourer(GameTestHelper helper) {
        contact(helper, "contactDevourer", "devourer", true);
    }

    @GameTest(template = "arena", timeoutTicks = 1000, batch = "kill_giant_worm")
    public static void killGiantWorm(GameTestHelper helper) {
        contact(helper, "killGiantWorm", "giant_worm", false);
    }

    private static void contact(GameTestHelper helper, String test, String id, boolean tanky) {
        if (!Kit.run(helper, test, true)) return;
        Arena a = new Arena(helper, -25, 85, 6, 46, -45, 65);
        a.fill(Blocks.STONE.defaultBlockState());
        Vec3 room = a.at(40.5, 26, 10.5);
        a.room(room, 4);
        Map<String, LivingEntity> victims = new LinkedHashMap<>();
        Villager villager = EntityType.VILLAGER.create(a.level);
        place(villager, room.add(0, 0, 0));
        Husk zombie = EntityType.HUSK.create(a.level);
        place(zombie, room.add(2, 0, 1));
        zombie.setNoAi(true);
        zombie.setPersistenceRequired();
        Villager rider = EntityType.VILLAGER.create(a.level);
        place(rider, room.add(-2, 0, 2));
        rider.setNoAi(true);
        Minecart cart = EntityType.MINECART.create(a.level);
        cart.moveTo(room.x - 2, room.y, room.z + 2, 0, 0);
        ServerPlayer player = Kit.trackedPlayer(a.level, room.add(0, 0, -2.5));
        for (LivingEntity v : List.of(villager, zombie, rider, player))
            if (tanky) {
                v.getAttribute(Attributes.MAX_HEALTH).setBaseValue(1000);
                v.setHealth(1000);
            }
        villager.setNoAi(true); // a villager would walk out of the room; it still takes hits
        a.level.addFreshEntity(villager);
        a.level.addFreshEntity(zombie);
        a.level.addFreshEntity(cart);
        a.level.addFreshEntity(rider);
        rider.startRiding(cart, true);
        victims.put("villager (still)", villager);
        victims.put("husk (no AI; a zombie burns in my unlit slab)", zombie);
        victims.put("villager riding a minecart", rider);
        victims.put("survival player", player);
        float[] start = new float[4];
        int i = 0;
        for (LivingEntity v : victims.values()) start[i++] = v.getHealth();
        Mob worm = (Mob) Kit.spawn(a.level, id, a.at(10.5, 26, 10.5));
        worm.setTarget(villager);
        Kit.say(test, "setup", id + " attack " + worm.getAttributeValue(Attributes.ATTACK_DAMAGE) + ", victims at " + (tanky ? "1000" : "their normal") + " health, riding: " + rider.isPassenger());
        Probes.HITS.clear();
        Probes.recordHits = true;
        int[] t = {0};
        int[] deadAt = {-1, -1, -1, -1};
        int ticks = tanky ? 400 : 400;
        helper.onEachTick(() -> {
            if (t[0] > ticks) return;
            int k = 0;
            for (LivingEntity v : victims.values()) {
                if (deadAt[k] < 0 && !v.isAlive()) deadAt[k] = t[0];
                k++;
            }
            if (t[0] == ticks) {
                Probes.recordHits = false;
                List<String> hits;
                synchronized (Probes.HITS) {
                    hits = new ArrayList<>(Probes.HITS);
                }
                Map<String, Integer> byVictim = new TreeMap<>();
                Map<String, Double> dmg = new TreeMap<>();
                List<String> distinct = new ArrayList<>();
                for (String h : hits) {
                    String vic = h.substring(0, h.indexOf('#'));
                    int vid = Integer.parseInt(h.substring(h.indexOf('#') + 1, h.indexOf(' ')));
                    String key = vic + "#" + (vid == villager.getId() ? "still villager" : vid == rider.getId() ? "cart rider" : vid == zombie.getId() ? "zombie" : vid == player.getId() ? "player" : "other");
                    byVictim.merge(key, 1, Integer::sum);
                    dmg.merge(key, Double.parseDouble(h.substring(h.lastIndexOf(' ') + 1)), Double::sum);
                    String src = h.substring(h.indexOf("<-"));
                    src = src.substring(0, src.lastIndexOf(" @"));
                    if (!distinct.contains(src)) distinct.add(src);
                }
                Kit.say(test, "damage events by victim", byVictim);
                Kit.say(test, "damage dealt by victim (after armor/resistances)", dmg);
                Kit.say(test, "damage sources", distinct);
                Kit.say(test, "game ticks of the hits on the still villager", hits.stream().filter(h -> h.startsWith("minecraft:villager#" + villager.getId() + " ")).map(h -> h.substring(h.indexOf('@') + 1, h.lastIndexOf(':'))).toList());
                List<String> perHit = hits.stream().limit(12).toList();
                Kit.say(test, "first hits", perHit);
                int j = 0;
                for (var e : victims.entrySet()) {
                    LivingEntity v = e.getValue();
                    Kit.say(test, "victim " + e.getKey(), "health " + start[j] + " -> " + Kit.f(v.getHealth()) + (v.isAlive() ? "" : " DEAD at tick " + deadAt[j]) + (v == rider ? ", still riding " + rider.isPassenger() : ""));
                    j++;
                }
                Kit.removeTrackedPlayer(player);
                worm.discard();
                cart.discard();
                a.close();
                helper.succeed();
            }
            t[0]++;
        });
    }

    private static void place(Mob m, Vec3 at) {
        m.moveTo(at.x, at.y, at.z, 0, 0);
        m.setPersistenceRequired();
        m.setSilent(true);
    }
}
