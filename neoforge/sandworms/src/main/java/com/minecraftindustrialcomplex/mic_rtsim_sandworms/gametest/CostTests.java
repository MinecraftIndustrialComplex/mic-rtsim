package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import com.minecraftindustrialcomplex.mic_rtsim_sandworms.Probes;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Test 2: server tick time with 0, 1 and 3 worms, side by side in one run. The GameTest server ticks unthrottled, so a tick's wall
 * time is its cost. A worm is 91 entities (90 body segments and a head), each a 7.5-block box that looks for its owner in a
 * 400-block cube every tick. Two ways of loading it: "attack run" spawns the worm, lets it build for 15 ticks and measures the next
 * 45 (its dash to a villager 60 blocks east, 20 down), five times over; "hover" freezes the head so the worm stays where it is
 * and is measured over 200 ticks. The player is 250 blocks away (out of the entities' tracking range) in the "far" test, and 40
 * blocks away (tracking all of the worm's entities) in the "near" test; the mod discards a worm with no player within 400 blocks.
 * The sequence runs twice and begins with a warm-up.
 */
@GameTestHolder(MicRtsimSandworms.MODID)
@PrefixGameTestTemplate(false)
public final class CostTests {
    private CostTests() {}

    private record Config(String label, int n, boolean hover) {}

    private static final List<Config> ROUND = List.of(
            new Config("0 worms", 0, false),
            new Config("1 worm attack run", 1, false),
            new Config("3 worms attack run", 3, false),
            new Config("1 worm hover", 1, true),
            new Config("3 worms hover", 3, true),
            new Config("0 worms again", 0, false));

    private static final List<Config> CONFIGS = java.util.stream.Stream.concat(
            ROUND.stream().map(c -> new Config("R1 " + c.label, c.n, c.hover)),
            ROUND.stream().map(c -> new Config("R2 " + c.label, c.n, c.hover))).toList();

    @GameTest(template = "arena", timeoutTicks = 20000, batch = "zz_cost_far")
    public static void msptFarPlayer(GameTestHelper helper) {
        run(helper, "msptFarPlayer", 250);
    }

    @GameTest(template = "arena", timeoutTicks = 20000, batch = "zzz_cost_near")
    public static void msptNearPlayer(GameTestHelper helper) {
        run(helper, "msptNearPlayer", 40);
    }

    private static void run(GameTestHelper helper, String test, int playerDist) {
        if (!Kit.run(helper, test, true)) return;
        Arena a = DepthTests.slab(helper);
        Vec3 target = a.at(60.5, DepthTests.SURFACE - 20, 0.5);
        a.room(target, 2);
        LivingEntity marker = Kit.victim(a.level, target, 1000);
        ServerPlayer p = Kit.trackedPlayer(a.level, a.at(0.5, DepthTests.SURFACE, playerDist + 0.5), "worm-bystander");
        a.room(p.position(), 2);
        Kit.say(test, "player", "added to the level " + playerDist + " blocks from the worms' lane");
        List<Entity> worms = new ArrayList<>();
        int[] cfg = {-1};
        int[] rep = {0};
        int[] t = {0};      // ticks into the current spawn
        int[] warm = {0};
        boolean[] warmed = {false};
        int settleRun = 15, measureRun = 45, reps = 5, settleHover = 40, measureHover = 200;
        helper.onEachTick(() -> {
            if (cfg[0] >= CONFIGS.size()) return;
            if (!warmed[0]) {
                if (warm[0] == 0) spawn(a, worms, 3, marker, false);
                if (++warm[0] >= 200) {
                    clear(worms);
                    warmed[0] = true;
                    cfg[0] = -1;
                    t[0] = 1_000_000;
                }
                return;
            }
            Config c = cfg[0] < 0 ? null : CONFIGS.get(cfg[0]);
            int settle = c != null && c.hover ? settleHover : settleRun;
            int measure = c != null && c.hover ? measureHover : measureRun;
            int nreps = c == null ? 0 : c.hover || c.n == 0 ? 1 : reps;
            if (c != null && c.n == 0) measure = 200;
            if (c == null || t[0] >= settle + measure) {
                if (c != null) {
                    Probes.clockOn = false;
                    clear(worms);
                    rep[0]++;
                    if (rep[0] >= nreps) {
                        report(test, c, nreps);
                        c = null;
                    }
                }
                if (c == null || rep[0] >= nreps) {
                    cfg[0]++;
                    rep[0] = 0;
                    if (cfg[0] >= CONFIGS.size()) {
                        marker.discard();
                        Kit.removeTrackedPlayer(p);
                        a.close();
                        helper.succeed();
                        return;
                    }
                    Probes.TICKS.clear();
                }
                Config next = CONFIGS.get(cfg[0]);
                spawn(a, worms, next.n, marker, next.hover);
                t[0] = 0;
            }
            Config cur = CONFIGS.get(cfg[0]);
            int st = cur.hover ? settleHover : settleRun;
            if (cur.hover) worms.forEach(w -> { if (Sw.segmentCount(w) > 0) Sw.freeze(w); });
            if (t[0] == st) Probes.clockOn = true;
            t[0]++;
        });
    }

    private static void spawn(Arena a, List<Entity> out, int n, LivingEntity target, boolean hover) {
        for (int i = 0; i < n; i++) {
            Entity w = Sw.create(a.level, a.at(0.5, DepthTests.SURFACE - 3, -30 + 30 * i + 0.5));
            Sw.aggro(w, target);
            Sw.add(a.level, w);
            out.add(w);
        }
    }

    private static void clear(List<Entity> worms) {
        worms.forEach(Entity::discard);
        worms.clear();
    }

    private static void report(String test, Config c, int reps) {
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
                + Kit.f2(ns.get((int) (ns.size() * 0.95)) / 1e6) + " max " + Kit.f2(ns.get(ns.size() - 1) / 1e6) + " (" + ns.size() + " ticks over " + reps + " spawn" + (reps > 1 ? "s" : "") + ")");
    }
}
