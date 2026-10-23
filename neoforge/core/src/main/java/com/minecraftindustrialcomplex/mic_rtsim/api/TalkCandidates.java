package com.minecraftindustrialcomplex.mic_rtsim.api;

import net.minecraft.network.chat.Component;

import java.util.ArrayList;
import java.util.List;

/**
 * The topics a talker could speak about now, each with a weight. The core drops a topic that has no
 * lines in the talker's voice and picks one by weight (TALK.md §5.6).
 */
public final class TalkCandidates {
    /** One candidate: a topic (a line key suffix such as {@code mood.liked}), its weight and its arguments. */
    public record Candidate(String topic, double weight, Object[] args) {}

    private final List<Candidate> list = new ArrayList<>();

    /**
     * Adds a topic. String arguments become {@code Component.literal}; Components pass through; numbers
     * and booleans stay as they are; anything else goes in as its string.
     */
    public void add(String topic, double weight, Object... args) {
        if (!(weight > 0) || Double.isInfinite(weight))
            return;
        Object[] out = new Object[args.length];
        for (int i = 0; i < args.length; i++) {
            Object a = args[i];
            out[i] = a instanceof Component || a instanceof Number || a instanceof Boolean ? a
                    : Component.literal(String.valueOf(a));
        }
        list.add(new Candidate(topic, weight, out));
    }

    /** What was added, in order. */
    public List<Candidate> list() {
        return list;
    }
}
