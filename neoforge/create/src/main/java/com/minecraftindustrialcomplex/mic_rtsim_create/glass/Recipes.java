package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Rat;
import com.simibubi.create.AllDataComponents;
import com.simibubi.create.AllRecipeTypes;
import com.simibubi.create.api.registry.CreateDataMaps;
import com.simibubi.create.content.equipment.sandPaper.SandPaperPolishingRecipe;
import com.simibubi.create.content.fluids.spout.FillingBySpout;
import com.simibubi.create.content.fluids.transfer.GenericItemEmptying;
import com.simibubi.create.content.kinetics.crafter.MechanicalCrafterBlock;
import com.simibubi.create.content.kinetics.crafter.MechanicalCrafterBlockEntity;
import com.simibubi.create.content.kinetics.crafter.MechanicalCraftingRecipe;
import com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity;
import com.simibubi.create.content.kinetics.deployer.ItemApplicationRecipe;
import com.simibubi.create.content.kinetics.fan.processing.AllFanProcessingTypes;
import com.simibubi.create.content.kinetics.fan.processing.FanProcessingType;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlockEntity;
import com.simibubi.create.content.processing.basin.BasinBlockEntity;
import com.simibubi.create.content.processing.basin.BasinRecipe;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlockEntity;
import com.simibubi.create.content.processing.recipe.HeatCondition;
import com.simibubi.create.content.processing.recipe.ProcessingOutput;
import com.simibubi.create.content.processing.recipe.ProcessingRecipe;
import com.simibubi.create.content.processing.sequenced.SequencedAssemblyRecipe;
import com.simibubi.create.content.processing.sequenced.SequencedRecipe;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import com.simibubi.create.foundation.recipe.RecipeConditions;
import com.simibubi.create.foundation.recipe.RecipeFinder;
import com.simibubi.create.infrastructure.config.AllConfigs;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import it.unimi.dsi.fastutil.ints.IntOpenHashSet;
import net.createmod.catnip.data.Pair;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.NonNullList;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.AbstractCookingRecipe;
import net.minecraft.world.item.crafting.CraftingRecipe;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.item.crafting.ShapedRecipe;
import net.minecraft.world.item.crafting.ShapelessRecipe;
import net.minecraft.world.item.crafting.SingleRecipeInput;
import net.minecraft.world.level.block.entity.BlastFurnaceBlockEntity;
import net.minecraft.world.level.block.entity.SmokerBlockEntity;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.crafting.SizedFluidIngredient;
import net.neoforged.neoforge.items.ItemStackHandler;
import net.neoforged.neoforge.items.wrapper.RecipeWrapper;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

/**
 * Each machine's recipes for the goods that can reach it, found the way the machine finds them (GLASSBOX.md
 * §2, G-B5): the millstone's and crusher's {@code AllRecipeTypes.find}, the press's and deployer's own
 * public {@code getRecipe}, the saw's filtered cutting list, the fan types' {@code canProcess}, the basin
 * operators' static filters. Sequenced assembly steps are recognised by re-enacting
 * {@code SequencedAssemblyRecipe.appliesTo}, {@code getStep} and {@code advance} (they are private), so
 * each step's output is the transitional item with the next {@code SequencedAssembly} component, a distinct
 * good. Chance outputs are exact: {@code ProcessingOutput.rollOutput} keeps a stack unless
 * {@code nextFloat() > chance}, and {@code nextFloat} is {@code k / 2^24}.
 */
final class Recipes {
    private Recipes() {}

    private static final Object SAW_KEY = new Object(), SAW_STONECUTTING_KEY = new Object();

    // ---- terms ----------------------------------------------------------------------------------

    /** P(kept) of a ProcessingOutput with this chance, as num/den. */
    static long[] keep(float chance) {
        if (chance >= 1f)
            return new long[] {1, 1};
        if (!(chance > 0f))
            return new long[] {1, 1L << 24}; // nextFloat() <= 0 only for k = 0
        long k = (long) Math.floor((double) chance * (1 << 24));
        return new long[] {Math.min(k + 1, 1L << 24), 1L << 24};
    }

    static void outputs(CreateCompile c, GraphBuilder.Recipe r, List<ProcessingOutput> outs, long per) {
        for (ProcessingOutput o : outs) {
            ItemStack s = o.getStack();
            if (s.isEmpty())
                continue;
            long[] p = keep(o.getChance());
            int flags = p[0] == p[1] ? 0 : GlassRecords.Recipe.TERM_CHANCE;
            r.out(c.good(s), per * s.getCount() * p[0], p[1], flags);
        }
    }

    static void fluidOutputs(CreateCompile c, GraphBuilder.Recipe r, List<FluidStack> outs) {
        for (FluidStack f : outs)
            if (!f.isEmpty())
                r.out(c.good(f), f.getAmount(), 1);
    }

    static int heat(HeatCondition h) {
        return switch (h) {
            case NONE -> GlassRecords.Heat.NONE;
            case HEATED -> GlassRecords.Heat.NEEDS_HEATED;
            case SUPERHEATED -> GlassRecords.Heat.NEEDS_SUPERHEATED;
        };
    }

    static int typeCode(Recipe<?> r) {
        RecipeType<?> t = r.getType();
        if (t == AllRecipeTypes.MILLING.getType())
            return GlassRecords.RecipeType.MILLING;
        if (t == AllRecipeTypes.CRUSHING.getType())
            return GlassRecords.RecipeType.CRUSHING;
        if (t == AllRecipeTypes.PRESSING.getType())
            return GlassRecords.RecipeType.PRESSING;
        if (t == AllRecipeTypes.MIXING.getType())
            return GlassRecords.RecipeType.MIXING;
        if (t == AllRecipeTypes.COMPACTING.getType())
            return GlassRecords.RecipeType.COMPACTING;
        if (t == AllRecipeTypes.CUTTING.getType())
            return GlassRecords.RecipeType.CUTTING;
        if (t == AllRecipeTypes.DEPLOYING.getType())
            return GlassRecords.RecipeType.DEPLOYING;
        if (t == AllRecipeTypes.ITEM_APPLICATION.getType())
            return GlassRecords.RecipeType.ITEM_APPLICATION;
        if (t == AllRecipeTypes.SPLASHING.getType())
            return GlassRecords.RecipeType.SPLASHING;
        if (t == AllRecipeTypes.HAUNTING.getType())
            return GlassRecords.RecipeType.HAUNTING;
        if (t == AllRecipeTypes.FILLING.getType())
            return GlassRecords.RecipeType.FILLING;
        if (t == AllRecipeTypes.EMPTYING.getType())
            return GlassRecords.RecipeType.EMPTYING;
        if (t == RecipeType.SMELTING)
            return GlassRecords.RecipeType.SMELTING;
        if (t == RecipeType.BLASTING)
            return GlassRecords.RecipeType.BLASTING;
        if (t == RecipeType.SMOKING)
            return GlassRecords.RecipeType.SMOKING;
        if (t == RecipeType.STONECUTTING)
            return GlassRecords.RecipeType.STONECUTTING;
        if (t == RecipeType.CRAFTING || t == AllRecipeTypes.MECHANICAL_CRAFTING.getType())
            return GlassRecords.RecipeType.CRAFTING;
        return GlassRecords.RecipeType.OTHER;
    }

    static int duration(Recipe<?> r) {
        return r instanceof ProcessingRecipe<?, ?> p ? p.getProcessingDuration() : 0;
    }

    // ---- sequenced assembly ---------------------------------------------------------------------

    /** A stack's place in a sequenced assembly whose next step is of the machine's type. */
    record Seq(ResourceLocation id, SequencedAssemblyRecipe recipe, int step, ProcessingRecipe<?, ?> next) {}

    static Seq sequenced(ServerLevel level, ItemStack stack, RecipeType<?> type) {
        for (RecipeHolder<SequencedAssemblyRecipe> h : level.getRecipeManager()
                .getAllRecipesFor(AllRecipeTypes.SEQUENCED_ASSEMBLY.<RecipeWrapper, SequencedAssemblyRecipe>getType())) {
            SequencedAssemblyRecipe r = h.value();
            if (!appliesTo(h.id(), r, stack))
                continue;
            List<SequencedRecipe<?>> seq = r.getSequence();
            if (seq.isEmpty())
                continue;
            int step = step(stack);
            ProcessingRecipe<?, ?> next = seq.get(step % seq.size()).getRecipe();
            if (next.getType() == type)
                return new Seq(h.id(), r, step, next);
        }
        return null;
    }

    /** {@code SequencedAssemblyRecipe.appliesTo} (private). */
    private static boolean appliesTo(ResourceLocation id, SequencedAssemblyRecipe r, ItemStack s) {
        if (r.getIngredient().test(s))
            return true;
        SequencedAssemblyRecipe.SequencedAssembly sa = s.get(AllDataComponents.SEQUENCED_ASSEMBLY);
        return sa != null && r.getTransitionalItem().getItem() == s.getItem() && sa.id().equals(id);
    }

    /** {@code SequencedAssemblyRecipe.getStep} (private). */
    private static int step(ItemStack s) {
        SequencedAssemblyRecipe.SequencedAssembly sa = s.get(AllDataComponents.SEQUENCED_ASSEMBLY);
        return sa == null ? 0 : sa.step();
    }

    /** {@code SequencedAssemblyRecipe.advance}: the next transitional stack, or the weighted result pool. */
    static void advance(CreateCompile c, GraphBuilder.Recipe out, Seq s) {
        int size = s.recipe().getSequence().size(), loops = s.recipe().getLoops();
        if ((s.step() + 1) / size >= loops) {
            float total = 0;
            for (ProcessingOutput o : s.recipe().resultPool)
                total += o.getChance();
            if (!(total > 0))
                return;
            Rat t = Rat.ofFloat(total);
            for (ProcessingOutput o : s.recipe().resultPool) {
                ItemStack st = o.getStack();
                if (st.isEmpty() || !(o.getChance() > 0))
                    continue;
                Rat w = Rat.ofFloat(o.getChance()).div(t);
                int flags = w.num() == w.den() ? 0 : GlassRecords.Recipe.TERM_CHANCE;
                out.out(c.good(st), w.num() * st.getCount(), w.den(), flags);
            }
            return;
        }
        ItemStack adv = s.recipe().getTransitionalItem().copyWithCount(1);
        adv.set(AllDataComponents.SEQUENCED_ASSEMBLY,
                new SequencedAssemblyRecipe.SequencedAssembly(s.id(), s.step() + 1, (s.step() + 1f) / (size * loops)));
        out.out(c.good(adv), 1, 1);
    }

    private static String seqName(Seq s) {
        return s.id() + "#" + s.step();
    }

    // ---- single-input machines ------------------------------------------------------------------

    /** Recipes for good {@code gid} (prototype {@code is}) newly reaching a single-input machine. */
    static void single(CreateCompile c, CreateCompile.Info i, int gid, ItemStack is, IntOpenHashSet in) {
        ServerLevel level = c.level;
        switch (i.kind) {
            case MILLSTONE -> {
                Optional<RecipeHolder<Recipe<RecipeWrapper>>> r = AllRecipeTypes.MILLING.find(wrap(is), level);
                r.ifPresent(h -> processing(c, i, gid, h, 1, NONE_CYCLE));
            }
            case CRUSHER -> {
                Optional<RecipeHolder<Recipe<RecipeWrapper>>> r = AllRecipeTypes.CRUSHING.find(wrap(is), level);
                if (r.isEmpty())
                    r = AllRecipeTypes.MILLING.find(wrap(is), level);
                r.ifPresent(h -> processing(c, i, gid, h, CreateCompile.FEEDER_BATCH, NONE_CYCLE));
            }
            case SAW -> saw(c, i, gid, is);
            case PRESS -> {
                if (i.be instanceof MechanicalPressBlockEntity p) {
                    Seq s = sequenced(level, is, AllRecipeTypes.PRESSING.getType());
                    if (s != null) {
                        stepRecipe(c, i, gid, s, null);
                        return;
                    }
                    p.getRecipe(is).ifPresent(h -> processing(c, i, gid, h, 1, NONE_CYCLE));
                }
            }
            case DEPLOYER -> deployer(c, i, gid, is);
            case FAN -> fan(c, i, gid, is);
            case DRAIN -> {
                if (!GenericItemEmptying.canItemBeEmptied(level, is))
                    return;
                Pair<FluidStack, ItemStack> res = GenericItemEmptying.emptyItem(level, is.copyWithCount(1), true);
                if (res == null || res.getFirst().isEmpty())
                    return;
                GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.EMPTYING, GlassRecords.Heat.NONE, 0,
                        c.g.name("create:emptying/" + c.g.goodName(is)));
                r.in(gid, 1, 1);
                r.out(c.good(res.getFirst()), res.getFirst().getAmount(), 1);
                if (!res.getSecond().isEmpty())
                    r.out(c.good(res.getSecond()), res.getSecond().getCount(), 1);
                c.addCraft(i, r, 1, c.cycle(i, 0, 1), gid);
            }
            case FURNACE -> furnace(c, i, gid, is);
            default -> {}
        }
    }

    private static final CreateCompile.Cyc NONE_CYCLE = null;

    private static RecipeWrapper wrap(ItemStack... stacks) {
        ItemStackHandler h = new ItemStackHandler(stacks.length);
        for (int k = 0; k < stacks.length; k++)
            h.setStackInSlot(k, stacks[k].copyWithCount(1));
        return new RecipeWrapper(h);
    }

    /** One input item of {@code gid} into a processing recipe's outputs. */
    private static void processing(CreateCompile c, CreateCompile.Info i, int gid, RecipeHolder<? extends Recipe<?>> h, int batch, CreateCompile.Cyc cycle) {
        Recipe<?> rec = h.value();
        if (!(rec instanceof ProcessingRecipe<?, ?> p))
            return;
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(typeCode(rec), heat(p.getRequiredHeat()), p.getProcessingDuration(), c.g.name(h.id().toString()));
        r.in(gid, 1, 1);
        outputs(c, r, p.getRollableResults(), 1);
        fluidOutputs(c, r, p.getFluidResults());
        c.addCraft(i, r, batch, cycle != null ? cycle : c.cycle(i, p.getProcessingDuration(), batch), gid);
    }

    /** A sequenced step on one item of {@code gid}; {@code extra} adds the step's other input (a held item, a fluid). */
    private static void stepRecipe(CreateCompile c, CreateCompile.Info i, int gid, Seq s, java.util.function.Consumer<GraphBuilder.Recipe> extra) {
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.SEQUENCED, heat(s.next().getRequiredHeat()), s.next().getProcessingDuration(),
                c.g.name(seqName(s)));
        r.in(gid, 1, 1);
        if (extra != null)
            extra.accept(r);
        advance(c, r, s);
        c.addCraft(i, r, 1, c.cycle(i, s.next().getProcessingDuration(), 1), gid);
    }

    // ---- saw (recipeIndex rotation) --------------------------------------------------------------

    private static void saw(CreateCompile c, CreateCompile.Info i, int gid, ItemStack is) {
        ServerLevel level = c.level;
        FilteringBehaviour f = BlockEntityBehaviour.get(i.be, FilteringBehaviour.TYPE);
        Seq s = sequenced(level, is, AllRecipeTypes.CUTTING.getType());
        if (s != null && (f == null || f.test(s.next().getResultItem(level.registryAccess())))) {
            stepRecipe(c, i, gid, s, null);
            return;
        }
        boolean stonecutting = AllConfigs.server().recipes.allowStonecuttingOnSaw.get();
        List<RecipeHolder<? extends Recipe<?>>> all = RecipeFinder.get(stonecutting ? SAW_STONECUTTING_KEY : SAW_KEY, level,
                stonecutting ? RecipeConditions.isOfType(AllRecipeTypes.CUTTING.getType(), RecipeType.STONECUTTING)
                        : RecipeConditions.isOfType(AllRecipeTypes.CUTTING.getType()));
        List<RecipeHolder<? extends Recipe<?>>> found = new ArrayList<>();
        for (RecipeHolder<? extends Recipe<?>> h : all)
            if ((f == null || RecipeConditions.outputMatchesFilter(f).test(h)) && RecipeConditions.firstIngredientMatches(is).test(h)
                    && !AllRecipeTypes.shouldIgnoreInAutomation(h))
                found.add(h);
        if (found.isEmpty())
            return;
        // recipeIndex advances at every start: over k starts each recipe runs once, so the long-run
        // recipe is the mean of the k (outputs and time).
        int k = found.size();
        StringBuilder name = new StringBuilder();
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.CUTTING, GlassRecords.Heat.NONE, 0, 0);
        int[] durations = new int[k];
        for (int j = 0; j < k; j++) {
            Recipe<?> rec = found.get(j).value();
            durations[j] = rec instanceof ProcessingRecipe<?, ?> p ? p.getProcessingDuration() : 50;
            if (rec instanceof ProcessingRecipe<?, ?> p) {
                for (ProcessingOutput o : p.getRollableResults()) {
                    ItemStack st = o.getStack();
                    long[] q = keep(o.getChance());
                    r.out(c.good(st), st.getCount() * q[0], q[1] * k, q[0] == q[1] ? 0 : GlassRecords.Recipe.TERM_CHANCE);
                }
            } else {
                ItemStack st = rec.getResultItem(level.registryAccess());
                if (!st.isEmpty())
                    r.out(c.good(st), st.getCount(), k);
            }
            name.append(j == 0 ? "" : "+").append(found.get(j).id());
        }
        r.in(gid, 1, 1);
        GraphBuilder.Recipe named = new GraphBuilder.Recipe(r.type, r.heat, durations[0], c.g.name(name.toString()));
        named.inputs.addAll(r.inputs);
        named.outputs.addAll(r.outputs);
        CreateCompile.Cyc cyc = (rpm, batch) -> {
            Rat sum = Rat.ZERO;
            for (int d : durations) {
                int t = Cycles.saw(d, batch, rpm);
                if (t <= 0)
                    return null;
                sum = sum.add(Rat.of(t, 1));
            }
            return sum.div(Rat.of(k, 1));
        };
        c.addCraft(i, named, CreateCompile.FEEDER_BATCH, cyc, gid);
    }

    // ---- deployer ---------------------------------------------------------------------------------

    private static void deployer(CreateCompile c, CreateCompile.Info i, int gid, ItemStack is) {
        if (!(i.be instanceof DeployerBlockEntity d) || i.node.kind != GlassRecords.Node.MACHINE)
            return;
        ItemStack held = d.getPlayer() == null ? ItemStack.EMPTY : d.getPlayer().getMainHandItem();
        if (held.isEmpty() || ItemStack.isSameItemSameComponents(held, is))
            return;
        int heldGood = c.good(held.copyWithCount(1));
        i.consumed.add(heldGood); // the held item's refill stays in the deployer
        Seq s = sequenced(c.level, is, AllRecipeTypes.DEPLOYING.getType());
        if (s != null && s.next() instanceof ItemApplicationRecipe ar && ar.getRequiredHeldItem().test(held)) {
            stepRecipe(c, i, gid, s, r -> r.in(heldGood, 1, 1, ar.shouldKeepHeldItem() ? GlassRecords.Recipe.TERM_CATALYST : 0));
            return;
        }
        RecipeHolder<? extends Recipe<?>> h = d.getRecipe(is.copyWithCount(1));
        if (h == null)
            return;
        Recipe<?> rec = h.value();
        if (!(rec instanceof ProcessingRecipe<?, ?> p))
            return;
        // the deployer's getRecipe may hand back a sequenced step (it calls SequencedAssemblyRecipe.getRecipe)
        Seq again = sequenced(c.level, is, rec.getType());
        if (again != null && again.next() == rec) {
            boolean keep = rec instanceof ItemApplicationRecipe ar2 && ar2.shouldKeepHeldItem();
            stepRecipe(c, i, gid, again, r -> r.in(heldGood, 1, 1, keep ? GlassRecords.Recipe.TERM_CATALYST : 0));
            return;
        }
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(typeCode(rec), GlassRecords.Heat.NONE, p.getProcessingDuration(), c.g.name(h.id().toString()));
        r.in(gid, 1, 1);
        boolean catalyst = rec instanceof SandPaperPolishingRecipe || (rec instanceof ItemApplicationRecipe ar && ar.shouldKeepHeldItem());
        r.in(heldGood, 1, 1, catalyst ? GlassRecords.Recipe.TERM_CATALYST : 0);
        outputs(c, r, p.getRollableResults(), 1);
        c.addCraft(i, r, 1, c.cycle(i, 0, 1), gid);
    }

    // ---- fans -------------------------------------------------------------------------------------

    private static void fan(CreateCompile c, CreateCompile.Info i, int gid, ItemStack is) {
        Object[] x = (Object[]) i.extra;
        FanProcessingType type = (FanProcessingType) x[0];
        ServerLevel level = c.level;
        ItemStack one = is.copyWithCount(1);
        if (!type.canProcess(one, level))
            return;
        GraphBuilder.Recipe r;
        if (type == AllFanProcessingTypes.SPLASHING || type == AllFanProcessingTypes.HAUNTING) {
            AllRecipeTypes t = type == AllFanProcessingTypes.SPLASHING ? AllRecipeTypes.SPLASHING : AllRecipeTypes.HAUNTING;
            Optional<RecipeHolder<Recipe<SingleRecipeInput>>> h = t.find(new SingleRecipeInput(one), level);
            if (h.isEmpty() || !(h.get().value() instanceof ProcessingRecipe<?, ?> p))
                return;
            r = new GraphBuilder.Recipe(typeCode(p), GlassRecords.Heat.NONE, 0, c.g.name(h.get().id().toString()));
            r.in(gid, 1, 1);
            outputs(c, r, p.getRollableResults(), 1);
        } else {
            // smoking and blasting are deterministic: the type's own process() on one item
            List<ItemStack> out = type.process(one, level);
            if (out == null)
                return;
            int code = type == AllFanProcessingTypes.SMOKING ? GlassRecords.RecipeType.SMOKING : GlassRecords.RecipeType.BLASTING;
            r = new GraphBuilder.Recipe(code, GlassRecords.Heat.NONE, 0,
                    c.g.name((code == GlassRecords.RecipeType.SMOKING ? "create:fan_smoking/" : "create:fan_blasting/") + c.g.goodName(one)));
            r.in(gid, 1, 1);
            for (ItemStack o : out)
                if (!o.isEmpty())
                    r.out(c.good(o), o.getCount(), 1);
        }
        // On a belt: processed in place if the stack stays long enough (no rate entry); on a depot: one
        // stack per fanTicks(n) ticks.
        double exposure = c.fanExposure(i);
        int fanTime = c.fanProcessingTime();
        if (exposure < Cycles.fanTicks(1, fanTime))
            return; // passes through unprocessed
        if (exposure < Cycles.fanTicks(64, fanTime))
            c.markBounded(i); // a large stack may leave before it is done
        CreateCompile.Cyc cyc = Double.isInfinite(exposure) ? (rpm, batch) -> Rat.of(Cycles.fanTicks(batch, fanTime), 1) : null;
        c.addCraft(i, r, CreateCompile.FEEDER_BATCH, cyc, gid);
    }

    // ---- spout -------------------------------------------------------------------------------------

    private static void spout(CreateCompile c, CreateCompile.Info i, IntOpenHashSet in) {
        ServerLevel level = c.level;
        IntArrayList items = new IntArrayList(), fluids = new IntArrayList();
        for (int g : in) {
            if (c.itemProto(g) != null)
                items.add(g);
            else if (c.fluidProto(g) != null)
                fluids.add(g);
        }
        items.sort(null);
        fluids.sort(null);
        for (int ig : items)
            for (int fg : fluids) {
                long key = ((long) ig << 32) | (fg & 0xFFFF_FFFFL);
                if (!i.triedPairs.add(key))
                    continue;
                ItemStack is = c.itemProto(ig).copyWithCount(1);
                FluidStack fs = c.fluidProto(fg);
                Seq s = sequenced(level, is, AllRecipeTypes.FILLING.getType());
                if (s != null && s.next().getFluidIngredients().size() == 1 && s.next().getFluidIngredients().get(0).ingredient().test(fs)) {
                    int amount = s.next().getFluidIngredients().get(0).amount();
                    stepRecipe(c, i, ig, s, r -> r.in(fg, amount, 1));
                    continue;
                }
                if (!FillingBySpout.canItemBeFilled(level, is))
                    continue;
                FluidStack probe = fs.copyWithAmount(Integer.MAX_VALUE / 2);
                int required = FillingBySpout.getRequiredAmountForItem(level, is, probe);
                if (required <= 0)
                    continue;
                ItemStack result = FillingBySpout.fillItem(level, required, is.copy(), probe.copyWithAmount(required));
                if (result.isEmpty())
                    continue;
                GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.FILLING, GlassRecords.Heat.NONE, 0,
                        c.g.name("create:filling/" + c.g.goodName(is) + "+" + c.g.goodName(fs)));
                r.in(ig, 1, 1);
                r.in(fg, required, 1);
                r.out(c.good(result), result.getCount(), 1);
                c.addCraft(i, r, 1, c.cycle(i, 0, 1), ig);
            }
    }

    // ---- vanilla furnaces ---------------------------------------------------------------------------

    private static void furnace(CreateCompile c, CreateCompile.Info i, int gid, ItemStack is) {
        RecipeType<? extends AbstractCookingRecipe> type = i.be instanceof BlastFurnaceBlockEntity ? RecipeType.BLASTING
                : i.be instanceof SmokerBlockEntity ? RecipeType.SMOKING : RecipeType.SMELTING;
        @SuppressWarnings("unchecked")
        RecipeType<AbstractCookingRecipe> t = (RecipeType<AbstractCookingRecipe>) type;
        Optional<RecipeHolder<AbstractCookingRecipe>> h = c.level.getRecipeManager().getRecipeFor(t, new SingleRecipeInput(is.copyWithCount(1)), c.level);
        if (h.isEmpty())
            return;
        AbstractCookingRecipe rec = h.get().value();
        ItemStack out = rec.getResultItem(c.level.registryAccess());
        if (out.isEmpty())
            return;
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(typeCode(rec), GlassRecords.Heat.NONE, rec.getCookingTime(), c.g.name(h.get().id().toString()));
        r.in(gid, 1, 1);
        r.out(c.good(out), out.getCount(), 1);
        c.markBounded(i); // fuel is not modelled in G1 (GB)
        c.addCraft(i, r, 1, c.cycle(i, rec.getCookingTime(), 1), gid);
    }

    // ---- blaze burner fuel --------------------------------------------------------------------------

    /** {@code BlazeBurnerBlockEntity.tryUpdateFuel}'s burn time and heat for an item, or null when it is no fuel. */
    @SuppressWarnings({"deprecation", "removal"})
    static int[] burnerFuel(ItemStack s) {
        var special = s.getItem().builtInRegistryHolder().getData(CreateDataMaps.SUPERHEATED_BLAZE_BURNER_FUELS);
        if (special != null)
            return new int[] {special.burnTime(), GlassRecords.Heat.SEETHING};
        var regular = s.getItem().builtInRegistryHolder().getData(CreateDataMaps.REGULAR_BLAZE_BURNER_FUELS);
        if (regular != null)
            return new int[] {regular.burnTime(), GlassRecords.Heat.KINDLED};
        if (com.simibubi.create.AllTags.AllItemTags.BLAZE_BURNER_FUEL_SPECIAL.matches(s))
            return new int[] {3200, GlassRecords.Heat.SEETHING};
        int vanilla = s.getBurnTime(null);
        if (vanilla > 0)
            return new int[] {vanilla, GlassRecords.Heat.KINDLED};
        if (com.simibubi.create.AllTags.AllItemTags.BLAZE_BURNER_FUEL_REGULAR.matches(s))
            return new int[] {1600, GlassRecords.Heat.KINDLED};
        return null;
    }

    static void fuel(CreateCompile c, CreateCompile.Info i, int gid, ItemStack is) {
        if (is == null || !(i.be instanceof BlazeBurnerBlockEntity b) || b.isCreative())
            return;
        int[] f = burnerFuel(is);
        if (f == null || f[0] <= 0)
            return;
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.FUEL, f[1], f[0], c.g.name("create:blaze_burner_fuel/" + c.g.goodName(is)));
        r.in(gid, 1, 1);
        final int burn = f[0];
        c.addCraft(i, r, 1, (rpm, batch) -> Rat.of(burn, 1), gid);
        if (i.node.params[2] <= 0 || burn < i.node.params[2])
            i.node.params[2] = burn;
    }

    // ---- multi-input machines: basins and crafter grids ---------------------------------------------

    static void multi(CreateCompile c, CreateCompile.Info i, IntOpenHashSet in) {
        if (in.size() == i.multiSeen)
            return;
        i.multiSeen = in.size();
        if (i.kind == CreateCompile.K.CRAFTER)
            crafter(c, i, in);
        else if (i.kind == CreateCompile.K.SPOUT)
            spout(c, i, in);
        else if (i.extra instanceof BasinBlockEntity basin)
            basin(c, i, basin, in);
    }

    /** The lowest-numbered good in {@code in} an ingredient accepts, or NONE (GB: one variant per recipe). */
    private static int pick(CreateCompile c, Ingredient ing, IntArrayList sorted) {
        for (int g : sorted) {
            ItemStack p = c.itemProto(g);
            if (p != null && ing.test(p))
                return g;
        }
        return GraphBuilder.NONE;
    }

    private static int pickFluid(CreateCompile c, SizedFluidIngredient ing, IntArrayList sorted) {
        for (int g : sorted) {
            FluidStack p = c.fluidProto(g);
            if (p != null && ing.ingredient().test(p))
                return g;
        }
        return GraphBuilder.NONE;
    }

    private static List<RecipeHolder<? extends Recipe<?>>> basinCandidates(CreateCompile c, boolean mixer) {
        List<RecipeHolder<? extends Recipe<?>>> out = new ArrayList<>();
        var rm = c.level.getRecipeManager();
        var cfg = AllConfigs.server().recipes;
        if (mixer) {
            out.addAll(rm.getAllRecipesFor(AllRecipeTypes.MIXING.<net.minecraft.world.item.crafting.RecipeInput, BasinRecipe>getType()));
            if (cfg.allowShapelessInMixer.get())
                for (RecipeHolder<CraftingRecipe> h : rm.getAllRecipesFor(RecipeType.CRAFTING))
                    if (h.value() instanceof ShapelessRecipe && h.value().getIngredients().size() > 1 && !MechanicalPressBlockEntity.canCompress(h.value()))
                        out.add(h);
        } else {
            out.addAll(rm.getAllRecipesFor(AllRecipeTypes.COMPACTING.<net.minecraft.world.item.crafting.RecipeInput, BasinRecipe>getType()));
            for (RecipeHolder<CraftingRecipe> h : rm.getAllRecipesFor(RecipeType.CRAFTING))
                if (!(h.value() instanceof MechanicalCraftingRecipe) && MechanicalPressBlockEntity.canCompress(h.value()))
                    out.add(h);
        }
        out.removeIf(AllRecipeTypes::shouldIgnoreInAutomation);
        return out;
    }

    private static void basin(CreateCompile c, CreateCompile.Info i, BasinBlockEntity basin, IntOpenHashSet in) {
        boolean mixer = i.kind == CreateCompile.K.MIXER;
        IntArrayList sorted = new IntArrayList(in);
        sorted.sort(null);
        FilteringBehaviour filter = basin.getFilter();
        if (i.candidates == null)
            i.candidates = basinCandidates(c, mixer);
        for (RecipeHolder<? extends Recipe<?>> h : i.candidates) {
            if (i.triedRecipes.contains(h.id()))
                continue;
            Recipe<?> rec = h.value();
            NonNullList<Ingredient> ings = rec.getIngredients();
            List<SizedFluidIngredient> fluids = rec instanceof ProcessingRecipe<?, ?> p ? p.getFluidIngredients() : List.of();
            int[] items = new int[ings.size()];
            boolean ok = true;
            for (int k = 0; k < ings.size() && ok; k++) {
                if (ings.get(k).isEmpty()) {
                    items[k] = GraphBuilder.NONE;
                    continue;
                }
                items[k] = pick(c, ings.get(k), sorted);
                ok = items[k] != GraphBuilder.NONE;
            }
            int[] fl = new int[fluids.size()];
            for (int k = 0; k < fluids.size() && ok; k++) {
                fl[k] = pickFluid(c, fluids.get(k), sorted);
                ok = fl[k] != GraphBuilder.NONE;
            }
            if (!ok)
                continue;
            List<ProcessingOutput> outs;
            List<FluidStack> fouts;
            HeatCondition heat = HeatCondition.NONE;
            if (rec instanceof ProcessingRecipe<?, ?> p) {
                outs = p.getRollableResults();
                fouts = p.getFluidResults();
                heat = p.getRequiredHeat();
            } else {
                ItemStack res = rec.getResultItem(c.level.registryAccess());
                outs = List.of(new ProcessingOutput(res, 1f));
                fouts = List.of();
            }
            // the basin's filter: outputs must match it (BasinOperatingBlockEntity.getMatchingRecipes)
            if (filter != null && !filter.getFilter().isEmpty() && !RecipeConditions.outputMatchesFilter(filter).test(h))
                continue;
            if (heat != HeatCondition.NONE && !c.hasHeater(i))
                continue;
            i.triedRecipes.add(h.id());
            int dur = duration(rec);
            GraphBuilder.Recipe r = new GraphBuilder.Recipe(rec instanceof CraftingRecipe ? (mixer ? GlassRecords.RecipeType.MIXING : GlassRecords.RecipeType.COMPACTING) : typeCode(rec),
                    heat(heat), dur, c.g.name(h.id().toString()));
            for (int g : items)
                if (g != GraphBuilder.NONE)
                    r.in(g, 1, 1);
            for (int k = 0; k < fl.length; k++)
                r.in(fl[k], fluids.get(k).amount(), 1);
            outputs(c, r, outs, 1);
            fluidOutputs(c, r, fouts);
            c.addCraft(i, r, 1, c.cycle(i, dur, 1), GraphBuilder.NONE);
        }
    }

    /** A crafter grid: recipes whose pattern fills exactly the grid's uncovered cells (either mirror). */
    private static void crafter(CreateCompile c, CreateCompile.Info i, IntOpenHashSet in) {
        @SuppressWarnings("unchecked")
        List<MechanicalCrafterBlockEntity> chain = (List<MechanicalCrafterBlockEntity>) i.extra;
        if (chain == null || chain.isEmpty())
            return;
        Direction facing = chain.get(0).getBlockState().getValue(MechanicalCrafterBlock.HORIZONTAL_FACING);
        Direction right = facing.getClockWise();
        int minU = Integer.MAX_VALUE, maxU = Integer.MIN_VALUE, minV = Integer.MAX_VALUE, maxV = Integer.MIN_VALUE;
        List<int[]> cells = new ArrayList<>();
        for (MechanicalCrafterBlockEntity m : chain) {
            boolean covered = m.craftingItemOrCoverPresent() && !m.craftingItemPresent();
            if (covered)
                continue;
            BlockPos p = m.getBlockPos();
            int u = p.getX() * right.getStepX() + p.getZ() * right.getStepZ();
            int v = p.getY();
            cells.add(new int[] {u, v});
            minU = Math.min(minU, u);
            maxU = Math.max(maxU, u);
            minV = Math.min(minV, v);
            maxV = Math.max(maxV, v);
        }
        if (cells.isEmpty())
            return;
        int w = maxU - minU + 1, h = maxV - minV + 1;
        boolean[][] occupied = new boolean[h][w];
        for (int[] cell : cells)
            occupied[maxV - cell[1]][cell[0] - minU] = true;
        IntArrayList sorted = new IntArrayList(in);
        sorted.sort(null);
        var rm = c.level.getRecipeManager();
        List<RecipeHolder<? extends Recipe<?>>> cands = new ArrayList<>(rm.getAllRecipesFor(AllRecipeTypes.MECHANICAL_CRAFTING.<net.minecraft.world.item.crafting.CraftingInput, MechanicalCraftingRecipe>getType()));
        if (AllConfigs.server().recipes.allowRegularCraftingInCrafter.get())
            cands.addAll(rm.getAllRecipesFor(RecipeType.CRAFTING));
        int depth = CreateCompile.crafterDepth((MechanicalCrafterBlockEntity) i.be);
        for (RecipeHolder<? extends Recipe<?>> hold : cands) {
            if (i.triedRecipes.contains(hold.id()) || AllRecipeTypes.shouldIgnoreInAutomation(hold))
                continue;
            Recipe<?> rec = hold.value();
            NonNullList<Ingredient> ings = rec.getIngredients();
            boolean fits;
            if (rec instanceof ShapedRecipe sr) {
                fits = sr.getWidth() == w && sr.getHeight() == h && (layout(ings, occupied, w, h, false) || layout(ings, occupied, w, h, true));
            } else if (rec instanceof ShapelessRecipe) {
                fits = ings.size() == cells.size();
            } else {
                fits = false;
            }
            if (!fits)
                continue;
            GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.CRAFTING, GlassRecords.Heat.NONE, 0, c.g.name(hold.id().toString()));
            boolean ok = true;
            for (Ingredient ing : ings) {
                if (ing.isEmpty())
                    continue;
                int g = pick(c, ing, sorted);
                if (g == GraphBuilder.NONE) {
                    ok = false;
                    break;
                }
                r.in(g, 1, 1);
            }
            if (!ok)
                continue;
            ItemStack res = rec.getResultItem(c.level.registryAccess());
            if (res.isEmpty())
                continue;
            i.triedRecipes.add(hold.id());
            r.out(c.good(res), res.getCount(), 1);
            final int d = depth;
            c.addCraft(i, r, 1, (rpm, batch) -> {
                int t = Cycles.crafter(d, rpm);
                return t <= 0 ? null : Rat.of(t, 1);
            }, GraphBuilder.NONE);
        }
    }

    /** True when a shaped pattern's non-empty cells are exactly the occupied grid cells. */
    private static boolean layout(NonNullList<Ingredient> ings, boolean[][] occupied, int w, int h, boolean mirror) {
        for (int row = 0; row < h; row++)
            for (int col = 0; col < w; col++) {
                Ingredient ing = ings.get(row * w + (mirror ? w - 1 - col : col));
                if (ing.isEmpty() == occupied[row][col])
                    return false;
            }
        return true;
    }
}
