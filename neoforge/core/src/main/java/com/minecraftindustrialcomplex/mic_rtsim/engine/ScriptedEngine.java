package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Engine;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The test and debug surface of an engine: what the GameTests, the unit tests and {@code /rtsim stub}
 * use on top of {@link Engine}. Both engines implement it, the pure-Java {@link StubEngine} and the
 * Rust engine behind {@link NativeEngine}, so every test runs against either.
 *
 * <p>A scripted command is an <b>order</b>: the engine writes it into the command buffer of the next
 * step that has room for it, with its own command sequence and dictionary ids. Orders queue in the
 * order they are given. The inspection calls read the engine's memory; the received log keeps the
 * last 10,000 observation records (TICK records are not kept).
 */
public interface ScriptedEngine extends Engine {

    /** One observation record the engine received, for tests and {@code /rtsim stub}. */
    record Received(long handover, int kind, int aux, long obsSeq, boolean skipped, byte[] bytes) {
        /** The record's bytes, little-endian, offsets as in {@code Obs}. */
        public ByteBuffer view() {
            return ByteBuffer.wrap(bytes).order(Layout.ORDER);
        }
    }

    /** One cargo stack of a scripted member: (slot, item name, count). */
    record ItemSpec(int slot, String item, int count) {}

    /** One member of a scripted SPAWN_GROUP. Health 0 is full; body bits -1 let the body roll them. */
    record MemberSpec(int index, float health, int bodyA, int bodyB, List<ItemSpec> items) {
        public MemberSpec {
            items = List.copyOf(items);
        }

        public static MemberSpec plain(int index) {
            return new MemberSpec(index, 0f, -1, -1, List.of());
        }
    }

    /** One scripted SPAWN_GROUP, for {@link #spawnTogether}. */
    record SpawnSpec(long agent, int epoch, String groupKind, String dimension, double x, double y, double z,
                     List<MemberSpec> members) {
        public SpawnSpec {
            members = List.copyOf(members);
        }
    }

    /** One good of a scripted MARKET, in rtsim-trade units (thousandths; millionths of a coin per item). */
    record GoodSpec(String item, long stock, long target, long anchor, int rNum, int rDen) {}

    /** One offer of a scripted MERCHANT_OFFERS; costB null for none. */
    record OfferSpec(String costA, int costACount, String costB, int costBCount, String result, int resultCount,
                     int maxUses, int flags) {}

    /** Orders a SPAWN_GROUP. */
    void spawnGroup(long agent, int epoch, String groupKind, String dimension, double x, double y, double z,
                    List<MemberSpec> members);

    /** Orders several spawns as one unit: they all land in the same step's command buffer, or all wait. */
    void spawnTogether(SpawnSpec... spawns);

    /** Orders a DESPAWN_GROUP (reason: command). */
    void despawnGroup(long agent, int epoch);

    /** Orders a STEER_GROUP. */
    void steerGroup(long agent, int epoch, double x, double y, double z, float speed);

    /**
     * Orders a TRANSFER_TO_CONTAINER of one stack. The engine does not check its own op ledger first,
     * so a test can make it issue an op id Java has already applied (what an engine rolled back by a
     * crash does).
     */
    void transferToContainer(long opId, String dimension, int x, int y, int z, String item, int count);

    // ---- phase 1 orders -------------------------------------------------------------------------

    /**
     * A test scripts this village's phase 1 commands: the engine applies none of its observations to
     * its own world and sends no phase 1 command of its own for it. The stub, which decides nothing,
     * ignores it.
     */
    void scriptVillage(long village);

    /**
     * Orders a MARKET: a village's treasury and goods.
     *
     * @param asOfObsSeq the observation sequence the figures are as of
     * @param currency   the coin item
     * @param goods      stock, target and anchor price per good, in rtsim-trade units
     */
    void market(long village, long asOfObsSeq, String currency, long treasury, long treasuryTarget, int worstNum, int worstDen,
                List<GoodSpec> goods);

    /** Orders a MERCHANT_OFFERS: the offers one group member's merchant menu shows. */
    void merchantOffers(long agent, int epoch, int member, List<OfferSpec> offers);

    /**
     * Orders a WRITE_GOSSIP: gossip about {@code player} written into a vanilla village's gossip (type 1
     * to 5 as in {@code Cmd.WriteGossip.GOSSIP}). {@code opId} makes each villager take it at most once.
     */
    void writeGossip(long opId, long village, UUID player, int gossipType, int amount, int report, int flags);

    /**
     * Orders a SENTIMENT: a holder's (a village's) feelings toward players, on a scale of -128 to 127.
     *
     * @param replace true for a full snapshot (players not listed are neutral), false to update the last one
     */
    void sentiment(long holder, int dislike, int friend, boolean replace, Map<UUID, Integer> entries);

    /** Orders a PLACE_HOUSE: a house template placed for a village. {@code processors} may be null. */
    void placeHouse(long opId, long village, String dimension, String template, String processors, int x, int y, int z,
                    int rotation, int mirror, int flags, int radius, long seed, int yOffset);

    /** Orders a SPAWN_RESIDENT: a resident of a village in a house. {@code profession} may be null. */
    void spawnResident(long opId, long village, String dimension, String body, String profession, int flags,
                       double x, double y, double z, long house);

    /**
     * Makes the next ordered command carry {@code next} as its sequence, as an engine whose counter went
     * backwards would. Java must skip anything at or below its last applied one.
     */
    void rewindCommandSequence(long next);

    /** Saves the pages at the end of the next step, whatever it holds. */
    void requestSaveAtNextStep();

    /**
     * Makes the engine fail on purpose at its next step (the native engine: a panic in the step),
     * after which every command buffer comes back empty with {@code CMD_FLAG_FAILED}. Tests only.
     */
    void failForTest();

    /** The name behind an id of Java's dictionary, as the engine received it (null if never). */
    String javaDictName(int id);

    /**
     * How many times an event's op id (TAKEN, DIED, TRADE, CARGO_CHANGED) arrived not skipped by
     * sequence: the engine applies it once, so a count above 1 is a resend the op id caught. From the
     * received log (the last 10,000 records).
     */
    default int eventOpCount(long opId) {
        int n = 0;
        for (Received r : received()) {
            if (r.skipped())
                continue;
            int at = switch (r.kind()) {
                case com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.TAKEN -> com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Taken.OP_ID;
                case com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.DIED -> com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Died.OP_ID;
                case com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.TRADE -> com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Trade.OP_ID;
                case com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.CARGO_CHANGED -> com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.CargoChanged.OP_ID;
                default -> -1;
            };
            if (at > 0 && r.bytes().length >= at + 8 && r.view().getLong(at) == opId)
                n++;
        }
        return n;
    }

    /** The received log, oldest first. */
    List<Received> received();

    /** The received records of one kind, oldest first. */
    List<Received> received(int kind);

    /** The engine's ledger for one store: the sum of the TRANSFER amounts applied to it. */
    long ledger(long store);

    /** True when the engine has applied (or heard Java applied) this op id. */
    boolean opApplied(long opId);

    /** Steps that have finished. */
    long steps();

    /** Journaled records skipped because their sequence was already applied. */
    long staleSkipped();

    /** TRANSFER records refused because their op id was already applied. */
    long opDuplicates();

    /** The highest observation sequence the engine has applied. */
    long maxObsSeq();

    /** The highest observation sequence the pages on disk include: what survives a crash. */
    long pagesObsSeq();

    /** Orders (or units of orders) not yet written into a command buffer. */
    int scriptedPending();

    /** The first protocol error the engine saw, or null. */
    String protocolError();

    /**
     * Test only: blocks until every step this object submitted has run (10 s, then
     * IllegalStateException). Never call it on a tick in real play: the bridge never waits.
     */
    void awaitIdle();

    /**
     * Test only: a crash. Closes this engine WITHOUT saving and returns a new engine of the same kind
     * built from the last pages that reached disk. This object is dead afterwards.
     */
    ScriptedEngine crashAndReopen();
}
