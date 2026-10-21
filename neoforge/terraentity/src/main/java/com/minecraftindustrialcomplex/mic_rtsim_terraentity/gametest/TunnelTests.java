package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import net.minecraft.core.BlockPos;
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
import java.util.List;
import java.util.function.Supplier;

/**
 * Test 1: does a Terra Entity worm tunnel to a target inside stone, and what steers it? One worm in a
 * stone slab 40 blocks thick (relative y 6 to 46 above the structure), a target 30 blocks away inside a
 * carved room. Phases: A, vanilla {@code Mob.setTarget(marker 1)} for 600 ticks (30 s); B, retarget to
 * marker 2, 30 blocks away at a right angle, for 400 ticks; C, target cleared (the wander goal), 400 ticks.
 * The head's position is sampled every 10 ticks. Nothing here asserts a behaviour: it is measured.
 */
@GameTestHolder(MicRtsimTerraEntity.MODID)
@PrefixGameTestTemplate(false)
public final class TunnelTests {
    private TunnelTests() {}

    private record Sample(int t, Vec3 head, Vec3 tail, boolean headSolid, boolean inWall, boolean outside, double dist, boolean hasTarget, float hp) {}

    private record Phase(String name, int ticks, Supplier<LivingEntity> target, Runnable onStart) {}

    @GameTest(template = "arena", timeoutTicks = 2000, batch = "tunnel_giant_worm")
    public static void tunnelGiantWorm(GameTestHelper helper) {
        chase(helper, "tunnelGiantWorm", "giant_worm");
    }

    @GameTest(template = "arena", timeoutTicks = 2000, batch = "tunnel_devourer")
    public static void tunnelDevourer(GameTestHelper helper) {
        chase(helper, "tunnelDevourer", "devourer");
    }

    @GameTest(template = "arena", timeoutTicks = 2000, batch = "tunnel_bone_serpent")
    public static void tunnelBoneSerpent(GameTestHelper helper) {
        chase(helper, "tunnelBoneSerpent", "bone_serpent");
    }

    @GameTest(template = "arena", timeoutTicks = 2000, batch = "tunnel_leech")
    public static void tunnelLeech(GameTestHelper helper) {
        chase(helper, "tunnelLeech", "leech");
    }

    @GameTest(template = "arena", timeoutTicks = 2000, batch = "tunnel_tomb_crawler")
    public static void tunnelTombCrawler(GameTestHelper helper) {
        chase(helper, "tunnelTombCrawler", "tomb_crawler");
    }

    /** No setTarget at all: a survival player in a stone room 30 blocks away. Does the worm notice and tunnel to it by itself? */
    @GameTest(template = "arena", timeoutTicks = 1200, batch = "acquire_giant_worm_30")
    public static void acquirePlayer30(GameTestHelper helper) {
        acquire(helper, "acquirePlayer30", "giant_worm", 30);
    }

    @GameTest(template = "arena", timeoutTicks = 1200, batch = "acquire_giant_worm_48")
    public static void acquirePlayer48(GameTestHelper helper) {
        acquire(helper, "acquirePlayer48", "giant_worm", 48);
    }

    @GameTest(template = "arena", timeoutTicks = 1200, batch = "acquire_devourer_30")
    public static void acquirePlayerDevourer30(GameTestHelper helper) {
        acquire(helper, "acquirePlayerDevourer30", "devourer", 30);
    }

    private static Arena slab(GameTestHelper helper) {
        Arena a = new Arena(helper, -25, 85, 6, 46, -45, 65);
        a.fill(Blocks.STONE.defaultBlockState());
        return a;
    }

    private static void chase(GameTestHelper helper, String test, String id) {
        if (!Kit.run(helper, test, true)) return;
        Arena a = slab(helper);
        Vec3 start = a.at(10.5, 26, 10.5);
        Vec3 t1 = a.at(40.5, 26, 10.5);
        Vec3 t2 = a.at(40.5, 26, 40.5);
        a.room(t1, 2);
        a.room(t2, 2);
        Entity worm = Kit.spawn(a.level, id, start);
        Mob w = (Mob) worm;
        LivingEntity m1 = Kit.marker(a.level, t1);
        LivingEntity m2 = Kit.marker(a.level, t2);
        Kit.say(test, "setup", id + " at " + Kit.v(start) + ", stone y " + a.y0 + ".." + a.y1 + ", targets " + Kit.v(t1) + " then " + Kit.v(t2)
                + ", health " + w.getMaxHealth() + ", parts " + worm.getParts().length);
        List<Phase> phases = List.of(
                new Phase("A setTarget marker1 (30 b east)", 600, () -> m1, () -> w.setTarget(m1)),
                new Phase("B setTarget marker2 (30 b south of marker1)", 400, () -> m2, () -> w.setTarget(m2)),
                new Phase("C target cleared", 400, () -> null, () -> w.setTarget(null)));
        run(helper, a, test, w, phases, () -> {
            m1.discard();
            m2.discard();
        });
    }

    private static void acquire(GameTestHelper helper, String test, String id, int dist) {
        if (!Kit.run(helper, test, true)) return;
        Arena a = slab(helper);
        Vec3 start = a.at(10.5, 26, 10.5);
        Vec3 tp = a.at(10.5 + dist, 26, 10.5);
        a.room(tp, 2);
        Mob w = (Mob) Kit.spawn(a.level, id, start);
        ServerPlayer p = FakePlayers.add(a.level, tp, "worm-bait");
        Kit.say(test, "setup", id + " at " + Kit.v(start) + ", player in a stone room " + dist + " blocks east, followRange "
                + w.getAttributeValue(net.minecraft.world.entity.ai.attributes.Attributes.FOLLOW_RANGE));
        List<Phase> phases = List.of(new Phase("A no setTarget; player " + dist + " b east", 600, () -> p, () -> {}));
        run(helper, a, test, w, phases, () -> FakePlayers.remove(p));
    }

    private static void run(GameTestHelper helper, Arena a, String test, Mob worm, List<Phase> phases, Runnable cleanup) {
        List<Sample> all = new ArrayList<>();
        List<String> csv = new ArrayList<>();
        csv.add("t,phase,hx,hy,hz,tx,ty,tz,headSolid,inWall,outside,dist,hasTarget,hp");
        int[] t = {0};
        int[] phase = {-1};
        int[] phaseStart = {0};
        int total = phases.stream().mapToInt(Phase::ticks).sum();
        helper.onEachTick(() -> {
            if (t[0] > total) return;
            if (phase[0] < 0 || t[0] - phaseStart[0] >= phases.get(phase[0]).ticks()) {
                if (phase[0] >= 0) summarize(test, phases.get(phase[0]), all, phaseStart[0], t[0], a);
                phase[0]++;
                phaseStart[0] = t[0];
                if (phase[0] >= phases.size()) {
                    Kit.csv(test, csv);
                    cleanup.run();
                    boolean alive = worm.isAlive();
                    worm.discard();
                    a.close();
                    Kit.say(test, "end", "worm alive " + alive);
                    t[0] = total + 1;
                    helper.succeed();
                    return;
                }
                phases.get(phase[0]).onStart().run();
            }
            if (t[0] % 10 == 0 && worm.isAlive()) {
                Vec3 head = worm.position();
                Vec3 eye = worm.getEyePosition();
                var parts = worm.getParts();
                Vec3 tail = parts != null && parts.length > 0 ? parts[parts.length - 1].position() : head;
                boolean solid = !a.level.getBlockState(BlockPos.containing(eye)).isAir();
                LivingEntity tg = phases.get(phase[0]).target().get();
                double d = tg == null ? -1 : head.distanceTo(tg.position());
                Sample s = new Sample(t[0], head, tail, solid, worm.isInWall(), !a.contains(head), d, worm.getTarget() != null, worm.getHealth());
                all.add(s);
                csv.add(t[0] + "," + phase[0] + "," + Kit.v(head) + "," + Kit.v(tail) + "," + solid + "," + s.inWall + "," + s.outside + "," + Kit.f(d) + "," + s.hasTarget + "," + s.hp);
            }
            t[0]++;
        });
    }

    private static void summarize(String test, Phase ph, List<Sample> all, int t0, int t1, Arena a) {
        List<Sample> s = all.stream().filter(x -> x.t >= t0 && x.t < t1).toList();
        if (s.isEmpty()) {
            Kit.say(test, ph.name(), "no samples (worm dead?)");
            return;
        }
        double path = 0, maxY = -1e9, minY = 1e9, min = 1e9;
        int minAt = 0, firstNear = -1, solid = 0, wall = 0, outside = 0, withTarget = 0;
        for (int i = 0; i < s.size(); i++) {
            Sample x = s.get(i);
            if (i > 0) path += x.head.distanceTo(s.get(i - 1).head);
            maxY = Math.max(maxY, x.head.y);
            minY = Math.min(minY, x.head.y);
            if (x.dist >= 0 && x.dist < min) { min = x.dist; minAt = x.t - t0; }
            if (x.dist >= 0 && x.dist <= 4 && firstNear < 0) firstNear = x.t - t0;
            if (x.headSolid) solid++;
            if (x.inWall) wall++;
            if (x.outside) outside++;
            if (x.hasTarget) withTarget++;
        }
        Sample first = s.get(0), last = s.get(s.size() - 1);
        Kit.say(test, "phase " + ph.name(),
                "dist " + Kit.f(first.dist) + " -> " + Kit.f(last.dist) + " m, min " + Kit.f(min) + " at +" + minAt + "t, first within 4 m at "
                        + (firstNear < 0 ? "never" : "+" + firstNear + "t") + "; head in solid " + solid + "/" + s.size() + ", isInWall " + wall + "/" + s.size()
                        + ", outside slab " + outside + "/" + s.size() + ", y " + Kit.f(minY) + ".." + Kit.f(maxY) + " (slab " + a.y0 + ".." + (a.y1 + 1)
                        + "); path " + Kit.f(path) + " m in " + (t1 - t0) / 20 + " s = " + Kit.f(path / ((t1 - t0) / 20.0)) + " m/s; getTarget non-null " + withTarget + "/" + s.size()
                        + "; tail-to-head " + Kit.f(last.tail.distanceTo(last.head)) + " m");
    }
}
