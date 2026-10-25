package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthana;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaRaptor;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import net.minecraft.world.entity.Entity;

/**
 * What each of Mowzie's hostile bodies is worth in an observed fight (MZB-435; MOWZIE.md §4.13, ledger 19): the strength
 * IND_FIGHT carries for an attacker, in the engine's unit of one ordinary mob (a zombie, health times attack 20 times
 * 3). The shares: the Frostmaw 41.7, the Umvuthi 5.0, a raptor 1.0 and any other Umvuthana (a grove warrior, a raptor's
 * follower, a crane) 0.4, so that a raptor with its four followers is 2.6 once each has joined the fight. NaN for
 * anything else, which leaves the entity to the next plugin and then to the core's 1. One plugin for the whole mod, so
 * that the Frostmaw (M6) and the Umvuthana share a table.
 *
 * <p>Asked once per attacker per fight, by entity class, so a body made by this mod's makers and a body the jar made
 * are the same.
 */
public final class MowzieStrength implements IndustryPlugin {
    public static final MowzieStrength INSTANCE = new MowzieStrength();

    public static final float FROSTMAW = 41.7f, UMVUTHI = 5.0f, RAPTOR = 1.0f, UMVUTHANA = 0.4f;

    private MowzieStrength() {}

    @Override
    public float fightStrength(Entity attacker) {
        if (attacker instanceof EntityFrostmaw)
            return FROSTMAW;
        if (attacker instanceof EntityUmvuthi)
            return UMVUTHI;
        if (attacker instanceof EntityUmvuthanaRaptor)
            return RAPTOR;
        if (attacker instanceof EntityUmvuthana)
            return UMVUTHANA;
        return Float.NaN;
    }
}
