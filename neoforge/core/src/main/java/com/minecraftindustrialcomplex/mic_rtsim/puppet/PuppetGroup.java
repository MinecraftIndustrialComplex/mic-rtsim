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
        /** The items it dropped when it died (LivingDropsEvent), which carry its DIED. */
        public java.util.List<net.minecraft.world.entity.item.ItemEntity> pendingDrops;
        /** Its own cargo as the bridge last left it; anything else that changes it is reported (CARGO_CHANGED). */
        public java.util.Map<net.minecraft.world.item.Item, Integer> expectedCargo;
        /** The offers the engine set for this member (MERCHANT_OFFERS), in order; null when none. */
        public java.util.List<com.minecraftindustrialcomplex.mic_rtsim.market.PuppetOffers.Spec> offers;

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
    /**
     * The last STEER_GROUP target as the engine sent it, and the image of it the leader was last sent
     * toward (TP11): Toroidal World and Deep Time's wrap engine fold a leader that crosses an edge back
     * into the domain without an event, so each pass the target is taken again as its image nearest the
     * leader, and the leader re-steered when that image moved. Null when never steered.
     */
    public net.minecraft.world.phys.Vec3 steerTarget, steerImage;
    public float steerSpeed;

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
