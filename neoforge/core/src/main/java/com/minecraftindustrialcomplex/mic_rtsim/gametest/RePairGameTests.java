package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/** The re-pair's per-player rule (MZB-813, MZB-1308), on a body of the core's own GameTest type. */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class RePairGameTests {
    private RePairGameTests() {}

    /**
     * A re-pair is skipped when any tracker's own pairing data hold something per player (the M11 review's P3, MZB-1308). Two hearers
     * track a {@link TestPairing} body, whose {@code sendPairingData} adds a payload for the players {@link TestPairing#to} names. The
     * control: a payload for nobody, the re-pair goes ({@code repairs} rises). Then a payload for every player but the level's first:
     * the re-pair is skipped ({@code repairsSkipped} rises, nothing sent). Fails on the code before MZB-1308: it read the pairing data
     * of the level's first player only, found nothing per player and sent that bundle to every tracker.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "repair_per_player")
    public static void aRePairIsSkippedWhenAnyTrackerHasPerPlayerData(GameTestHelper helper) {
        String test = "aRePairIsSkippedWhenAnyTrackerHasPerPlayerData";
        ServerLevel level = helper.getLevel();
        Vec3 at = helper.absoluteVec(new Vec3(1.5, 2, 1.5));
        TestPairing.Body[] body = new TestPairing.Body[1];
        FakePlayers.Hearer[] hearer = new FakePlayers.Hearer[2];
        long[] was = new long[2]; // repairs, repairsSkipped
        ConflictKit.onEnd(helper, () -> {
            TestPairing.to = p -> false;
            for (int i = 0; i < hearer.length; i++)
                if (hearer[i] != null && !hearer[i].isRemoved()) {
                    FakePlayers.removeSeen(hearer[i]);
                    hearer[i] = null;
                }
            if (body[0] != null && !body[0].isRemoved())
                body[0].discard();
        });
        helper.startSequence()
                .thenExecute(() -> {
                    check("the GameTest type is registered", TestPairing.BODY != null);
                    TestPairing.Body b = TestPairing.BODY.get().create(level);
                    check("the body is made", b != null);
                    b.moveTo(at.x, at.y, at.z, 0f, 0f);
                    b.setNoAi(true);
                    b.setPersistenceRequired();
                    check("it joins", level.addFreshEntity(b));
                    body[0] = b;
                    hearer[0] = FakePlayers.watching(level, at.add(6, 0, 0), "repair-per-player-a");
                    hearer[1] = FakePlayers.watching(level, at.add(-6, 0, 0), "repair-per-player-b");
                })
                .thenWaitUntil(() -> {
                    for (FakePlayers.Hearer h : hearer)
                        FakePlayers.pump(h);
                    List<ServerPlayer> seen = level.getChunkSource().chunkMap.getPlayersWatching(body[0]);
                    check("both hearers track the body (" + seen.size() + " players do)", seen.contains(hearer[0]) && seen.contains(hearer[1]));
                })
                .thenExecute(() -> {
                    was[0] = ConflictKit.bridge().repairs;
                    was[1] = ConflictKit.bridge().repairsSkipped;
                    boolean control = ConflictKit.bridge().rePair(body[0]);
                    check("the control's re-pair went (" + control + ", repairs +" + (ConflictKit.bridge().repairs - was[0]) + ", skipped +"
                            + (ConflictKit.bridge().repairsSkipped - was[1]) + ")", control && ConflictKit.bridge().repairs == was[0] + 1);
                    ServerPlayer first = level.players().get(0);
                    boolean sent;
                    TestPairing.to = p -> p != first;
                    try {
                        sent = ConflictKit.bridge().rePair(body[0]);
                    } finally {
                        TestPairing.to = p -> false;
                    }
                    long repairs = ConflictKit.bridge().repairs - was[0], skipped = ConflictKit.bridge().repairsSkipped - was[1];
                    check("a re-pair with per-player data for every player but the level's first (" + first.getGameProfile().getName()
                            + ") is skipped (sent " + sent + ", repairs +" + repairs + ", skipped +" + skipped + ")", !sent && repairs == 1 && skipped == 1);
                    record(test, "per player", "the control went; with a payload for all but " + first.getGameProfile().getName() + " it was skipped");
                    for (int i = 0; i < hearer.length; i++) {
                        FakePlayers.removeSeen(hearer[i]);
                        hearer[i] = null;
                    }
                    body[0].discard();
                })
                .thenSucceed();
    }
}
