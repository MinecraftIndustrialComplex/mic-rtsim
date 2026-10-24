package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKey;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.seibel.distanthorizons.api.DhApi;
import com.seibel.distanthorizons.api.enums.rendering.EDhApiBlockMaterial;
import com.seibel.distanthorizons.api.interfaces.render.IDhApiCustomRenderObjectFactory;
import com.seibel.distanthorizons.api.interfaces.render.IDhApiCustomRenderRegister;
import com.seibel.distanthorizons.api.interfaces.render.IDhApiRenderableBoxGroup;
import com.seibel.distanthorizons.api.interfaces.world.IDhApiLevelWrapper;
import com.seibel.distanthorizons.api.methods.events.sharedParameterObjects.DhApiRenderParam;
import com.seibel.distanthorizons.api.objects.math.DhApiVec3d;
import com.seibel.distanthorizons.api.objects.render.DhApiRenderableBox;
import java.awt.Color;
import java.io.Closeable;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.world.phys.Vec3;

/**
 * The far view of trains: one DH group per carriage, moved by one controller per frame
 * (research/distant-horizons-2026-10-21.md §4.1, §5, §6.4). The same controller runs the dragons first
 * ({@link FarDragonView}) and gives the carriages what of the tier's budget the dragons left.
 *
 * <ul>
 *   <li><b>One relative group per carriage</b>, its boxes cut by {@link StaircaseCutter} around the run's
 *       midpoint. Each frame the controller moves the origin to the carriage's interpolated midpoint, which
 *       DH treats as a uniform; the boxes are cut again only when {@link HeadingRule} says the run turned,
 *       paced by {@link RecutPacer}.</li>
 *   <li><b>One controller</b> in {@code DhApiBeforeGenericRenderSetupEvent}, run once per frame (DH fires
 *       that event for both of its generic passes). Groups carry no lambdas of their own.</li>
 *   <li><b>The hide test</b> (§5, {@link FarHide#dhDraws}, {@link HandOverRule}): a carriage is not drawn
 *       while the client has its real carriage entity, or while its midpoint is in a chunk the client has
 *       loaded (vanilla draws over DH there, so a proxy would show only where vanilla draws sky;
 *       {@link FarBandRenderer} draws it there instead, when it has no entity), unless some of it could be in
 *       DH's vanilla fade ({@link FadeRing}). There both draw it, the band with this group's boxes
 *       ({@link #shownCut}), so the band's box fades into the same box (DHT45).</li>
 *   <li><b>The budget</b> ({@link FarDetail}): the nearest visible carriages are drawn, up to the tier's
 *       groups and boxes; hidden ones keep their group, inactive, until the registered budget needs it.</li>
 *   <li><b>The register</b> is the one of the level being drawn, from the render parameter (§3.1). DH closes a
 *       level's groups with the level, so a new level (or a level load or unload event, or leaving the world)
 *       starts a fresh set.</li>
 * </ul>
 *
 * <p>A carriage whose look has its model ({@link FarModelRenderer}, docs/choices-dh-feed.md DHT63) is drawn as that,
 * not here: these boxes are for the rest.
 *
 * <p>Everything here runs on the render thread, except {@link #levelChanged()}, which only sets a flag.
 */
final class FarTrainView {
    static final String GROUP_NAME = MicRtsimDh.MODID + ":carriage";
    /** A carriage far away: 3 wide, 3 tall, 1 block past each bogey point (research note §4.1). */
    static final double WIDTH = 3.0, HEIGHT = 3.0, OVERHANG = 1.0;
    /**
     * The longest run between a carriage's bogey points that is drawn (DHT73). Create assembles no carriage
     * longer than its maxAssemblyLength (128 blocks by default); a longer run is a bad sample or a bad
     * extrapolation, and drawn it would be a box hundreds of blocks long. Such a mover is skipped, counted and
     * logged instead.
     */
    static final double MAX_RUN = 160.0;
    /** How long a carriage nobody sees keeps its group before it is released. */
    private static final long FORGET_NANOS = 10_000_000_000L;

    private static final Map<FarKey, Proxy> PROXIES = new HashMap<>();
    private static final Map<Integer, Color> COLOURS = new HashMap<>();
    private static final List<Candidate> CANDIDATES = new ArrayList<>();
    private static final double[] SEG = new double[6];
    private static final DhApiVec3d ORIGIN = new DhApiVec3d(0, 0, 0);

    private static volatile IDhApiCustomRenderRegister register;
    private static volatile boolean levelChanged;
    private static long frame;
    private static long updatedFrame = -1;
    private static int errors;

    // What the last frame did, for the log and the bench
    static int lastActive, lastBoxes, lastRegistered, lastHidden, lastCandidates;
    static long recuts, confirms, creations, releases;
    /** Carriage samples skipped for a run longer than {@link #MAX_RUN} (DHT73), and how many were logged. */
    static long oddRuns;
    private static int oddLogged;
    static long controllerNanos, controllerFrames;

    private FarTrainView() {}

    /** One carriage's group and the run its boxes were cut for. */
    private static final class Proxy {
        final FarKey key;
        final IDhApiRenderableBoxGroup group;
        final RecutPacer pacer = new RecutPacer();
        double cutX, cutY, cutZ;
        /** The boxes this group was cut with, relative to its origin (the band draws these in the fade). */
        List<double[]> cut = List.of();
        int boxCount;
        int rgb;
        long wantedAt;
        long touchedFrame = -1;
        boolean active;

        Proxy(FarKey key, IDhApiRenderableBoxGroup group) {
            this.key = key;
            this.group = group;
        }

        void setActive(boolean on) {
            if (active != on) {
                group.setActive(on);
                active = on;
            }
        }
    }

    private record Candidate(FarMover mover, double lx, double ly, double lz, double tx, double ty, double tz,
            double distanceSq) {
    }

    /** The farthest a carriage's boxes reach from its run's midpoint, for a run (rx, ry, rz) between bogeys. */
    static double reach(double rx, double ry, double rz) {
        return Math.sqrt(rx * rx + ry * ry + rz * rz) / 2.0 + OVERHANG + WIDTH;
    }

    /** True when the run in {@code seg} is not finite or is longer than {@link #MAX_RUN}; logs nothing. */
    static boolean tooLong(double[] seg) {
        double rx = seg[0] - seg[3], ry = seg[1] - seg[4], rz = seg[2] - seg[5];
        return !(rx * rx + ry * ry + rz * rz <= MAX_RUN * MAX_RUN);
    }

    /**
     * True when carriage {@code m}'s run at {@code clock} ({@code seg}, as {@link FarMover#segmentAt} wrote it)
     * is not a finite vector or is longer than {@link #MAX_RUN}: it is not drawn, by the boxes or the models
     * (DHT73). Counted, and the first few logged with the samples it came from.
     */
    static boolean oddRun(FarMover m, double[] seg, double clock) {
        if (!tooLong(seg))
            return false;
        double rx = seg[0] - seg[3], ry = seg[1] - seg[4], rz = seg[2] - seg[5];
        double run2 = rx * rx + ry * ry + rz * rz;
        oddRuns++;
        if (oddLogged < 10 || oddRuns % 1000 == 0) {
            oddLogged++;
            MicRtsimDh.LOGGER.warn("mic_rtsim_dh: carriage {} has a run of {} blocks between its bogey points (over {});"
                    + " not drawn ({} so far). Drawn at {} {} {} to {} {} {}, from {} to {} between far clock {} and"
                    + " {}, now {}", m.key(), String.format(Locale.ROOT, "%.1f", Math.sqrt(run2)), MAX_RUN, oddRuns,
                    seg[0], seg[1], seg[2], seg[3], seg[4], seg[5], java.util.Arrays.toString(m.prev()),
                    java.util.Arrays.toString(m.cur()), m.sampleTick(), m.sampleTick() + m.intervalTicks(), clock);
        }
        return true;
    }

    /**
     * The boxes DH drew for this carriage at the controller's last run, relative to its midpoint, or null when DH
     * did not draw it (render thread only; the list is not to be changed).
     */
    static List<double[]> shownCut(FarKey key) {
        Proxy p = PROXIES.get(key);
        return p != null && p.active ? p.cut : null;
    }

    // -----------------------------------------------------------------------------------------------------
    // Events

    /** A new frame (NeoForge's RenderFrameEvent.Pre): the controller runs at most once per frame. */
    static void newFrame() {
        frame++;
    }

    /** Frames counted so far. */
    static long frame() {
        return frame;
    }

    /** A DH level with a render register loaded: the next frame releases every group and starts over. */
    static void levelChanged() {
        levelChanged = true;
    }

    /**
     * A DH level unloaded (on whatever thread DH fires it): if it is the one drawn (or it no longer says), the
     * next frame releases every group. Another level's unload (a single-player server level) changes nothing.
     */
    static void levelUnloaded(IDhApiCustomRenderRegister reg) {
        if (reg == null || reg == register)
            levelChanged = true;
    }

    /** DhApiBeforeGenericRenderSetupEvent: fired twice a frame (the SSAO pass and the other); acts once. */
    static void beforeSetup(DhApiRenderParam param) {
        if (frame == updatedFrame)
            return;
        updatedFrame = frame;
        long t0 = System.nanoTime();
        try {
            update(param, t0);
        } catch (RuntimeException | LinkageError e) {
            // Never take DH's frame down; say what broke, a few times
            if (errors++ < 5)
                MicRtsimDh.LOGGER.error("mic_rtsim_dh: far train view failed this frame", e);
        }
        controllerNanos += System.nanoTime() - t0;
        controllerFrames++;
    }

    /** The client left its world: release every group now (on the render thread, which is the client's). */
    static void releaseAll() {
        FarDragonView.releaseAll(register);
        Iterator<Proxy> it = PROXIES.values().iterator();
        while (it.hasNext()) {
            release(it.next());
            it.remove();
        }
        register = null;
        lastActive = lastBoxes = lastRegistered = lastHidden = lastCandidates = 0;
    }

    // -----------------------------------------------------------------------------------------------------
    // The controller

    private static void update(DhApiRenderParam param, long now) {
        if (levelChanged) {
            levelChanged = false;
            releaseAll();
        }
        if (!FarGates.ready()) {
            if (!PROXIES.isEmpty() || FarDragonView.held() > 0)
                releaseAll();
            return;
        }
        IDhApiLevelWrapper wrapper = param.clientLevelWrapper;
        IDhApiCustomRenderRegister reg = wrapper == null ? null : wrapper.getRenderRegister();
        IDhApiCustomRenderObjectFactory factory = DhApi.Delayed.customRenderObjectFactory;
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        if (reg == null || factory == null || level == null)
            return;
        if (reg != register) {
            releaseAll();
            register = reg;
        }

        FadeRing.update();
        FarDetail tier = DhFarConfig.detail();
        StaircaseCutter.Shape shape = new StaircaseCutter.Shape(WIDTH, HEIGHT, OVERHANG, tier.step);
        String dimension = level.dimension().location().toString();
        Vec3 cam = mc.gameRenderer.getMainCamera().getPosition();
        double radius = FarGates.radiusBlocks();
        double radiusSq = radius * radius;
        double clock = FarClock.ticks() + param.partialTicks;

        // Dragons first; the carriages get what is left of the budget
        FarDragonView.Used dragons = FarDragonView.update(reg, factory, level, tier, dimension, cam, radiusSq, clock,
                now, frame);
        int groupCap = Math.max(0, tier.activeGroups - dragons.groups());
        int boxCap = Math.max(0, tier.boxes - dragons.boxes());
        int registeredCap = Math.max(0, tier.registeredGroups - dragons.registered());

        // Who could be drawn: in this level, inside DH's radius, and DH's to draw
        CANDIDATES.clear();
        int hidden = 0;
        for (FarMover m : FarObjects.movers()) {
            if (m.kind() != FarKind.CARRIAGE || !dimension.equals(m.dimension()))
                continue;
            m.segmentAt(clock, SEG);
            double mx = (SEG[0] + SEG[3]) / 2.0, my = (SEG[1] + SEG[4]) / 2.0, mz = (SEG[2] + SEG[5]) / 2.0;
            double dx = mx - cam.x, dz = mz - cam.z;
            double d2 = dx * dx + dz * dz;
            if (!(d2 <= radiusSq) || oddRun(m, SEG, clock))
                continue;
            // The real carriage's look, where it has one (FarModelRenderer); its group, if any, goes dark
            if (FarModelRenderer.cover(m) != FarModelRenderer.Cover.BOX)
                continue;
            Proxy p = PROXIES.get(m.key());
            if (p != null)
                p.wantedAt = now;
            if (!FarHide.dhDraws(m, level, mx, my, mz, reach(SEG[0] - SEG[3], SEG[1] - SEG[4], SEG[2] - SEG[5]),
                    cam)) {
                hidden++;
                continue;
            }
            CANDIDATES.add(new Candidate(m, SEG[0], SEG[1], SEG[2], SEG[3], SEG[4], SEG[5], d2));
        }
        CANDIDATES.sort((a, b) -> Double.compare(a.distanceSq, b.distanceSq));

        // The nearest, within the tier's budget
        int active = 0, boxes = 0;
        for (Candidate c : CANDIDATES) {
            if (active >= groupCap)
                break;
            FarMover m = c.mover;
            double rx = c.lx - c.tx, ry = c.ly - c.ty, rz = c.lz - c.tz;
            ORIGIN.x = (c.lx + c.tx) / 2.0;
            ORIGIN.y = (c.ly + c.ty) / 2.0;
            ORIGIN.z = (c.lz + c.tz) / 2.0;
            Proxy p = PROXIES.get(m.key());
            if (p == null) {
                List<double[]> cut = StaircaseCutter.cut(rx, ry, rz, shape);
                if (cut.isEmpty() || boxes + cut.size() > boxCap)
                    continue;
                if (PROXIES.size() >= registeredCap && !releaseOneUnseen())
                    continue;
                p = create(factory, m, cut, rx, ry, rz, now);
                if (p == null)
                    continue;
            } else {
                if (boxes + p.boxCount > boxCap)
                    continue;
                if (p.rgb != m.rgb() || HeadingRule.needsRecut(p.cutX, p.cutY, p.cutZ, rx, ry, rz)) {
                    if (p.pacer.mayCut(now))
                        recut(p, m, StaircaseCutter.cut(rx, ry, rz, shape), rx, ry, rz, now);
                } else if (p.pacer.confirmDue(now)) {
                    p.group.triggerBoxChange();
                    confirms++;
                }
                p.group.setOriginBlockPos(ORIGIN);
            }
            p.touchedFrame = frame;
            p.wantedAt = now;
            p.setActive(true);
            active++;
            boxes += p.boxCount;
        }

        // Everything not drawn this frame goes dark; what nobody has wanted for a while is released
        Iterator<Proxy> it = PROXIES.values().iterator();
        while (it.hasNext()) {
            Proxy p = it.next();
            if (p.touchedFrame == frame)
                continue;
            p.setActive(false);
            if (now - p.wantedAt > FORGET_NANOS) {
                release(p);
                it.remove();
            }
        }
        lastActive = active;
        lastBoxes = boxes;
        lastRegistered = PROXIES.size();
        lastHidden = hidden;
        lastCandidates = CANDIDATES.size();
        CANDIDATES.clear();
    }

    private static Proxy create(IDhApiCustomRenderObjectFactory factory, FarMover m, List<double[]> cut,
            double rx, double ry, double rz, long now) {
        IDhApiRenderableBoxGroup group = factory.createRelativePositionedGroup(GROUP_NAME,
                new DhApiVec3d(ORIGIN.x, ORIGIN.y, ORIGIN.z), boxes(cut, m.rgb()));
        try {
            register.add(group);
        } catch (IllegalArgumentException e) {
            close(group);
            return null;
        }
        Proxy p = new Proxy(m.key(), group);
        p.active = group.isActive();
        p.cutX = rx;
        p.cutY = ry;
        p.cutZ = rz;
        p.cut = cut;
        p.boxCount = cut.size();
        p.rgb = m.rgb();
        p.pacer.cut(now);
        PROXIES.put(m.key(), p);
        creations++;
        return p;
    }

    private static void recut(Proxy p, FarMover m, List<double[]> cut, double rx, double ry, double rz, long now) {
        if (cut.isEmpty())
            return;
        p.group.clear();
        p.group.addAll(boxes(cut, m.rgb()));
        p.group.triggerBoxChange();
        p.cutX = rx;
        p.cutY = ry;
        p.cutZ = rz;
        p.cut = cut;
        p.boxCount = cut.size();
        p.rgb = m.rgb();
        p.pacer.cut(now);
        recuts++;
    }

    /** Releases the group that has gone unwanted the longest, if any is not drawn this frame. */
    private static boolean releaseOneUnseen() {
        Proxy oldest = null;
        for (Proxy p : PROXIES.values())
            if (p.touchedFrame != frame && (oldest == null || p.wantedAt < oldest.wantedAt))
                oldest = p;
        if (oldest == null)
            return false;
        release(oldest);
        PROXIES.remove(oldest.key);
        return true;
    }

    private static List<DhApiRenderableBox> boxes(List<double[]> cut, int rgb) {
        Color colour = COLOURS.computeIfAbsent(rgb, Color::new);
        List<DhApiRenderableBox> out = new ArrayList<>(cut.size());
        for (double[] b : cut)
            out.add(new DhApiRenderableBox(new DhApiVec3d(b[0], b[1], b[2]), new DhApiVec3d(b[3], b[4], b[5]),
                    colour, EDhApiBlockMaterial.METAL));
        return out;
    }

    private static void release(Proxy p) {
        if (register != null) {
            try {
                register.remove(p.group.getId());
            } catch (RuntimeException e) {
                // The level's renderer is closing; DH drops its map with it
            }
        }
        close(p.group);
        releases++;
    }

    /** DH's remove does not free the group's buffers; RenderableBoxGroup is Closeable and queues that. */
    private static void close(IDhApiRenderableBoxGroup group) {
        if (group instanceof Closeable c) {
            try {
                c.close();
            } catch (Exception e) {
                // Freeing is best effort; the level's close frees the rest
            }
        }
    }
}
