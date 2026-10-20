package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.saveddata.SavedData;

/**
 * Small per-world bridge state that is not the journal: whether the sim has taken the
 * {@code doTraderSpawning} gamerule, and the value it had before, so switching the trade or travel
 * module off restores what the world had ({@code data/mic_rtsim_bridge.dat}).
 */
public final class BridgeStateData extends SavedData {
    public static final String FILE = "mic_rtsim_bridge";

    boolean traderRuleOwned;
    boolean traderRulePrior = true;

    public static final Factory<BridgeStateData> FACTORY = new Factory<>(BridgeStateData::new, BridgeStateData::load);

    public static BridgeStateData get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
    }

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        tag.putBoolean("traderRuleOwned", traderRuleOwned);
        tag.putBoolean("traderRulePrior", traderRulePrior);
        return tag;
    }

    private static BridgeStateData load(CompoundTag tag, HolderLookup.Provider registries) {
        BridgeStateData d = new BridgeStateData();
        d.traderRuleOwned = tag.getBoolean("traderRuleOwned");
        d.traderRulePrior = !tag.contains("traderRulePrior") || tag.getBoolean("traderRulePrior");
        return d;
    }

    public boolean traderRuleOwned() {
        return traderRuleOwned;
    }

    public boolean traderRulePrior() {
        return traderRulePrior;
    }
}
