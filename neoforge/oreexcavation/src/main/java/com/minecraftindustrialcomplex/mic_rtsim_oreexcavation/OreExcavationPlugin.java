package com.minecraftindustrialcomplex.mic_rtsim_oreexcavation;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.tom.createores.OreDataAttachment;
import com.tom.createores.block.entity.ExcavatingBlockEntity;
import com.tom.createores.recipe.VeinRecipe;
import com.tom.createores.OreData;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;

/**
 * Ore Excavation veins as finite deposits (I11, F15; INDUSTRY.md §6 case 4).
 *
 * <ul>
 *   <li><b>Read</b> once per sweep, for each loaded member chunk with a drilling machine or extractor
 *       ({@code ExcavatingBlockEntity}): {@code OreDataAttachment.getData(chunk)}, the vein's recipe, and
 *       {@code OreData.getResourcesRemaining(recipe)}, in extraction cycles (each finished cycle calls
 *       {@code extract(1)}). Ore Excavation answers 0 for an infinite vein (sent with the infinite flag,
 *       so it is never stock) and -1 for an exhausted one (sent as 0). Only chunks with a machine are read:
 *       {@code getData} generates a vein on first access (choice Q4).</li>
 *   <li><b>Good:</b> {@code d/0/<vein recipe id>/<chunk hash>}, one per chunk ({@link Deposits}).</li>
 *   <li><b>Write</b> in the chunk's reconcile (WithChunk): {@code OreData.extract(n)} adds n to the
 *       extracted amount (a negative n gives cycles back), clamped so the vein never goes below empty;
 *       the applied amount is the change in what remains. The attachment is saved with the chunk, with the
 *       reconcile's epoch.</li>
 * </ul>
 */
public final class OreExcavationPlugin implements IndustryPlugin {

    @Override
    public void deposits(ServerLevel level, District district, LevelChunk chunk, DepositSink out) {
        boolean machine = false;
        for (BlockEntity be : chunk.getBlockEntities().values())
            if (be instanceof ExcavatingBlockEntity<?>) {
                machine = true;
                break;
            }
        if (!machine)
            return;
        OreData data = OreDataAttachment.getData(chunk);
        VeinRecipe recipe = recipe(level, data);
        if (recipe == null)
            return;
        long remaining = data.getResourcesRemaining(recipe);
        String good = Deposits.name(data.getRecipeId(), level.dimension(), chunk.getPos().x, chunk.getPos().z);
        out.add(IndRecords.DepositPlugin.ORE_EXCAVATION, good, Math.max(0, remaining), remaining == 0);
    }

    @Override
    public Long applyDeposit(ServerLevel level, int chunkX, int chunkZ, String good, long delta) {
        ResourceLocation kind = Deposits.kindIn(good, level.dimension(), chunkX, chunkZ);
        if (kind == null)
            return null;
        LevelChunk chunk = level.getChunkSource().getChunkNow(chunkX, chunkZ);
        if (chunk == null)
            return null; // not loaded: not this call's (a reconcile always has its chunk loaded)
        OreData data = OreDataAttachment.getData(chunk);
        if (!kind.equals(data.getRecipeId()))
            return null;
        VeinRecipe recipe = recipe(level, data);
        if (recipe == null)
            return 0L;
        long before = data.getResourcesRemaining(recipe);
        if (before == 0)
            return 0L; // infinite: never stock, so the engine sends it nothing
        long have = Math.max(0, before);
        long n = delta < 0 ? Math.min(-delta, have) : -delta;
        if (n == 0)
            return 0L;
        data.extract((int) Math.max(Integer.MIN_VALUE, Math.min(Integer.MAX_VALUE, n)));
        chunk.setUnsaved(true);
        long after = data.getResourcesRemaining(recipe); // a finite vein answers > 0, or -1 when exhausted
        return Math.max(0, after) - have;
    }

    private static VeinRecipe recipe(ServerLevel level, OreData data) {
        if (data.getRecipeId() == null)
            return null;
        RecipeHolder<VeinRecipe> h = data.getRecipe(level.getRecipeManager());
        return h == null ? null : h.value();
    }
}
