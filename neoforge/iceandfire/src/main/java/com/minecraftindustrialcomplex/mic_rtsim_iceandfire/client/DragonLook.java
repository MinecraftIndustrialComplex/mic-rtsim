package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.client;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.iafenvoy.iceandfire.entity.LightningDragonEntity;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarLook;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarModel;
import com.minecraftindustrialcomplex.mic_rtsim.client.NullVertexConsumer;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKey;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.IafDragons;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexBuffer;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.blaze3d.vertex.VertexFormatElement;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;

/**
 * Far dragons as Ice and Fire's own model (docs/choices-dh-feed.md DHT65). Per species, colour and stage, Ice
 * and Fire's renderer draws a client-only stand-in at {@link #FRAMES} points of its flight loop and once at
 * rest, and each draw's vertices are kept as static buffers, one per render type the renderer asked for: a far
 * dragon is then a draw per render type, the real model from every side, solid geometry that hills hide and
 * DH's fog reads, with its wings beating at {@link #FRAMES} steps a loop. The nearest few (the renderer asks
 * with {@code live}) are drawn by Ice and Fire's renderer itself each frame, at their own age, so their wings
 * beat smoothly; a live stand-in is started on the bakes' clock (the same renders before its first frame), so
 * its wings and the baked frames' are at the same point of the loop at the same time, and a dragon that goes
 * from one to the other keeps its beat.
 *
 * <p>The stand-ins are entities made with the species' type and never added to a level: they never tick, no
 * server knows them, nothing but this class holds them, and their ids are negative, so no entity the server
 * sends shares one (D3). NeoForge still tells listeners of their construction and their renders. Bakes run a
 * render at a time inside the frame's budget, so a new colour costs no hitch. Render thread.
 */
public final class DragonLook implements FarLook {
    /** Points of the flight loop baked; renders before them let the animation settle into the loop. */
    static final int FRAMES = 12, WARM = 2, REST_WARM = 2;
    /** Ice and Fire's flight loop in ticks: its 1.2 s flight animation played at 0.4 (DragonBaseEntity's controller). */
    static final double LOOP = 60.0;
    private static final int T0 = 200_000;
    /** Open sky, no block light: the lightmap darkens them at night as it does DH's terrain. */
    static final int SKY_LIGHT = 15 << 20;
    /** Where the renders that only move the animation on draw to. */
    private static final MultiBufferSource NULL_SOURCE = rt -> NullVertexConsumer.INSTANCE;
    /** A pose set nobody drew for this long is freed, a live stand-in sooner (PLACEHOLDERS). */
    static final long KEEP_NANOS = 60_000_000_000L, LIVE_KEEP_NANOS = 5_000_000_000L;
    /**
     * At most this many pose sets at once (about 2 MiB of GPU each); past it, the set least recently drawn makes
     * room if nobody drew it for {@link #EVICT_NANOS}, and otherwise a new one waits as a box (PLACEHOLDERS).
     */
    static final int MAX_SETS = 32;
    static final long EVICT_NANOS = 1_000_000_000L;
    /** A wanted set nobody asked for in this long is forgotten, and freed if half baked. */
    static final long WANT_NANOS = 10_000_000_000L;
    /** The stand-ins' ids, counting down from here: below every id a server gives out. */
    private static final int FIRST_ID = -1_000_000;
    private static int nextId = FIRST_ID;

    /** A baked set: the flight loop's frames and the rest pose, and the sphere that holds every one of them. */
    private static final class Poses {
        final int species, variant, stage;
        @SuppressWarnings("unchecked")
        final List<FarModel.Part>[] frames = new List[FRAMES];
        List<FarModel.Part> rest;
        /** The next render: warm-ups, the frames, the rest's warm-ups, the rest. */
        int step;
        DragonBaseEntity probe;
        /** About the model's origin (its feet), at yaw 0: lowest and highest y, widest horizontal distance squared. */
        float minY = Float.MAX_VALUE, maxY = -Float.MAX_VALUE, maxH2;
        int vertices;
        long bytes, nanos, used;

        Poses(int species, int variant, int stage) {
            this.species = species;
            this.variant = variant;
            this.stage = stage;
        }

        static final int STEPS = WARM + FRAMES + REST_WARM + 1;

        boolean ready() {
            return step >= STEPS;
        }

        void close() {
            for (List<FarModel.Part> f : frames)
                if (f != null)
                    for (FarModel.Part p : f)
                        p.buffer().close();
            if (rest != null)
                for (FarModel.Part p : rest)
                    p.buffer().close();
        }
    }

    private static final class StandIn {
        DragonBaseEntity entity;
        int species = -1;
        boolean flying;
        long used;
        /** The tick its flight loop last started at: one {@link #loopStart} gives. */
        long loop;
    }

    private final Map<Integer, Poses> sets = new HashMap<>();
    /** Sets asked for and not baked, oldest ask first: key to when last asked. */
    private final LinkedHashMap<Integer, Long> wanted = new LinkedHashMap<>();
    private final Map<FarKey, StandIn> standIns = new HashMap<>();
    private final Map<FarKey, Float> lastYaw = new HashMap<>();
    /** Sets that would not bake: boxes until the level changes. */
    private final Set<Integer> failed = new HashSet<>();
    /** Ice and Fire's render size at an age, per species (a probe's, aged): the model's scale is a third of it. */
    private final Map<Integer, Float> renderSizes = new HashMap<>();
    private final DragonBaseEntity[] probes = new DragonBaseEntity[IafDragons.SPECIES.size()];
    private ClientLevel levelOf;
    private long clock, lastSweep;
    private int errors;
    private long setsBaked, bakeNanos;
    private double maxStepMs, maxRenderMs;
    /** Which render of a bake {@link #maxRenderMs} was. */
    private String maxRenderWhat = "none";
    /** Live models placed since the last stats (once a frame each, however many passes draw them). */
    private int liveDrawn;

    static int key(int species, int variant, int stage) {
        return (species * 16 + Math.floorMod(variant, 16)) * 8 + stage;
    }

    private static int species(FarMover m) {
        return m.style() >= 0 && m.style() < IafDragons.SPECIES.size() ? m.style() : -1;
    }

    private static int stage(FarMover m) {
        return Math.max(1, Math.min(5, m.size()));
    }

    /** The age in days the baked frames of {@code stage} are drawn at: the stage's middle. */
    static int ageFor(int stage) {
        return stage >= 5 ? 112 : (stage - 1) * 25 + 12;
    }

    private static int age(FarMover m) {
        return m.age() >= 0 ? Math.min(m.age(), 1000) : ageFor(stage(m));
    }

    @Override
    public Status status(FarMover m) {
        int species = species(m);
        if (species < 0)
            return Status.NONE;
        int k = key(species, m.variant(), stage(m));
        if (failed.contains(k))
            return Status.NONE;
        Poses p = sets.get(k);
        if (p != null && p.ready()) {
            p.used = clock;
            return Status.READY;
        }
        wanted.put(k, clock);
        return Status.PENDING;
    }

    @Override
    public boolean place(FarMover m, double[] seg, double clock, boolean live, FarModel out) {
        int species = species(m);
        if (species < 0 || levelOf == null)
            return false;
        int stage = stage(m);
        Poses p = sets.get(key(species, m.variant(), stage));
        if (p == null || !p.ready())
            return false;
        // The heading: the segment runs from the tail to the head; a dragon at rest keeps the last one it had
        double dx = seg[0] - seg[3], dz = seg[2] - seg[5];
        float yawDeg;
        if (dx * dx + dz * dz > 1e-6) {
            yawDeg = (float) Math.toDegrees(Math.atan2(-dx, dz));
            lastYaw.put(m.key(), yawDeg);
        } else
            yawDeg = lastYaw.getOrDefault(m.key(), 0f);
        float yawRad = (float) Math.toRadians(yawDeg);
        double time = clock + Math.floorMod(m.key().hashCode(), 60);
        boolean flying = m.state() != FarState.ROOST;
        int age = age(m);
        float s = renderSize(species, age) / renderSize(species, ageFor(stage));
        if (!(s > 0f) || !Float.isFinite(s))
            s = 1f;
        // The feed's point is the hitbox's middle; the model stands on its feet (DragonFeedSource)
        float centre = (float) (0.6 * Math.min(renderSize(species, age) * 0.35, 7.0));
        float lo = s * p.minY - centre, hi = s * p.maxY - centre, half = Math.max(Math.abs(lo), Math.abs(hi));
        out.radius = (float) Math.sqrt(s * s * p.maxH2 + half * half);
        if (live) {
            StandIn st = standIns.computeIfAbsent(m.key(), k -> new StandIn());
            EntityRenderer<DragonBaseEntity> r;
            long loop = loopStart(time);
            if (st.entity == null || st.species != species || st.flying != flying || loop < st.loop) {
                // A fresh stand-in (its animation starts over: a new look, or time run back), started as the
                // bakes start theirs; kept only once started, so a warm-up that throws leaves none half started
                st.entity = null;
                DragonBaseEntity fresh = newStandIn(levelOf, species);
                r = renderer(fresh);
                st.loop = startOnBakeClock(fresh, r, m.variant(), age, flying, yawDeg, time);
                st.entity = fresh;
                st.species = species;
                st.flying = flying;
            } else {
                r = renderer(st.entity);
                if (loop > st.loop) {
                    // GeckoLib starts a loop at the first render past the last one's end and drops what it ran
                    // over: a render at the end itself keeps the stand-in on the bakes' clock, after gaps too
                    pose(st.entity, m.variant(), age, flying, yawDeg, loop);
                    r.render(st.entity, yawDeg, 0f, new PoseStack(), NULL_SOURCE, SKY_LIGHT);
                    st.loop = loop;
                }
            }
            st.used = this.clock;
            DragonBaseEntity d = st.entity;
            pose(d, m.variant(), age, flying, yawDeg, time);
            float frac = (float) (time - Math.floor(time));
            out.parts = List.of();
            out.local.identity();
            out.lightYaw = Float.NaN;
            out.live = (ps, buffers) -> {
                ps.pushPose();
                ps.translate(0f, -centre, 0f);
                r.render(d, yawDeg, frac, ps, buffers, SKY_LIGHT);
                ps.popPose();
            };
            liveDrawn++;
            return true;
        }
        List<FarModel.Part> parts;
        if (flying)
            parts = p.frames[frameAt(time)];
        else
            parts = p.rest;
        out.parts = parts;
        // Ice and Fire turns the model by (180 - yaw) about +y; the baked frames have yaw 0 in them already
        out.local.identity().translate(0f, -centre, 0f).rotateY(-yawRad).scale(s);
        out.lightYaw = -yawRad;
        out.live = null;
        return true;
    }

    @Override
    public void frame(long now, long deadline) {
        clock = now;
        ClientLevel level = Minecraft.getInstance().level;
        if (level != levelOf) {
            clear();
            levelOf = level;
        }
        if (level == null)
            return;
        if (now - lastSweep > 1_000_000_000L) {
            lastSweep = now;
            sweep(now);
        }
        long t0 = System.nanoTime();
        // The oldest ask first; a set bakes to its end before the next starts, unless the budget runs out
        Iterator<Map.Entry<Integer, Long>> it = wanted.entrySet().iterator();
        while (it.hasNext() && System.nanoTime() < deadline) {
            int k = it.next().getKey();
            Poses p = sets.get(k);
            if (p == null) {
                if (sets.size() >= MAX_SETS && !evictOne(now))
                    continue; // full of sets in use: this one waits, as a box, and the others bake on
                p = new Poses(k / 8 / 16, k / 8 % 16, k % 8);
                sets.put(k, p);
            }
            if (p.ready()) {
                it.remove();
                continue;
            }
            long s0 = System.nanoTime();
            try {
                do {
                    long r0 = System.nanoTime();
                    int was = p.step;
                    boolean fresh = p.probe == null;
                    step(level, p);
                    double ms = (System.nanoTime() - r0) / 1e6;
                    if (ms > maxRenderMs) {
                        maxRenderMs = ms;
                        maxRenderWhat = (fresh ? "a new probe's " : "") + stepName(was);
                    }
                } while (!p.ready() && System.nanoTime() < deadline);
            } catch (RuntimeException | LinkageError e) {
                // A box until the level changes: a renderer that failed once would most likely fail every frame
                it.remove();
                sets.remove(k);
                p.close();
                failed.add(k);
                if (errors++ < 5)
                    MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] a far dragon's poses would not bake; it stays a box",
                            e);
                continue;
            } finally {
                p.nanos += System.nanoTime() - s0;
            }
            if (p.ready()) {
                it.remove();
                p.probe = null;
                p.used = now;
                setsBaked++;
                bakeNanos += p.nanos;
            }
        }
        maxStepMs = Math.max(maxStepMs, (System.nanoTime() - t0) / 1e6);
    }

    /** Frees the ready set least recently drawn, if nobody drew it for {@link #EVICT_NANOS}; false when none. */
    private boolean evictOne(long now) {
        Integer oldest = null;
        long at = Long.MAX_VALUE;
        for (Map.Entry<Integer, Poses> e : sets.entrySet()) {
            Poses p = e.getValue();
            if (p.ready() && now - p.used >= EVICT_NANOS && p.used < at) {
                at = p.used;
                oldest = e.getKey();
            }
        }
        if (oldest == null)
            return false;
        sets.remove(oldest).close();
        return true;
    }

    /**
     * The baked frame of the flight loop nearest {@code time}: frame {@code f} was drawn at {@code T0 + f} steps,
     * so a stand-in started on the same clock ({@link #startOnBakeClock}) is at frame {@code f} whenever its
     * time is that, a whole number of loops on.
     */
    static int frameAt(double time) {
        double into = (time - T0) % LOOP;
        if (into < 0)
            into += LOOP;
        return (int) Math.round(into / (LOOP / FRAMES)) % FRAMES;
    }

    /**
     * The last tick at or before {@code time} that a probe's flight loop starts at, a whole number of loops on:
     * Ice and Fire's controllers (GeckoLib's) start their loop at the render that ends their transition, which in a
     * bake is the last warm-up, {@code T0 - LOOP / FRAMES}.
     */
    static long loopStart(double time) {
        long now = (long) Math.floor(time), first = T0 - (long) (LOOP / FRAMES);
        return now - Math.floorMod(now - first, (long) LOOP);
    }

    /**
     * Gives a fresh stand-in the warm-ups a bake gives its probe, ending at {@link #loopStart}{@code (time)}, so
     * its animation starts as the bake's did; returns that tick.
     */
    private static long startOnBakeClock(DragonBaseEntity d, EntityRenderer<DragonBaseEntity> r, int variant, int age,
            boolean flying, float yaw, double time) {
        long loop = loopStart(time);
        for (int s = 0; s < WARM; s++) {
            pose(d, variant, age, flying, yaw, loop + (s + 1 - WARM) * (LOOP / FRAMES));
            r.render(d, yaw, 0f, new PoseStack(), NULL_SOURCE, SKY_LIGHT);
        }
        return loop;
    }

    private static String stepName(int s) {
        return s < WARM ? "warm-up" : s < WARM + FRAMES ? "frame " + (s - WARM) : s < WARM + FRAMES + REST_WARM
                ? "rest warm-up" : "rest";
    }

    /**
     * Behind the loading screen: a probe of each species, drawn flying and at rest, so the first bake inside a
     * frame finds Ice and Fire's renderers and GeckoLib's animation code warm (cold, the first render took about
     * 90 ms). The probes stay for {@link #renderSize}.
     */
    @Override
    public void prewarm() {
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null)
            return;
        if (level != levelOf) {
            clear();
            levelOf = level;
        }
        long t0 = System.nanoTime();
        for (int species = 0; species < probes.length; species++) {
            try {
                DragonBaseEntity d = probes[species] != null ? probes[species] : (probes[species] = newStandIn(level, species));
                EntityRenderer<DragonBaseEntity> r = renderer(d);
                for (int i = 0; i < 2; i++) {
                    pose(d, 0, ageFor(3), i == 0, 0f, T0 + i * 10.0);
                    r.render(d, 0f, 0f, new PoseStack(), NULL_SOURCE, SKY_LIGHT);
                }
            } catch (RuntimeException | LinkageError e) {
                if (errors++ < 5)
                    MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] a far dragon's probe would not warm up", e);
            }
        }
        MicRtsimIceAndFire.LOGGER.info("[mic_rtsim_iceandfire] far dragons warmed up in {} ms",
                String.format(Locale.ROOT, "%.1f", (System.nanoTime() - t0) / 1e6));
    }

    /** One render of a bake: a warm-up, a frame of the flight loop, a warm-up at rest, or the rest pose. */
    private void step(ClientLevel level, Poses p) {
        if (p.probe == null)
            p.probe = newStandIn(level, p.species);
        DragonBaseEntity d = p.probe;
        EntityRenderer<DragonBaseEntity> r = renderer(d);
        int s = p.step, age = ageFor(p.stage);
        if (s < WARM) {
            pose(d, p.variant, age, true, 0f, T0 + (s - WARM) * (LOOP / FRAMES));
            r.render(d, 0f, 0f, new PoseStack(), NULL_SOURCE, SKY_LIGHT);
        } else if (s < WARM + FRAMES) {
            int f = s - WARM;
            pose(d, p.variant, age, true, 0f, T0 + f * (LOOP / FRAMES));
            p.frames[f] = capture(d, r, p);
        } else if (s < WARM + FRAMES + REST_WARM) {
            pose(d, p.variant, age, false, 0f, T0 + LOOP + (s - WARM - FRAMES) * 10.0);
            r.render(d, 0f, 0f, new PoseStack(), NULL_SOURCE, SKY_LIGHT);
        } else {
            pose(d, p.variant, age, false, 0f, T0 + LOOP + REST_WARM * 10.0 + 10.0);
            p.rest = capture(d, r, p);
        }
        p.step++;
    }

    /** Draws the stand-in once into fresh buffers, a buffer per render type in the order it asked for them. */
    private static List<FarModel.Part> capture(DragonBaseEntity d, EntityRenderer<DragonBaseEntity> r, Poses p) {
        Map<RenderType, ByteBufferBuilder> memory = new LinkedHashMap<>();
        Map<RenderType, BufferBuilder> builders = new LinkedHashMap<>();
        MultiBufferSource src = rt -> builders.computeIfAbsent(rt, t -> {
            ByteBufferBuilder mem = new ByteBufferBuilder(64 * 1024);
            memory.put(t, mem);
            return new BufferBuilder(mem, t.mode(), t.format());
        });
        List<FarModel.Part> parts = new ArrayList<>();
        try {
            r.render(d, 0f, 0f, new PoseStack(), src, SKY_LIGHT);
            for (Map.Entry<RenderType, BufferBuilder> e : builders.entrySet()) {
                MeshData md = e.getValue().build();
                if (md == null)
                    continue;
                RenderType rt = e.getKey();
                int n = md.drawState().vertexCount();
                bounds(md, rt, n, p);
                VertexBuffer vb = new VertexBuffer(VertexBuffer.Usage.STATIC);
                try {
                    vb.bind();
                    vb.upload(md);
                } catch (RuntimeException | LinkageError x) {
                    vb.close();
                    throw x;
                } finally {
                    VertexBuffer.unbind();
                }
                parts.add(new FarModel.Part(rt, vb, n));
                p.vertices += n;
                p.bytes += (long) n * rt.format().getVertexSize();
            }
        } catch (RuntimeException | LinkageError e) {
            for (FarModel.Part part : parts)
                part.buffer().close();
            throw e;
        } finally {
            for (ByteBufferBuilder mem : memory.values())
                mem.close();
        }
        return List.copyOf(parts);
    }

    /** Widens the set's bounds by a draw's vertices (their positions, as the renderer wrote them). */
    private static void bounds(MeshData md, RenderType rt, int n, Poses p) {
        if (!rt.format().contains(VertexFormatElement.POSITION))
            return;
        int stride = rt.format().getVertexSize(), at = rt.format().getOffset(VertexFormatElement.POSITION);
        ByteBuffer b = md.vertexBuffer().duplicate().order(ByteOrder.nativeOrder());
        for (int i = 0; i < n; i++) {
            int o = i * stride + at;
            float x = b.getFloat(o), y = b.getFloat(o + 4), z = b.getFloat(o + 8);
            if (!Float.isFinite(x) || !Float.isFinite(y) || !Float.isFinite(z))
                continue;
            p.minY = Math.min(p.minY, y);
            p.maxY = Math.max(p.maxY, y);
            p.maxH2 = Math.max(p.maxH2, x * x + z * z);
        }
    }

    /** Ice and Fire's render size at {@code age} days, for {@code species}. */
    private float renderSize(int species, int age) {
        return renderSizes.computeIfAbsent(species * 4096 + Math.max(0, Math.min(age, 4095)), k -> {
            DragonBaseEntity d = probes[species];
            if (d == null)
                d = probes[species] = newStandIn(levelOf, species);
            d.setAgeInDays(age);
            return d.getRenderSize();
        });
    }

    /** A client-only stand-in of {@code species}, never added to a level, with an id no server entity has. */
    static DragonBaseEntity newStandIn(ClientLevel level, int species) {
        EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.get(ResourceLocation.parse(IafDragons.SPECIES.get(species)));
        Entity e = type.create(level);
        if (!(e instanceof DragonBaseEntity d))
            throw new IllegalStateException(IafDragons.SPECIES.get(species) + " made " + e);
        // GeckoLib keys a render's animation by the id: a real dragon's would share the stand-in's pose
        d.setId(nextId);
        nextId = nextId <= Integer.MIN_VALUE / 2 ? FIRST_ID : nextId - 1;
        return d;
    }

    /** Puts a far dragon's look on a stand-in: colour, age, flying or at rest, heading, a time in its loop. */
    static void pose(DragonBaseEntity d, int variant, int age, boolean flying, float yaw, double time) {
        var colours = d.dragonType.colors();
        String colour = colours.get(Math.floorMod(variant, colours.size())).getName();
        if (!colour.equals(d.getVariant()))
            d.setVariant(colour);
        if (d.getAgeInDays() != age)
            d.setAgeInDays(age);
        if (d.isFlying() != flying)
            d.setFlying(flying);
        d.flyProgress = flying ? 20f : 0f;
        d.hoverProgress = 0f;
        d.sitProgress = 0f;
        d.sleepProgress = 0f;
        d.tickCount = (int) Math.floor(time);
        d.setYRot(yaw);
        d.yRotO = yaw;
        d.yBodyRot = yaw;
        d.yBodyRotO = yaw;
        d.yHeadRot = yaw;
        d.yHeadRotO = yaw;
        d.setXRot(0f);
        d.xRotO = 0f;
    }

    @SuppressWarnings("unchecked")
    static EntityRenderer<DragonBaseEntity> renderer(DragonBaseEntity d) {
        return (EntityRenderer<DragonBaseEntity>) (EntityRenderer<?>) Minecraft.getInstance().getEntityRenderDispatcher()
                .getRenderer(d);
    }

    /**
     * Forgets asks older than {@link #WANT_NANOS}; frees pose sets nobody drew for {@link #KEEP_NANOS}, half-baked
     * sets nobody wants now, and stand-ins not drawn live for {@link #LIVE_KEEP_NANOS}.
     */
    private void sweep(long now) {
        wanted.values().removeIf(t -> now - t > WANT_NANOS);
        sets.entrySet().removeIf(e -> {
            Poses p = e.getValue();
            if (p.ready() ? now - p.used > KEEP_NANOS : !wanted.containsKey(e.getKey())) {
                p.close();
                return true;
            }
            return false;
        });
        standIns.values().removeIf(s -> now - s.used > LIVE_KEEP_NANOS);
        if (lastYaw.size() > 4096)
            lastYaw.clear();
    }

    /**
     * Not while a lightning dragon breathes: its renderer adds a bolt to one shared renderer each time it draws
     * one (LightningDragonEntityRenderer), so a copy would double the bolts in vanilla's own pass too.
     */
    @Override
    public boolean copies(Entity body) {
        return !(body instanceof LightningDragonEntity l && l.hasLightningTarget());
    }

    @Override
    public void clear() {
        reset();
        failed.clear();
        lastYaw.clear();
        renderSizes.clear();
        java.util.Arrays.fill(probes, null);
        levelOf = null;
    }

    /**
     * The far feed started over (docs/choices-dh-feed.md DHT70): the pose sets and the live stand-ins go; the
     * probes the prewarm drew and their render sizes stay, as do the headings last seen (a dragon at rest is drawn
     * facing the way it last flew) and the looks that would not bake (they would not bake again).
     */
    @Override
    public void reset() {
        for (Poses p : sets.values())
            p.close();
        sets.clear();
        wanted.clear();
        standIns.clear();
    }

    @Override
    public String stats() {
        int ready = 0, vertices = 0;
        long bytes = 0;
        for (Poses p : sets.values())
            if (p.ready()) {
                ready++;
                vertices += p.vertices;
                bytes += p.bytes;
            }
        String s = String.format(Locale.ROOT,
                "dragons: %d pose sets (%d vertices, %.1f MiB), %d wanted, %d failed, %d stand-ins, %d live frames; "
                        + "%d sets baked, %.2f ms each, longest frame's bake %.2f ms, longest render %.2f ms (%s)",
                ready, vertices, bytes / (double) (1 << 20), wanted.size(), failed.size(), standIns.size(), liveDrawn,
                setsBaked,
                setsBaked == 0 ? 0.0 : bakeNanos / 1e6 / setsBaked, maxStepMs, maxRenderMs, maxRenderWhat);
        maxStepMs = 0;
        maxRenderMs = 0;
        maxRenderWhat = "none";
        liveDrawn = 0;
        return s;
    }
}
