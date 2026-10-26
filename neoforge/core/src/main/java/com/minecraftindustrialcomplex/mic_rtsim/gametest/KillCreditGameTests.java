package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.KillCredit;
import com.mojang.authlib.GameProfile;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.animal.Wolf;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.util.FakePlayer;
import net.neoforged.neoforge.common.util.FakePlayerFactory;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The one kill-credit rule every bridge's death report uses ({@link KillCredit}, EQUILIBRIUM.md §2.2, slice S3, MZB-1006 and
 * MZB-1016), on the server's real entities and damage sources. No engine: the rule is Java's, and the site ledger's
 * GameTests in mowziesmobs and iceandfire show what the engine makes of each code.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class KillCreditGameTests {
    private KillCreditGameTests() {}

    private static void expect(String what, KillCredit.Credit k, int by, UUID player) {
        check(what + ": " + k, k.by() == by && (player == null ? k.player() == null : player.equals(k.player())));
    }

    /**
     * Each rule in order: a real player's hit is the player's; a NeoForge FakePlayer's (a machine, MZB-111) is OTHER with no
     * player; a tame wolf's hit is its owner's; a wild mob's is MOB; lava with no entity is NONE; a fall after a player's hit
     * (vanilla's kill credit) is the player's; and lava after a pet's hit is the owner's.
     */
    @GameTest(template = RtsimTests.TEMPLATE, batch = "kill_credit", timeoutTicks = 100)
    public static void eachRuleCreditsWhoItNames(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Vec3 c = RtsimTests.centre(helper);
        ServerPlayer player = FakePlayers.add(level, c.add(3, 0, 0), "credit-player");
        try {
            UUID id = player.getUUID();
            Zombie dead = EntityType.ZOMBIE.create(level);
            Zombie wild = EntityType.ZOMBIE.create(level);
            Wolf pet = EntityType.WOLF.create(level);
            check("the bodies are created", dead != null && wild != null && pet != null);
            pet.tame(player);
            FakePlayer machine = FakePlayerFactory.get(level, new GameProfile(UUID.randomUUID(), "credit-machine"));
            DamageSource lava = level.damageSources().lava();

            expect("a real player's hit", KillCredit.of(dead, level.damageSources().playerAttack(player)), KillCredit.PLAYER, id);
            expect("a machine's hit", KillCredit.of(dead, level.damageSources().playerAttack(machine)), KillCredit.OTHER, null);
            expect("a tame wolf's hit", KillCredit.of(dead, level.damageSources().mobAttack(pet)), KillCredit.PLAYER, id);
            expect("a wild mob's hit", KillCredit.of(dead, level.damageSources().mobAttack(wild)), KillCredit.MOB, null);
            expect("lava, with no hit", KillCredit.of(dead, lava), KillCredit.NONE, null);

            LivingEntity hitByPlayer = EntityType.ZOMBIE.create(level);
            hitByPlayer.setLastHurtByPlayer(player);
            expect("a fall after a player's hit", KillCredit.of(hitByPlayer, level.damageSources().fall()), KillCredit.PLAYER, id);
            LivingEntity hitByPet = EntityType.ZOMBIE.create(level);
            hitByPet.setLastHurtByMob(pet);
            expect("lava after a pet's hit", KillCredit.of(hitByPet, lava), KillCredit.PLAYER, id);
            record("eachRuleCreditsWhoItNames", "rules", "player, machine, pet, wild mob, lava, fall after a hit, lava after a pet's hit");
        } finally {
            FakePlayers.remove(player);
        }
        helper.succeed();
    }
}
