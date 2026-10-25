package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.context.CommandContext;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;

import java.util.List;

/**
 * {@code /rtsim district sections [name|id]} (SECTIONS.md §8.1, slice S7): one line per section of the district's
 * last complete compile, then the stores shared between sections. It asks the engine for the district's glass
 * report, which it builds on request (nothing is kept or sent per tick), and reads nothing else. Permission 2, as
 * every {@code /rtsim district} command. The stub engine has no glass box, and a district with no complete
 * compile has no sections: either is said in one line.
 */
public final class GlassSectionsCommand {
    private GlassSectionsCommand() {}

    public static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
        dispatcher.register(Commands.literal("rtsim")
                .requires(src -> src.hasPermission(2))
                .then(Commands.literal("district")
                        .requires(src -> src.hasPermission(2))
                        .then(Commands.literal("sections").executes(c -> sections(c, null))
                                .then(Commands.argument("name", StringArgumentType.string())
                                        .executes(c -> sections(c, StringArgumentType.getString(c, "name")))))));
    }

    private static int sections(CommandContext<CommandSourceStack> ctx, String name) {
        Industry ind = IndustryApi.current();
        if (ind == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no industry module (no session, or the industry module is off)"));
            return 0;
        }
        District d = name == null
                ? ind.districtAt(ctx.getSource().getLevel(), BlockPos.containing(ctx.getSource().getPosition()))
                : ind.districts().byName(name);
        if (d == null) {
            ctx.getSource().sendFailure(Component.literal("mic_rtsim: no district " + (name == null ? "here" : "'" + name + "'")));
            return 0;
        }
        var fetch = ind.diagnostics().glassReportOf(d);
        if (fetch.report() == null) {
            ctx.getSource().sendSuccess(() -> Component.literal("district " + d.id + " '" + d.displayName() + "': " + fetch.note()), false);
            return 0;
        }
        List<String> lines = GlassSectionsText.lines(d.id, d.displayName(), fetch.report(), id -> ind.goodName(id));
        for (String l : lines)
            ctx.getSource().sendSuccess(() -> Component.literal(l), false);
        return fetch.report().sections().size();
    }
}
