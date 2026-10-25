package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

/**
 * The engine's body epochs (docs/replay-gate.md §5.3): an unsigned 32 bits a dragon's engine side raises with each new
 * body ({@code Dragons::next_epoch}). Pure, so JUnit reaches it.
 */
final class Epochs {
    private Epochs() {}

    /**
     * Whether a DRAGON_ADOPT at epoch {@code incoming} supersedes the puppet of the same agent held at {@code held}
     * (RP-245): only a strictly newer epoch does. The engine moves a dragon to a new epoch whenever it embodies it
     * again and drops every record it later gets at an older one ({@code Dragons::embodied} matches the current epoch
     * only), so a puppet at an older epoch is a body the engine no longer has. The same or an older epoch is a repeat
     * or a late command, and keeps today's ALREADY_EMBODIED. Unsigned, as on the wire (an epoch wraps to 1, which is
     * 2^32 bodies away).
     */
    static boolean supersedes(int held, int incoming) {
        return Integer.compareUnsigned(incoming, held) > 0;
    }
}
