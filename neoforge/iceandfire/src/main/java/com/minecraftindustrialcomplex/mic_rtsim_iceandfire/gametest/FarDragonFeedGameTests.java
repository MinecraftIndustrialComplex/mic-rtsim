package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarFeed;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarLegs;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarPayloads;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarView;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarWire;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.IafDragons;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.atomic.AtomicBoolean;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.GameType;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

/**
 * An embodied dragon on the far feed (docs/choices-dh-feed.md; research/distant-horizons-2026-10-21.md §5):
 * what a subscribed client is sent about a real Ice and Fire puppet, through the server's own once-a-second
 * sweep. While it has a body it is sampled from the body, with the body's entity id (so the client hides the
 * proxy while it has the entity) and the looks the engine's legs give the same species and colour (so the
 * hand-over to a leg needs no new intro); once the body departs the handle carries no entity: the engine's leg
 * on the same handle, or GONE once the view's grace has run out (this scripted dragon has no engine row). Its own
 * file and batch: the feed's sink and player lookup are server-wide.
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class FarDragonFeedGameTests {
    private FarDragonFeedGameTests() {}

    private static FarWire.Intro intro(List<FarWire.Message> got, long agent) {
        for (FarWire.Message m : got)
            for (FarWire.Rec r : m.recs())
                if (r instanceof FarWire.Intro i && i.id() == agent)
                    return i;
        return null;
    }

    @GameTest(template = DragonKit.ARENA, timeoutTicks = 800, batch = "dragon_far_feed")
    public static void anEmbodiedDragonIsSampledWithItsEntityAndHandedBack(GameTestHelper helper) {
        String test = "anEmbodiedDragonIsSampledWithItsEntityAndHandedBack";
        long agent = DragonKit.newAgent();
        ServerLevel level = helper.getLevel();
        Runnable unforce = DragonKit.forceAround(helper, 3);
        Vec3 c = DragonKit.centre(helper);
        Vec3 at = c.add(0, 12, 0);
        // A spectator far off: no dragon targets it, and it is still 80 blocks inside the feed's radius
        ServerPlayer watcher = FakePlayers.add(level, c.add(0, 0, -80), "far_dragon_watcher");
        watcher.setGameMode(GameType.SPECTATOR);
        UUID w = watcher.getUUID();
        List<FarWire.Message> got = new CopyOnWriteArrayList<>();
        int[] handle = {-1}, entity = {-1}, before = {0};
        AtomicBoolean cleaned = new AtomicBoolean();
        Runnable cleanup = () -> {
            if (cleaned.getAndSet(true))
                return;
            FarFeed.unsubscribe(w);
            FarFeed.setSink(null);
            FarFeed.setPlayers(null);
            FakePlayers.remove(watcher);
            unforce.run();
        };
        // Passed or failed, the server-wide hooks, the watcher and the forced chunks go
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                cleanup.run();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                cleanup.run();
            }

            @Override
            public void testAddedForRerun(GameTestInfo old, GameTestInfo fresh, GameTestRunner runner) {}
        });
        int[] malformed = {0};
        FarFeed.setSink((p, m) -> {
            if (!p.getUUID().equals(w))
                return;
            try {
                got.add(FarWire.decode(m));
            } catch (IllegalArgumentException e) {
                malformed[0]++; // the sweep runs in the server's tick: never throw there
            }
        });
        FarFeed.setPlayers(u -> u.equals(w) ? watcher : level.getServer().getPlayerList().getPlayer(u));
        FarFeed.subscribe(watcher, new FarPayloads.Subscribe(FarPayloads.SUB_VERSION, 256, 2, 64, FarKind.DRAGON.bit()));
        DragonKit.materialize(DragonKit.young(agent, helper, at, 0));

        helper.startSequence()
                .thenWaitUntil(() -> DragonKit.body(agent))
                .thenWaitUntil(() -> {
                    check("every message decodes (" + malformed[0] + " did not)", malformed[0] == 0);
                    FarWire.Intro i = intro(got, agent);
                    check("the watcher is introduced to the dragon", i != null);
                    int id = DragonKit.body(agent).getId();
                    boolean real = false, sampled = false;
                    for (FarWire.Message m : got)
                        for (FarWire.Rec r : m.recs()) {
                            real |= r instanceof FarWire.Real e && e.handle() == i.handle() && e.entity() == id;
                            sampled |= r instanceof FarWire.Sample s && s.handle() == i.handle();
                        }
                    check("it is sampled with its entity id", real && sampled);
                })
                .thenExecute(() -> {
                    DragonBaseEntity d = DragonKit.body(agent);
                    FarWire.Intro i = intro(got, agent);
                    int species = IafDragons.SPECIES.indexOf(DragonKit.FIRE), variant = IafDragons.variant(d);
                    check("its looks: " + i, i.kind() == FarKind.DRAGON.wire && i.style() == species
                            && i.variant() == variant && i.size() == d.getDragonStage());
                    check("the colour the engine's legs give it too (" + Integer.toHexString(i.rgb()) + ")",
                            i.rgb() == FarLegs.style(DragonKit.FIRE).rgb(variant));
                    double[] seg = null;
                    for (FarWire.Message m : got)
                        for (FarWire.Rec r : m.recs())
                            if (r instanceof FarWire.Sample s && s.handle() == i.handle())
                                seg = s.segment();
                    double mx = (seg[0] + seg[3]) / 2.0, mz = (seg[2] + seg[5]) / 2.0;
                    // Up to a second of flight behind the body
                    check("sampled where the body is (" + mx + ", " + mz + " against " + d.position() + ")",
                            Math.hypot(mx - d.getX(), mz - d.getZ()) < 25.0);
                    handle[0] = i.handle();
                    entity[0] = d.getId();
                    before[0] = got.size();
                    record(test, "embodied", "handle " + i.handle() + ", entity " + d.getId() + ", stage "
                            + d.getDragonStage() + ", " + got.size() + " messages");
                    DragonKit.depart(agent, 1, 1, 40, c.add(-400, 40, 0));
                })
                .thenWaitUntil(() -> check("the body departs", DragonKit.bridge().body(agent) == null))
                .thenWaitUntil(() -> {
                    boolean gone = false, leg = false, cleared = false, intro = false;
                    long lastSample = -1, goneAt = -1;
                    for (FarWire.Message m : got)
                        for (FarWire.Rec r : m.recs()) {
                            if (r.handle() == handle[0] && r instanceof FarWire.Sample)
                                lastSample = m.gameTime();
                        }
                    for (FarWire.Message m : got.subList(before[0], got.size()))
                        for (FarWire.Rec r : m.recs()) {
                            if (r.handle() != handle[0])
                                continue;
                            if (r instanceof FarWire.Gone && !gone)
                                goneAt = m.gameTime();
                            gone |= r instanceof FarWire.Gone;
                            leg |= r instanceof FarWire.Leg;
                            cleared |= r instanceof FarWire.Real e && e.entity() == -1;
                            intro |= r instanceof FarWire.Intro;
                        }
                    check("handed back: GONE, or the engine's leg with no entity", gone || (leg && cleared));
                    check("no new intro on the handle while it is the same dragon", gone || !intro);
                    // The scripted dragon has no engine row, so no leg follows: GONE, after the view's grace
                    if (gone)
                        check("held for the grace before GONE (" + (goneAt - lastSample) + " ticks after its last sample)",
                                goneAt - lastSample >= (long) FarView.GRACE_SWEEPS * FarFeed.INTERVAL_TICKS);
                    record(test, "departed", gone ? "gone " + (goneAt - lastSample) + " ticks after its last sample"
                            : "the engine's leg on the same handle, entity -1");
                })
                .thenSucceed();
    }
}
