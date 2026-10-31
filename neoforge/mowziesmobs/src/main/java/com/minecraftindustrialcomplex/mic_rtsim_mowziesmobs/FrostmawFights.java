package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * Two Frostmaws of one den fight to the death (MZB-1404; fork F3 as Ben changed it, 2026-10-30, "They fight to the death"): a
 * Frostmaw written off as missing that came back to its full den, and the den's Frostmaw that holds its slot. The engine's
 * FIGHT line names the two ({@link DenKind#fight}); the survivor holds the slot, and the engine books the death as the fight's.
 *
 * <p><b>Read from the 1.8.2 jar</b> (vineflower of {@code EntityFrostmaw}): its only target goal is a
 * {@code NearestAttackableTargetGoal} for players; {@code hurt} never takes another Frostmaw as its attacker's target (it skips
 * {@code instanceof EntityFrostmaw}), so the jar alone never lets two fight; a target set from outside stays until it dies or
 * turns invisible, and with one an active Frostmaw walks to it and swipes ({@code AnimationAreaAttackAI} hits every living
 * entity in its arc). An asleep one ({@code ACTIVE} false) wakes only with an attackable entity within 8 blocks, so both are
 * woken here ({@code setActive(true)}, the jar's public setter). No mixin is needed.
 *
 * <p><b>Kept on it:</b> a mob's target is not saved, and the player target goal's {@code stop()} clears whatever target the
 * Frostmaw had when a player it chased goes. So every {@link #REASSERT} ticks each pair whose two stand alive in one level is
 * set again, unless a Frostmaw has a player as its target (a player who joins the fight is fought, and the kill credit is the
 * usual one). A pair one of whose bodies died, unloaded or changed level is over; the engine's next census of the full den
 * pairs them again. Not saved; at most one pair a den, so a few in a world (D1).
 */
public final class FrostmawFights {
    private FrostmawFights() {}

    /** How often each pair's targets are set again, in ticks (PLACEHOLDER). */
    public static final int REASSERT = 20;

    private record Pair(EntityFrostmaw a, EntityFrostmaw b) {}

    private static final List<Pair> PAIRS = new ArrayList<>(2);
    /** Fights set, pairs set again, and fights over (GameTests). */
    public static long started, reasserted, ended;

    /** The engine's FIGHT: {@code holder} (the slot's body) and {@code rival} (the returner) fight from now on. */
    static void start(EntityFrostmaw holder, EntityFrostmaw rival) {
        PAIRS.removeIf(p -> p.a == holder || p.b == holder || p.a == rival || p.b == rival); // a body fights one rival
        PAIRS.add(new Pair(holder, rival));
        set(holder, rival);
        set(rival, holder);
        started++;
    }

    /** The pairs fighting now (GameTests). */
    public static int active() {
        return PAIRS.size();
    }

    private static boolean stands(EntityFrostmaw f) {
        return f.isAlive() && !f.isRemoved();
    }

    /** Wakes {@code f} and sets it on {@code foe}, unless it is busy with a player. */
    private static void set(EntityFrostmaw f, EntityFrostmaw foe) {
        LivingEntity t = f.getTarget();
        if (t instanceof Player p && p.isAlive())
            return;
        if (!f.getActive())
            f.setActive(true);
        if (t != foe)
            f.setTarget(foe);
    }

    /** ServerTickEvent.Post: every {@link #REASSERT} ticks, each pair is set again, or is over. */
    static void tick(MinecraftServer server) {
        if (PAIRS.isEmpty() || server.getTickCount() % REASSERT != 0)
            return;
        for (Iterator<Pair> it = PAIRS.iterator(); it.hasNext(); ) {
            Pair p = it.next();
            if (!stands(p.a) || !stands(p.b) || p.a.level() != p.b.level()) {
                it.remove();
                ended++;
                continue;
            }
            set(p.a, p.b);
            set(p.b, p.a);
            reasserted++;
        }
    }

    /** The server stopped: no pair outlives its bodies' level. */
    static void clear() {
        PAIRS.clear();
        started = reasserted = ended = 0;
    }
}
