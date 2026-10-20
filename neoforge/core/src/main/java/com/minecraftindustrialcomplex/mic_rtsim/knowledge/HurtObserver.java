package com.minecraftindustrialcomplex.mic_rtsim.knowledge;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Residents;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.player.Player;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.UUID;

/**
 * Damage to traders, llamas and ribbits (PLAN.md §8.7: assault and killing). Hits on a puppet member or
 * on a resident of an adopted village are summed per (attacker, victim) over {@link #WINDOW} ticks
 * (PLACEHOLDER) and journaled once per window as HURT, so a fight is one report and not forty
 * (choice G9). A resident's death is journaled at once (RESIDENT_KILLED); a puppet's death is DIED.
 *
 * <p>Vanilla already records a villager hurt or killed by a player as gossip
 * ({@code Villager.onReputationEventFrom}); such a record is flagged VANILLA_GOSSIP so the engine never
 * writes it into that village's gossip again (PLAN.md §8.7: nothing counts twice).
 */
public final class HurtObserver {
    /** Ticks over which hits on one victim by one attacker are summed into one HURT (PLACEHOLDER). */
    public static final int WINDOW = 100;

    private record Key(UUID victim, UUID attacker) {}

    private static final class Pending {
        long first;
        float amount;
        int hits;
        int what;
        long agent;
        int epoch;
        int member;
        long node;
        int attackerKind;
        String body;
        String profession;
        int flags;
    }

    private final RtsimSession session;
    private final VillageRegistry villages;
    private final Map<Key, Pending> pending = new LinkedHashMap<>();
    long journaled;

    public HurtObserver(RtsimSession session, VillageRegistry villages) {
        this.session = session;
        this.villages = villages;
    }

    public long journaled() {
        return journaled;
    }

    public int pending() {
        return pending.size();
    }

    static int attackerKind(Entity a) {
        if (a == null)
            return Obs.Hurt.ATTACKER_NONE;
        if (a instanceof Player)
            return Obs.Hurt.ATTACKER_PLAYER;
        if (a instanceof Mob)
            return Obs.Hurt.ATTACKER_MOB;
        return Obs.Hurt.ATTACKER_OTHER;
    }

    /** LivingDamageEvent.Post: damage that was really dealt. */
    public void onDamage(LivingEntity victim, Entity attacker, float amount, long now) {
        if (amount <= 0)
            return;
        Pending p = describe(victim, attacker);
        if (p == null)
            return;
        Key key = new Key(victim.getUUID(), attacker == null ? null : attacker.getUUID());
        Pending old = pending.get(key);
        if (old == null) {
            p.first = now;
            pending.put(key, p);
            old = p;
        }
        old.amount += amount;
        old.hits++;
    }

    /** A resident died (LivingDeathEvent, confirmed at the end of the tick): RESIDENT_KILLED at once. */
    public void onResidentDeath(LivingEntity victim, Entity killer, long now) {
        Pending p = describe(victim, killer);
        if (p == null || p.what != Obs.Hurt.RESIDENT_HURT)
            return;
        Key key = new Key(victim.getUUID(), killer == null ? null : killer.getUUID());
        Pending old = pending.remove(key);
        p.what = Obs.Hurt.RESIDENT_KILLED;
        p.flags |= Obs.Hurt.FLAG_LETHAL;
        p.amount = old == null ? 0 : old.amount;
        p.hits = old == null ? 0 : old.hits;
        journal(key, p, now);
    }

    private Pending describe(LivingEntity victim, Entity attacker) {
        Pending p = new Pending();
        AgentTag tag = RtsimAttachments.tag(victim);
        if (tag != null) {
            PuppetGroup g = session.registry().live(tag, victim.getUUID());
            if (g == null)
                return null;
            p.what = Obs.Hurt.PUPPET_HURT;
            p.agent = g.agent;
            p.epoch = g.epoch;
            p.member = g.member(victim.getUUID()).index;
            p.body = g.kind.slots().get(p.member).adapter().name();
            Village v = villages.at(victim.level().dimension(), victim.blockPosition());
            p.node = v == null ? 0 : v.id;
        } else {
            if (Residents.kind(victim) == null)
                return null;
            Village v = Residents.villageOf(villages, victim);
            if (v == null)
                return null;
            p.what = Obs.Hurt.RESIDENT_HURT;
            p.node = v.id;
            p.body = BuiltInRegistries.ENTITY_TYPE.getKey(victim.getType()).toString();
            p.profession = Residents.profession(victim);
            if (victim instanceof Villager && attacker instanceof Player)
                p.flags |= Obs.Hurt.FLAG_VANILLA_GOSSIP;
        }
        p.attackerKind = attackerKind(attacker);
        return p;
    }

    /** Journals every window that has closed. */
    public void flush(long now, boolean all) {
        for (Iterator<Map.Entry<Key, Pending>> it = pending.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<Key, Pending> e = it.next();
            if (all || now - e.getValue().first >= WINDOW) {
                it.remove();
                journal(e.getKey(), e.getValue(), now);
            }
        }
    }

    private void journal(Key key, Pending p, long now) {
        int body = session.dictId(Obs.DICT_BODY, p.body);
        int profession = p.profession == null ? 0 : session.dictId(Obs.DICT_PROFESSION, p.profession);
        ByteBuffer r = session.boundary().beginJournaled(Obs.HURT, Obs.Hurt.SIZE, p.what, now);
        r.putLong(Obs.Hurt.AGENT, p.agent);
        r.putInt(Obs.Hurt.EPOCH, p.epoch);
        r.putInt(Obs.Hurt.MEMBER, p.member);
        r.putLong(Obs.Hurt.VICTIM_MOST, key.victim().getMostSignificantBits());
        r.putLong(Obs.Hurt.VICTIM_LEAST, key.victim().getLeastSignificantBits());
        if (key.attacker() != null) {
            r.putLong(Obs.Hurt.ATTACKER_MOST, key.attacker().getMostSignificantBits());
            r.putLong(Obs.Hurt.ATTACKER_LEAST, key.attacker().getLeastSignificantBits());
        }
        r.putInt(Obs.Hurt.ATTACKER_KIND, p.attackerKind);
        r.putFloat(Obs.Hurt.AMOUNT, p.amount);
        r.putLong(Obs.Hurt.NODE, p.node);
        r.putInt(Obs.Hurt.VICTIM_BODY, body);
        r.putInt(Obs.Hurt.FLAGS, p.flags);
        r.putInt(Obs.Hurt.HITS, p.hits);
        r.putInt(Obs.Hurt.PROFESSION, profession);
        session.boundary().commit(r);
        journaled++;
    }

    /** For tests: how many windows are open. */
    public java.util.List<UUID> openVictims() {
        java.util.List<UUID> out = new ArrayList<>();
        for (Key k : pending.keySet())
            out.add(k.victim());
        return out;
    }
}
