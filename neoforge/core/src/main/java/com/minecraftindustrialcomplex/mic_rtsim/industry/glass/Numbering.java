package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;

/**
 * The node numbering of one compile, as Java sent it: the position of each node, and which are STORE nodes.
 * A district keeps the numbering of its latest compile ({@code GlassCompiler.State.numbering}) and the one
 * whose IND_GRAPH_END it last sent ({@code State.sent}): the engine numbers an IND_COUNT_SET and reads an
 * IND_COUNT_SAMPLE in the numbers of the compile it holds, and the two differ while a newer compile waits
 * in the send queue (SECTIONS.md §4.2).
 */
final class Numbering {
    final int compile;
    /** Per node: its position, {@code BlockPos.asLong}, in the compile's frame. */
    final long[] pos;
    /** Per node: whether it is a STORE node. */
    final boolean[] store;
    /** 0 the level; else a Sable sub-level's id (positions of a moving frame are not the world's). */
    final long frame;
    private Long2ObjectOpenHashMap<int[]> moversAt;
    private Long2ObjectOpenHashMap<int[]> storesAt;

    Numbering(int compile, long[] pos, boolean[] store, long frame) {
        this.compile = compile;
        this.pos = pos;
        this.store = store;
        this.frame = frame;
    }

    /** The nodes that are not stores at {@code p}, ascending (a packager is two). */
    int[] moversAt(long p) {
        index();
        int[] r = moversAt.get(p);
        return r == null ? NONE : r;
    }

    /** The STORE nodes at {@code p}, ascending. */
    int[] storesAt(long p) {
        index();
        int[] r = storesAt.get(p);
        return r == null ? NONE : r;
    }

    private static final int[] NONE = new int[0];

    /** Built on first use: only a district whose count set Java took asks. */
    private void index() {
        if (moversAt != null)
            return;
        Long2ObjectOpenHashMap<int[]> m = new Long2ObjectOpenHashMap<>(), s = new Long2ObjectOpenHashMap<>();
        for (int i = 0; i < pos.length; i++) {
            Long2ObjectOpenHashMap<int[]> into = store[i] ? s : m;
            int[] old = into.get(pos[i]);
            if (old == null) {
                into.put(pos[i], new int[] {i});
            } else {
                int[] grown = java.util.Arrays.copyOf(old, old.length + 1);
                grown[old.length] = i;
                into.put(pos[i], grown);
            }
        }
        storesAt = s;
        moversAt = m;
    }
}
