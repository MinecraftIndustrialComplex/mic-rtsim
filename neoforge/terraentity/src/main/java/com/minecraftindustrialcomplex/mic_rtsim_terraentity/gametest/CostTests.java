package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import com.minecraftindustrialcomplex.mic_rtsim_terraentity.Probes;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Random;

/**
 * Test 2: server tick time with 0, 1, 5 and 20 worms, side by side in one run. The GameTest server ticks
 * unthrottled, so a tick's wall time is its cost. Worms are spread through a stone slab; "wander" has no
 * target, "chase" gives them all one marker to dash at. Each configuration settles for 60 ticks and is
 * measured over 300. The zero-worm configuration is measured first and last, which shows the drift of the
 * loaded machine, and the sequence is warmed up first.
 */
@GameTestHolder(MicRtsimTerraEntity.MODID)
@PrefixGameTestTemplate(false)
public final class CostTests {
    private CostTests() {}

    private record Config(String label, String type, int n, boolean chase) {}

    private static final List<Config> ROUND = List.of(
            new Config("0 worms", "giant_worm", 0, false),
            new Config("1 giant_worm wander", "giant_worm", 1, false),
            new Config("5 giant_worm wander", "giant_worm", 5, false),
            new Config("20 giant_worm wander", "giant_worm", 20, false),
            new Config("1 giant_worm chase", "giant_worm", 1, true),
            new Config("5 giant_worm chase", "giant_worm", 5, true),
            new Config("20 giant_worm chase", "giant_worm", 20, true),
            new Config("5 devourer wander", "devourer", 5, false),
            new Config("5 bone_serpent wander (18 parts)", "bone_serpent", 5, false),
            new Config("20 bone_serpent chase (18 parts)", "bone_serpent", 20, true),
            new Config("0 worms again", "giant_worm", 0, false));

    /** Two rounds of the same sequence, so the spread between rounds shows the noise of a loaded laptop. */
    private static final List<Config> CONFIGS = java.util.stream.Stream.concat(
            ROUND.stream().map(c -> new Config("R1 " + c.label, c.type, c.n, c.chase)),
            ROUND.stream().map(c -> new Config("R2 " + c.label, c.type, c.n, c.chase))).toList();

    @GameTest(template = "arena", timeoutTicks = 20000, batch = "zz_cost")
    public static void msptByWormCount(GameTestHelper helper) {
        run(helper, "msptByWormCount", false);
    }

    /** The same with a survival player (a real level player, tracking entities) standing in the slab. */
    @GameTest(template = "arena", timeoutTicks = 20000, batch = "zzz_cost_player")
    public static void msptByWormCountWithPlayer(GameTestHelper helper) {
        run(helper, "msptByWormCountWithPlayer", true);
    }

    private static void run(GameTestHelper helper, String test, boolean player) {
        if (!Kit.run(helper, test, true)) return;
        Arena a = new Arena(helper, -25, 85, 6, 46, -45, 65);
        a.fill(Blocks.STONE.defaultBlockState());
        Vec3 target = a.at(60.5, 26, 40.5);
        a.room(target, 2);
        LivingEntity marker = Kit.marker(a.level, target);
        ServerPlayer p = null;
        if (player) {
            p = Kit.trackedPlayer(a.level, a.at(40.5, 26, 10.5));
            a.room(p.position(), 2);
            Kit.say(test, "player", "added to the level at " + Kit.v(p.position()));
        }
        ServerPlayer pl = p;
        Random rnd = new Random(7);
        List<Entity> worms = new ArrayList<>();
        int[] cfg = {-1};
        int[] t = {0};
        int[] phaseStart = {0};
        int settle = 60, measure = 300;
        // warm-up: 5 worms for 200 ticks before anything is counted
        int warm = 400;
        boolean[] warmed = {false};
        helper.onEachTick(() -> {
            if (cfg[0] >= CONFIGS.size()) return;
            if (!warmed[0]) {
                if (t[0] == 0) spawn(a, worms, "giant_worm", 20, rnd, marker);
                if (t[0] >= warm) {
                    clear(worms);
                    warmed[0] = true;
                    t[0] = -1;
                    cfg[0] = -1;
                }
                t[0]++;
                return;
            }
            int into = t[0] - phaseStart[0];
            if (cfg[0] < 0 || into >= settle + measure) {
                if (cfg[0] >= 0) {
                    Probes.clockOn = false;
                    report(test, CONFIGS.get(cfg[0]));
                    clear(worms);
                }
                cfg[0]++;
                phaseStart[0] = t[0];
                if (cfg[0] >= CONFIGS.size()) {
                    marker.discard();
                    if (pl != null) Kit.removeTrackedPlayer(pl);
                    a.close();
                    helper.succeed();
                    return;
                }
                Config c = CONFIGS.get(cfg[0]);
                spawn(a, worms, c.type, c.n, rnd, c.chase ? marker : null);
                into = 0;
            }
            if (into == settle) {
                Probes.TICKS.clear();
                Probes.clockOn = true;
            }
            t[0]++;
        });
    }

    private static void spawn(Arena a, List<Entity> out, String type, int n, Random rnd, LivingEntity target) {
        for (int i = 0; i < n; i++) {
            Vec3 at = a.at(-10 + rnd.nextInt(60), 14 + rnd.nextInt(24), -30 + rnd.nextInt(80));
            Entity w = Kit.spawn(a.level, type, at);
            w.setYRot(rnd.nextInt(360));
            if (target != null) ((Mob) w).setTarget(target);
            out.add(w);
        }
    }

    private static void clear(List<Entity> worms) {
        worms.forEach(Entity::discard);
        worms.clear();
    }

    private static void report(String test, Config c) {
        List<Long> ns;
        synchronized (Probes.TICKS) {
            ns = new ArrayList<>(Probes.TICKS);
        }
        Collections.sort(ns);
        if (ns.isEmpty()) {
            Kit.say(test, c.label, "no ticks");
            return;
        }
        double sum = 0;
        for (long x : ns) sum += x;
        Kit.say(test, c.label, "mspt mean " + Kit.f2(sum / ns.size() / 1e6) + " median " + Kit.f2(ns.get(ns.size() / 2) / 1e6) + " p95 "
                + Kit.f2(ns.get((int) (ns.size() * 0.95)) / 1e6) + " max " + Kit.f2(ns.get(ns.size() - 1) / 1e6) + " (" + ns.size() + " ticks)");
    }
}
