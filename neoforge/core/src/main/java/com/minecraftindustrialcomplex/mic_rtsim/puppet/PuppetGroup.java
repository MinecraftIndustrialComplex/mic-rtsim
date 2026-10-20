package com.minecraftindustrialcomplex.mic_rtsim.puppet;

import com.minecraftindustrialcomplex.mic_rtsim.body.GroupKind;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;

import java.util.UUID;

/**
 * One embodied agent: a group of one or more bodies that spawn, are captured and despawn as one unit
 * (C25, PLAN.md §6.1). Every member carries the group's engine id and epoch. Member 0 is the leader.
 */
public final class PuppetGroup {

    /** One body of the group. */
    public static final class Member {
        public final int index;
        public final UUID uuid;
        public Entity entity;
        /** Left the group: taken, dead or removed. Its slot stays, so indices never shift. */
        public boolean gone;
        /** State captured when the member left tracking, before the group capture could run. */
        public MemberCapture precaptured;
        /**
         * State captured at LivingDeathEvent, before the drops. The DIED is journaled at the end of the
         * tick, once the member is really dead: a later handler may still cancel the death.
         */
        public MemberCapture pendingDeath;
        public java.util.UUID pendingKiller;

        Member(int index, UUID uuid, Entity entity) {
            this.index = index;
            this.uuid = uuid;
            this.entity = entity;
        }
    }

    public final long agent;
    public final int epoch;
    public final GroupKind kind;
    public ResourceKey<Level> dimension;
    public final Member[] members;
    public final long spawnedAt;

    /** Set while the bridge itself removes the members, so the leave handler ignores those removals. */
    public boolean removingByBridge;
    /** A leave event saw a member stop being tracked; the whole group is captured at the end of the tick. */
    public int captureRequested;
    /** A player interacted with a member (a lead): check the group on the next tick, not the next pass. */
    public boolean dirty;

    public PuppetGroup(long agent, int epoch, GroupKind kind, ResourceKey<Level> dimension, Entity[] bodies, long spawnedAt) {
        this.agent = agent;
        this.epoch = epoch;
        this.kind = kind;
        this.dimension = dimension;
        this.spawnedAt = spawnedAt;
        this.members = new Member[bodies.length];
        for (int i = 0; i < bodies.length; i++)
            members[i] = new Member(i, bodies[i].getUUID(), bodies[i]);
    }

    public AgentTag tag() {
        return new AgentTag(agent, epoch);
    }

    public Member member(UUID uuid) {
        for (Member m : members)
            if (m.uuid.equals(uuid))
                return m;
        return null;
    }

    /** The leader's entity, or null when it is gone. */
    public Entity leader() {
        Member m = members[0];
        return m.gone ? null : m.entity;
    }

    public int liveMembers() {
        int n = 0;
        for (Member m : members)
            if (!m.gone)
                n++;
        return n;
    }

    @Override
    public String toString() {
        return "PuppetGroup[" + kind.name() + " agent=" + agent + " epoch=" + epoch + " members=" + liveMembers() + "/" + members.length + "]";
    }
}
