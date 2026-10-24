package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaFollowerToPlayer;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.MaskType;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictNews;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.Hearer;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.FightWatch;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.HoldingKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Markers;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.Wolf;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The Umvuthi's grove on the jar's own bodies (MOWZIE.md §4.5, §4.9, §7 tests 15, 25, 27, 29 and 30;
 * docs/choices-mowzie-build.md, M9b): a chief killed at its grove and reported with the killer, a masked payment
 * booked by the engine, a chief's trade memory written onto its body without a journal line, the devoted
 * Umvuthana dying with the Umvuthi, the Rule U holds, and the heir's pair. Every test works in a far place of its own
 * (the world is reused), declares its groves under the name no culture claims (MZB-227) unless the native engine must
 * run them, and has a batch of its own, since the reconciler's counters are global (a test reads them as deltas).
 * Groves of one test stand 320 blocks apart in x and a test's players are offset in z only, so that no player is
 * near a grove that is not its own (the holds read 160 blocks).
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class UmvuthiGameTests {
    private UmvuthiGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;

    private static final int DONE = ConflictRecords.CampBodies.DONE, STALE = ConflictRecords.CampBodies.STALE;
    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN, ADOPT = ConflictRecords.CampReconcile.ADOPT,
            TAKE = ConflictRecords.CampReconcile.TAKE, REMOVE = ConflictRecords.CampReconcile.REMOVE,
            SEAT_HEIR = ConflictRecords.CampReconcile.SEAT_HEIR, MASK = ConflictRecords.CampReconcile.MASK;
    private static final int CHIEF_SLOT = ConflictRecords.CampReconcile.CHIEF_SLOT, CHIEF = ConflictRecords.CampReconcile.LINE_CHIEF;
    private static final String UMVUTHI = MowzieKit.UMVUTHI, WARRIOR = MowzieKit.WARRIOR;

    // ---- helpers ----------------------------------------------------------------------------------

    /** A grove 20 chunks (320 blocks) from its neighbours in x and z, so no player of one is near another (the holds read 160). */
    private static Grove groveAt(Far far, int i, int j) {
        return new Grove(far, far.k + 20 * i, far.z0 + 20 * j);
    }

    /** A point {@code dx}, {@code dz} from the grove's centre, one block over the ground (for a player). */
    private static Vec3 off(Grove g, double dx, double dz) {
        return new Vec3(g.spot.x + dx, g.spot.y + 1, g.spot.z + dz);
    }

    private static long epochOf(Grove g) {
        return g.site().epoch();
    }

    /** ADOPT of a stray into the chief's slot as generation {@code gen}; the op. */
    private static long adoptChief(Grove g, Entity e, int gen) {
        return MowzieKit.reconcile(g.faction, epochOf(g), List.of(CampLine.of(CHIEF_SLOT, gen, UMVUTHI, ADOPT, CHIEF, e.getUUID())));
    }

    /** Inside {@code thenWaitUntil}: every grove has sent its first census. */
    private static Runnable censused(Grove... gs) {
        return () -> {
            for (Grove g : gs)
                check("the census of " + Long.toHexString(g.faction), MowzieKit.censusCount(g.faction) >= 1);
        };
    }

    /** Inside {@code thenWaitUntil}: the entity is out of the level (its chunk is unloaded). */
    private static Runnable gone(ServerLevel level, UUID... ids) {
        return io(() -> {
            for (UUID id : ids)
                check("entity " + id + " is unloaded", level.getEntity(id) == null);
        });
    }

    private static boolean unanswered(long faction, long op) {
        return ConflictKit.outcomes(faction, op).isEmpty();
    }

    /**
     * A chief's CAMP_DEATH: who it is by, the killer's UUID (null for none), flagged chief, an Umvuthi, the chief's slot,
     * the given generation, with a Java op id.
     */
    private static void chiefDeath(String what, ScriptedEngine.Received d, int by, UUID killer, int generation) {
        check(what + ": by " + by + ", not " + d.aux(), d.aux() == by);
        UUID got = ConflictKit.uuidAt(d, ConflictRecords.CampDeath.KILLER_MOST);
        check(what + ": the killer is " + killer + ", not " + got, killer == null ? got == null : killer.equals(got));
        check(what + ": flagged chief", (d.view().getInt(ConflictRecords.CampDeath.FLAGS) & ConflictRecords.CampDeath.FLAG_CHIEF) != 0);
        check(what + ": an Umvuthi", d.view().getInt(ConflictRecords.CampDeath.BODY) == ConflictKit.bodyId(UMVUTHI));
        check(what + ": slot -1, generation " + generation, d.view().getInt(ConflictRecords.CampDeath.SLOT) == CHIEF_SLOT
                && d.view().getInt(ConflictRecords.CampDeath.GENERATION) == generation);
        check(what + ": with a Java op id", (d.view().getLong(ConflictRecords.CampDeath.OP_ID) & Obs.JAVA_OP_BIT) != 0);
    }

    // ---- 15: a chief killed at its grove ----------------------------------------------------------

    /**
     * A chief killed at its grove is reported with the one who gets the credit (§7 test 15, the Umvuthi half; MZB-432).
     * One scripted grove (the camp is the test's, MZB-227: the engine's own bookings after a chief's death, the
     * Killing signal, cowed_until, P = 1 and the heir due, are the engine's and not visible from Java) and five
     * Umvuthi adopted into its chief slot as generations 1 to 5, one after the other: killed by a real player, by a
     * tame wolf of the player's, by a generic kill 40 ticks after the player's hit (vanilla's kill credit), by a
     * zombie, and by a generic kill with no hit before. Each leaves exactly one CAMP_DEATH (slot -1, the generation,
     * flagged chief, a Java op id) by the player, the player, the player, a mob (no killer) and none, a record marker
     * (tagged, carrying the death) where it fell, no tag on the corpse, and one more news post. Nothing reaches the
     * hearing player's chat (no line of rtsim's), action bar or talk list, and the jar's double death event
     * (EntityUmvuthi.die posts it twice) books one death for each.
     */
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "umvuthi_kill", skyAccess = true)
    public static void aChiefKilledAtTheGroveIsReportedWithTheKiller(GameTestHelper helper) {
        String test = "aChiefKilledAtTheGroveIsReportedWithTheKiller";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        int n = 5;
        EntityUmvuthi[] u = new EntityUmvuthi[n];
        Vec3[] fell = new Vec3[n];
        long[] op = new long[n];
        Hearer[] hearer = new Hearer[1];
        Markers[] markers = new Markers[1];
        long[] news = new long[1];
        int[] heard = new int[3];
        int[] by = {ConflictRecords.BY_PLAYER, ConflictRecords.BY_PLAYER, ConflictRecords.BY_PLAYER, ConflictRecords.BY_MOB, ConflictRecords.BY_NONE};
        String[] what = {"the player's kill", "the tame wolf's kill", "the generic kill after the player's hit", "the zombie's kill",
                "the generic kill with no hit"};

        GameTestSequence seq = helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare();
                    markers[0] = new Markers(helper, level, g.area());
                    hearer[0] = FakePlayers.hearing(level, g.at(0, 24).add(0, 1, 0), "mowzie-hearer");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(hearer[0]));
                })
                .thenWaitUntil(censused(g))
                .thenExecute(() -> {
                    news[0] = ConflictNews.posted();
                    heard[0] = hearer[0].raw.size();
                    heard[1] = hearer[0].overlay.size();
                    heard[2] = hearer[0].heard.size();
                });
        for (int k = 0; k < n; k++) {
            final int i = k;
            seq = seq
                    .thenExecute(() -> {
                        u[i] = MowzieKit.umvuthi(level, g.at(-12 + 6 * i, 0));
                        op[i] = adoptChief(g, u[i], i + 1);
                    })
                    .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[i], 1))
                    .thenExecute(() -> {
                        MowzieKit.allOutcomes("ADOPT " + i, ConflictKit.outcomes(g.faction, op[i]), DONE);
                        check("Umvuthi " + i + " is tagged (faction, -1, " + (i + 1) + ")", MowzieKit.tagged(u[i], g.faction, CHIEF_SLOT, i + 1));
                        check("the grove has booked " + i + " deaths so far", ConflictKit.campDeaths(g.faction).size() == i);
                        fell[i] = u[i].position();
                        switch (i) {
                            case 0 -> MowzieKit.kill(u[i], level.damageSources().playerAttack(hearer[0]));
                            case 1 -> {
                                Wolf wolf = EntityType.WOLF.create(level);
                                check("a wolf is made", wolf != null);
                                wolf.moveTo(u[i].position().add(-2, 0, 0));
                                wolf.setTame(true, true);
                                wolf.setOwnerUUID(hearer[0].getUUID());
                                check("the wolf's owner is the player", wolf.getOwner() == hearer[0]);
                                MowzieKit.kill(u[i], level.damageSources().mobAttack(wolf));
                            }
                            case 2 -> {
                                boolean hit = u[i].hurt(level.damageSources().playerAttack(hearer[0]), 10f);
                                check("the player's hit lands (" + hit + ")", hit);
                                check("it has booked nothing yet", ConflictKit.campDeaths(g.faction).size() == i);
                            }
                            case 3 -> {
                                Zombie zombie = EntityType.ZOMBIE.create(level);
                                check("a zombie is made", zombie != null);
                                zombie.moveTo(u[i].position().add(-2, 0, 0));
                                MowzieKit.kill(u[i], level.damageSources().mobAttack(zombie));
                            }
                            default -> MowzieKit.kill(u[i], level.damageSources().genericKill());
                        }
                    });
            if (i == 2)
                seq = seq.thenIdle(40).thenExecute(() -> {
                    check("the Umvuthi lives after the hit and no death is booked", u[i].isAlive() && ConflictKit.campDeaths(g.faction).size() == i);
                    MowzieKit.kill(u[i], level.damageSources().genericKill());
                });
            seq = seq
                    .thenWaitUntil(() -> check("CAMP_DEATH " + i, ConflictKit.campDeaths(g.faction).size() == i + 1))
                    .thenExecute(() -> {
                        chiefDeath(what[i], ConflictKit.campDeaths(g.faction).get(i), by[i],
                                by[i] == ConflictRecords.BY_PLAYER ? hearer[0].getUUID() : null, i + 1);
                        List<Markers.Seen> m = markers[0].near(fell[i], 4);
                        check("a record marker where it fell (" + m.size() + ")", !m.isEmpty());
                        check("it is tagged and carries the death", m.get(m.size() - 1).tagged() && m.get(m.size() - 1).carriesRecords());
                        check("the tag is off the corpse", MowzieKit.tag(u[i]) == null);
                        check("the news hook was reached " + (i + 1) + " times (" + (ConflictNews.posted() - news[0]) + ")",
                                ConflictNews.posted() == news[0] + i + 1);
                    });
        }
        seq.thenIdle(130)
                .thenExecute(() -> {
                    check("exactly five deaths were booked, one for each Umvuthi (" + ConflictKit.campDeaths(g.faction).size() + ")",
                            ConflictKit.campDeaths(g.faction).size() == n);
                    Hearer h = hearer[0];
                    check("no talk line reached the player (" + (h.heard.size() - heard[2]) + ")", h.heard.size() == heard[2]);
                    check("nothing reached the action bar (" + (h.overlay.size() - heard[1]) + ")", h.overlay.size() == heard[1]);
                    for (int i = heard[0]; i < h.raw.size(); i++) {
                        Component c = h.raw.get(i);
                        check("no line of rtsim's reached the chat (" + c.getString() + ")",
                                !(c.getContents() instanceof TranslatableContents t && t.getKey().startsWith("mic_rtsim.")));
                    }
                    record(test, "kills", "player, tame wolf and a generic kill after his hit: by a player; zombie: by a mob; generic: by none; "
                            + (h.raw.size() - heard[0]) + " chat lines came, none rtsim's");
                })
                .thenSucceed();
    }

    // ---- 25: a masked payment ---------------------------------------------------------------------

    /**
     * A masked payment to the adopted Umvuthi is booked by the engine (§7 test 25, the Java part; MZB-418). Native
     * engine only (the grove is declared under the structure a culture claims, and the engine adopts the Umvuthi that
     * stands in it): a masked player pays the Umvuthi's own desires through the jar's trade handler; exactly one
     * CHIEF_PAID reaches the engine, with the camp's faction, the chief's slot and generation, flags 0 (a tagged
     * chief), the payer's profile id, the item and the count of the desires, and a Java op id. A second trade by the
     * same player (Sun's Blessing) journals nothing more. What the engine does with the payment (a settled demand or a
     * gift, the engine's P) is the engine's and is not read from Java (MZB-601).
     */
    @GameTest(template = ARENA, timeoutTicks = 2000, batch = "umvuthi_paid", skyAccess = true)
    public static void aMaskedPaymentToTheAdoptedUmvuthiIsBookedByTheEngine(GameTestHelper helper) {
        String test = "aMaskedPaymentToTheAdoptedUmvuthiIsBookedByTheEngine";
        if (!(RtsimTests.engine() instanceof NativeEngine)) {
            record(test, "skipped", "the grove is the engine's only on the native engine");
            helper.succeed();
            return;
        }
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        ServerPlayer[] payer = new ServerPlayer[1];
        int[] generation = new int[1];
        long[] paid0 = new long[1];
        ItemStack[] desires = new ItemStack[1];

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare(MicRtsimMowziesMobs.GROVE_STRUCTURE, GroveKind.INSTANCE);
                    u[0] = MowzieKit.umvuthi(level, g.at(-10, 0));
                })
                .thenWaitUntil(() -> {
                    var tag = MowzieKit.tag(u[0]);
                    check("the engine adopted the Umvuthi into the chief's slot", tag != null && tag.slot() == CHIEF_SLOT);
                })
                .thenExecute(() -> {
                    generation[0] = MowzieKit.tag(u[0]).generation();
                    desires[0] = u[0].getDesires().copy();
                    payer[0] = MowzieKit.player(helper, g.at(0, 8).add(0, 1, 0), "mowzie-payer");
                    MowzieKit.mask(payer[0]);
                    paid0[0] = MowzieKit.camps().chiefPaid;
                    check("no CHIEF_PAID yet", paidRecords(g).isEmpty());
                    check("the jar remembers the payer after the trade", MowzieKit.payUmvuthi(payer[0], u[0]));
                })
                .thenWaitUntil(() -> check("one CHIEF_PAID reached the engine (" + paidRecords(g).size() + ")", paidRecords(g).size() == 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received r = paidRecords(g).get(0);
                    check("one item entry (aux " + r.aux() + ")", r.aux() == 1);
                    check("the camp reported the chief's slot", r.view().getInt(ConflictRecords.ChiefPaid.SLOT) == CHIEF_SLOT);
                    check("and its generation " + generation[0], r.view().getInt(ConflictRecords.ChiefPaid.GENERATION) == generation[0]);
                    check("flags 0 (a tagged chief, not a stray)", r.view().getInt(ConflictRecords.ChiefPaid.FLAGS) == 0);
                    check("the payer is the masked player", payer[0].getGameProfile().getId().equals(ConflictKit.uuidAt(r, ConflictRecords.ChiefPaid.PLAYER_MOST)));
                    int item = r.view().getInt(ConflictRecords.ChiefPaid.ITEMS + ConflictRecords.I_ITEM);
                    check("the item is the desires' (" + item + ")", item == RtsimTests.session().itemId(desires[0].getItem()));
                    check("and the count is the desires' (" + desires[0].getCount() + ")",
                            r.view().getInt(ConflictRecords.ChiefPaid.ITEMS + ConflictRecords.I_COUNT) == desires[0].getCount());
                    check("with a Java op id", (r.view().getLong(ConflictRecords.ChiefPaid.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    check("the reconciler counted one payment", MowzieKit.camps().chiefPaid == paid0[0] + 1);
                    check("a second trade by the same player (Sun's Blessing) goes through", MowzieKit.payUmvuthi(payer[0], u[0]));
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("and journals no second payment (" + paidRecords(g).size() + ")", paidRecords(g).size() == 1);
                    check("nor does the reconciler count one", MowzieKit.camps().chiefPaid == paid0[0] + 1);
                    record(test, "paid", "desires " + desires[0] + ", chief generation " + generation[0]);
                })
                .thenSucceed();
    }

    private static List<ScriptedEngine.Received> paidRecords(Grove g) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.CHIEF_PAID, ConflictRecords.ChiefPaid.FACTION, g.faction);
    }

    // ---- 25: a chief's trade memory ---------------------------------------------------------------

    /**
     * A chief's trade memory is written onto its body and journals nothing (§7 test 25, the memory part; MZB-421).
     * Three players A, B and C stand 300 blocks off (no hold). After the census a CHIEF_BODY naming A and B is kept
     * for the epoch (no body yet: nothing written). A chief SPAWN with a throne (direction 3) then makes the
     * Umvuthi at the throne facing direction 3, restricted to the throne, remembering A and B and not C, the stored
     * memory consumed and written (one write). A second CHIEF_BODY naming C is written at once onto the standing
     * body (two writes, the set is A, B and C). No CHIEF_PAID is journaled and the reconciler counts no payment: a
     * copy of a memory is no trade.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "umvuthi_memory", skyAccess = true)
    public static void aChiefsTradeMemoryIsWrittenOntoItsBodyAndJournalsNothing(GameTestHelper helper) {
        String test = "aChiefsTradeMemoryIsWrittenOntoItsBodyAndJournalsNothing";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        ServerPlayer[] pl = new ServerPlayer[3];
        long[] counters = new long[3];
        long[] op = new long[1];
        Vec3[] throne = new Vec3[1];

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare();
                    String[] names = {"mowzie-trader-a", "mowzie-trader-b", "mowzie-trader-c"};
                    for (int i = 0; i < 3; i++)
                        pl[i] = MowzieKit.player(helper, off(g, 300, 0), names[i]);
                })
                .thenWaitUntil(censused(g))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    counters[0] = camps.chiefBodies;
                    counters[1] = camps.chiefBodiesWritten;
                    counters[2] = camps.chiefPaid;
                    ConflictKit.chiefBody(g.faction, epochOf(g), 0f, List.of(pl[0].getUUID(), pl[1].getUUID()));
                })
                .thenWaitUntil(() -> check("the memory is kept for the epoch", MowzieKit.camps().chiefMemory(g.faction) != null))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    check("one CHIEF_BODY was taken (" + (camps.chiefBodies - counters[0]) + ")", camps.chiefBodies == counters[0] + 1);
                    check("none was written (no body yet)", camps.chiefBodiesWritten == counters[1]);
                    check("the kept memory names A and B", camps.chiefMemory(g.faction).traded().equals(List.of(pl[0].getUUID(), pl[1].getUUID())));
                    throne[0] = g.at(8, 8);
                    op[0] = MowzieKit.reconcile(g.faction, epochOf(g), throne[0], 3f, List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, SPAWN, CHIEF)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("SPAWN", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    List<Entity> held = MowzieKit.held(g.faction, CHIEF_SLOT, 1);
                    check("one Umvuthi, tagged (faction, -1, 1) (" + held.size() + ")", held.size() == 1 && held.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi u = (EntityUmvuthi) held.get(0);
                    check("it stands at the throne (" + u.position().distanceTo(throne[0]) + " blocks off)", u.position().distanceTo(throne[0]) < 5);
                    check("facing direction 3 (" + u.getDirectionData() + ")", u.getDirectionData() == 3);
                    check("restricted to the throne", u.hasRestriction() && u.getRestrictCenter().equals(BlockPos.containing(throne[0])));
                    check("it remembers A and B (" + u.getTradedPlayers() + ")", u.getTradedPlayers().equals(Set.of(pl[0].getUUID(), pl[1].getUUID())));
                    check("hasTradedWith: A and B yes, C no", u.hasTradedWith(pl[0]) && u.hasTradedWith(pl[1]) && !u.hasTradedWith(pl[2]));
                    Camps camps = MowzieKit.camps();
                    check("the stored memory was consumed", camps.chiefMemory(g.faction) == null);
                    check("one write (" + (camps.chiefBodiesWritten - counters[1]) + ")", camps.chiefBodiesWritten == counters[1] + 1);
                    check("no payment was counted", camps.chiefPaid == counters[2]);
                    check("and no CHIEF_PAID was journaled", paidRecords(g).isEmpty());
                    ConflictKit.chiefBody(g.faction, epochOf(g), 0f, List.of(pl[2].getUUID()));
                })
                .thenWaitUntil(() -> check("the second memory was written at once", MowzieKit.camps().chiefBodiesWritten == counters[1] + 2))
                .thenExecute(() -> {
                    EntityUmvuthi u = (EntityUmvuthi) MowzieKit.held(g.faction, CHIEF_SLOT, 1).get(0);
                    check("it remembers A, B and C (" + u.getTradedPlayers() + ")",
                            u.getTradedPlayers().equals(Set.of(pl[0].getUUID(), pl[1].getUUID(), pl[2].getUUID())));
                    check("two CHIEF_BODY records were taken", MowzieKit.camps().chiefBodies == counters[0] + 2);
                    check("nothing was kept for later", MowzieKit.camps().chiefMemory(g.faction) == null);
                    check("still no payment, counted or journaled", MowzieKit.camps().chiefPaid == counters[2] && paidRecords(g).isEmpty());
                    record(test, "memory", "A, B then C written onto the chief body; no CHIEF_PAID");
                })
                .thenSucceed();
    }

    // ---- 27: the devoted die with the Umvuthi ------------------------------------------------------

    /**
     * Killing the Umvuthi kills the devoted Umvuthana within 30 blocks and no others (§7 test 27; MZB-432). One
     * scripted grove with the Umvuthi (adopted, chief) and three adopted warriors (FEAR masks: a FURY mask blocks a
     * hit) 10, 25 and 35 blocks from it, and a player's follower (a stray, not devoted) 10 blocks off. A player hits
     * the 25-block warrior; the Umvuthi, last hurt by that player, is killed by him. Exactly three CAMP_DEATHs follow,
     * the warriors' within 30 ticks of the kill: the chief (flagged, by the player), the 10-block warrior (by none:
     * the jar's own indirect magic and no hit) and the 25-block warrior (by the player: vanilla's kill credit from
     * his hit). The 35-block warrior (still tagged) and the follower live; the Umvuthi's loot has a Sol Visage (at
     * least one; the count is recorded).
     */
    @GameTest(template = ARENA, timeoutTicks = 2500, batch = "umvuthi_devoted", skyAccess = true)
    public static void killingTheUmvuthiKillsTheDevotedUmvuthanaWithin30Blocks(GameTestHelper helper) {
        String test = "killingTheUmvuthiKillsTheDevotedUmvuthanaWithin30Blocks";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        EntityUmvuthanaFollowerToPlayer[] follower = new EntityUmvuthanaFollowerToPlayer[1];
        ServerPlayer[] player = new ServerPlayer[1];
        long[] op = new long[1];
        long[] killedAt = new long[2];
        double[] dist = new double[3];

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare();
                    u[0] = MowzieKit.umvuthi(level, g.at(-22, 0));
                    double[] at = {-12, 3, 13};
                    for (int i = 0; i < 3; i++) {
                        w[i] = MowzieKit.warrior(level, g.at(at[i], 0));
                        w[i].setMask(MaskType.FEAR);
                        dist[i] = w[i].position().distanceTo(u[0].position());
                    }
                    follower[0] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_FOLLOWER_TO_PLAYER.get(), g.at(-22, 10).x, g.at(-22, 10).y,
                            g.at(-22, 10).z, true, false);
                    player[0] = MowzieKit.player(helper, g.at(0, 12).add(0, 1, 0), "mowzie-slayer");
                    MowzieKit.everyTick(helper, () -> {
                        int deaths = ConflictKit.campDeaths(g.faction).size();
                        if (killedAt[1] == 0 && deaths >= 3)
                            killedAt[1] = level.getGameTime();
                    });
                })
                .thenWaitUntil(censused(g))
                .thenExecute(() -> {
                    check("the warriors stand 10, 25 and 35 blocks from the Umvuthi (" + dist[0] + ", " + dist[1] + ", " + dist[2] + ")",
                            Math.abs(dist[0] - 10) < 1 && Math.abs(dist[1] - 25) < 1 && Math.abs(dist[2] - 35) < 1);
                    List<CampLine> lines = new java.util.ArrayList<>();
                    lines.add(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, ADOPT, CHIEF, u[0].getUUID()));
                    for (int i = 0; i < 3; i++)
                        lines.add(CampLine.of(i, 1, WARRIOR, ADOPT, 0, w[i].getUUID()));
                    op[0] = MowzieKit.reconcile(g.faction, epochOf(g), lines);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[0], 4))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    check("the Umvuthi is tagged (faction, -1, 1)", MowzieKit.tagged(u[0], g.faction, CHIEF_SLOT, 1));
                    for (int i = 0; i < 3; i++)
                        check("warrior " + i + " is tagged (faction, " + i + ", 1)", MowzieKit.tagged(w[i], g.faction, i, 1));
                    boolean hit = w[1].hurt(level.damageSources().playerAttack(player[0]), 1f);
                    check("the player's hit on the 25-block warrior lands (" + hit + ")", hit);
                    check("no death is booked yet", ConflictKit.campDeaths(g.faction).isEmpty());
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("the warrior lives after the hit", w[1].isAlive() && ConflictKit.campDeaths(g.faction).isEmpty());
                    u[0].setLastHurtByPlayer(player[0]);
                    killedAt[0] = level.getGameTime();
                    MowzieKit.kill(u[0], level.damageSources().playerAttack(player[0]));
                })
                .thenWaitUntil(() -> check("three deaths are booked (" + ConflictKit.campDeaths(g.faction).size() + ")",
                        ConflictKit.campDeaths(g.faction).size() >= 3))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> deaths = ConflictKit.campDeaths(g.faction);
                    check("exactly three (" + deaths.size() + ")", deaths.size() == 3);
                    long took = killedAt[1] - killedAt[0];
                    check("the warriors died within 30 ticks of the kill (" + took + ")", took >= 0 && took <= 30);
                    ScriptedEngine.Received chief = null, w0 = null, w1 = null;
                    for (ScriptedEngine.Received d : deaths) {
                        int slot = d.view().getInt(ConflictRecords.CampDeath.SLOT);
                        if (slot == CHIEF_SLOT)
                            chief = d;
                        else if (slot == 0)
                            w0 = d;
                        else if (slot == 1)
                            w1 = d;
                    }
                    check("a death for the chief and for the warriors of slots 0 and 1", chief != null && w0 != null && w1 != null);
                    chiefDeath("the Umvuthi", chief, ConflictRecords.BY_PLAYER, player[0].getUUID(), 1);
                    check("the 10-block warrior: by none, not a chief", w0.aux() == ConflictRecords.BY_NONE
                            && ConflictKit.uuidAt(w0, ConflictRecords.CampDeath.KILLER_MOST) == null
                            && (w0.view().getInt(ConflictRecords.CampDeath.FLAGS) & ConflictRecords.CampDeath.FLAG_CHIEF) == 0);
                    check("the 25-block warrior: by the player, not a chief", w1.aux() == ConflictRecords.BY_PLAYER
                            && player[0].getUUID().equals(ConflictKit.uuidAt(w1, ConflictRecords.CampDeath.KILLER_MOST))
                            && (w1.view().getInt(ConflictRecords.CampDeath.FLAGS) & ConflictRecords.CampDeath.FLAG_CHIEF) == 0);
                    check("the 35-block warrior lives, tagged", w[2].isAlive() && MowzieKit.tagged(w[2], g.faction, 2, 1));
                    check("so does the follower", follower[0].isAlive() && !follower[0].isRemoved());
                    for (int i = 0; i < 2; i++)
                        check("the dead warriors lost their tags", MowzieKit.tag(w[i]) == null);
                })
                .thenWaitUntil(() -> check("the Umvuthi dropped a Sol Visage (" + visages(level, g) + ")", visages(level, g) >= 1))
                .thenIdle(60)
                .thenExecute(() -> {
                    check("still three deaths (" + ConflictKit.campDeaths(g.faction).size() + ")", ConflictKit.campDeaths(g.faction).size() == 3);
                    check("the far warrior and the follower still live", w[2].isAlive() && follower[0].isAlive());
                    record(test, "devoted", "chief by the player, the 10-block warrior by none, the 25-block warrior by the player, within "
                            + (killedAt[1] - killedAt[0]) + " ticks; " + visages(level, g) + " Sol Visage dropped");
                })
                .thenSucceed();
    }

    private static int visages(ServerLevel level, Grove g) {
        ResourceLocation sol = ResourceLocation.fromNamespaceAndPath("mowziesmobs", "sol_visage");
        int n = 0;
        for (ItemEntity e : level.getEntitiesOfClass(ItemEntity.class, g.area()))
            if (sol.equals(BuiltInRegistries.ITEM.getKey(e.getItem().getItem())))
                n += e.getItem().getCount();
        return n;
    }

    // ---- 29: Rule U ---------------------------------------------------------------------------------

    /**
     * The Umvuthi's removals wait while a player is near it or it has one as a target (§7 test 29, the first clause;
     * MZB-433, MZB-440). Two scripted groves (320 blocks apart), each with an adopted Umvuthi and a player 150 blocks
     * off in z: grove A is sent a TAKE of the chief, grove B a REMOVE. Both lines are held (two more held removals,
     * two more held lines, no answer, both Umvuthi alive and tagged), B's Umvuthi is a second body (its REMOVE is
     * held) and A's is not (a held TAKE leaves the body the chief). The hold has no time limit: after
     * {@link Camps#HOLD_MAX} + 100 ticks the same. The players then step to 170 blocks; each Umvuthi has its player as
     * a target (set every tick): still held. With the targets cleared both lines are DONE within a few ticks, both
     * Umvuthi are gone, a record marker (tagged, carrying records) stands where each stood, no death is booked and the
     * held lines are back to the start.
     */
    @GameTest(template = ARENA, timeoutTicks = 8000, batch = "umvuthi_hold_removal", skyAccess = true)
    public static void theUmvuthisRemovalsWaitWhileAPlayerIsNearOrFightsIt(GameTestHelper helper) {
        String test = "theUmvuthisRemovalsWaitWhileAPlayerIsNearOrFightsIt";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 1, 0)};
        int[] action = {TAKE, REMOVE};
        EntityUmvuthi[] u = new EntityUmvuthi[2];
        ServerPlayer[] p = new ServerPlayer[2];
        Markers[] markers = new Markers[2];
        Vec3[] stood = new Vec3[2];
        long[] op = new long[2];
        long[] removals0 = new long[1];
        int[] lines0 = new int[1];
        boolean[] fight = new boolean[1];

        Runnable stillHeld = () -> {
            Camps camps = MowzieKit.camps();
            check("two more held removals (" + (camps.heldRemovals - removals0[0]) + ")", camps.heldRemovals == removals0[0] + 2);
            check("two more held lines (" + (camps.heldLines() - lines0[0]) + ")", camps.heldLines() == lines0[0] + 2);
            for (int i = 0; i < 2; i++) {
                check("line " + i + " has no answer", unanswered(g[i].faction, op[i]));
                check("Umvuthi " + i + " lives, tagged", u[i].isAlive() && !u[i].isRemoved() && MowzieKit.tagged(u[i], g[i].faction, CHIEF_SLOT, 1));
            }
            check("a held TAKE leaves the body the chief", !camps.secondBody(u[0]));
            check("a held REMOVE makes the body a second one", camps.secondBody(u[1]));
        };

        helper.startSequence()
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.force();
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        g[i].lay().declare();
                        markers[i] = new Markers(helper, level, g[i].area());
                        u[i] = MowzieKit.umvuthi(level, g[i].at(0, 0));
                        stood[i] = u[i].position();
                        p[i] = MowzieKit.player(helper, off(g[i], 0, 150), "mowzie-near-" + i);
                    }
                    MowzieKit.everyTick(helper, () -> {
                        if (fight[0])
                            for (int i = 0; i < 2; i++)
                                if (!u[i].isRemoved() && u[i].isAlive())
                                    u[i].setTarget(p[i]);
                    });
                })
                .thenWaitUntil(censused(g))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++)
                        op[i] = adoptChief(g[i], u[i], 1);
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < 2; i++)
                        MowzieKit.answered(g[i].faction, op[i], 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        MowzieKit.allOutcomes("ADOPT " + i, ConflictKit.outcomes(g[i].faction, op[i]), DONE);
                        check("the player of grove " + i + " is within 160 blocks of its Umvuthi",
                                p[i].distanceToSqr(u[i]) < GroveKind.HOLD_RANGE * GroveKind.HOLD_RANGE);
                    }
                    removals0[0] = MowzieKit.camps().heldRemovals;
                    lines0[0] = MowzieKit.camps().heldLines();
                    for (int i = 0; i < 2; i++)
                        op[i] = MowzieKit.reconcile(g[i].faction, epochOf(g[i]), List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, action[i], CHIEF)));
                })
                .thenIdle(5)
                .thenExecute(stillHeld)
                .thenIdle(Camps.HOLD_MAX + 100)
                .thenExecute(() -> {
                    stillHeld.run();
                    for (int i = 0; i < 2; i++)
                        p[i].moveTo(off(g[i], 0, 170).x, off(g[i], 0, 170).y, off(g[i], 0, 170).z);
                    fight[0] = true;
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        check("the player of grove " + i + " is beyond 160 blocks", p[i].distanceToSqr(u[i]) > GroveKind.HOLD_RANGE * GroveKind.HOLD_RANGE);
                        check("Umvuthi " + i + " has the player as its target", u[i].getTarget() == p[i]);
                    }
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    stillHeld.run();
                    fight[0] = false;
                    for (int i = 0; i < 2; i++)
                        u[i].setTarget(null);
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < 2; i++)
                        MowzieKit.answered(g[i].faction, op[i], 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        MowzieKit.allOutcomes("line " + i, ConflictKit.outcomes(g[i].faction, op[i]), DONE);
                        check("Umvuthi " + i + " is gone", u[i].isRemoved());
                        List<Markers.Seen> m = markers[i].near(stood[i], 4);
                        check("a record marker where Umvuthi " + i + " stood (" + m.size() + ")", !m.isEmpty());
                        check("it is tagged and carries records", m.get(0).tagged() && m.get(0).carriesRecords());
                        check("no death was booked for grove " + i, ConflictKit.campDeaths(g[i].faction).isEmpty());
                    }
                    check("the held lines are back to the start (" + MowzieKit.camps().heldLines() + ")", MowzieKit.camps().heldLines() == lines0[0]);
                    check("the held-removal counter counted each once", MowzieKit.camps().heldRemovals == removals0[0] + 2);
                    record(test, "holds", "TAKE and REMOVE held through " + (Camps.HOLD_MAX + 100) + " ticks with a player at 150 blocks, then through "
                            + "a player-target at 170, then played");
                })
                .thenSucceed();
    }

    /**
     * A chief's SPAWN waits for the player to leave or for {@link Camps#HOLD_MAX} ticks (§7 test 29, the second
     * clause; MZB-433). Two scripted groves with no body, a throne at 6 blocks and direction 3, a chief SPAWN each and
     * a player 100 blocks off in z: both lines are held (two more held spawns, no body, no answer). C's player steps
     * to 170 blocks: C's line is DONE with a tagged Umvuthi at the throne facing 3 and the held-spawn counter
     * unchanged. D's player stays: D is still unanswered 5,900 ticks after the reconcile, and answered within a few
     * ticks of 6,000 (the Umvuthi is made with the player still within 160 blocks).
     */
    @GameTest(template = ARENA, timeoutTicks = 7500, batch = "umvuthi_hold_spawn", skyAccess = true)
    public static void aChiefSpawnWaitsForThePlayerToLeaveOrSixThousandTicks(GameTestHelper helper) {
        String test = "aChiefSpawnWaitsForThePlayerToLeaveOrSixThousandTicks";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove[] g = {groveAt(far, 0, 0), groveAt(far, 1, 0)}; // C, D
        ServerPlayer[] p = new ServerPlayer[2];
        Vec3[] throne = new Vec3[2];
        long[] op = new long[2];
        long[] spawns0 = new long[1];
        long[] t0 = new long[1];
        long[] answeredAt = new long[1];

        helper.startSequence()
                .thenExecute(() -> {
                    for (Grove x : g)
                        x.force();
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        g[i].lay().declare();
                        p[i] = MowzieKit.player(helper, off(g[i], 6, 100), "mowzie-stay-" + i);
                    }
                    MowzieKit.everyTick(helper, () -> {
                        if (answeredAt[0] == 0 && op[1] != 0 && !unanswered(g[1].faction, op[1]))
                            answeredAt[0] = level.getGameTime();
                    });
                })
                .thenWaitUntil(censused(g))
                .thenExecute(() -> {
                    spawns0[0] = MowzieKit.camps().heldSpawns;
                    t0[0] = level.getGameTime();
                    for (int i = 0; i < 2; i++) {
                        throne[i] = g[i].at(6, 0);
                        op[i] = MowzieKit.reconcile(g[i].faction, epochOf(g[i]), throne[i], 3f,
                                List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, SPAWN, CHIEF)));
                    }
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    check("two more held spawns (" + (MowzieKit.camps().heldSpawns - spawns0[0]) + ")", MowzieKit.camps().heldSpawns == spawns0[0] + 2);
                    for (int i = 0; i < 2; i++) {
                        check("line " + i + " has no answer", unanswered(g[i].faction, op[i]));
                        check("no Umvuthi stands in grove " + i, MowzieKit.held(g[i].faction).isEmpty());
                        check("the player of grove " + i + " is within 160 blocks of the throne", p[i].distanceToSqr(throne[i]) < 160 * 160);
                    }
                    p[0].moveTo(off(g[0], 6, 170).x, off(g[0], 6, 170).y, off(g[0], 6, 170).z);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g[0].faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("C's SPAWN", ConflictKit.outcomes(g[0].faction, op[0]), DONE);
                    List<Entity> held = MowzieKit.held(g[0].faction, CHIEF_SLOT, 1);
                    check("an Umvuthi, tagged (faction, -1, 1) (" + held.size() + ")", held.size() == 1 && held.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi u = (EntityUmvuthi) held.get(0);
                    check("at the throne (" + u.position().distanceTo(throne[0]) + " blocks off)", u.position().distanceTo(throne[0]) < 5);
                    check("facing direction 3 (" + u.getDirectionData() + ")", u.getDirectionData() == 3);
                    check("the held-spawn counter did not move again", MowzieKit.camps().heldSpawns == spawns0[0] + 2);
                    check("D is still held", unanswered(g[1].faction, op[1]) && MowzieKit.held(g[1].faction).isEmpty());
                })
                .thenWaitUntil(() -> check("5,900 ticks after the reconcile (" + (level.getGameTime() - t0[0]) + ")", level.getGameTime() >= t0[0] + 5900))
                .thenExecute(() -> {
                    check("D is still unanswered with no body", unanswered(g[1].faction, op[1]) && MowzieKit.held(g[1].faction).isEmpty());
                    check("its player is still within 160 blocks of the throne", p[1].distanceToSqr(throne[1]) < 160 * 160);
                })
                .thenWaitUntil(() -> check("D is answered", answeredAt[0] != 0))
                .thenExecute(() -> {
                    long after = answeredAt[0] - t0[0];
                    check("D was answered 6,000 ticks after the reconcile, give or take a few (" + after + ")", after >= 5990 && after <= 6040);
                    MowzieKit.allOutcomes("D's SPAWN", ConflictKit.outcomes(g[1].faction, op[1]), DONE);
                    List<Entity> held = MowzieKit.held(g[1].faction, CHIEF_SLOT, 1);
                    check("an Umvuthi, tagged (faction, -1, 1) (" + held.size() + ")", held.size() == 1);
                    check("made with the player still near (" + p[1].distanceToSqr(throne[1]) + ")", p[1].distanceToSqr(throne[1]) < 160 * 160);
                    check("the held-spawn counter is unchanged", MowzieKit.camps().heldSpawns == spawns0[0] + 2);
                    record(test, "hold", "C played when its player left; D played " + after + " ticks after the reconcile with its player near");
                })
                .thenSucceed();
    }

    /**
     * An epoch's end answers every held line stale (§7 test 29, the third clause; MZB-433). A scripted grove with an
     * adopted Umvuthi (generation 1) and a player 100 blocks off in z, and one reconcile with a throne carrying a TAKE
     * of the chief and a chief SPAWN of generation 2: both held (two more held lines). The grove's chunks are
     * released: the camp goes out (CAMP_OUT), both lines are answered STALE and the held lines are back to the start.
     * Forced again, the new census opens a new epoch, and the Umvuthi that loads (the same UUID, another object) is
     * still tagged (faction, -1, 1): a held TAKE never played.
     */
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "umvuthi_hold_epoch", skyAccess = true)
    public static void anEpochsEndAnswersEveryHeldLineStale(GameTestHelper helper) {
        String test = "anEpochsEndAnswersEveryHeldLineStale";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        EntityUmvuthi[] u = new EntityUmvuthi[1];
        UUID[] id = new UUID[1];
        long[] op = new long[2];
        int[] lines0 = new int[1];
        long[] epoch = new long[1];

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare();
                    u[0] = MowzieKit.umvuthi(level, g.at(0, 0));
                    id[0] = u[0].getUUID();
                    MowzieKit.player(helper, off(g, 0, 100), "mowzie-near");
                })
                .thenWaitUntil(censused(g))
                .thenExecute(() -> {
                    epoch[0] = epochOf(g);
                    op[0] = adoptChief(g, u[0], 1);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    lines0[0] = MowzieKit.camps().heldLines();
                    op[1] = MowzieKit.reconcile(g.faction, epoch[0], g.at(6, 0), 3f, List.of(
                            CampLine.of(CHIEF_SLOT, 1, UMVUTHI, TAKE, CHIEF),
                            CampLine.of(CHIEF_SLOT, 2, UMVUTHI, SPAWN, CHIEF)));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    check("two more held lines (" + (MowzieKit.camps().heldLines() - lines0[0]) + ")", MowzieKit.camps().heldLines() == lines0[0] + 2);
                    check("no answer", unanswered(g.faction, op[1]));
                    far.releaseRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(() -> check("CAMP_OUT", !ConflictKit.campOuts(g.faction).isEmpty()))
                .thenWaitUntil(gone(level, id[0]))
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[1], 2))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the held lines", ConflictKit.outcomes(g.faction, op[1]), STALE);
                    check("the held lines are back to the start (" + MowzieKit.camps().heldLines() + ")", MowzieKit.camps().heldLines() == lines0[0]);
                    g.far.forceRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("a second census", MowzieKit.censusCount(g.faction) >= 2))
                .thenWaitUntil(io(() -> {
                    Entity e = level.getEntity(id[0]);
                    check("the Umvuthi is loaded again", e != null && MowzieKit.tagged(e, g.faction, CHIEF_SLOT, 1));
                }))
                .thenExecute(() -> {
                    check("a new epoch (" + epoch[0] + " then " + MowzieKit.epoch(MowzieKit.census(g.faction, 2)) + ")",
                            MowzieKit.epoch(MowzieKit.census(g.faction, 2)) != epoch[0]);
                    check("no body was taken or removed", level.getEntity(id[0]).isAlive());
                    record(test, "epoch", "a held TAKE and a held chief SPAWN answered STALE at CAMP_OUT; the Umvuthi reloaded tagged");
                })
                .thenSucceed();
    }

    // ---- 30: the heir ---------------------------------------------------------------------------------

    /**
     * An Umvuthi heir takes the farthest free warrior (§7 test 30, the plain seat; MZB-408, MZB-433). Three scripted
     * groves with no chief body. G3 has one warrior and no throne in the reconcile: the pair is played at once, both
     * lines DONE, the MASK answer naming the warrior (slot 0, generation 1), the heir within 4 blocks of the fire pit
     * on the ground facing direction 1, restricted near it, and the warrior gone. G2 has no warrior: both lines STALE
     * and no Umvuthi. G1 (declared with a kind that also holds a warrior's removal, since the real kind never holds
     * one) has three warriors at 15 blocks west, 5 and 20 blocks east of its centre, a held TAKE of the 20-block one
     * (a line in flight, so it is not taken), a throne 8 blocks south and a player 100 blocks off: the pair is held
     * (one held pair, no answer, all warriors home). The player steps to 170 blocks west of the centre; the warriors are
     * then 155, 175 and 190 blocks from it, the farthest is held, so the pair takes the 5-block warrior: the MASK
     * answer names slot 1, generation 1, the heir stands at the throne facing 3 restricted to it with no trade
     * memory, and the other two warriors are still tagged.
     */
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "umvuthi_heir", skyAccess = true)
    public static void anUmvuthiHeirTakesTheFarthestFreeWarrior(GameTestHelper helper) {
        String test = "anUmvuthiHeirTakesTheFarthestFreeWarrior";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g1 = groveAt(far, 0, 0), g2 = groveAt(far, 1, 0), g3 = groveAt(far, 2, 0);
        EntityUmvuthanaMinion[] w1 = new EntityUmvuthanaMinion[3];
        EntityUmvuthanaMinion[] w3 = new EntityUmvuthanaMinion[1];
        ServerPlayer[] p1 = new ServerPlayer[1];
        Vec3[] throne = new Vec3[1];
        long[] op = new long[7];
        long[] counters = new long[4];
        int[] lines0 = new int[1];

        helper.startSequence()
                .thenExecute(() -> {
                    g1.force();
                    g2.force();
                    g3.force();
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g1.lay().declare(MowzieKit.TEST_GROVE, HoldingKind.INSTANCE);
                    g2.lay().declare();
                    g3.lay().declare();
                    double[] at = {-15, 5, 20};
                    for (int i = 0; i < 3; i++)
                        w1[i] = MowzieKit.warrior(level, g1.at(at[i], 0));
                    w3[0] = MowzieKit.warrior(level, g3.at(4, 0));
                    HoldingKind.hold(helper, w1[2]);
                    throne[0] = g1.at(0, 8);
                    p1[0] = MowzieKit.player(helper, new Vec3(throne[0].x, throne[0].y + 1, throne[0].z + 100), "mowzie-heir-p1");
                })
                .thenWaitUntil(censused(g1, g2, g3))
                .thenExecute(() -> {
                    List<CampLine> lines = new java.util.ArrayList<>();
                    for (int i = 0; i < 3; i++)
                        lines.add(CampLine.of(i, 1, WARRIOR, ADOPT, 0, w1[i].getUUID()));
                    op[0] = MowzieKit.reconcile(g1.faction, epochOf(g1), lines);
                    op[1] = MowzieKit.reconcile(g3.faction, epochOf(g3), List.of(CampLine.of(0, 1, WARRIOR, ADOPT, 0, w3[0].getUUID())));
                })
                .thenWaitUntil(() -> {
                    MowzieKit.answered(g1.faction, op[0], 3);
                    MowzieKit.answered(g3.faction, op[1], 1);
                })
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("G1's ADOPT", ConflictKit.outcomes(g1.faction, op[0]), DONE);
                    MowzieKit.allOutcomes("G3's ADOPT", ConflictKit.outcomes(g3.faction, op[1]), DONE);
                    Camps camps = MowzieKit.camps();
                    counters[0] = camps.heirs;
                    counters[1] = camps.masks;
                    counters[2] = camps.heldPairs;
                    counters[3] = camps.heldRemovals;
                    lines0[0] = camps.heldLines();
                    // G2 has no warrior at all; G3's reconcile names no throne (the fire pit)
                    List<CampLine> pair = List.of(CampLine.of(0, 0, WARRIOR, MASK, 0), CampLine.of(CHIEF_SLOT, 2, UMVUTHI, SEAT_HEIR, CHIEF));
                    op[2] = MowzieKit.reconcile(g2.faction, epochOf(g2), pair);
                    op[3] = MowzieKit.reconcile(g3.faction, epochOf(g3), pair);
                })
                .thenWaitUntil(() -> {
                    MowzieKit.answered(g2.faction, op[2], 2);
                    MowzieKit.answered(g3.faction, op[3], 2);
                })
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("G2's pair, no warrior", ConflictKit.outcomes(g2.faction, op[2]), STALE);
                    check("no Umvuthi stands in G2", MowzieKit.held(g2.faction).isEmpty());
                    MowzieKit.allOutcomes("G3's pair", ConflictKit.outcomes(g3.faction, op[3]), DONE);
                    Map<Integer, MowzieKit.Answered> a = MowzieKit.answeredLines(g3.faction, op[3]);
                    check("G3's MASK names the warrior (slot 0, generation 1)", a.get(0).slot() == 0 && a.get(0).generation() == 1);
                    List<Entity> heir = MowzieKit.held(g3.faction, CHIEF_SLOT, 2);
                    check("G3 has an heir, tagged (faction, -1, 2) (" + heir.size() + ")", heir.size() == 1 && heir.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi h = (EntityUmvuthi) heir.get(0);
                    double flat = Math.hypot(h.getX() - g3.spot.x, h.getZ() - g3.spot.z);
                    check("within 6 blocks of the fire pit (" + flat + ")", flat < 6);
                    check("on the ground (" + h.getY() + " over " + g3.at(0, 0).y + ")", Math.abs(h.getY() - g3.at(h.getX() - g3.spot.x, h.getZ() - g3.spot.z).y) < 1.5);
                    check("facing direction 1 (" + h.getDirectionData() + ")", h.getDirectionData() == 1);
                    check("restricted near the fire pit", h.hasRestriction() && Math.hypot(h.getRestrictCenter().getX() + 0.5 - g3.spot.x,
                            h.getRestrictCenter().getZ() + 0.5 - g3.spot.z) < 7);
                    check("the warrior is gone, no other body stands", w3[0].isRemoved() && MowzieKit.held(g3.faction).size() == 1);
                    Camps camps = MowzieKit.camps();
                    check("one heir and one mask so far (" + (camps.heirs - counters[0]) + ", " + (camps.masks - counters[1]) + ")",
                            camps.heirs == counters[0] + 1 && camps.masks == counters[1] + 1);
                    // G1: a TAKE of the 20-block warrior is held (a line in flight), then the pair
                    op[4] = MowzieKit.reconcile(g1.faction, epochOf(g1), throne[0], 3f, List.of(CampLine.of(2, 1, WARRIOR, TAKE, 0)));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    check("the TAKE of the 20-block warrior is held (" + (camps.heldRemovals - counters[3]) + ")", camps.heldRemovals == counters[3] + 1);
                    check("it has no answer and the warrior is home", unanswered(g1.faction, op[4]) && w1[2].isAlive());
                    op[5] = MowzieKit.reconcile(g1.faction, epochOf(g1), throne[0], 3f, List.of(
                            CampLine.of(0, 0, WARRIOR, MASK, 0), CampLine.of(CHIEF_SLOT, 2, UMVUTHI, SEAT_HEIR, CHIEF)));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    check("the pair is held (" + (camps.heldPairs - counters[2]) + ")", camps.heldPairs == counters[2] + 1);
                    check("two held lines more than at the start (the TAKE and the pair) (" + (camps.heldLines() - lines0[0]) + ")",
                            camps.heldLines() == lines0[0] + 2);
                    check("the pair has no answer", unanswered(g1.faction, op[5]));
                    for (int i = 0; i < 3; i++)
                        check("warrior " + i + " is home, tagged", w1[i].isAlive() && MowzieKit.tagged(w1[i], g1.faction, i, 1));
                    check("no Umvuthi stands in G1", MowzieKit.held(g1.faction, CHIEF_SLOT, 2).isEmpty());
                    // the player steps 170 blocks west of the centre: 155, 175 and 190 blocks from the warriors
                    p1[0].moveTo(g1.spot.x - 170, g1.spot.y + 1, g1.spot.z);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g1.faction, op[5], 2))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("G1's pair", ConflictKit.outcomes(g1.faction, op[5]), DONE);
                    Map<Integer, MowzieKit.Answered> a = MowzieKit.answeredLines(g1.faction, op[5]);
                    check("the MASK names the 5-block warrior (slot 1, generation 1) (" + a.get(0) + ")", a.get(0).slot() == 1 && a.get(0).generation() == 1);
                    check("that warrior is gone", w1[1].isRemoved());
                    check("the others are still tagged", MowzieKit.tagged(w1[0], g1.faction, 0, 1) && MowzieKit.tagged(w1[2], g1.faction, 2, 1));
                    List<Entity> heir = MowzieKit.held(g1.faction, CHIEF_SLOT, 2);
                    check("an heir, tagged (faction, -1, 2) (" + heir.size() + ")", heir.size() == 1 && heir.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi h = (EntityUmvuthi) heir.get(0);
                    check("at the throne (" + h.position().distanceTo(throne[0]) + " blocks off)", h.position().distanceTo(throne[0]) < 5);
                    check("facing direction 3 (" + h.getDirectionData() + ")", h.getDirectionData() == 3);
                    check("restricted to the throne", h.hasRestriction() && h.getRestrictCenter().equals(BlockPos.containing(throne[0])));
                    check("no trade memory (" + h.getTradedPlayers() + ")", h.getTradedPlayers().isEmpty());
                    Camps camps = MowzieKit.camps();
                    check("two heirs and two masks in all (" + (camps.heirs - counters[0]) + ", " + (camps.masks - counters[1]) + ")",
                            camps.heirs == counters[0] + 2 && camps.masks == counters[1] + 2);
                    check("the pair was held once (" + (camps.heldPairs - counters[2]) + ")", camps.heldPairs == counters[2] + 1);
                    record(test, "heir", "G3 plain seat at the fire pit; G2 stale with no warrior; G1 held for a player at the throne, then took the 5-block "
                            + "warrior (155, 175 and 190 blocks from the player, the farthest held)");
                })
                .thenSucceed();
    }

    /**
     * An Umvuthi heir seated in place replaces the old chief (§7 test 30, the seat in place; MZB-410, MZB-431). Two
     * scripted groves, each with an adopted old Umvuthi (generation 1) and an adopted warrior (slot 0). G4: the old
     * Umvuthi, with a memory and 10 health, has a REMOVE of its generation held for a player 150 blocks off (a second
     * body: its REMOVE is held). The pair (MASK, and SEAT_HEIR naming generation 1 in L_A, with a throne) is played at
     * once: the heir (generation 2) stands where the old one stood at full health with no trade memory, the old body is
     * gone, the REMOVE is answered STALE, no CAMP_DEATH is booked, the warrior is taken (the MASK answer names it) and
     * the seated-in-place, heir and mask counters each rise by one. G5: the old Umvuthi has a player as its target (set
     * every tick) and a player hit it: the pair is held (one held pair, no answer, the warrior home) until the fight
     * is 600 ticks old, then played in place (DONE, the heir at the old position, the old gone).
     */
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "umvuthi_heir_in_place", skyAccess = true)
    public static void anUmvuthiHeirSeatedInPlaceReplacesTheOldChief(GameTestHelper helper) {
        String test = "anUmvuthiHeirSeatedInPlaceReplacesTheOldChief";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g4 = groveAt(far, 0, 0), g5 = groveAt(far, 1, 0);
        EntityUmvuthi[] old = new EntityUmvuthi[2];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[2];
        ServerPlayer[] p = new ServerPlayer[2];
        Vec3[] oldAt = new Vec3[2];
        long[] op = new long[8];
        long[] counters = new long[5];
        long[] hitAt = new long[1];
        boolean[] landed = new boolean[1];

        helper.startSequence()
                .thenExecute(() -> {
                    g4.force();
                    g5.force();
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g4.lay().declare();
                    g5.lay().declare();
                    old[0] = MowzieKit.umvuthi(level, g4.at(-8, 0));
                    w[0] = MowzieKit.warrior(level, g4.at(10, 0));
                    old[1] = MowzieKit.umvuthi(level, g5.at(-8, 0));
                    w[1] = MowzieKit.warrior(level, g5.at(10, 0));
                    p[0] = MowzieKit.player(helper, off(g4, 0, 150), "mowzie-seat-p4");
                    p[1] = MowzieKit.player(helper, off(g5, 0, 50), "mowzie-seat-p5");
                    MowzieKit.everyTick(helper, () -> {
                        if (hitAt[0] != 0 && old[1].isAlive() && !old[1].isRemoved())
                            old[1].setTarget(p[1]);
                    });
                })
                .thenWaitUntil(censused(g4, g5))
                .thenExecute(() -> {
                    op[0] = MowzieKit.reconcile(g4.faction, epochOf(g4), List.of(
                            CampLine.of(CHIEF_SLOT, 1, UMVUTHI, ADOPT, CHIEF, old[0].getUUID()), CampLine.of(0, 1, WARRIOR, ADOPT, 0, w[0].getUUID())));
                    op[1] = MowzieKit.reconcile(g5.faction, epochOf(g5), List.of(
                            CampLine.of(CHIEF_SLOT, 1, UMVUTHI, ADOPT, CHIEF, old[1].getUUID()), CampLine.of(0, 1, WARRIOR, ADOPT, 0, w[1].getUUID())));
                })
                .thenWaitUntil(() -> {
                    MowzieKit.answered(g4.faction, op[0], 2);
                    MowzieKit.answered(g5.faction, op[1], 2);
                })
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("G4's ADOPT", ConflictKit.outcomes(g4.faction, op[0]), DONE);
                    MowzieKit.allOutcomes("G5's ADOPT", ConflictKit.outcomes(g5.faction, op[1]), DONE);
                    Camps camps = MowzieKit.camps();
                    counters[0] = camps.heirs;
                    counters[1] = camps.masks;
                    counters[2] = camps.seatedInPlace;
                    counters[3] = camps.heldRemovals;
                    counters[4] = camps.heldPairs;
                    // G4: the old chief has a memory and is hurt; a REMOVE of it is held for the player near it
                    GroveKind.INSTANCE.chiefBody(old[0], new CampKind.ChiefBody(0f, 0, List.of(UUID.randomUUID())));
                    old[0].setHealth(10f);
                    oldAt[0] = old[0].position();
                    op[2] = MowzieKit.reconcile(g4.faction, epochOf(g4), List.of(CampLine.of(CHIEF_SLOT, 1, UMVUTHI, REMOVE, CHIEF)));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    check("the REMOVE is held (" + (camps.heldRemovals - counters[3]) + ")", camps.heldRemovals == counters[3] + 1);
                    check("it has no answer", unanswered(g4.faction, op[2]));
                    check("the old Umvuthi is a second body", camps.secondBody(old[0]));
                    check("it remembers a trade and is hurt", !old[0].getTradedPlayers().isEmpty() && old[0].getHealth() <= 10f);
                    op[3] = MowzieKit.reconcile(g4.faction, epochOf(g4), g4.at(0, 8), 3f, List.of(
                            CampLine.of(0, 0, WARRIOR, MASK, 0), new CampLine(CHIEF_SLOT, 2, UMVUTHI, SEAT_HEIR, CHIEF, 1L, 0L)));
                })
                .thenWaitUntil(() -> {
                    MowzieKit.answered(g4.faction, op[3], 2);
                    MowzieKit.answered(g4.faction, op[2], 1);
                })
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("G4's pair", ConflictKit.outcomes(g4.faction, op[3]), DONE);
                    check("the held REMOVE is STALE", ConflictKit.outcomes(g4.faction, op[2]).get(0) == STALE);
                    Map<Integer, MowzieKit.Answered> a = MowzieKit.answeredLines(g4.faction, op[3]);
                    check("the MASK names the warrior (slot 0, generation 1)", a.get(0).slot() == 0 && a.get(0).generation() == 1);
                    List<Entity> heir = MowzieKit.held(g4.faction, CHIEF_SLOT, 2);
                    check("an heir, tagged (faction, -1, 2) (" + heir.size() + ")", heir.size() == 1 && heir.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi h = (EntityUmvuthi) heir.get(0);
                    check("it stands where the old one stood (" + h.position().distanceTo(oldAt[0]) + " blocks off)", h.position().distanceTo(oldAt[0]) < 1.5);
                    check("at full health (" + h.getHealth() + " of " + h.getMaxHealth() + ")", h.getHealth() >= h.getMaxHealth() - 0.5f);
                    check("with no trade memory", h.getTradedPlayers().isEmpty());
                    check("the old body is gone", old[0].isRemoved());
                    check("the warrior is taken", w[0].isRemoved());
                    check("no CAMP_DEATH was booked", ConflictKit.campDeaths(g4.faction).isEmpty());
                    Camps camps = MowzieKit.camps();
                    check("seated in place, one heir, one mask (" + (camps.seatedInPlace - counters[2]) + ", " + (camps.heirs - counters[0]) + ", "
                            + (camps.masks - counters[1]) + ")", camps.seatedInPlace == counters[2] + 1 && camps.heirs == counters[0] + 1
                            && camps.masks == counters[1] + 1);
                    // G5: a fight a player began holds the seat
                    oldAt[1] = old[1].position();
                    hitAt[0] = level.getGameTime();
                    old[1].setTarget(p[1]);
                    landed[0] = old[1].hurt(level.damageSources().playerAttack(p[1]), 1f);
                    if (landed[0]) {
                        check("the hit reached the fight watch", FightWatch.recent(old[1], level.getGameTime()));
                    } else {
                        FightWatch.hit(old[1], level.getGameTime());
                        record(test, "fallback", "the Umvuthi refused the 1 point hit; the fight watch was told as its event does");
                    }
                })
                .thenIdle(100)
                .thenExecute(() -> {
                    op[4] = MowzieKit.reconcile(g5.faction, epochOf(g5), g5.at(0, 8), 3f, List.of(
                            CampLine.of(0, 0, WARRIOR, MASK, 0), new CampLine(CHIEF_SLOT, 2, UMVUTHI, SEAT_HEIR, CHIEF, 1L, 0L)));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    check("the pair is held (" + (camps.heldPairs - counters[4]) + ")", camps.heldPairs == counters[4] + 1);
                    check("it has no answer", unanswered(g5.faction, op[4]));
                    check("the warrior is home and the old Umvuthi stands", w[1].isAlive() && old[1].isAlive()
                            && MowzieKit.tagged(old[1], g5.faction, CHIEF_SLOT, 1));
                    check("the old Umvuthi has the player as its target", old[1].getTarget() == p[1]);
                })
                .thenWaitUntil(() -> check("590 ticks after the hit (" + (level.getGameTime() - hitAt[0]) + ")", level.getGameTime() >= hitAt[0] + 590))
                .thenExecute(() -> check("still held at 590 ticks", unanswered(g5.faction, op[4]) && old[1].isAlive()))
                .thenWaitUntil(() -> MowzieKit.answered(g5.faction, op[4], 2))
                .thenExecute(() -> {
                    long after = level.getGameTime() - hitAt[0];
                    check("played when the fight was 600 ticks old (" + after + ")", after >= 600 && after <= 640);
                    MowzieKit.allOutcomes("G5's pair", ConflictKit.outcomes(g5.faction, op[4]), DONE);
                    List<Entity> heir = MowzieKit.held(g5.faction, CHIEF_SLOT, 2);
                    check("an heir, tagged (faction, -1, 2) (" + heir.size() + ")", heir.size() == 1);
                    check("it stands where the old one stood", heir.get(0).position().distanceTo(oldAt[1]) < 1.5);
                    check("the old body is gone and the warrior taken", old[1].isRemoved() && w[1].isRemoved());
                    check("no CAMP_DEATH was booked", ConflictKit.campDeaths(g5.faction).isEmpty());
                    check("two seats in place in all (" + (MowzieKit.camps().seatedInPlace - counters[2]) + ")", MowzieKit.camps().seatedInPlace == counters[2] + 2);
                    record(test, "in place", "G4 over a held REMOVE (stale, no death); G5 held until the fight was " + after + " ticks old"
                            + (landed[0] ? "" : " (the hit did not land: the fight watch was told directly)"));
                })
                .thenSucceed();
    }
}
