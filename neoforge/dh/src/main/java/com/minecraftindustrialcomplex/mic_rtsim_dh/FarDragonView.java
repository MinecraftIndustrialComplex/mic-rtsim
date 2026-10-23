package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarKey;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.seibel.distanthorizons.api.enums.rendering.EDhApiBlockMaterial;
import com.seibel.distanthorizons.api.interfaces.render.IDhApiCustomRenderObjectFactory;
import com.seibel.distanthorizons.api.interfaces.render.IDhApiCustomRenderRegister;
import com.seibel.distanthorizons.api.interfaces.render.IDhApiRenderableBoxGroup;
import com.seibel.distanthorizons.api.objects.math.DhApiVec3d;
import com.seibel.distanthorizons.api.objects.render.DhApiRenderableBox;
import java.awt.Color;
import java.io.Closeable;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.world.phys.Vec3;

/**
 * The far view of dragons (research/distant-horizons-2026-10-21.md §4.2, §5): per dragon, one DH group for its
 * body and one for its wings in each pose, moved together by {@link FarTrainView}'s controller every frame.
 *
 * <ul>
 *   <li><b>The flipbook.</b> Every pose's wings are cut once ({@link DragonModel}); each frame only the pose of
 *       the moment is active ({@link DragonModel#poseAt}), so a wingbeat costs a flag, not a re-cut. A dragon at
 *       rest folds its wings: only its body is drawn.</li>
 *   <li><b>Re-cuts</b> happen when it turns to another of the 16 headings, grows a stage, changes colour or the
 *       tier's step changes, paced by {@link RecutPacer}. A tier with another number of poses starts the dragon
 *       over.</li>
 *   <li><b>The hide test</b> is the trains' ({@link FarHide#dhDraws}): not while the client has the dragon's
 *       entity, and not in a chunk the client has loaded (the band renderer draws it there) unless some of it
 *       could be in DH's vanilla fade; there the band draws this dragon's cut ({@link #shown}).</li>
 *   <li><b>The budget.</b> Dragons are drawn first, nearest first, up to the tier's dragons; each takes two of
 *       the tier's active groups (body and wings) and its boxes. What is left goes to the carriages.</li>
 * </ul>
 *
 * <p>Render thread only.
 */
final class FarDragonView {
    static final String BODY_NAME = MicRtsimDh.MODID + ":dragon";
    static final String WINGS_NAME = MicRtsimDh.MODID + ":dragon_wings";
    /** The wings are this much darker than the body, so they read as wings. */
    static final double WING_SHADE = 0.7;
    /** Dragon proxies kept registered, drawn or not, per dragon the tier draws. */
    static final int KEEP_PER_DRAWN = 2;
    /** Matte, like hide. */
    static final EDhApiBlockMaterial MATERIAL = EDhApiBlockMaterial.TERRACOTTA;
    private static final long FORGET_NANOS = 10_000_000_000L;
    private static final int POSES = DragonModel.Pose.values().length;

    private static final Map<FarKey, Proxy> PROXIES = new HashMap<>();
    private static final Map<Integer, Color> COLOURS = new HashMap<>();
    private static final List<Candidate> CANDIDATES = new ArrayList<>();
    private static final double[] SEG = new double[6];
    private static final DhApiVec3d ORIGIN = new DhApiVec3d(0, 0, 0);

    // What the last frame did, for the log and the bench
    static int lastDrawn, lastGroups, lastBoxes, lastRegistered, lastHidden, lastCandidates;
    static long recuts, confirms, creations, releases, flips;

    private FarDragonView() {}

    /** What the dragons took of the tier's budget this frame: active groups, boxes, registered groups. */
    record Used(int groups, int boxes, int registered) {
        static final Used NONE = new Used(0, 0, 0);
    }

    private record Candidate(FarMover mover, double x, double y, double z, double hx, double hz, double distanceSq) {
    }

    /** What a dragon's groups were cut for: its stage, its heading step and the staircase step. */
    record Shown(int stage, int heading, double step) {
    }

    /** One dragon's groups and what they were cut for. */
    private static final class Proxy {
        final FarKey key;
        final IDhApiRenderableBoxGroup body;
        /** By pose ordinal; null for a pose the dragon's flipbook does not have. */
        final IDhApiRenderableBoxGroup[] wings = new IDhApiRenderableBoxGroup[POSES];
        final RecutPacer pacer = new RecutPacer();
        int heading, stage, rgb, poses;
        double step;
        int bodyBoxes, wingBoxes;
        Shown shown;
        long wantedAt;
        long touchedFrame = -1;
        boolean bodyOn;
        int wingOn = -1;

        Proxy(FarKey key, IDhApiRenderableBoxGroup body) {
            this.key = key;
            this.body = body;
        }

        int groups() {
            int n = 1;
            for (IDhApiRenderableBoxGroup w : wings)
                if (w != null)
                    n++;
            return n;
        }

        /** Shows the body and the wings of {@code pose} (none when null) at {@link #ORIGIN}. */
        void show(DragonModel.Pose pose) {
            body.setOriginBlockPos(ORIGIN);
            if (!bodyOn) {
                body.setActive(true);
                bodyOn = true;
            }
            int want = pose == null ? -1 : pose.ordinal();
            if (want >= 0 && wings[want] != null)
                wings[want].setOriginBlockPos(ORIGIN);
            if (want != wingOn) {
                if (wingOn >= 0 && wings[wingOn] != null)
                    wings[wingOn].setActive(false);
                if (want >= 0 && wings[want] != null)
                    wings[want].setActive(true);
                wingOn = want;
                flips++;
            }
        }

        void hide() {
            if (bodyOn) {
                body.setActive(false);
                bodyOn = false;
            }
            if (wingOn >= 0 && wings[wingOn] != null)
                wings[wingOn].setActive(false);
            wingOn = -1;
        }
    }

    // -----------------------------------------------------------------------------------------------------
    // The controller's dragon pass

    /**
     * Places this frame's dragons; returns what they took of the tier's budget. {@code reg} is the register of
     * the level drawn, which {@link FarTrainView} keeps; a change of it releases everything first.
     */
    static Used update(IDhApiCustomRenderRegister reg, IDhApiCustomRenderObjectFactory factory, ClientLevel level,
            FarDetail tier, String dimension, Vec3 cam, double radiusSq, double clock, long now, long frame) {
        CANDIDATES.clear();
        int hidden = 0;
        for (FarMover m : FarObjects.movers()) {
            if (m.kind() != FarKind.DRAGON || !dimension.equals(m.dimension()))
                continue;
            m.segmentAt(clock, SEG);
            double x = (SEG[0] + SEG[3]) / 2.0, y = (SEG[1] + SEG[4]) / 2.0, z = (SEG[2] + SEG[5]) / 2.0;
            double dx = x - cam.x, dz = z - cam.z;
            double d2 = dx * dx + dz * dz;
            if (!(d2 <= radiusSq))
                continue;
            Proxy p = PROXIES.get(m.key());
            if (p != null)
                p.wantedAt = now;
            if (!FarHide.dhDraws(m, level, x, y, z, reach(m.size()), cam)) {
                hidden++;
                continue;
            }
            CANDIDATES.add(new Candidate(m, x, y, z, SEG[0] - SEG[3], SEG[2] - SEG[5], d2));
        }
        CANDIDATES.sort((a, b) -> Double.compare(a.distanceSq, b.distanceSq));

        int drawn = 0, groups = 0, boxes = 0;
        int keep = tier.dragons * KEEP_PER_DRAWN;
        for (Candidate c : CANDIDATES) {
            if (drawn >= tier.dragons || groups + 2 > tier.activeGroups)
                break;
            FarMover m = c.mover;
            int stage = Math.max(1, Math.min(5, m.size()));
            int poses = tier.wingPoses;
            Proxy p = PROXIES.get(m.key());
            int heading = DragonModel.headingStep(c.hx, c.hz, p == null ? -1 : p.heading);
            ORIGIN.x = c.x;
            ORIGIN.y = c.y;
            ORIGIN.z = c.z;
            if (p != null && p.poses != poses) {
                // Another flipbook: start this dragon over
                release(reg, p);
                PROXIES.remove(p.key);
                p = null;
            }
            if (p == null) {
                if (PROXIES.size() >= keep && !releaseOneUnseen(reg, frame))
                    continue;
                p = create(reg, factory, m, stage, heading, poses, tier.step, boxes, tier.boxes, now);
                if (p == null)
                    continue;
            } else {
                if (boxes + p.bodyBoxes + p.wingBoxes > tier.boxes)
                    continue;
                if (p.heading != heading || p.stage != stage || p.rgb != m.rgb() || p.step != tier.step) {
                    if (p.pacer.mayCut(now))
                        recut(p, m, stage, heading, tier.step, now);
                } else if (p.pacer.confirmDue(now)) {
                    p.body.triggerBoxChange();
                    for (IDhApiRenderableBoxGroup w : p.wings)
                        if (w != null)
                            w.triggerBoxChange();
                    confirms++;
                }
            }
            DragonModel.Pose pose = DragonModel.poseAt(clock, p.poses, flapSeed(m.key()),
                    m.state());
            p.show(pose);
            p.touchedFrame = frame;
            p.wantedAt = now;
            drawn++;
            groups += pose == null ? 1 : 2;
            boxes += p.bodyBoxes + (pose == null ? 0 : p.wingBoxes);
        }

        int registered = 0;
        Iterator<Proxy> it = PROXIES.values().iterator();
        while (it.hasNext()) {
            Proxy p = it.next();
            if (p.touchedFrame != frame) {
                p.hide();
                if (now - p.wantedAt > FORGET_NANOS) {
                    release(reg, p);
                    it.remove();
                    continue;
                }
            }
            registered += p.groups();
        }
        lastDrawn = drawn;
        lastGroups = groups;
        lastBoxes = boxes;
        lastRegistered = registered;
        lastHidden = hidden;
        lastCandidates = CANDIDATES.size();
        CANDIDATES.clear();
        return new Used(groups, boxes, registered);
    }

    /** The farthest a dragon of size {@code size} reaches from its midpoint: its wingspan. */
    static double reach(int size) {
        return DragonModel.length(Math.max(1, Math.min(5, size))) * DragonModel.WINGSPAN;
    }

    /**
     * What DH drew this dragon with at the controller's last run, or null when DH did not draw it (render thread
     * only).
     */
    static Shown shown(FarKey key) {
        Proxy p = PROXIES.get(key);
        return p != null && p.bodyOn ? p.shown : null;
    }

    /** Dragons with groups registered. */
    static int held() {
        return PROXIES.size();
    }

    /** Every dragon's groups go (the level changed, or the view stopped). */
    static void releaseAll(IDhApiCustomRenderRegister reg) {
        for (Proxy p : PROXIES.values())
            release(reg, p);
        PROXIES.clear();
        lastDrawn = lastGroups = lastBoxes = lastRegistered = lastHidden = lastCandidates = 0;
    }

    // -----------------------------------------------------------------------------------------------------
    // Groups

    private static Proxy create(IDhApiCustomRenderRegister reg, IDhApiCustomRenderObjectFactory factory, FarMover m,
            int stage, int heading, int poses, double step, int boxesSoFar, int boxCap, long now) {
        List<double[]> body = DragonModel.body(stage, heading, step);
        DragonModel.Pose[] groups = DragonModel.groups(poses);
        List<List<double[]>> wingCuts = new ArrayList<>(groups.length);
        int wingBoxes = 0;
        for (DragonModel.Pose pose : groups) {
            List<double[]> w = DragonModel.wings(stage, heading, pose, step);
            wingCuts.add(w);
            wingBoxes = Math.max(wingBoxes, w.size());
        }
        if (body.isEmpty() || boxesSoFar + body.size() + wingBoxes > boxCap)
            return null;
        IDhApiRenderableBoxGroup bodyGroup = add(reg, factory, BODY_NAME, boxes(body, m.rgb()));
        if (bodyGroup == null)
            return null;
        Proxy p = new Proxy(m.key(), bodyGroup);
        p.bodyOn = bodyGroup.isActive();
        int wingRgb = shade(m.rgb(), WING_SHADE);
        for (int i = 0; i < groups.length; i++) {
            IDhApiRenderableBoxGroup g = add(reg, factory, WINGS_NAME, boxes(wingCuts.get(i), wingRgb));
            if (g == null) {
                release(reg, p);
                return null;
            }
            // Only the pose of the moment is drawn
            g.setActive(false);
            p.wings[groups[i].ordinal()] = g;
        }
        p.heading = heading;
        p.stage = stage;
        p.rgb = m.rgb();
        p.poses = poses;
        p.step = step;
        p.shown = new Shown(stage, heading, step);
        p.bodyBoxes = body.size();
        p.wingBoxes = wingBoxes;
        p.pacer.cut(now);
        PROXIES.put(m.key(), p);
        creations++;
        return p;
    }

    private static IDhApiRenderableBoxGroup add(IDhApiCustomRenderRegister reg, IDhApiCustomRenderObjectFactory factory,
            String name, List<DhApiRenderableBox> boxes) {
        IDhApiRenderableBoxGroup g = factory.createRelativePositionedGroup(name,
                new DhApiVec3d(ORIGIN.x, ORIGIN.y, ORIGIN.z), boxes);
        try {
            reg.add(g);
        } catch (IllegalArgumentException e) {
            close(g);
            return null;
        }
        return g;
    }

    private static void recut(Proxy p, FarMover m, int stage, int heading, double step, long now) {
        List<double[]> body = DragonModel.body(stage, heading, step);
        if (body.isEmpty())
            return;
        replace(p.body, boxes(body, m.rgb()));
        int wingRgb = shade(m.rgb(), WING_SHADE), wingBoxes = 0;
        for (DragonModel.Pose pose : DragonModel.Pose.values()) {
            IDhApiRenderableBoxGroup g = p.wings[pose.ordinal()];
            if (g == null)
                continue;
            List<double[]> w = DragonModel.wings(stage, heading, pose, step);
            replace(g, boxes(w, wingRgb));
            wingBoxes = Math.max(wingBoxes, w.size());
        }
        p.heading = heading;
        p.stage = stage;
        p.rgb = m.rgb();
        p.step = step;
        p.shown = new Shown(stage, heading, step);
        p.bodyBoxes = body.size();
        p.wingBoxes = wingBoxes;
        p.pacer.cut(now);
        recuts++;
    }

    private static void replace(IDhApiRenderableBoxGroup g, List<DhApiRenderableBox> boxes) {
        g.clear();
        g.addAll(boxes);
        g.triggerBoxChange();
    }

    /** Releases the dragon that has gone unwanted the longest, if one is not drawn this frame. */
    private static boolean releaseOneUnseen(IDhApiCustomRenderRegister reg, long frame) {
        Proxy oldest = null;
        for (Proxy p : PROXIES.values())
            if (p.touchedFrame != frame && (oldest == null || p.wantedAt < oldest.wantedAt))
                oldest = p;
        if (oldest == null)
            return false;
        release(reg, oldest);
        PROXIES.remove(oldest.key);
        return true;
    }

    private static List<DhApiRenderableBox> boxes(List<double[]> cut, int rgb) {
        Color colour = COLOURS.computeIfAbsent(rgb, Color::new);
        List<DhApiRenderableBox> out = new ArrayList<>(cut.size());
        for (double[] b : cut)
            out.add(new DhApiRenderableBox(new DhApiVec3d(b[0], b[1], b[2]), new DhApiVec3d(b[3], b[4], b[5]),
                    colour, MATERIAL));
        return out;
    }

    /** Where in its wingbeat a dragon starts: from its id, so each keeps its own beat. */
    static long flapSeed(FarKey key) {
        long h = key.object().getMostSignificantBits() ^ key.object().getLeastSignificantBits();
        h ^= h >>> 29;
        h *= 0xBF58476D1CE4E5B9L;
        return h ^ (h >>> 32);
    }

    /** {@code rgb} with each channel times {@code f}. */
    static int shade(int rgb, double f) {
        int r = (int) Math.round(((rgb >> 16) & 0xFF) * f);
        int g = (int) Math.round(((rgb >> 8) & 0xFF) * f);
        int b = (int) Math.round((rgb & 0xFF) * f);
        return (Math.min(255, r) << 16) | (Math.min(255, g) << 8) | Math.min(255, b);
    }

    private static void release(IDhApiCustomRenderRegister reg, Proxy p) {
        drop(reg, p.body);
        for (IDhApiRenderableBoxGroup w : p.wings)
            if (w != null)
                drop(reg, w);
        releases++;
    }

    private static void drop(IDhApiCustomRenderRegister reg, IDhApiRenderableBoxGroup g) {
        if (reg != null) {
            try {
                reg.remove(g.getId());
            } catch (RuntimeException e) {
                // The level's renderer is closing; DH drops its map with it
            }
        }
        close(g);
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
