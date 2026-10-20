package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import java.nio.ByteBuffer;

/**
 * The engine as the bridge sees it: four buffers and a handful of calls (PLAN.md §1.1, §4.5).
 *
 * <p><b>The boundary contract</b> (full layout in {@code docs/boundary.md}):
 * <ul>
 *   <li>Two observation buffers (Java writes, the engine reads) and two command buffers (the engine
 *       writes, Java reads), allocated by the engine, reused forever, seen by Java as direct
 *       {@code ByteBuffer}s.</li>
 *   <li>Hand-overs are numbered h = 1, 2, 3 ... per session. Hand-over h puts its observations in
 *       observation buffer {@code h & 1}; the step it starts writes its commands into command buffer
 *       {@code h & 1}.</li>
 *   <li>Each buffer's header has a {@code published} and a {@code consumed} word, accessed only with
 *       acquire and release ({@link Seq}). The producer publishes h after writing; the consumer marks
 *       h consumed when it is done.</li>
 *   <li><b>One hand-over per tick</b>, at most: {@link #submit} is the one call per tick, and it
 *       returns at once. At most one step is in flight. Java submits h only when step h-1 has
 *       published its commands and Java has finished applying the commands of step h-2.</li>
 *   <li><b>The bridge never waits.</b> If the step is not done, Java applies nothing new that tick
 *       and keeps adding observations to the open buffer. Commands are at least one tick late.</li>
 * </ul>
 *
 * <p>Implementations: {@code NativeEngine}, the Rust engine behind rtsim-jni on mic_native's
 * background pool, and {@code StubEngine} (pure Java, simulates nothing, for tests).
 */
public interface Engine extends AutoCloseable {

    /** Observation buffer 0 or 1, as allocated by the engine. Called once per session. */
    ByteBuffer observationBuffer(int index);

    /** Command buffer 0 or 1, as allocated by the engine. Called once per session. */
    ByteBuffer commandBuffer(int index);

    /**
     * Once per session, before the first hand-over: the engine loads its pages and reports what they
     * include. {@code lastAppliedCmdSeq} is the journal's; the engine's commands this session carry
     * sequences above it. May block (it runs at server start, not on a tick).
     */
    SessionStart start(long lastAppliedCmdSeq);

    /**
     * Starts step {@code handover}: read observation buffer {@code handover & 1}, mark it consumed,
     * simulate, write command buffer {@code handover & 1}, publish it. Returns at once; the step runs
     * on the engine's own worker (the host's background pool).
     */
    void submit(long handover);

    /**
     * Minecraft is saving, and its journal holds every observation up to {@code throughObsSeq}. The
     * engine snapshots its pages after applying exactly the observations up to that sequence (no
     * further: pages ahead of the world save are Proposal P29's case), writes them off-thread, and once
     * they are on disk a later command header acknowledges them. A signal only: never blocks.
     */
    void requestSave(long throughObsSeq);

    /** A one-line description for logs and {@code /rtsim perf}. */
    String describe();

    /** Ends the session. May block briefly to let a step in flight finish and pages be written. */
    @Override
    void close();
}
