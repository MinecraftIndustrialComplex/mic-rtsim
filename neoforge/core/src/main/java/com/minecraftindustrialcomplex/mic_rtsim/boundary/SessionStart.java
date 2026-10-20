package com.minecraftindustrialcomplex.mic_rtsim.boundary;

/**
 * What the engine tells Java once per session, before the first hand-over.
 *
 * @param pagesObsSeq the highest observation sequence the engine's saved pages include. Java moves
 *                    its next observation sequence past it, so a world save older than the engine's
 *                    pages never reuses a sequence the engine has already seen.
 * @param nextCmdSeq  the first command sequence this session's commands will carry; always above the
 *                    {@code lastAppliedCmdSeq} Java passed in.
 */
public record SessionStart(long pagesObsSeq, long nextCmdSeq) {}
