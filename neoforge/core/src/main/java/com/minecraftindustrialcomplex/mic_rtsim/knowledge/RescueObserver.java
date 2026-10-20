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
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.player.Player;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

/**
 * Rescues count (C24): a player kills a mob whose target was a puppet member (a caravan) or a village
 * resident. Vanilla zombies already target {@code AbstractVillager}, wandering traders included
 * (PLAN.md §7), so rescues need no sim content. Seen at {@code LivingDeathEvent} (the mob's target is
 * still set there) and journaled at the end of the tick once the mob is really dead, as deaths are
 * (another mod can still cancel the death after the bridge saw it).
 */
public final class RescueObserver {

    private record Pending(Mob mob, LivingEntity victim, Player rescuer, int what, long agent, int epoch, int member, long node) {}

    private final RtsimSession session;
    private final VillageRegistry villages;
    private final List<Pending> pending = new ArrayList<>();
    long journaled;

    public RescueObserver(RtsimSession session, VillageRegistry villages) {
        this.session = session;
        this.villages = villages;
    }

    public long journaled() {
        return journaled;
    }

    /** LivingDeathEvent of any entity, at the lowest priority and only if not cancelled. */
    public void onDeath(LivingEntity dead, Object killer) {
        if (!(dead instanceof Mob mob) || !(killer instanceof Player player) || !(mob.getTarget() instanceof LivingEntity target))
            return;
        AgentTag tag = RtsimAttachments.tag(target);
        if (tag != null) {
            PuppetGroup g = session.registry().live(tag, target.getUUID());
            if (g == null)
                return;
            Village v = villages.at(target.level().dimension(), target.blockPosition());
            pending.add(new Pending(mob, target, player, Obs.Rescue.PUPPET, g.agent, g.epoch, g.member(target.getUUID()).index, v == null ? 0 : v.id));
            return;
        }
        if (Residents.kind(target) == null)
            return;
        Village v = Residents.villageOf(villages, target);
        if (v != null)
            pending.add(new Pending(mob, target, player, Obs.Rescue.RESIDENT, 0, 0, 0, v.id));
    }

    /** End of tick: journal the rescues whose mob really died. */
    public void settle(long now) {
        if (pending.isEmpty())
            return;
        for (Pending p : pending) {
            if (!(p.mob().isDeadOrDying() || p.mob().isRemoved()))
                continue; // the death was cancelled
            int threat = session.dictId(Obs.DICT_BODY, BuiltInRegistries.ENTITY_TYPE.getKey(p.mob().getType()).toString());
            ByteBuffer r = session.boundary().beginJournaled(Obs.RESCUE, Obs.Rescue.SIZE, p.what(), now);
            r.putLong(Obs.Rescue.AGENT, p.agent());
            r.putInt(Obs.Rescue.EPOCH, p.epoch());
            r.putInt(Obs.Rescue.MEMBER, p.member());
            r.putLong(Obs.Rescue.RESCUER_MOST, p.rescuer().getUUID().getMostSignificantBits());
            r.putLong(Obs.Rescue.RESCUER_LEAST, p.rescuer().getUUID().getLeastSignificantBits());
            r.putLong(Obs.Rescue.VICTIM_MOST, p.victim().getUUID().getMostSignificantBits());
            r.putLong(Obs.Rescue.VICTIM_LEAST, p.victim().getUUID().getLeastSignificantBits());
            r.putInt(Obs.Rescue.THREAT, threat);
            r.putLong(Obs.Rescue.NODE, p.node());
            session.boundary().commit(r);
            journaled++;
        }
        pending.clear();
    }
}
