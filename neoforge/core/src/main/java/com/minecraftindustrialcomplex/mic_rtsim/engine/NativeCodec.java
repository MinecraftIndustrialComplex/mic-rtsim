package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The two byte formats {@link NativeEngine} exchanges with rtsim-jni besides the boundary buffers:
 * test and debug <b>orders</b> going in ({@code nOrder}) and the <b>received log</b> coming out
 * ({@code nDrainReceived}). Pure Java, so it is unit-tested without loading the library. The Rust
 * side (crates/rtsim-jni) mirrors it.
 *
 * <p><b>Orders</b>, little-endian, no padding or alignment. One {@code nOrder} call carries one or
 * more orders and they are one unit (all land in the same step's command buffer, or all wait). Each:
 * u16 opcode, u16 reserved 0, u32 size (the whole order, header included), then the body. A string
 * is u32 byte length, then UTF-8; the empty string stands for null (none). Opcodes 8 to 15 are
 * reserved.
 * <ul>
 *   <li>1 SPAWN_GROUP: u64 agent, u32 epoch, str groupKind, str dimension, f64 x, f64 y, f64 z,
 *       u32 memberCount, then per member: u32 index, f32 health, i32 bodyA, i32 bodyB, u32 itemCount,
 *       then per item: u32 slot, i32 count, str item.</li>
 *   <li>2 DESPAWN_GROUP: u64 agent, u32 epoch, u32 reason.</li>
 *   <li>3 STEER_GROUP: u64 agent, u32 epoch, f32 speed, f64 x, f64 y, f64 z.</li>
 *   <li>4 TRANSFER_TO_CONTAINER: u64 opId (0: the engine allocates one), str dimension, str item,
 *       i32 x, i32 y, i32 z, i32 count.</li>
 *   <li>5 SAVE_AT_NEXT_STEP: no body.</li>
 *   <li>6 REWIND_CMD_SEQ: u64 next.</li>
 *   <li>7 FAIL: no body. The step that writes it panics on purpose (an engine opened with
 *       {@code FLAG_TEST} only).</li>
 *   <li>8 INDUSTRY_THREAT: u64 id, u32 source, u32 class, u32 count, u32 target kind, f64 strength,
 *       u64 target, u64 gameTime (rtsim-jni's orders.rs; GameTests send it).</li>
 *   <li>9 INDUSTRY_COMMAND: u32 names, then per name: u32 dictionary kind, u32 Java's id, str name
 *       (the Java dictionary entries the record uses); then an industry command record exactly as
 *       docs/industry-boundary.md §4 lays it out (its cmdSeq ignored), with Java's own dictionary ids
 *       (GOOD, NAME, BLOCK_STATE and dimension). The engine writes it as if its industry engine had
 *       decided it (choice W17).</li>
 *   <li>16 MARKET: u64 village, u64 asOfObsSeq, str currency, i64 treasury, i64 treasuryTarget,
 *       i32 worstNum, i32 worstDen, u32 goodCount, then per good: str item, i64 stock, i64 target,
 *       i64 anchor, i32 rNum, i32 rDen.</li>
 *   <li>17 MERCHANT_OFFERS: u64 agent, u32 epoch, u32 member, u32 offerCount, then per offer:
 *       str costA, i32 costACount, str costB ("" for none), i32 costBCount, str result,
 *       i32 resultCount, i32 maxUses, u32 flags.</li>
 *   <li>18 WRITE_GOSSIP: u64 opId, u64 village, u64 playerMost, u64 playerLeast, u32 gossipType,
 *       i32 amount, u32 report, u32 flags.</li>
 *   <li>19 SENTIMENT: u64 holder, i32 dislike, i32 friend, u32 flags (1: replace), u32 entryCount,
 *       then per entry: u64 playerMost, u64 playerLeast, i32 sentiment.</li>
 *   <li>20 PLACE_HOUSE: u64 opId, u64 village, str dimension, str template, str processors ("" for
 *       none), i32 x, i32 y, i32 z, u32 rotation, u32 mirror, u32 flags, u32 radius, u64 seed,
 *       i32 yOffset.</li>
 *   <li>21 SPAWN_RESIDENT: u64 opId, u64 village, str dimension, str body, str profession ("" for
 *       none), u32 flags, f64 x, f64 y, f64 z, u64 house.</li>
 *   <li>22 SCRIPT_VILLAGE: u64 village. The engine applies none of that village's observations to its
 *       own world and sends no phase 1 command of its own for it.</li>
 *   <li>23 P1_SPAWN_NEAR: u64 agent, u32 epoch, str dimension, f64 x, f64 y, f64 z, u32 radius,
 *       f64 searchX, f64 searchZ. Materialize the phase 1 caravan with wares nearest (searchX, searchZ)
 *       within radius blocks, as group agent/epoch, at (x, y, z) (an engine opened with
 *       {@code FLAG_TEST} only).</li>
 *   <li>24 P1_ADVANCE: u64 ticks. The phase 1 world runs this many more ticks ahead of game time from
 *       now on (an engine opened with {@code FLAG_TEST} only).</li>
 *   <li>25 DRAGON_COMMAND: as INDUSTRY_COMMAND, with a dragon command record (docs/boundary.md §6.4,
 *       its cmdSeq ignored) whose species ids are Java's (body dictionary) and whose dimension is
 *       Java's id. The engine writes it as the dragons' own (an engine opened with {@code FLAG_TEST}
 *       only; the Ice and Fire plugin's GameTests).</li>
 *   <li>27 HERD_COMMAND: as DRAGON_COMMAND, with a herd command record (docs/boundary.md §8.4): its
 *       species ids Java's (body dictionary), its dimension Java's id. The engine writes it as the
 *       herds' own (an engine opened with {@code FLAG_TEST} only; the vanilla herd adapter's
 *       GameTests).</li>
 *   <li>28 HERD_FOUND: str name, str species, u32 dimension (Java's id), i32 x, i32 z, u64 count, u64
 *       seed (0: the herds' own). A herd founded there when the engine takes the order (an engine
 *       opened with {@code FLAG_TEST} only: nothing in the game founds herds yet).</li>
 *   <li>30 WORKS_GRANT: u64 village, i64 amount. Coin (thousandths of the village's own currency) put
 *       straight into a phase 1 village's works purse (SINK.md), ledgered as minted (an engine opened
 *       with {@code FLAG_TEST} only).</li>
 *   <li>31 WORKS_RAID: u64 village, u32 tribute (0 or 1). A dragon raided the village: it buys golems
 *       first for a season, and with tribute its purse is the dragon's (an engine opened with
 *       {@code FLAG_TEST} only).</li>
 *   <li>32 TRIBUTE_START: u64 village, u64 lair. The village becomes the lair's tributary and pays a
 *       season's tribute at once (SINK.md §4.6; an engine opened with {@code FLAG_TEST} only).</li>
 *   <li>34 NEST_DAY: u64 ticks. The oil nests' day (their watch period and the unit of their waits,
 *       OILGATE.md §4.6) is this many ticks from now on; 0 restores the default (an engine opened with
 *       {@code FLAG_TEST} only; the nest GameTests).</li>
 *   <li>40 TALK_REPORT: u64 village, u16 kind, u8 severity, u8 flags, who (u8 class, u64 a, u64 b), whom
 *       (the same), i32 x, i32 z, u32 ago. One report put on a village's noticeboard (TK40; a TALK ref's
 *       class and ids, docs/boundary.md §13.2; an engine opened with {@code FLAG_TEST} only).</li>
 * </ul>
 *
 * <p><b>The received log</b>: entries of u64 handover, u64 obsSeq (0 if not journaled), u32 flags
 * (bit 0: skipped, its sequence was already applied), u32 length, then {@code length} bytes: the
 * observation record exactly as Java wrote it.
 */
final class NativeCodec {
    private NativeCodec() {}

    static final ByteOrder ORDER = ByteOrder.LITTLE_ENDIAN;

    static final int SPAWN_GROUP = 1;
    static final int DESPAWN_GROUP = 2;
    static final int STEER_GROUP = 3;
    static final int TRANSFER_TO_CONTAINER = 4;
    static final int SAVE_AT_NEXT_STEP = 5;
    static final int REWIND_CMD_SEQ = 6;
    static final int FAIL = 7;
    static final int INDUSTRY_THREAT = 8;
    static final int INDUSTRY_COMMAND = 9;
    // 10 to 15: reserved.
    static final int MARKET = 16;
    static final int MERCHANT_OFFERS = 17;
    static final int WRITE_GOSSIP = 18;
    static final int SENTIMENT = 19;
    static final int PLACE_HOUSE = 20;
    static final int SPAWN_RESIDENT = 21;
    static final int SCRIPT_VILLAGE = 22;
    static final int P1_SPAWN_NEAR = 23;
    static final int P1_ADVANCE = 24;
    static final int DRAGON_COMMAND = 25;
    // 26: SHIP_COMMAND (no Java encoder: no ships plugin yet).
    static final int HERD_COMMAND = 27;
    static final int HERD_FOUND = 28;
    // 29: CONFLICT_COMMAND (no Java encoder here).
    static final int WORKS_GRANT = 30;
    static final int WORKS_RAID = 31;
    static final int TRIBUTE_START = 32;
    // 33: left free (a gap between peers' order codes).
    static final int NEST_DAY = 34;
    // 35 to 39: left free (a gap between peers' order codes).
    static final int TALK_REPORT = 40;

    /** Bytes before an order's body: u16 opcode, u16 reserved, u32 size. */
    static final int ORDER_HEADER = 8;
    /** DESPAWN_GROUP reason the orders send: 1, a command. */
    static final int DESPAWN_REASON_COMMAND = 1;

    /** Bytes before a received entry's record: u64 handover, u64 obsSeq, u32 flags, u32 length. */
    static final int RECEIVED_HEADER = 24;
    /** Received entry flag: the record's sequence was already applied, so the engine skipped it. */
    static final int RECEIVED_SKIPPED = 1;

    /** Builds the payload of one {@code nOrder} call. */
    static final class Orders {
        private ByteBuffer b = ByteBuffer.allocate(128).order(ORDER);
        private int start = -1;

        Orders spawnGroup(ScriptedEngine.SpawnSpec s) {
            begin(SPAWN_GROUP);
            u64(s.agent());
            u32(s.epoch());
            str(s.groupKind());
            str(s.dimension());
            f64(s.x());
            f64(s.y());
            f64(s.z());
            u32(s.members().size());
            for (ScriptedEngine.MemberSpec m : s.members()) {
                u32(m.index());
                f32(m.health());
                u32(m.bodyA());
                u32(m.bodyB());
                u32(m.items().size());
                for (ScriptedEngine.ItemSpec i : m.items()) {
                    u32(i.slot());
                    u32(i.count());
                    str(i.item());
                }
            }
            return end();
        }

        Orders despawnGroup(long agent, int epoch) {
            begin(DESPAWN_GROUP);
            u64(agent);
            u32(epoch);
            u32(DESPAWN_REASON_COMMAND);
            return end();
        }

        Orders steerGroup(long agent, int epoch, double x, double y, double z, float speed) {
            begin(STEER_GROUP);
            u64(agent);
            u32(epoch);
            f32(speed);
            f64(x);
            f64(y);
            f64(z);
            return end();
        }

        Orders transferToContainer(long opId, String dimension, int x, int y, int z, String item, int count) {
            begin(TRANSFER_TO_CONTAINER);
            u64(opId);
            str(dimension);
            str(item);
            u32(x);
            u32(y);
            u32(z);
            u32(count);
            return end();
        }

        Orders saveAtNextStep() {
            begin(SAVE_AT_NEXT_STEP);
            return end();
        }

        Orders fail() {
            begin(FAIL);
            return end();
        }

        Orders rewindCommandSequence(long next) {
            begin(REWIND_CMD_SEQ);
            u64(next);
            return end();
        }

        Orders market(long village, long asOfObsSeq, String currency, long treasury, long treasuryTarget,
                      int worstNum, int worstDen, List<ScriptedEngine.GoodSpec> goods) {
            begin(MARKET);
            u64(village);
            u64(asOfObsSeq);
            str(currency);
            u64(treasury);
            u64(treasuryTarget);
            u32(worstNum);
            u32(worstDen);
            u32(goods.size());
            for (ScriptedEngine.GoodSpec g : goods) {
                str(g.item());
                u64(g.stock());
                u64(g.target());
                u64(g.anchor());
                u32(g.rNum());
                u32(g.rDen());
            }
            return end();
        }

        Orders merchantOffers(long agent, int epoch, int member, List<ScriptedEngine.OfferSpec> offers) {
            begin(MERCHANT_OFFERS);
            u64(agent);
            u32(epoch);
            u32(member);
            u32(offers.size());
            for (ScriptedEngine.OfferSpec o : offers) {
                str(o.costA());
                u32(o.costACount());
                str(o.costB());
                u32(o.costBCount());
                str(o.result());
                u32(o.resultCount());
                u32(o.maxUses());
                u32(o.flags());
            }
            return end();
        }

        Orders writeGossip(long opId, long village, UUID player, int gossipType, int amount, int report, int flags) {
            begin(WRITE_GOSSIP);
            u64(opId);
            u64(village);
            u64(player.getMostSignificantBits());
            u64(player.getLeastSignificantBits());
            u32(gossipType);
            u32(amount);
            u32(report);
            u32(flags);
            return end();
        }

        Orders sentiment(long holder, int dislike, int friend, boolean replace, Map<UUID, Integer> entries) {
            begin(SENTIMENT);
            u64(holder);
            u32(dislike);
            u32(friend);
            u32(replace ? Cmd.Sentiment.FLAG_REPLACE : 0);
            u32(entries.size());
            for (Map.Entry<UUID, Integer> e : entries.entrySet()) {
                u64(e.getKey().getMostSignificantBits());
                u64(e.getKey().getLeastSignificantBits());
                u32(e.getValue());
            }
            return end();
        }

        Orders placeHouse(long opId, long village, String dimension, String template, String processors, int x, int y, int z,
                          int rotation, int mirror, int flags, int radius, long seed, int yOffset) {
            begin(PLACE_HOUSE);
            u64(opId);
            u64(village);
            str(dimension);
            str(template);
            str(processors);
            u32(x);
            u32(y);
            u32(z);
            u32(rotation);
            u32(mirror);
            u32(flags);
            u32(radius);
            u64(seed);
            u32(yOffset);
            return end();
        }

        Orders spawnResident(long opId, long village, String dimension, String body, String profession, int flags,
                             double x, double y, double z, long house) {
            begin(SPAWN_RESIDENT);
            u64(opId);
            u64(village);
            str(dimension);
            str(body);
            str(profession);
            u32(flags);
            f64(x);
            f64(y);
            f64(z);
            u64(house);
            return end();
        }

        Orders scriptVillage(long village) {
            begin(SCRIPT_VILLAGE);
            u64(village);
            return end();
        }

        Orders phase1SpawnNear(long agent, int epoch, String dimension, double x, double y, double z, int radius,
                               double searchX, double searchZ) {
            begin(P1_SPAWN_NEAR);
            u64(agent);
            u32(epoch);
            str(dimension);
            f64(x);
            f64(y);
            f64(z);
            u32(radius);
            f64(searchX);
            f64(searchZ);
            return end();
        }

        Orders phase1Advance(long ticks) {
            begin(P1_ADVANCE);
            u64(ticks);
            return end();
        }

        Orders industryThreat(long id, int source, int threatClass, int count, int targetKind, double strength, long target,
                              long gameTime) {
            begin(INDUSTRY_THREAT);
            u64(id);
            u32(source);
            u32(threatClass);
            u32(count);
            u32(targetKind);
            f64(strength);
            u64(target);
            u64(gameTime);
            return end();
        }

        Orders worksGrant(long village, long amount) {
            begin(WORKS_GRANT);
            u64(village);
            u64(amount);
            return end();
        }

        Orders worksRaid(long village, boolean tribute) {
            begin(WORKS_RAID);
            u64(village);
            u32(tribute ? 1 : 0);
            return end();
        }

        Orders tributeStart(long village, long lair) {
            begin(TRIBUTE_START);
            u64(village);
            u64(lair);
            return end();
        }

        Orders nestDay(long ticks) {
            begin(NEST_DAY);
            u64(ticks);
            return end();
        }

        Orders talkReport(long village, int kind, int severity, int flags, NativeEngine.TalkActor who,
                          NativeEngine.TalkActor whom, int x, int z, long ago) {
            begin(TALK_REPORT);
            u64(village);
            u16(kind);
            u8(severity);
            u8(flags);
            actor(who);
            actor(whom);
            u32(x);
            u32(z);
            u32((int) ago);
            return end();
        }

        private void actor(NativeEngine.TalkActor a) {
            u8(a.cls());
            u64(a.a());
            u64(a.b());
        }

        Orders industryCommand(List<NativeEngine.DictEntry> names, byte[] record) {
            begin(INDUSTRY_COMMAND);
            u32(names.size());
            for (NativeEngine.DictEntry n : names) {
                u32(n.kind());
                u32(n.id());
                str(n.name());
            }
            room(record.length);
            b.put(record);
            return end();
        }

        Orders dragonCommand(List<NativeEngine.DictEntry> names, byte[] record) {
            begin(DRAGON_COMMAND);
            u32(names.size());
            for (NativeEngine.DictEntry n : names) {
                u32(n.kind());
                u32(n.id());
                str(n.name());
            }
            room(record.length);
            b.put(record);
            return end();
        }

        Orders herdCommand(List<NativeEngine.DictEntry> names, byte[] record) {
            begin(HERD_COMMAND);
            u32(names.size());
            for (NativeEngine.DictEntry n : names) {
                u32(n.kind());
                u32(n.id());
                str(n.name());
            }
            room(record.length);
            b.put(record);
            return end();
        }

        Orders herdFound(String name, String species, int dimension, int x, int z, long count, long seed) {
            begin(HERD_FOUND);
            str(name);
            str(species);
            u32(dimension);
            u32(x);
            u32(z);
            u64(count);
            u64(seed);
            return end();
        }

        byte[] toBytes() {
            if (start >= 0)
                throw new IllegalStateException("an order is still open");
            byte[] out = new byte[b.position()];
            b.get(0, out);
            return out;
        }

        private void begin(int opcode) {
            if (start >= 0)
                throw new IllegalStateException("an order is still open");
            start = b.position();
            room(ORDER_HEADER);
            b.putShort((short) opcode);
            b.putShort((short) 0);
            b.putInt(0); // the size, once the body is written
        }

        private Orders end() {
            b.putInt(start + 4, b.position() - start);
            start = -1;
            return this;
        }

        private void u64(long v) {
            room(8);
            b.putLong(v);
        }

        private void u32(int v) {
            room(4);
            b.putInt(v);
        }

        private void u16(int v) {
            room(2);
            b.putShort((short) v);
        }

        private void u8(int v) {
            room(1);
            b.put((byte) v);
        }

        private void f32(float v) {
            room(4);
            b.putFloat(v);
        }

        private void f64(double v) {
            room(8);
            b.putDouble(v);
        }

        /** A string; null is the empty string, which the engine reads as none. */
        private void str(String s) {
            byte[] utf8 = (s == null ? "" : s).getBytes(StandardCharsets.UTF_8);
            room(4 + utf8.length);
            b.putInt(utf8.length);
            b.put(utf8);
        }

        private void room(int n) {
            if (b.remaining() >= n)
                return;
            ByteBuffer bigger = ByteBuffer.allocate(Math.max(b.capacity() * 2, b.position() + n)).order(ORDER);
            b.flip();
            bigger.put(b);
            b = bigger;
        }
    }

    /**
     * Decodes a received log into {@code out}.
     *
     * @throws IllegalStateException if the log is malformed (an entry runs past the end, or a record
     *                               is shorter than its own 8-byte header)
     */
    static void decodeReceived(byte[] log, List<ScriptedEngine.Received> out) {
        ByteBuffer b = ByteBuffer.wrap(log).order(ORDER);
        int at = 0;
        while (at < log.length) {
            if (log.length - at < RECEIVED_HEADER)
                throw new IllegalStateException("received log: " + (log.length - at) + " trailing bytes at " + at + ", less than an entry header");
            long handover = b.getLong(at);
            long obsSeq = b.getLong(at + 8);
            int flags = b.getInt(at + 16);
            int length = b.getInt(at + 20);
            int body = at + RECEIVED_HEADER;
            if (length < Layout.RECORD_HEADER || length > log.length - body)
                throw new IllegalStateException("received log: entry at " + at + " has length " + length
                        + " (" + (log.length - body) + " bytes left)");
            byte[] record = new byte[length];
            b.get(body, record);
            int kind = Short.toUnsignedInt(b.getShort(body + Layout.R_KIND));
            int aux = b.getInt(body + Layout.R_AUX);
            out.add(new ScriptedEngine.Received(handover, kind, aux, obsSeq, (flags & RECEIVED_SKIPPED) != 0, record));
            at = body + length;
        }
    }

    static List<ScriptedEngine.Received> decodeReceived(byte[] log) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        decodeReceived(log, out);
        return out;
    }
}
