package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.simibubi.create.api.behaviour.display.DisplaySource;
import com.simibubi.create.content.redstone.displayLink.DisplayLinkContext;
import com.simibubi.create.content.redstone.displayLink.target.DisplayTargetStats;
import com.simibubi.create.foundation.gui.ModularGuiLineBuilder;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;

import java.util.List;

/**
 * The "District ledger" Display Source (I18, phase I-A): on a Display Link attached to a Create Stock
 * Link or a Train Station, it shows a district's ledger on any display target (Display Boards, signs,
 * Nixie tubes, lecterns): its state (§2.3), its top goods per real hour, what it would have made while
 * unloaded, and its next breakpoint. The district is the one the source block is in, or the one named in
 * the link's "District" option. Read only: it asks the engine with IND_LEDGER_REQUEST and shows the
 * latest IND_LEDGER, so it never advances the sim.
 */
public final class DistrictLedgerSource extends DisplaySource {

    public static final String CONFIG_DISTRICT = "District";

    @Override
    public List<MutableComponent> provideText(DisplayLinkContext context, DisplayTargetStats stats) {
        Industry ind = IndustryApi.current();
        if (ind == null)
            return List.of(Component.literal("rtsim: no session"));
        District d = district(ind, context);
        if (d == null)
            return List.of(Component.literal("rtsim: not in a district"));
        ind.ledgers().request(d);
        return ind.ledgers().displayLines(d, Math.max(1, stats.maxRows()));
    }

    private static District district(Industry ind, DisplayLinkContext context) {
        String name = context.sourceConfig().getString(CONFIG_DISTRICT);
        if (name != null && !name.isBlank()) {
            District d = ind.districts().byName(name.trim());
            if (d != null)
                return d;
        }
        return ind.districtAt(context.level(), context.getSourcePos());
    }

    @Override
    public int getPassiveRefreshTicks() {
        return 100;
    }

    @Override
    public void initConfigurationWidgets(DisplayLinkContext context, ModularGuiLineBuilder builder, boolean isFirstLine) {
        if (isFirstLine)
            builder.addTextInput(0, 137, (e, t) -> {
                e.setValue("");
                t.withTooltip(List.of(Component.literal("District name or id; empty: the source block's own")));
            }, CONFIG_DISTRICT);
    }
}
