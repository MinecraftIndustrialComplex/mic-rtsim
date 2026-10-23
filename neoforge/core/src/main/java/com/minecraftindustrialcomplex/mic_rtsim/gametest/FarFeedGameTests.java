package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarFeed;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarItem;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarLegs;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarPayloads;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarSource;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarView;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarWire;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;
import java.util.function.Consumer;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.Pig;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

/**
 * The far feed end to end on the server (docs/choices-dh-feed.md; research/distant-horizons-2026-10-21.md §5,
 * §6.3): a subscriber, the engine's FAR_LEG records as the session keeps them, a body source, the filter and
 * the bytes that go out. The hand-over is the point: a dragon flies a leg, is embodied (the body's samples and
 * its entity id replace the leg on the same handle, with no new INTRO), is held through the gap between its body's
 * capture and the engine's next leg, goes back to a leg (entity id -1, still the same handle), then is GONE once
 * the view's grace has run out. Every sweep is called by the test in one tick, so the server's own
 * once-a-second sweep cannot run between them; its own batch, since the sink and the player lookup are
 * server-wide.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class FarFeedGameTests {
    private FarFeedGameTests() {}

    private static final String BODY = "mic_rtsim:gametest_far_dragon";
    private static final int STYLE = 2, VARIANT = 1, STAGE = 3, RGB = 0x445566;

    /** The test's body source: what an embodied dragon's plugin would give, while {@link #item} is set. */
    private static final class BodySource implements FarSource {
        volatile FarItem item;

        @Override
        public String name() {
            return "gametest far body";
        }

        @Override
        public void collect(ServerLevel level, long gameTime, Consumer<FarItem> out) {
            FarItem i = item;
            if (i != null)
                out.accept(i);
        }
    }

    private static BodySource source;

    private static BodySource source() {
        if (source == null) {
            source = new BodySource();
            FarFeed.addSource(source);
        }
        return source;
    }

    /** A FAR_LEG as the engine sends it (docs/boundary.md §12.1), into the session's far legs. */
    private static void leg(RtsimSession session, String dim, long agent, FarState state, int y, int x0, int z0,
            int x1, int z1, long t0, long t1, int radius) {
        ByteBuffer b = ByteBuffer.allocate(FarLegs.SIZE).order(ByteOrder.LITTLE_ENDIAN);
        b.putShort(0, (short) FarLegs.FAR_LEG);
        b.putShort(2, (short) FarLegs.SIZE);
        b.putInt(4, state.wire);
        b.putLong(FarLegs.AGENT, agent);
        b.putInt(FarLegs.BODY, 7);
        b.putInt(FarLegs.DIM, 3);
        b.put(FarLegs.VARIANT, (byte) VARIANT);
        b.put(FarLegs.STAGE, (byte) STAGE);
        b.putInt(FarLegs.Y, y);
        b.putInt(FarLegs.X0, x0);
        b.putInt(FarLegs.Z0, z0);
        b.putInt(FarLegs.X1, x1);
        b.putInt(FarLegs.Z1, z1);
        b.putLong(FarLegs.T0, t0);
        b.putLong(FarLegs.T1, t1);
        b.putInt(FarLegs.RADIUS, radius);
        SessionPlugin.EngineNames names = (kind, id) -> kind == Obs.DICT_DIMENSION && id == 3 ? dim
                : kind == Obs.DICT_BODY && id == 7 ? BODY : null;
        session.farLegs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, state.wire, names);
    }

    /** What one sweep sent the watcher, decoded; null when nothing was sent. */
    private static FarWire.Message sweep(GameTestHelper helper, List<byte[]> sent) {
        sent.clear();
        FarFeed.sweep(helper.getLevel().getServer());
        check("one sweep sends the watcher at most one message, got " + sent.size(), sent.size() <= 1);
        return sent.isEmpty() ? null : FarWire.decode(sent.get(0));
    }

    /** The records of {@code handle} in {@code m}. */
    private static List<FarWire.Rec> of(FarWire.Message m, int handle) {
        List<FarWire.Rec> out = new ArrayList<>();
        if (m != null)
            for (FarWire.Rec r : m.recs())
                if (r.handle() == handle)
                    out.add(r);
        return out;
    }

    private static boolean has(List<FarWire.Rec> recs, Class<? extends FarWire.Rec> type) {
        return recs.stream().anyMatch(type::isInstance);
    }

    /**
     * A dragon's far view from leg to body and back (§5): the same handle throughout, the body's entity id while
     * it has a body, the leg again when it goes, and only what changed each second. The body source beats the
     * engine's BODY record and a stale FLY leg alike; a carriage outside the subscriber's kinds and a dragon
     * outside its radius are never sent.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100, batch = "mic_rtsim_far_feed")
    public static void aDragonIsHandedOverBetweenItsLegAndItsBody(GameTestHelper helper) {
        RtsimSession session = RtsimTests.session();
        check("the far feed's dragons need the ecology module on", RtsimConfig.on(Module.ECOLOGY));
        ServerLevel level = helper.getLevel();
        String dim = level.dimension().location().toString();
        Vec3 at = helper.absoluteVec(new Vec3(1.5, 2, 1.5));
        ServerPlayer watcher = FakePlayers.add(level, at, "far_watcher");
        Pig body = helper.spawn(EntityType.PIG, 3, 2, 3);
        body.setNoAi(true);
        UUID w = watcher.getUUID();
        List<byte[]> sent = new ArrayList<>();
        long agent = 0x44L << 56 | 0xFE_0000_0000L | ThreadLocalRandom.current().nextInt(1 << 24);
        long far = agent + (1L << 24), carriage = agent + (2L << 24);
        BodySource src = source();
        FarLegs.registerStyle(BODY, new FarLegs.Style(STYLE, new int[]{0x112233, RGB}));
        FarFeed.setSink((p, m) -> {
            if (p.getUUID().equals(w))
                sent.add(m);
        });
        FarFeed.setPlayers(u -> u.equals(w) ? watcher : level.getServer().getPlayerList().getPlayer(u));
        int px = (int) Math.floor(at.x), py = (int) Math.floor(at.y), pz = (int) Math.floor(at.z);
        long now = level.getGameTime();
        try {
            FarFeed.subscribe(watcher, new FarPayloads.Subscribe(FarPayloads.SUB_VERSION, 512, 2, 64,
                    FarKind.DRAGON.bit()));

            // 1. A far dragon flying a leg 300 blocks out, one past the radius, and a carriage the watcher does not draw
            leg(session, dim, agent, FarState.FLY, py + 40, px + 300, pz, px + 100, pz, now, now + 400, 0);
            leg(session, dim, far, FarState.FLY, py + 40, px + 900, pz, px + 700, pz, now, now + 400, 0);
            src.item = null;
            FarItem cart = new FarItem(carriage, FarKind.CARRIAGE, 0, 0, 0, 0x404040, FarState.MOVING,
                    new double[]{px + 20, py, pz, px + 14, py, pz}, null, -1, 0);
            BodySource carts = new BodySource();
            carts.item = cart;
            FarFeed.addSource(carts);
            FarWire.Message m1;
            try {
                m1 = sweep(helper, sent);
            } finally {
                FarFeed.removeSource(carts);
            }
            check("the first message starts the view over", m1 != null && (m1.flags() & FarWire.RESET) != 0);
            int handle = -1;
            for (FarWire.Rec r : m1.recs()) {
                if (r instanceof FarWire.Intro i && i.id() == agent)
                    handle = i.handle();
                check("an object outside the radius is not sent", !(r instanceof FarWire.Intro i && i.id() == far));
                check("a kind the watcher does not draw is not sent",
                        !(r instanceof FarWire.Intro i && i.id() == carriage));
            }
            check("the dragon is introduced", handle >= 0);
            List<FarWire.Rec> r1 = of(m1, handle);
            FarWire.Intro intro = (FarWire.Intro) r1.get(0);
            check("its intro carries its looks: " + intro, intro.kind() == FarKind.DRAGON.wire && intro.style() == STYLE
                    && intro.variant() == VARIANT && intro.size() == STAGE && intro.rgb() == RGB);
            check("it flies a leg", has(r1, FarWire.Leg.class) && !has(r1, FarWire.Sample.class)
                    && !has(r1, FarWire.Real.class));
            record("far feed hand-over", "first message", m1.recs().size() + " records, " + sent.get(0).length + " bytes");

            // 2. Embodied: the engine says BODY, a stale FLY would lose too, and the body's samples win on the same handle
            leg(session, dim, agent, FarState.BODY, py + 2, px + 2, pz + 2, px + 2, pz + 2, now, now, 0);
            Vec3 b0 = body.position();
            src.item = new FarItem(agent, FarKind.DRAGON, STYLE, VARIANT, STAGE, RGB, FarState.FLY,
                    new double[]{b0.x + 0.5, b0.y + 1, b0.z, b0.x - 0.5, b0.y + 1, b0.z}, null, body.getId(), 10);
            FarWire.Message m2 = sweep(helper, sent);
            List<FarWire.Rec> r2 = of(m2, handle);
            check("embodied, it is sampled on the same handle with its entity id, and no new intro: " + r2,
                    has(r2, FarWire.Sample.class) && !has(r2, FarWire.Intro.class) && !has(r2, FarWire.Leg.class)
                            && !has(r2, FarWire.Gone.class)
                            && r2.stream().anyMatch(r -> r instanceof FarWire.Real e && e.entity() == body.getId()));
            check("the second message does not start over", (m2.flags() & FarWire.RESET) == 0);

            // 3. The body moves: only its sample goes out
            src.item = new FarItem(agent, FarKind.DRAGON, STYLE, VARIANT, STAGE, RGB, FarState.FLY,
                    new double[]{b0.x + 2.5, b0.y + 1, b0.z, b0.x + 1.5, b0.y + 1, b0.z}, null, body.getId(), 10);
            List<FarWire.Rec> r3 = of(sweep(helper, sent), handle);
            check("a moving body sends only its sample: " + r3, r3.size() == 1 && r3.get(0) instanceof FarWire.Sample);

            // 4. The body is still: its sample once more, unchanged, so a client stops it there (DHT44); then nothing
            List<FarWire.Rec> stop = of(sweep(helper, sent), handle);
            check("a body that stopped sends its sample once more: " + stop,
                    stop.size() == 1 && stop.get(0) instanceof FarWire.Sample);
            List<FarWire.Rec> r4 = of(sweep(helper, sent), handle);
            check("a still body sends nothing: " + r4, r4.isEmpty());

            // 5. The body is captured before the engine's next leg lands: in neither source, it is held, not GONE
            src.item = null;
            List<FarWire.Rec> gap = of(sweep(helper, sent), handle);
            check("a dragon between its body and its next leg is held: " + gap, gap.isEmpty());

            // 6. The engine's leg lands: its leg again, entity id -1, the same handle
            leg(session, dim, agent, FarState.FLY, py + 40, px + 4, pz + 2, px + 200, pz + 2, now, now + 400, 0);
            List<FarWire.Rec> r5 = of(sweep(helper, sent), handle);
            check("without its body it flies the engine's leg on the same handle, with no entity: " + r5,
                    has(r5, FarWire.Leg.class) && !has(r5, FarWire.Intro.class) && !has(r5, FarWire.Gone.class)
                            && r5.stream().anyMatch(r -> r instanceof FarWire.Real e && e.entity() == -1));

            // 7. The client subscribes again (its wants changed, or a message did not decode): everything again,
            // from a RESET
            FarFeed.subscribe(watcher, new FarPayloads.Subscribe(FarPayloads.SUB_VERSION, 512, 2, 64,
                    FarKind.DRAGON.bit()));
            FarWire.Message again = sweep(helper, sent);
            check("a new subscription starts the view over", again != null && (again.flags() & FarWire.RESET) != 0);
            handle = -1;
            for (FarWire.Rec r : again.recs())
                if (r instanceof FarWire.Intro i && i.id() == agent)
                    handle = i.handle();
            check("and introduces the dragon again", handle >= 0 && has(of(again, handle), FarWire.Leg.class));

            // 8. The engine forgets it: held for the grace, then GONE
            leg(session, dim, agent, FarState.GONE, 0, 0, 0, 0, 0, 0, 0, 0);
            for (int i = 0; i < FarView.GRACE_SWEEPS; i++) {
                List<FarWire.Rec> held = of(sweep(helper, sent), handle);
                check("held for " + FarView.GRACE_SWEEPS + " sweeps: " + held, held.isEmpty());
            }
            List<FarWire.Rec> r8 = of(sweep(helper, sent), handle);
            check("a dragon the engine forgot is gone: " + r8, r8.size() == 1 && r8.get(0) instanceof FarWire.Gone);
            record("far feed hand-over", "handle", handle);

            // 9. Withdrawn, the watcher is sent nothing
            FarFeed.subscribe(watcher, new FarPayloads.Subscribe(FarPayloads.SUB_VERSION, 0, 0, 0, 0));
            check("a withdrawn subscriber is sent nothing", sweep(helper, sent) == null);
            helper.succeed();
        } finally {
            src.item = null;
            leg(session, dim, agent, FarState.GONE, 0, 0, 0, 0, 0, 0, 0, 0);
            leg(session, dim, far, FarState.GONE, 0, 0, 0, 0, 0, 0, 0, 0);
            FarFeed.unsubscribe(w);
            FarFeed.setSink(null);
            FarFeed.setPlayers(null);
            FakePlayers.remove(watcher);
            body.discard();
        }
    }
}
