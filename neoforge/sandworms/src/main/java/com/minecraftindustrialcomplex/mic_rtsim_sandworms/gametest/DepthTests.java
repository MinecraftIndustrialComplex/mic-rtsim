package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import com.minecraftindustrialcomplex.mic_rtsim_sandworms.Probes;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

/**
 * Test 1: depth. A sand-topped slab 61 blocks thick (relative y 0 to 60; its surface is at 61) with air above. The worm is built
 * directly (the mod's own spawn helpers put it 100 to 200 blocks away) with its head 3 blocks below the surface, and steered at a
 * target 60 blocks east in a carved stone room 20 or 40 blocks below the surface: with {@code setAggroTargetEntity} (a 1000-health
 * villager that cannot move) or with {@code thumperTarget} (a real thumper block on the room's floor). The head is sampled every 5
 * ticks for 800 ticks (40 s). Nothing here asserts a behaviour: it is measured.
 */
@GameTestHolder(MicRtsimSandworms.MODID)
@PrefixGameTestTemplate(false)
public final class DepthTests {
    private DepthTests() {}

    static final int SURFACE = 61; // first air row above the slab, relative to the structure origin
    static final int TICKS = 800;

    @GameTest(template = "arena", timeoutTicks = 1200, batch = "depth_aggro_20")
    public static void depthAggro20(GameTestHelper helper) {
        dive(helper, "depthAggro20", 20, false);
    }

    @GameTest(template = "arena", timeoutTicks = 1200, batch = "depth_aggro_40")
    public static void depthAggro40(GameTestHelper helper) {
        dive(helper, "depthAggro40", 40, false);
    }

    @GameTest(template = "arena", timeoutTicks = 1200, batch = "depth_thumper_20")
    public static void depthThumper20(GameTestHelper helper) {
        dive(helper, "depthThumper20", 20, true);
    }

    @GameTest(template = "arena", timeoutTicks = 1200, batch = "depth_thumper_40")
    public static void depthThumper40(GameTestHelper helper) {
        dive(helper, "depthThumper40", 40, true);
    }

    /** The target is level with the worm, 3 blocks below the surface in sand: the case the mod is built for. */
    @GameTest(template = "arena", timeoutTicks = 1200, batch = "depth_aggro_3")
    public static void depthAggro3(GameTestHelper helper) {
        dive(helper, "depthAggro3", 3, false);
    }

    static Arena slab(GameTestHelper helper) {
        Arena a = new Arena(helper, -190, 100, 0, SURFACE - 1, -45, 45);
        a.fill(Blocks.STONE.defaultBlockState());
        a.fillBox(a.x0, a.x1, a.origin.getY() + SURFACE - 6, a.y1, a.z0, a.z1, Blocks.SAND.defaultBlockState());
        return a;
    }

    private record Sample(int t, Vec3 head, Vec3 tail, boolean inBlocks, double dist, int stage, boolean breaching, boolean escaping, boolean hasAggro, int segs) {}

    private static void dive(GameTestHelper helper, String test, int depth, boolean thumper) {
        if (!Kit.run(helper, test, true)) return;
        Arena a = slab(helper);
        double ty = SURFACE - depth;
        Vec3 start = a.at(0.5, SURFACE - 3, 0.5);
        Vec3 targetPos = a.at(60.5, ty, 0.5);
        if (depth > 3) a.room(targetPos, 2);
        ServerPlayer bystander = Kit.trackedPlayer(a.level, a.at(0.5, SURFACE, 300.5), "worm-bystander");
        Entity worm = Sw.create(a.level, start);
        LivingEntity victim = null;
        BlockPos thumperPos = BlockPos.containing(targetPos);
        if (thumper) {
            a.set(thumperPos, Sw.thumperBlock(false));
            Sw.thumperTarget(worm, thumperPos.getCenter());
        } else {
            victim = Kit.victim(a.level, targetPos, 1000);
            Sw.aggro(worm, victim);
        }
        Sw.add(a.level, worm);
        Probes.HITS.clear();
        Probes.recordHits = true;
        Kit.say(test, "setup", "head starts " + Kit.v(start) + " (surface " + (a.origin.getY() + SURFACE) + "), " + (thumper ? "thumper block" : "villager") + " at "
                + Kit.v(targetPos) + " = " + depth + " blocks below the surface, 60 blocks east; slab x " + a.x0 + ".." + a.x1 + " y " + a.y0 + ".." + a.y1
                + "; biome datapack " + (Kit.BIOME_DEFAULT ? "off (desert tag only)" : "on (every overworld biome)"));
        List<Sample> samples = new ArrayList<>();
        List<String> csv = new ArrayList<>();
        csv.add("t,hx,hy,hz,tx,ty,tz,inBlocks,dist,stage,breaching,escaping,hasAggro,segments");
        LivingEntity v = victim;
        int[] t = {0};
        helper.onEachTick(() -> {
            if (t[0] > TICKS) return;
            if (t[0] % 5 == 0 && worm.isAlive()) {
                Vec3 head = Sw.head(worm);
                if (head != null) {
                    Vec3 tail = Sw.tail(worm);
                    boolean inBlocks = a.level.collidesWithSuffocatingBlock(null, new net.minecraft.world.phys.AABB(head.x - 3.75, head.y - 3.75, head.z - 3.75, head.x + 3.75, head.y + 3.75, head.z + 3.75));
                    double d = head.distanceTo(targetPos);
                    Sample s = new Sample(t[0], head, tail, inBlocks, d, Sw.stage(worm), Boolean.TRUE.equals(Sw.field(worm, "breaching")), Boolean.TRUE.equals(Sw.field(worm, "escaping")),
                            Sw.aggroTarget(worm) != null || Sw.thumperTarget(worm) != null, Sw.segmentsNonNull(worm));
                    samples.add(s);
                    csv.add(t[0] + "," + Kit.v(head) + "," + (tail == null ? "" : Kit.v(tail)) + "," + inBlocks + "," + Kit.f(d) + "," + s.stage + "," + s.breaching + "," + s.escaping + "," + s.hasAggro + "," + s.segs);
                }
            }
            if (t[0] == TICKS) {
                Kit.csv(test, csv);
                summarize(test, samples, a, targetPos, depth);
                boolean thumperLeft = thumper && Sw.isThumper(a.level, thumperPos);
                if (thumper) Kit.say(test, "thumper block still there at the end", thumperLeft);
                if (v != null) Kit.say(test, "villager health (1000 at start)", v.getHealth() + (v.isAlive() ? "" : " DEAD"));
                Kit.say(test, "damage events", Probes.HITS.size() + " " + Probes.HITS.stream().limit(6).toList());
                Probes.recordHits = false;
                Kit.say(test, "worm alive / segments alive", worm.isAlive() + " / " + (worm.isAlive() ? Sw.segmentsNonNull(worm) : 0) + " of " + Sw.segmentCount(worm));
                worm.discard();
                if (v != null) v.discard();
                if (thumper) a.level.removeBlockEntity(thumperPos);
                Kit.removeTrackedPlayer(bystander);
                a.close();
                helper.succeed();
            }
            t[0]++;
        });
    }

    private static void summarize(String test, List<Sample> s, Arena a, Vec3 target, int depth) {
        if (s.isEmpty()) {
            Kit.say(test, "head", "never built (no samples)");
            return;
        }
        double path = 0, minD = 1e9, minY = 1e9, maxY = -1e9, maxX = -1e9;
        int minAt = 0, firstNear = -1, blocks = 0, breach = 0, aggro = 0, escapeAt = -1, belowSurface = 0, deepest20 = 0;
        for (int i = 0; i < s.size(); i++) {
            Sample x = s.get(i);
            if (i > 0) path += x.head.distanceTo(s.get(i - 1).head);
            if (x.dist < minD) { minD = x.dist; minAt = x.t; }
            if (x.dist <= 6 && firstNear < 0) firstNear = x.t;
            minY = Math.min(minY, x.head.y);
            maxY = Math.max(maxY, x.head.y);
            maxX = Math.max(maxX, x.head.x);
            if (x.inBlocks) blocks++;
            if (x.breaching) breach++;
            if (x.hasAggro) aggro++;
            if (x.escaping && escapeAt < 0) escapeAt = x.t;
            if (x.head.y < a.origin.getY() + SURFACE - 8) belowSurface++;
            if (x.head.y < a.origin.getY() + SURFACE - 18) deepest20++;
        }
        Sample last = s.get(s.size() - 1);
        Kit.say(test, "head", "started " + Kit.v(s.get(0).head) + ", ended " + Kit.v(last.head) + ", y " + Kit.f(minY) + ".." + Kit.f(maxY) + " (surface " + (a.origin.getY() + SURFACE)
                + ", target y " + Kit.f(target.y) + "), max x " + Kit.f(maxX) + " (target x " + Kit.f(target.x) + "); path " + Kit.f(path) + " m in " + Kit.f(TICKS / 20.0) + " s");
        Kit.say(test, "reach", "closest " + Kit.f(minD) + " m at t=" + minAt + ", first within 6 m at " + (firstNear < 0 ? "never" : "t=" + firstNear) + "; ended " + Kit.f(last.dist) + " m away");
        Kit.say(test, "state", "head box in blocks " + blocks + "/" + s.size() + ", predicted-breach flag " + breach + "/" + s.size() + ", still has a target " + aggro + "/" + s.size()
                + ", escaping from t=" + (escapeAt < 0 ? "never" : escapeAt) + ", more than 8 below surface " + belowSurface + "/" + s.size() + ", more than 18 below " + deepest20 + "/" + s.size());
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < s.size(); i += 5) {
            Sample x = s.get(i);
            sb.append("t").append(x.t).append(":").append(Math.round(x.head.x - a.origin.getX())).append(",").append(Math.round(x.head.y - a.origin.getY())).append(",")
                    .append(Math.round(x.head.z - a.origin.getZ())).append("/s").append(x.stage).append(x.inBlocks ? "B" : "A").append(' ');
        }
        Kit.say(test, "path every 25 ticks (x,y,z relative to the origin / stage, B=in blocks, A=air)", sb.toString().trim());
    }
}
