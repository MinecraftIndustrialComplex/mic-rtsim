package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_sandworms.MicRtsimSandworms;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

/**
 * Test 5: saving. The worm is a chain entity plus 90-odd segment entities that find each other by UUID. A step list runs on one
 * tick counter. Steps 1 to 4 capture what vanilla would write (the NBT of every part), remove the parts and put back the shapes a
 * chunk load can produce: all of them; the chain alone; the chain and half of the segments; the segments alone. Steps 5 to 7 are
 * the real thing: a new worm builds, the chunks it lies in lose their forced tickets, the test waits for them to unload (entities go
 * to the entity storage), forces them again and counts what comes back.
 */
@GameTestHolder(MicRtsimSandworms.MODID)
@PrefixGameTestTemplate(false)
public final class SaveTests {
    private SaveTests() {}

    @GameTest(template = "arena", timeoutTicks = 6000, batch = "save")
    public static void savedWorm(GameTestHelper helper) {
        String test = "savedWorm";
        if (!Kit.run(helper, test, true)) return;
        Arena a = DepthTests.slab(helper);
        Vec3 target = a.at(60.5, DepthTests.SURFACE - 20, 0.5);
        a.room(target, 2);
        LivingEntity marker = Kit.victim(a.level, target, 1000);
        ServerPlayer bystander = Kit.trackedPlayer(a.level, a.at(0.5, DepthTests.SURFACE, 300.5), "worm-bystander");
        AABB box = new AABB(a.x0 - 250, a.y0 - 600, a.z0 - 250, a.x1 + 250, a.y1 + 300, a.z1 + 250);
        List<CompoundTag> chain = new ArrayList<>();
        List<CompoundTag> segs = new ArrayList<>();
        List<ChunkPos> chunks = new ArrayList<>();
        String[] names = {"S1 all parts back", "S2 chain alone", "S3 chain and the half of the segments nearest the tail", "S4 segments alone, no chain"};
        double[] frac = {1.0, 0.0, 0.5, 1.0};
        boolean[] withChain = {true, true, true, false};
        int[] step = {0};   // 0 build+capture, 1..4 scenarios, 5 build, 6 unload, 7 reload, 8 done
        int[] t0 = {0};
        int[] t = {0};
        helper.onEachTick(() -> {
            int s = step[0], into = t[0] - t0[0];
            switch (s) {
                case 0 -> {
                    if (into == 0) {
                        Entity worm = Sw.create(a.level, a.at(0.5, DepthTests.SURFACE - 3, 0.5));
                        Sw.aggro(worm, marker);
                        Sw.add(a.level, worm);
                    }
                    if (into == 100) {
                        Entity worm = Sw.chainsIn(a.level, box).get(0);
                        Vec3 head = Sw.head(worm);
                        Kit.say(test, "after 100 ticks", "chain " + Sw.segmentCount(worm) + " segments, " + Sw.segmentsNonNull(worm) + " alive, head " + (head == null ? "-" : Kit.v(head))
                                + ", tail " + Kit.v(Sw.tail(worm)) + ", " + Sw.segmentsIn(a.level, box).size() + " segment entities in the box");
                        capture(a.level, box, chain, segs);
                        int bytes = chain.stream().mapToInt(c -> c.toString().length()).sum() + segs.stream().mapToInt(c -> c.toString().length()).sum();
                        Kit.say(test, "captured NBT", "chain " + chain.size() + ", segments " + segs.size() + ", ~" + bytes + " characters of SNBT; chain tag keys "
                                + chain.get(0).getAllKeys().stream().filter(k -> !k.startsWith("segment_")).toList() + " plus segment_N uuids");
                        next(step, t0, t);
                    }
                }
                case 1, 2, 3, 4 -> {
                    int k = s - 1;
                    if (into == 0) {
                        removeAll(a.level, box);
                        List<CompoundTag> use = new ArrayList<>();
                        if (withChain[k]) use.addAll(chain);
                        int n = (int) Math.round(segs.size() * frac[k]);
                        use.addAll(segs.subList(0, n));
                        for (CompoundTag c : use)
                            EntityType.loadEntityRecursive(c, a.level, e -> {
                                a.level.addFreshEntity(e);
                                return e;
                            });
                        Kit.say(test, names[k], "put back " + use.size() + " entities (" + (withChain[k] ? 1 : 0) + " chain, " + n + " segments)");
                    }
                    if (into == 1 || into == 2 || into == 40 || into == 130 || into == 200) {
                        Kit.say(test, names[k] + " +" + into, "chains " + Sw.chainsIn(a.level, box).size() + ", segments " + Sw.segmentsIn(a.level, box).size() + describe(a.level, box) + " | anywhere: " + where(a.level));
                    }
                    if (into >= 200) next(step, t0, t);
                }
                case 5 -> {
                    if (into == 0) {
                        removeAll(a.level, box);
                        Entity w = Sw.create(a.level, a.at(0.5, DepthTests.SURFACE - 3, 0.5));
                        Sw.aggro(w, marker);
                        Sw.add(a.level, w);
                    }
                    if (into == 60) {
                        Kit.say(test, "real unload: before", "chains " + Sw.chainsIn(a.level, box).size() + ", segments " + Sw.segmentsIn(a.level, box).size() + describe(a.level, box));
                        for (int cx = (a.x0 >> 4) - 6; cx <= (a.x1 >> 4) + 6; cx++)
                            for (int cz = (a.z0 >> 4) - 6; cz <= (a.z1 >> 4) + 6; cz++) {
                                a.level.setChunkForced(cx, cz, false);
                                chunks.add(new ChunkPos(cx, cz));
                            }
                        next(step, t0, t);
                    }
                }
                case 6 -> {
                    int loaded = 0;
                    for (ChunkPos c : chunks) if (a.level.getChunkSource().getChunkNow(c.x, c.z) != null) loaded++;
                    if (into % 100 == 0) Kit.say(test, "real unload +" + into, "chunks still loaded " + loaded + " of " + chunks.size()
                            + ", chains " + Sw.chainsIn(a.level, box).size() + ", segments " + Sw.segmentsIn(a.level, box).size());
                    if (loaded == 0 || into >= 600) {
                        Kit.say(test, "real unload: done", "after " + into + " ticks " + (loaded == 0 ? "every chunk unloaded" : loaded + " chunks would not unload"));
                        for (ChunkPos c : chunks) a.level.setChunkForced(c.x, c.z, true);
                        next(step, t0, t);
                    }
                }
                case 7 -> {
                    if (into == 20 || into == 60 || into == 120 || into == 180) {
                        List<? extends Entity> cs = Sw.chainsIn(a.level, box);
                        Kit.say(test, "real unload: after reload +" + into, "chains " + cs.size() + ", segments " + Sw.segmentsIn(a.level, box).size() + describe(a.level, box));
                        if (!cs.isEmpty()) {
                            Entity w = cs.get(0);
                            Kit.say(test, "reloaded chain", "segmentCount " + Sw.segmentCount(w) + ", segments alive " + Sw.segmentsNonNull(w)
                                    + ", has target " + (Sw.aggroTarget(w) != null) + ", escaping " + Sw.field(w, "escaping"));
                        }
                    }
                    if (into >= 200) {
                        removeAll(a.level, box);
                        marker.discard();
                        Kit.removeTrackedPlayer(bystander);
                        a.close();
                        step[0] = 8;
                        helper.succeed();
                    }
                }
                default -> {}
            }
            t[0]++;
        });
    }

    /** Where the chain and the first and last of the segments are, wherever they are: a diagnostic for a box query that finds nothing. */
    private static String where(ServerLevel level) {
        AABB all = new AABB(-3.0e7, -2000, -3.0e7, 3.0e7, 2000, 3.0e7);
        List<? extends Entity> cs = Sw.chainsIn(level, all);
        List<? extends Entity> ss = Sw.segmentsIn(level, all);
        String c = cs.isEmpty() ? "no chain" : "chain at " + Kit.v(cs.get(0).position());
        String f = ss.isEmpty() ? "no segments" : ss.size() + " segments, first at " + Kit.v(ss.get(0).position()) + ", last at " + Kit.v(ss.get(ss.size() - 1).position());
        return c + "; " + f;
    }

    private static void next(int[] step, int[] t0, int[] t) {
        step[0]++;
        t0[0] = t[0] + 1;
    }

    /** Head and tail positions and the mean length of the chain, as the chain reports them. */
    private static String describe(ServerLevel level, AABB box) {
        List<? extends Entity> cs = Sw.chainsIn(level, box);
        if (cs.isEmpty()) return "";
        Entity w = cs.get(0);
        Vec3 h = Sw.head(w);
        return "; chain says segmentCount " + Sw.segmentCount(w) + ", alive " + Sw.segmentsNonNull(w) + ", head " + (h == null ? "-" : Kit.v(h));
    }

    private static void capture(ServerLevel level, AABB box, List<CompoundTag> chain, List<CompoundTag> segs) {
        chain.clear();
        segs.clear();
        List<? extends Entity> cs = Sw.chainsIn(level, box);
        for (Entity e : cs) {
            CompoundTag tag = new CompoundTag();
            if (e.save(tag)) chain.add(tag);
        }
        List<Entity> parts = new ArrayList<>(Sw.segmentsIn(level, box));
        Vec3 tail = cs.isEmpty() ? Vec3.ZERO : cs.get(0).position();
        parts.sort((p, q) -> Double.compare(p.position().distanceToSqr(tail), q.position().distanceToSqr(tail)));
        for (Entity e : parts) {
            CompoundTag tag = new CompoundTag();
            if (e.save(tag)) segs.add(tag);
        }
    }

    private static void removeAll(ServerLevel level, AABB box) {
        for (Entity e : new ArrayList<>(Sw.chainsIn(level, box))) e.discard();
        for (Entity e : new ArrayList<>(Sw.segmentsIn(level, box))) e.discard();
    }
}
