package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.client;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import com.mojang.blaze3d.vertex.PoseStack;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.List;
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
 * before the far model was flying. So for {@link #HOLD_TICKS} of its own from its first render, a body a far
 * mover names is held:
 *
 * <ul>
 *   <li>vanilla's render of it is cancelled, through GeckoLib's cancelable {@code GeoRenderEvent.Entity.Pre},
 *       posted on NeoForge's bus (no mixin; Ice and Fire's renderer is GeckoLib's, not a LivingEntityRenderer, so
 *       NeoForge's own RenderLivingEvent never fires for it);</li>
 *   <li>its renderer runs once a frame into nothing ({@link #warm}), so its animation goes through the transition
 *       unseen, on its own clock;</li>
 *   <li>the far renderer draws the far model as if the body were not there yet ({@link DragonLook#holds}).</li>
 * </ul>
 *
 * <p>Then vanilla draws it, already flying. The hold is decided in the looks' frame ({@link #frame}) and forgotten
 * at the frame's end ({@link #endFrame}), so with no far view running nothing is held. A body no far mover names,
 * and one a copy may not be drawn of ({@link DragonLook#copies}: its renderer does more than draw), are never
 * held. Nothing is saved and nothing reaches the server: uninstalled, a body is drawn from its first frame, as
 * before. Render thread.
 */
final class DragonArrivals {
    /** How long a body is held, in its own ticks from its first warm render: the transition's 2 ticks at 0.4. */
    static final double HOLD_TICKS = 5.0;
    /** And at most this long in wall time, for a game that pauses or freezes its ticks. */
    static final long HOLD_NANOS = 1_000_000_000L;
    private static final class Arrival {
        final DragonBaseEntity body;
        final long joined;
        /** The body's tick and partial tick at its first warm render, NaN before it. */
        double first = Double.NaN;

        Arrival(DragonBaseEntity body, long joined) {
            this.body = body;
            this.joined = joined;
        }
    }

    private final List<Arrival> arrivals = new ArrayList<>();
    private final Set<Entity> held = Collections.newSetFromMap(new IdentityHashMap<>());
    /** A warm render is running: its own GeckoLib event goes through. */
    private boolean warming;
    private int errors;
    /** Bodies held, and frames they were held for, since the last stats. */
    private long bodies, frames;

    /** A dragon joined a client level (EntityJoinLevelEvent). */
    void joined(Entity e) {
        if (!(e instanceof DragonBaseEntity d) || !e.level().isClientSide() || d.getId() < 0)
            return;
        long now = System.nanoTime();
        // Without a far view nothing calls frame: the list keeps only the last second's arrivals all the same
        arrivals.removeIf(a -> a.body.isRemoved() || now - a.joined > HOLD_NANOS);
        arrivals.add(new Arrival(d, now));
    }

    /**
     * Once a frame, in the looks' frame, before any render pass: which new bodies are held this frame, each one
     * warmed. {@code mayCopy}: whether a body's renderer may run more than once a frame.
     */
    void frame(ClientLevel level, Predicate<Entity> mayCopy) {
        held.clear();
        if (arrivals.isEmpty())
            return;
        long now = System.nanoTime();
        for (Iterator<Arrival> it = arrivals.iterator(); it.hasNext(); ) {
            Arrival a = it.next();
            DragonBaseEntity d = a.body;
            if (d.isRemoved() || d.level() != level || now - a.joined > HOLD_NANOS) {
                it.remove();
                continue;
            }
            float partial = partial(d);
            double t = d.tickCount + (double) partial;
            if (!Double.isNaN(a.first) && t - a.first >= HOLD_TICKS) {
                it.remove();
                continue;
            }
            if (!named(d.getId()) || !mayCopy.test(d))
                continue;
            if (!warm(d, partial)) {
                it.remove(); // vanilla draws it from now on, as without the hold
                continue;
            }
            if (Double.isNaN(a.first)) {
                a.first = t;
                bodies++;
            }
            held.add(d);
            frames++;
        }
    }

    /** Whether a far mover of the feed names entity {@code id}: else there is no far model to show instead. */
    private static boolean named(int id) {
        for (FarMover m : FarObjects.movers())
            if (m.entity() == id && m.kind() == FarKind.DRAGON)
                return true;
        return false;
    }

    private static float partial(Entity e) {
        return Minecraft.getInstance().getTimer()
                .getGameTimeDeltaPartialTick(!e.level().tickRateManager().isEntityFrozen(e));
    }

    /** One render of {@code d} into nothing, as vanilla would draw it this frame; false when it threw. */
    private boolean warm(DragonBaseEntity d, float partial) {
        warming = true;
        try {
            EntityRenderer<DragonBaseEntity> r = DragonLook.renderer(d);
            r.render(d, Mth.lerp(partial, d.yRotO, d.getYRot()), partial, new PoseStack(), DragonLook.NULL_SOURCE,
                    DragonLook.SKY_LIGHT);
            return true;
        } catch (RuntimeException | LinkageError e) {
            if (errors++ < 5)
                MicRtsimIceAndFire.LOGGER.warn("[mic_rtsim_iceandfire] a new dragon's body would not warm up; drawn as it is", e);
            return false;
        } finally {
            warming = false;
        }
    }

    /** Whether the far renderer draws the far model for {@code body} this frame, vanilla's render held back. */
    boolean holds(Entity body) {
        return held.contains(body);
    }

    /** Whether vanilla's render of {@code body} is cancelled now (GeoRenderEvent.Entity.Pre). */
    boolean cancels(Entity body) {
        return !warming && held.contains(body);
    }

    /** The frame is over (RenderFrameEvent.Post): nothing is held until the next frame's {@link #frame}. */
    void endFrame() {
        held.clear();
    }

    /** The client left its level for {@code keep}: the bodies of every other level go. */
    void clear(ClientLevel keep) {
        held.clear();
        arrivals.removeIf(a -> a.body.level() != keep);
    }

    /** What it did since the last call, for the bench's log. */
    String stats() {
        String s = "arrivals: " + bodies + " bodies held for " + frames + " frames, " + arrivals.size() + " waiting";
        bodies = frames = 0;
        return s;
    }
}
