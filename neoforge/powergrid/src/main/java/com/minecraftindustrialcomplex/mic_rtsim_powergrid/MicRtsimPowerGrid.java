package com.minecraftindustrialcomplex.mic_rtsim_powergrid;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_powergrid (A19): Create Power Grid 0.6.2 for the industry engine: electric block entities seed
 * districts, batteries are boundary stock (INDUSTRY.md §7), and transmission lines between districts are
 * power links (I13, IND_POWER_LINK). No block, item or entity of its own (D3).
 */
@Mod(MicRtsimPowerGrid.MODID)
public final class MicRtsimPowerGrid {
    public static final String MODID = "mic_rtsim_powergrid";

    public MicRtsimPowerGrid(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new PowerGridPlugin());
    }
}
