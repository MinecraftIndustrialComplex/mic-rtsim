package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

/**
 * The marches the engine last said, by party agent (MZB-468), with the agents whose march ended. A record replaces its
 * agent's path only with a newer revision; ENDED drops the path and the agent's later records; nothing here is
 * saved (UR-1). The bridge's, per session, on the server thread; kept apart from it so its rules run in a unit test.
 * Plain maps: a record arrives, it is not read per tick or per member.
 */
final class MarchTable {
    /** What {@link #offer} did with a record. */
    enum Outcome {
        /** It is the agent's march now. */
        KEPT,
        /** It was older than the held path or the agent's march had ended: dropped. */
        STALE,
        /** It ended the agent's march: the path is gone and the agent remembered. */
        ENDED
    }

    private final Map<Long, MarchPath> paths = new HashMap<>();
    private final Set<Long> ended = new HashSet<>();

    Outcome offer(MarchPath m) {
        long agent = m.agent();
        MarchPath held = paths.get(agent);
        if (ended.contains(agent) || (held != null && !m.newerThan(held)))
            return Outcome.STALE;
        if (m.ended()) {
            paths.remove(agent);
            ended.add(agent);
            return Outcome.ENDED;
        }
        paths.put(agent, m);
        return Outcome.KEPT;
    }

    MarchPath get(long agent) {
        return paths.get(agent);
    }

    boolean ended(long agent) {
        return ended.contains(agent);
    }

    int size() {
        return paths.size();
    }

    boolean isEmpty() {
        return paths.isEmpty() && ended.isEmpty();
    }

    void clear() {
        paths.clear();
        ended.clear();
    }
}
