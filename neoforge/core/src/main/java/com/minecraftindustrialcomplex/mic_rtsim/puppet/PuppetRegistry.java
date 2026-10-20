package com.minecraftindustrialcomplex.mic_rtsim.puppet;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The live puppet registry for this session (PLAN.md §6.1): the (id, epoch) and member UUIDs of every
 * puppet the bridge spawned and has not yet captured. A tagged entity that is not in here is a stale
 * copy. Server thread only.
 */
public final class PuppetRegistry {
    private final Map<Long, PuppetGroup> byAgent = new HashMap<>();
    private final Map<UUID, PuppetGroup> byUuid = new HashMap<>();

    public void add(PuppetGroup group) {
        if (byAgent.putIfAbsent(group.agent, group) != null)
            throw new IllegalStateException("agent " + group.agent + " is already embodied");
        for (PuppetGroup.Member m : group.members)
            byUuid.put(m.uuid, group);
    }

    public void remove(PuppetGroup group) {
        if (byAgent.get(group.agent) == group)
            byAgent.remove(group.agent);
        for (PuppetGroup.Member m : group.members)
            byUuid.remove(m.uuid, group);
    }

    /** A member left the group (taken, dead): its UUID no longer belongs to a puppet. */
    public void forgetMember(PuppetGroup group, PuppetGroup.Member member) {
        member.gone = true;
        byUuid.remove(member.uuid, group);
        if (group.liveMembers() == 0)
            remove(group);
    }

    public PuppetGroup byAgent(long agent) {
        return byAgent.get(agent);
    }

    public PuppetGroup byUuid(UUID uuid) {
        return byUuid.get(uuid);
    }

    /**
     * The group a tagged entity really belongs to, or null when it is a stale copy: no live group for
     * its id, another epoch, or a UUID that is not one of the group's live members.
     */
    public PuppetGroup live(AgentTag tag, UUID uuid) {
        PuppetGroup g = byAgent.get(tag.agent());
        if (g == null || g.epoch != tag.epoch())
            return null;
        PuppetGroup.Member m = g.member(uuid);
        return m == null || m.gone ? null : g;
    }

    public Collection<PuppetGroup> groups() {
        return byAgent.values();
    }

    /** A copy, for loops that capture or remove groups. */
    public List<PuppetGroup> snapshot() {
        return new ArrayList<>(byAgent.values());
    }

    /** Fills {@code into} (cleared first) with the live groups: the per-tick pass reuses one list. */
    public List<PuppetGroup> snapshotInto(List<PuppetGroup> into) {
        into.clear();
        into.addAll(byAgent.values());
        return into;
    }

    public int size() {
        return byAgent.size();
    }
}
