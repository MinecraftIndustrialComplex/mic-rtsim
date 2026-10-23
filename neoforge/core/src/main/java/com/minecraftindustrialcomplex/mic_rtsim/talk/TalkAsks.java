package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The ask path (TALK.md §9.2, docs/boundary.md §13): a talker whose {@link TalkHolder} is known asks the
 * engine what its village or caravan knows, and speaks when the answer comes. Its own
 * {@link SessionPlugin} (commands 0x8A00 to 0x8AFF).
 *
 * <p><b>Asks.</b> Each open ask has an id of Java's, per session, rising, never 0. Every end of tick all
 * the open asks go to the engine as one TALK_ASK record (latest wins: a dropped record is written again
 * the next tick, and the engine remembers the ids it answered). At most {@link #MAX_OPEN} are open: a later
 * one is refused and its talker speaks at once without facts. An ask closes on its TALK_FACTS, or after
 * {@link #TIMEOUT_TICKS} (TK5), or when the session starts, stops or fails; its callback runs once with the
 * answer, or with null when there is none, and decides what to say.
 *
 * <p><b>No answer possible.</b> With no running session (rtsim off, a client, an engine that did not
 * open), or on the pure-Java stub (which never answers), {@link #begin} refuses and the talker speaks at once.
 *
 * <p>Server thread only.
 */
public final class TalkAsks implements SessionPlugin {
    public static final TalkAsks INSTANCE = new TalkAsks();

    /** Asks open at once, at most (the engine holds 256 and a later one is dropped; this keeps Java well under). */
    public static final int MAX_OPEN = 32;
    /** An ask not answered by then is closed and its talker speaks without facts (TK5). */
    public static final int TIMEOUT_TICKS = 20;

    /**
     * Test hook: while true no TALK_ASK is written, so the engine never hears of an ask and the timeout is
     * what closes it (a GameTest of "an engine that never answers", on the native engine too).
     */
    public static volatile boolean mute;

    /** What happens when an ask closes: the answer, or null when none came. */
    @FunctionalInterface
    public interface Done {
        void closed(TalkAnswer answer);
    }

    private static final class Open {
        final long id;
        final long openedAt;
        final ResourceKey<Level> dimension;
        final TalkHolder holder;
        final long playerMost, playerLeast;
        final int x, z, flags;
        final Done done;

        Open(long id, long openedAt, ResourceKey<Level> dimension, TalkHolder holder, long playerMost, long playerLeast,
             int x, int z, int flags, Done done) {
            this.id = id;
            this.openedAt = openedAt;
            this.dimension = dimension;
            this.holder = holder;
            this.playerMost = playerMost;
            this.playerLeast = playerLeast;
            this.x = x;
            this.z = z;
            this.flags = flags;
            this.done = done;
        }
    }

    private RtsimSession session;
    private long nextId = 1;
    private final Map<Long, Open> open = new LinkedHashMap<>();

    // counters for tests and the log
    public long asked, answered, timedOut, refused, dropped, lateAnswers, malformed, flushed;

    private TalkAsks() {}

    @Override
    public String name() {
        return "talk";
    }

    @Override
    public boolean ownsCommand(int kind) {
        return TalkRecords.isTalkCommand(kind);
    }

    /** How many asks are open now. */
    public int openCount() {
        return open.size();
    }

    /** True when an ask can be opened now: a running session of an engine that can answer, and room. */
    public boolean canAsk() {
        RtsimSession s = RtsimSession.current();
        return s != null && s == session && !s.failed() && !(s.engine() instanceof StubEngine) && open.size() < MAX_OPEN;
    }

    /**
     * Opens an ask for this player about this talker's holder. True when it is open: {@code done} runs later,
     * once (on the answer, the timeout, or a session start, stop or failure). False when the talker must speak
     * at once without facts, and {@code done} never runs.
     */
    public boolean begin(ServerPlayer player, Entity talker, TalkHolder holder, Done done) {
        if (!canAsk()) {
            refused++;
            return false;
        }
        ServerLevel level = (ServerLevel) talker.level();
        long id = nextId++;
        open.put(id, new Open(id, session.gameTime(), level.dimension(), holder,
                player.getUUID().getMostSignificantBits(), player.getUUID().getLeastSignificantBits(),
                talker.getBlockX(), talker.getBlockZ(), TalkRecords.ASK_NEWS, done));
        asked++;
        return true;
    }

    // ---- SessionPlugin ---------------------------------------------------------------------------

    @Override
    public void sessionStarted(RtsimSession s) {
        closeAll();
        session = s;
        nextId = 1;
        asked = answered = timedOut = refused = dropped = lateAnswers = malformed = flushed = 0;
    }

    @Override
    public void sessionStopping(RtsimSession s) {
        closeAll();
    }

    @Override
    public void sessionFailed(RtsimSession s) {
        closeAll();
    }

    /** Every open ask closes with no answer: the talks speak without facts (or are dropped by their own checks). */
    private void closeAll() {
        if (open.isEmpty())
            return;
        List<Open> all = new ArrayList<>(open.values());
        open.clear();
        for (Open o : all)
            finish(o, null);
    }

    @Override
    public boolean accept(RtsimSession s, ByteBuffer b, int at, int kind, int size, int aux, EngineNames names) {
        if (kind != TalkRecords.TALK_FACTS || s != session)
            return true;
        TalkRecords.Facts facts = TalkRecords.decodeFacts(b, at, size, aux);
        if (facts == null) {
            malformed++;
            return true;
        }
        Open o = open.get(facts.askId());
        if (o == null) {
            // Closed by the timeout already, or never ours (a session ago): nobody is waiting.
            lateAnswers++;
            return true;
        }
        // Resolved before the ask closes: a throw here still lets the talker speak, without facts.
        TalkAnswer answer = null;
        try {
            answer = TalkAnswer.resolve(facts, names);
        } catch (RuntimeException e) {
            malformed++;
            Talk.failed(this, "resolve", e);
        }
        open.remove(o.id);
        if (answer != null)
            answered++;
        finish(o, answer);
        return true;
    }

    @Override
    public void endOfTick(RtsimSession s, long now) {
        if (open.isEmpty() || s != session)
            return;
        // The timeout first: an ask this old speaks without facts and is not sent again.
        List<Open> late = null;
        for (Open o : open.values())
            if (now - o.openedAt >= TIMEOUT_TICKS)
                (late == null ? late = new ArrayList<>() : late).add(o);
        if (late != null)
            for (Open o : late) {
                open.remove(o.id);
                timedOut++;
                finish(o, null);
            }
        if (open.isEmpty() || mute)
            return;
        // One record of every open ask. Ids before direct(): a dimension is defined before its first use.
        List<TalkRecords.Ask> asks = new ArrayList<>(open.size());
        for (Open o : open.values())
            asks.add(new TalkRecords.Ask(o.id, o.playerMost, o.playerLeast, o.holder.kind(), s.dimId(o.dimension),
                    o.holder.id(), o.x, o.z, o.flags));
        int at = s.boundary().direct(TalkRecords.TALK_ASK, TalkRecords.askRecordSize(asks.size()), asks.size(), now);
        if (at < 0) {
            dropped++;
            return; // written again next tick, as every latest-wins kind is
        }
        ByteBuffer b = s.boundary().openBuffer();
        for (int i = 0; i < asks.size(); i++)
            TalkRecords.writeAsk(b, at + TalkRecords.Ask.FIRST + i * TalkRecords.Ask.SIZE, asks.get(i));
        flushed++;
    }

    private void finish(Open o, TalkAnswer answer) {
        try {
            o.done.closed(answer);
        } catch (Throwable e) {
            Talk.failed(this, "answer", e);
        }
    }

    /** Forgets everything without calling anyone back (server start and stop clear, the talks' own state goes with it). */
    public void reset() {
        open.clear();
        session = null;
    }
}
