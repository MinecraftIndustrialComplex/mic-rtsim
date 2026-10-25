package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.client;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import com.mojang.blaze3d.vertex.PoseStack;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Set;
import java.util.function.Predicate;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;

/**
 * A dragon's body as it arrives on the client (docs/choices-dh-feed.md DHT68). Ice and Fire's controllers
 * (GeckoLib's) start a body's animation with a transition from the model's rest pose, 2 ticks at their speed of
 * 0.4, so for the first 5 ticks a new body is drawn it unfolds its wings and neck from that pose, where a moment
 * before the far model was flying; and its flight loop would then start at its own first render, anywhere in the
 * far model's beat. So for {@link #HOLD_TICKS} of its own from its first render, a body a far mover names is held:
 *
 * <ul>
 *   <li>vanilla's render of its model is cancelled, through GeckoLib's cancelable {@code GeoRenderEvent.Entity.Pre},
 *       posted on NeoForge's bus (no mixin; Ice and Fire's renderer is GeckoLib's, not a LivingEntityRenderer, so
 *       NeoForge's own RenderLivingEvent never fires for it). Its name tag goes too, through NeoForge's
 *       RenderNameTagEvent ({@link #holds}); its leash and shadow have no event and are still drawn;</li>
 *   <li>its renderer runs once a frame into nothing ({@link #warm}), so its animation goes through the transition
 *       unseen. A flying body's first warm renders are the ones a live stand-in starts with
 *       ({@code DragonLook.startOnBakeClock}), at its own ticks as far back as its far model's loop started, so its
 *       flight loop starts in phase with the far model's beat, and a render at each loop's end keeps it there;</li>
 *   <li>the far renderer draws the far model as if the body were not there yet ({@link DragonLook#holds}), turning
 *       it onto the body's heading and pitch before the hold ends ({@link #turn}).</li>
 * </ul>
 *
 * <p>Then vanilla draws it, already flying. The hold is decided in the looks' frame ({@link #frame}) and forgotten
 * at the frame's end ({@link #endFrame}), so with no far view running nothing is held; and a body vanilla has drawn
 * once is never held after. Never held: a body no far mover names, one a copy may not be drawn of
 * ({@link DragonLook#copies}: its renderer does more than draw), and one with riders (Ice and Fire draws them in
 * the dragon's render and hides their own). Nothing is saved and nothing reaches the server: uninstalled, a body
 * is drawn from its first frame, as before. Render thread.
 */
final class DragonArrivals {
    /** How long a body is held, in its own ticks from its first warm render: the transition's 2 ticks at 0.4. */
    static final double HOLD_TICKS = 5.0;
    /** And at most this long in wall time from its arrival, for a game that pauses or freezes its ticks. */
    static final long HOLD_NANOS = 1_000_000_000L;
    /**
     * The far model's turn onto the body ({@link #turn}) ends this far into the hold: a hold ends at the first frame
     * past its end, so the last frame held, up to a frame short of it, already has the body's angles.
     */
    static final double TURN_SHARE = 0.8;
    private static final long NONE = Long.MIN_VALUE, LOOP = (long) DragonLook.LOOP,
            STEP = (long) (DragonLook.LOOP / DragonLook.FRAMES);

    private static final class Arrival {
        final DragonBaseEntity body;
        final long joined;
        /** The body's tick and partial tick at its first warm render, NaN before it. */
        double first = Double.NaN;
        /** The body's tick its flight loop last started at, on its far model's beat; NONE when not put on it. */
        long loop = NONE;

        Arrival(DragonBaseEntity body, long joined) {
            this.body = body;
            this.joined = joined;
        }
    }

    /** New bodies not drawn by vanilla yet, each until its hold ends. */
    private final Map<Entity, Arrival> arrivals = new IdentityHashMap<>();
    private final Set<Entity> held = Collections.newSetFromMap(new IdentityHashMap<>());
    /**
     * Bodies whose hold ran out before vanilla drew them (its section not built yet, say): the far model still
     * stands in for them, and keeps the body's angles ({@link #turn}) until vanilla's first render of it.
     */
    private final Set<Entity> released = Collections.newSetFromMap(new IdentityHashMap<>());
    /** A warm render is running: its own GeckoLib event goes through. */
    private boolean warming;
    private int errors;
    /** Bodies held, those put on their far model's beat, and frames they were held for, since the last stats. */
    private long bodies, onBeat, frames;

    /** A dragon joined a client level (EntityJoinLevelEvent). */
    void joined(Entity e) {
        if (!(e instanceof DragonBaseEntity d) || !e.level().isClientSide() || d.getId() < 0)
            return;
        long now = System.nanoTime();
        // Without a far view nothing calls frame: the map keeps only the last second's arrivals all the same
        arrivals.values().removeIf(a -> a.body.isRemoved() || now - a.joined > HOLD_NANOS);
        released.removeIf(Entity::isRemoved);
        arrivals.put(d, new Arrival(d, now));
    }

    /**
     * Once a frame, in the looks' frame, before any render pass: which new bodies are held this frame, each one
     * warmed. {@code mayCopy}: whether a body's renderer may run more than once a frame.
     */
    void frame(ClientLevel level, Predicate<Entity> mayCopy) {
        held.clear();
        if (!released.isEmpty())
            released.removeIf(e -> e.isRemoved() || e.level() != level);
        if (arrivals.isEmpty())
            return;
        long now = System.nanoTime();
        for (Iterator<Arrival> it = arrivals.values().iterator(); it.hasNext(); ) {
            Arrival a = it.next();
            DragonBaseEntity d = a.body;
            if (d.isRemoved() || d.level() != level) {
                it.remove();
                continue;
            }
            float partial = partial(d);
            double t = d.tickCount + (double) partial;
            if (now - a.joined > HOLD_NANOS || !Double.isNaN(a.first) && t - a.first >= HOLD_TICKS) {
                it.remove();
                if (!Double.isNaN(a.first))
                    released.add(d);
                continue;
            }
            // Vanilla draws it from now on: so it is never held after it was seen
            FarMover m = named(d.getId());
            if (m == null || !mayCopy.test(d) || d.isVehicle() || !warm(a, m, partial)) {
                it.remove();
                continue;
            }
            if (Double.isNaN(a.first)) {
                a.first = t;
                bodies++;
                if (a.loop != NONE)
                    onBeat++;
            }
            held.add(d);
            frames++;
        }
    }

    /** The far mover of the feed that names entity {@code id}, null if none: then there is no far model to show. */
    private static FarMover named(int id) {
        for (FarMover m : FarObjects.movers())
            if (m.entity() == id && m.kind() == FarKind.DRAGON)
                return m;
        return null;
    }

    private static float partial(Entity e) {
        return Minecraft.getInstance().getTimer()
                .getGameTimeDeltaPartialTick(!e.level().tickRateManager().isEntityFrozen(e));
    }

    /**
     * One render of the body into nothing, as vanilla would draw it this frame, after any renders at its past
     * ticks that keep its flight loop on {@code m}'s far beat; false when one threw.
     *
     * <p>GeckoLib reads a body's clock from its {@code tickCount} ({@code GeoEntity.getTick}, which Ice and Fire
     * keeps) plus the render's partial tick, and a model's clock moves by the change in it from the last render of
     * any of its entities, so a render with the tick set back is, to this body's controller, a render at that tick
     * (the stand-ins rely on the same). Its far time is {@code far = FarClock.ticks() + beatOffset}, {@code far -
     * real} ticks ahead of the body's own; the far model's loop started at {@code DragonLook.loopStart(far)}, so the
     * body's starts at that less the same gap. Its first warm renders that ({@link DragonLook#WARM} renders a step
     * apart, ending there, as a live stand-in's do): the 2-tick transition at 0.4 ends at the second, where the
     * loop starts. GeckoLib starts the next loop at the first render past a loop's end, dropping what it ran over,
     * so a render at the end itself keeps the beat (the stand-ins do the same). Only a body that plays the flight
     * loop ({@link #inFlight}) for a far model that flies: one at rest plays no loop (Ice and Fire's ground pose has
     * no length), and the blast's charge or a breath would play out of sight. The flight animation has no
     * sound, particle or instruction keyframes, so the renders at past ticks start nothing. Not under {@code /tick
     * freeze}: the far clock runs on while the body's ticks do not.
     */
    private boolean warm(Arrival a, FarMover m, float partial) {
        DragonBaseEntity d = a.body;
        int real = d.tickCount;
        warming = true;
        try {
            EntityRenderer<DragonBaseEntity> r = DragonLook.renderer(d);
            if (Double.isNaN(a.first) && m.state() != FarState.ROOST && inFlight(d)
                    && !d.level().tickRateManager().isEntityFrozen(d)) {
                long far = FarClock.ticks() + DragonLook.beatOffset(m);
                long start = DragonLook.loopStart(far) - (far - real);
                for (int s = 0; s < DragonLook.WARM; s++) {
                    long at = start + (s + 1 - DragonLook.WARM) * STEP;
                    // A controller takes up its animation only at a time other than its -1 before any (GeckoLib's
                    // lastPollTime); any first render at or before start - STEP still ends the transition at start
                    renderAt(r, d, s == 0 && at == -1 ? at - 1 : at);
                }
                a.loop = start;
            }
            if (a.loop != NONE && real - a.loop >= LOOP) {
                a.loop += (real - a.loop) / LOOP * LOOP;
                renderAt(r, d, a.loop);
            }
            d.tickCount = real;
            r.render(d, Mth.lerp(partial, d.yRotO, d.getYRot()), partial, new PoseStack(), DragonLook.NULL_SOURCE,
                    DragonLook.SKY_LIGHT);
            return true;
        } catch (RuntimeException | LinkageError e) {
            if (errors++ < 5)
                MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] a new dragon's body would not warm up; drawn as it is", e);
            return false;
        } finally {
            d.tickCount = real;
            warming = false;
        }
    }

    /**
     * Whether Ice and Fire's controller plays the flight loop for {@code d}, as its {@code selectGeckoAnimation}
     * decides: the airborne pool (not dead, not swimming, flying or hovering or part way into either), and in it
     * neither the blast's charge nor a fire breath. Any other action, and none, plays the flight loop; a body new
     * to the client has no action at all ({@code getAnimation} null, not {@code NO_ANIMATION}).
     */
    private static boolean inFlight(DragonBaseEntity d) {
        return !d.isModelDead() && !(d.isSwimming || d.isInWater() && d.swimProgress > 0f)
                && (d.isFlying() || d.isHovering() || d.flyProgress > 0f || d.hoverProgress > 0f)
                && !d.isBreathingFire() && d.getAnimation() != DragonBaseEntity.ANIMATION_FIRECHARGE;
    }

    /** A render of {@code d} into nothing at its tick {@code tick}, partial 0. */
    private static void renderAt(EntityRenderer<DragonBaseEntity> r, DragonBaseEntity d, long tick) {
        d.tickCount = (int) tick;
        r.render(d, d.yRotO, 0f, new PoseStack(), DragonLook.NULL_SOURCE, DragonLook.SKY_LIGHT);
    }

    /** Whether the far renderer draws the far model for {@code body} this frame, vanilla's render held back. */
    boolean holds(Entity body) {
        return held.contains(body);
    }

    /**
     * The heading and pitch, in degrees, to draw the far model of entity {@code id} at this frame, into {@code out}:
     * false, and {@code out} untouched, when no body of that id is held or {@link #released}. While a body is held
     * its server turns it, and the far model keeps the feed's heading and no pitch; so through the hold the far
     * model turns from {@code farYaw} and level onto the body's heading and pitch as Ice and Fire's renderer reads
     * them (the body's {@code yBodyRot} and {@code dragonPitch}, interpolated), all the way by {@link #TURN_SHARE}
     * of the hold, by its ticks or its wall time, whichever is further, so the body takes over at the same angles;
     * a released body's far model has its angles outright (DHT74).
     */
    boolean turn(int id, float farYaw, float[] out) {
        if (held.isEmpty() && released.isEmpty())
            return false;
        for (Entity e : held) {
            if (e.getId() != id)
                continue;
            Arrival a = arrivals.get(e);
            if (a == null || Double.isNaN(a.first))
                return false;
            DragonBaseEntity d = a.body;
            float partial = partial(d);
            double w = Math.max((d.tickCount + (double) partial - a.first) / HOLD_TICKS,
                    (System.nanoTime() - a.joined) / (double) HOLD_NANOS) / TURN_SHARE;
            angles(d, partial, (float) Mth.clamp(w, 0.0, 1.0), farYaw, out);
            return true;
        }
        for (Entity e : released) {
            if (e.getId() != id)
                continue;
            DragonBaseEntity d = (DragonBaseEntity) e;
            angles(d, partial(d), 1f, farYaw, out);
            return true;
        }
        return false;
    }

    /** {@code f} of the way from {@code farYaw} and level to body {@code d}'s heading and pitch, into {@code out}. */
    private static void angles(DragonBaseEntity d, float partial, float f, float farYaw, float[] out) {
        float bodyYaw = Mth.rotLerp(partial, d.yBodyRotO, d.yBodyRot);
        float bodyPitch = Mth.lerp(partial, d.prevDragonPitch, d.getDragonPitch());
        out[0] = farYaw + f * Mth.wrapDegrees(bodyYaw - farYaw);
        out[1] = f * bodyPitch;
    }

    /**
     * GeoRenderEvent.Entity.Pre, for any GeckoLib entity: whether its render is cancelled now. A new body's render
     * that goes through (outside a hold, or a copy in DH's pass) means it was seen, so it is never held after.
     */
    boolean cancels(Entity body) {
        if (warming)
            return false;
        if (held.contains(body))
            return true;
        if (!arrivals.isEmpty())
            arrivals.remove(body);
        if (!released.isEmpty())
            released.remove(body);
        return false;
    }

    /** The frame is over (RenderFrameEvent.Post): nothing is held until the next frame's {@link #frame}. */
    void endFrame() {
        held.clear();
    }

    /** The client left its level for {@code keep} (null: left the world): the bodies of every other level go. */
    void clear(ClientLevel keep) {
        held.clear();
        arrivals.values().removeIf(a -> keep == null || a.body.level() != keep);
        released.removeIf(e -> keep == null || e.level() != keep);
    }

    /** What it did since the last call, for the bench's log. */
    String stats() {
        String s = "arrivals: " + bodies + " bodies held (" + onBeat + " on the far beat) for " + frames + " frames, "
                + arrivals.size() + " waiting, " + released.size() + " released and not drawn yet";
        bodies = onBeat = frames = 0;
        return s;
    }
}
