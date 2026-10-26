package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.neoforged.neoforge.event.entity.living.LivingDamageEvent;

import java.util.Map;
import java.util.WeakHashMap;

/**
 * The last tick a player hit an Umvuthi (MZB-410): a seat over a chief body waits while the old chief is in a fight a
 * player began, that is, while it has a player as its target and a player hit it within {@link #WINDOW} ticks. A
 * weak map keyed on the entity itself, so a removed body leaves nothing behind (UR-1: nothing is saved). Read and
 * written on the server thread only.
 */
public final class FightWatch {
    /** How long a player's hit keeps a fight on (PLACEHOLDER, 30 seconds). */
    public static final long WINDOW = 600;

    private static final Map<LivingEntity, Long> LAST = new WeakHashMap<>();

    private FightWatch() {}

    /** LivingDamageEvent.Post: an Umvuthi a real player (or a tamed animal's owner) hurt. */
    static void onDamage(LivingDamageEvent.Post event) {
        LivingEntity hit = event.getEntity();
        if (hit.level().isClientSide() || !(hit instanceof EntityUmvuthi))
            return;
        Player by = Camps.playerOf(event.getSource());
        if (by != null)
            hit(hit, hit.level().getGameTime());
    }

    /** Notes a player's hit at {@code now} (GameTests call it as the event does). */
    public static void hit(LivingEntity body, long now) {
        LAST.put(body, now);
    }

    /** Whether a player hit this body within {@link #WINDOW} ticks of {@code now}. */
    public static boolean recent(LivingEntity body, long now) {
        Long at = LAST.get(body);
        return at != null && now - at <= WINDOW;
    }
}
