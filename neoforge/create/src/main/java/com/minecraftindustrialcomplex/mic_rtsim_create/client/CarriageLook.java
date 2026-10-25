package com.minecraftindustrialcomplex.mic_rtsim_create.client;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarFeedClient;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarLook;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarLookReset;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarModel;
import com.minecraftindustrialcomplex.mic_rtsim.far.CarriageShape;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexBuffer;
import com.mojang.blaze3d.vertex.VertexFormat;
import com.mojang.blaze3d.vertex.VertexFormatElement;
import it.unimi.dsi.fastutil.ints.IntOpenHashSet;
import it.unimi.dsi.fastutil.longs.Long2LongMap;
import it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.block.BlockRenderDispatcher;
import net.minecraft.client.resources.model.BakedModel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.RenderShape;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.client.model.data.ModelData;
import org.lwjgl.system.MemoryUtil;

/**
 * Far carriages as the blocks their players built (docs/choices-dh-feed.md DHT64): a carriage's shape (its
 * blocks and bogeys, sent once by hash) is baked into one static vertex buffer per block layer, by the game's
 * own block renderer against a tiny level of the carriage's blocks, with Create's bogeys copied in; a hundred
 * identical coaches share one mesh. A bake runs a block at a time inside the frame's budget and picks up on the
 * next frame where it stopped, so a new design costs no hitch. Block entity renderers are not drawn (a far mesh
 * has no block entity data); blocks drawn only by one are absent.
 *
 * <p>Placed the way Create places a carriage: the midpoint of its two bogeys on the segment's midpoint, its
 * forward axis along the segment (heading, then pitch). Render thread.
 */
public final class CarriageLook implements FarLook {
    static final RenderType[] LAYERS = {RenderType.solid(), RenderType.cutoutMipped(), RenderType.cutout(), RenderType.translucent()};
    /** A mesh nobody drew for this long is freed (PLACEHOLDER). */
    static final long KEEP_NANOS = 60_000_000_000L;
    /** A shape asked for and not seen again for this long is no longer baked (PLACEHOLDER). */
    static final long WANT_NANOS = 10_000_000_000L;
    /** At most this many meshes at once; past it, new designs wait as boxes (PLACEHOLDER, about 30 MiB of GPU). */
    static final int MAX_MESHES = 512;
    /** Blocks that throw while drawn are left out of their carriage; past this many the carriage stays a box. */
    static final int MAX_BAD_BLOCKS = 8;
    /**
     * How far past its blocks a carriage mesh's vertices may reach (a block model sticking out of its block), and
     * how far about each bogey's block (Create's bogey model, {@link BogeyMeshes#REACH}); a vertex past that, or not
     * finite, keeps the carriage a box (DHT73).
     */
    static final float BLOCK_SLACK = 3f, BOGEY_SLACK = BogeyMeshes.REACH + 0.5f;

    /** A baked carriage: its parts and how they sit about the bogeys' midpoint. */
    private static final class Mesh {
        final List<FarModel.Part> parts;
        final float midX, midY, midZ, turn, radius;
        final int vertices;
        long used;

        Mesh(List<FarModel.Part> parts, float midX, float midY, float midZ, float turn, float radius, long used) {
            this.parts = parts;
            this.midX = midX;
            this.midY = midY;
            this.midZ = midZ;
            this.turn = turn;
            this.radius = radius;
            this.used = used;
            int n = 0;
            for (FarModel.Part p : parts)
                n += p.vertices();
            this.vertices = n;
        }

        void close() {
            for (FarModel.Part p : parts)
                p.buffer().close();
        }
    }

    /** A bake in progress, a block at a time across frames. */
    private static final class Job {
        final long hash;
        final CarriageShape shape;
        final ShapeLevel level;
        final IntOpenHashSet bad;
        final ByteBufferBuilder[] memory = new ByteBufferBuilder[LAYERS.length];
        final BufferBuilder[] builders = new BufferBuilder[LAYERS.length];
        final RandomSource random = RandomSource.create();
        final PoseStack ps = new PoseStack();
        int next, bogey, frames;
        long nanos;

        Job(long hash, CarriageShape shape, IntOpenHashSet bad) {
            this.hash = hash;
            this.shape = shape;
            this.level = new ShapeLevel(shape);
            this.bad = bad;
        }

        BufferBuilder builder(int layer) {
            if (builders[layer] == null) {
                memory[layer] = new ByteBufferBuilder(64 * 1024);
                builders[layer] = new BufferBuilder(memory[layer], VertexFormat.Mode.QUADS, DefaultVertexFormat.BLOCK);
            }
            return builders[layer];
        }

        void abandon() {
            for (int i = 0; i < memory.length; i++)
                if (memory[i] != null) {
                    memory[i].close();
                    memory[i] = null;
                    builders[i] = null;
                }
        }
    }

    private final Long2ObjectOpenHashMap<Mesh> meshes = new Long2ObjectOpenHashMap<>();
    /** Shapes asked for and not baked: hash to when last asked. */
    private final Long2LongOpenHashMap wanted = new Long2LongOpenHashMap();
    /** Shapes that would not decode or bake: drawn as boxes until the client leaves the world. */
    private final LongOpenHashSet failed = new LongOpenHashSet();
    private Job job;
    /** The far feed's last RESET, until what nobody asked for again since is freed (DHT70). */
    private final FarLookReset resets = new FarLookReset();
    private long clock, lastSweep;
    private int errors;
    private long bakes, bakeNanos, uploadNanos, bakeFrames, badBlocks, badMeshes;
    private double maxStepMs, maxPieceMs;
    /** What {@link #maxPieceMs} was: a block, a bogey or the upload. */
    private String maxPieceWhat = "none";

    @Override
    public Status status(FarMover m) {
        long h = m.shape();
        if (h == 0L || failed.contains(h))
            return Status.NONE;
        Mesh mesh = meshes.get(h);
        if (mesh != null) {
            mesh.used = clock;
            return Status.READY;
        }
        wanted.put(h, clock);
        return Status.PENDING;
    }

    /** The midpoint (carriage-local) the mesh of {@code hash} turns about, or null when none is baked ({@link CarriageProbe}). */
    float[] midOf(long hash) {
        Mesh mesh = meshes.get(hash);
        return mesh == null ? null : new float[]{mesh.midX, mesh.midY, mesh.midZ};
    }

    @Override
    public boolean place(FarMover m, double[] seg, double clock, boolean live, FarModel out) {
        Mesh mesh = meshes.get(m.shape());
        if (mesh == null)
            return false;
        double dx = seg[0] - seg[3], dy = seg[1] - seg[4], dz = seg[2] - seg[5];
        double horiz = Math.sqrt(dx * dx + dz * dz);
        float yaw = (float) Math.atan2(-dz, dx); // about +y, taking +x to the heading
        float pitch = (float) Math.atan2(dy, horiz);
        out.parts = mesh.parts;
        out.local.identity().rotateY(yaw).rotateZ(pitch).rotateY(mesh.turn).translate(-mesh.midX, -mesh.midY, -mesh.midZ);
        out.radius = mesh.radius;
        out.lightYaw = Float.NaN; // block layers are shaded in the bake, not by the level's lights
        out.live = null;
        return true;
    }

    @Override
    public void frame(long now, long deadline) {
        clock = now;
        if (Minecraft.getInstance().level == null)
            return;
        if (now - lastSweep > 1_000_000_000L) {
            lastSweep = now;
            sweep(now);
        }
        if (resets.due(now))
            freeUnasked();
        long t0 = System.nanoTime();
        while (System.nanoTime() < deadline) {
            if (job == null && (job = next()) == null)
                break;
            Job j = job;
            try {
                if (step(j, deadline)) {
                    job = null;
                    finish(j, now);
                }
            } catch (RuntimeException | LinkageError e) {
                job = null;
                j.abandon();
                if (e instanceof BadBlock b && j.bad.size() < MAX_BAD_BLOCKS) {
                    // Leave the block out and start the carriage again (a half-written block would break its layer)
                    j.bad.add(b.index);
                    badBlocks++;
                    if (errors++ < 5)
                        MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] a far carriage's block {} would not draw; left out",
                                b.state, b.getCause());
                    job = new Job(j.hash, j.shape, j.bad);
                    continue;
                }
                failed.add(j.hash);
                wanted.remove(j.hash);
                if (errors++ < 5)
                    MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] far carriage {} would not bake; it stays a box",
                            Long.toHexString(j.hash), e);
            }
        }
        maxStepMs = Math.max(maxStepMs, (System.nanoTime() - t0) / 1e6);
    }

    /** The next shape to bake whose bytes are here, or null. */
    private Job next() {
        if (wanted.isEmpty() || meshes.size() >= MAX_MESHES)
            return null;
        for (var it = wanted.long2LongEntrySet().fastIterator(); it.hasNext(); ) {
            Long2LongMap.Entry e = it.next();
            long h = e.getLongKey();
            if (meshes.containsKey(h) || failed.contains(h)) {
                it.remove();
                continue;
            }
            byte[] bytes = FarFeedClient.shape(h);
            if (bytes == null)
                continue; // still on the way
            it.remove();
            try {
                return new Job(h, CarriageShape.decode(h, bytes), new IntOpenHashSet());
            } catch (RuntimeException bad) {
                failed.add(h);
                if (errors++ < 5)
                    MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] far carriage {} would not decode; it stays a box",
                            Long.toHexString(h), bad);
            }
        }
        return null;
    }

    /** A block that threw while it was drawn. */
    private static final class BadBlock extends RuntimeException {
        final int index;
        final BlockState state;

        BadBlock(int index, BlockState state, Throwable cause) {
            super(cause);
            this.index = index;
            this.state = state;
        }
    }

    /** Runs a bake until the deadline or the end; true when every block and bogey is in. Always does some work. */
    private boolean step(Job j, long deadline) {
        j.frames++;
        BlockRenderDispatcher brd = Minecraft.getInstance().getBlockRenderer();
        List<CarriageShape.Block> blocks = j.shape.blocks();
        List<CarriageShape.Bogey> bogeys = j.shape.bogeys();
        Direction.Axis axis = Direction.from3DDataValue(j.shape.assembly()).getAxis();
        long t0 = System.nanoTime();
        try {
            do {
                long p0 = System.nanoTime();
                String what;
                if (j.next < blocks.size()) {
                    int index = j.next++;
                    what = "block";
                    if (j.bad.contains(index))
                        continue;
                    CarriageShape.Block b = blocks.get(index);
                    BlockState state = Block.stateById(b.state());
                    if (state.getRenderShape() != RenderShape.MODEL)
                        continue;
                    what = state.toString();
                    try {
                        BlockPos pos = new BlockPos(b.x(), b.y(), b.z());
                        BakedModel model = brd.getBlockModel(state);
                        j.random.setSeed(state.getSeed(pos));
                        ModelData data = model.getModelData(j.level, pos, state, ModelData.EMPTY);
                        for (RenderType rt : model.getRenderTypes(state, j.random, data)) {
                            j.ps.pushPose();
                            j.ps.translate(b.x(), b.y(), b.z());
                            brd.renderBatched(state, pos, j.level, j.ps, j.builder(layerOf(rt)), true, j.random, data, rt);
                            j.ps.popPose();
                        }
                    } catch (RuntimeException | LinkageError e) {
                        throw new BadBlock(index, state, e);
                    }
                } else if (j.bogey < bogeys.size()) {
                    // Create draws bogeys with its own renderer, so the block list has none of their geometry
                    CarriageShape.Bogey g = bogeys.get(j.bogey++);
                    what = "bogey " + g.style();
                    BogeyMeshes.Mesh mesh = BogeyMeshes.get(g.style(), g.size(), axis, g.upsideDown());
                    if (mesh != null)
                        for (int layer = 0; layer < LAYERS.length; layer++)
                            if (mesh.count[layer] > 0)
                                mesh.replay(layer, j.builder(layer), g.x(), g.y(), g.z());
                } else
                    return true;
                piece(p0, what);
            } while (System.nanoTime() < deadline);
            return false;
        } finally {
            j.nanos += System.nanoTime() - t0;
        }
    }

    /** Keeps the longest piece of a bake (a block, a bogey, an upload) since the last {@link #stats}. */
    private void piece(long t0, String what) {
        double ms = (System.nanoTime() - t0) / 1e6;
        if (ms > maxPieceMs) {
            maxPieceMs = ms;
            maxPieceWhat = what;
        }
    }

    /** Places the mesh on its bogeys' midpoint and uploads a buffer per layer. */
    private void finish(Job j, long now) {
        CarriageShape shape = j.shape;
        float midX = 0.5f, midY = 0f, midZ = 0.5f;
        List<CarriageShape.Bogey> bogeys = shape.bogeys();
        if (bogeys.size() >= 2) {
            CarriageShape.Bogey a = bogeys.get(0), c = bogeys.get(1);
            midX = (a.x() + c.x()) / 2f + 0.5f;
            midY = (a.y() + c.y()) / 2f;
            midZ = (a.z() + c.z()) / 2f + 0.5f;
        } else if (!bogeys.isEmpty()) {
            CarriageShape.Bogey a = bogeys.get(0);
            midX = a.x() + 0.5f;
            midY = a.y();
            midZ = a.z() + 0.5f;
        }
        float maxR2 = 0;
        for (CarriageShape.Block b : shape.blocks()) {
            float dx = b.x() + 0.5f - midX, dy = b.y() + 0.5f - midY, dz = b.z() + 0.5f - midZ;
            maxR2 = Math.max(maxR2, dx * dx + dy * dy + dz * dz);
        }
        long t1 = System.nanoTime();
        List<FarModel.Part> parts = new ArrayList<>(LAYERS.length);
        float[] own = bounds(shape);
        String bad = null;
        try {
            for (int i = 0; i < LAYERS.length && bad == null; i++) {
                if (j.builders[i] == null)
                    continue;
                MeshData md = j.builders[i].build();
                if (md != null && (bad = outside(md, own)) != null) {
                    md.close();
                    break;
                }
                if (md != null) {
                    int n = md.drawState().vertexCount();
                    VertexBuffer vb = new VertexBuffer(VertexBuffer.Usage.STATIC);
                    try {
                        vb.bind();
                        vb.upload(md);
                    } catch (RuntimeException | LinkageError x) {
                        vb.close();
                        throw x;
                    } finally {
                        VertexBuffer.unbind();
                    }
                    parts.add(new FarModel.Part(LAYERS[i], vb, n));
                }
            }
        } catch (RuntimeException | LinkageError e) {
            for (FarModel.Part part : parts)
                part.buffer().close();
            throw e;
        } finally {
            j.abandon();
        }
        uploadNanos += System.nanoTime() - t1;
        piece(t1, "upload");
        if (bad != null) {
            // A vertex outside the carriage's own bounds: drawn, it would be a sliver across the sky (DHT73)
            for (FarModel.Part part : parts)
                part.buffer().close();
            failed.add(j.hash);
            wanted.remove(j.hash);
            badMeshes++;
            if (badMeshes <= 10)
                MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] far carriage {} baked a vertex outside its own bounds"
                        + " ({}); it stays a box ({} so far)", Long.toHexString(j.hash), bad, badMeshes);
            return;
        }
        if (parts.isEmpty()) {
            // Nothing drew (its blocks all drawn by block entities, its bogeys a style with no mesh): a box (DHT63)
            failed.add(j.hash);
            wanted.remove(j.hash);
            return;
        }
        Direction forward = Direction.from3DDataValue(shape.assembly()).getOpposite();
        meshes.put(j.hash, new Mesh(List.copyOf(parts), midX, midY, midZ, forwardToX(forward),
                (float) Math.sqrt(maxR2) + 1.5f, now));
        bakes++;
        bakeNanos += j.nanos;
        bakeFrames += j.frames;
    }

    /**
     * The carriage's own bounds, {min x, y, z, max x, y, z} in its block coordinates: its blocks with
     * {@link #BLOCK_SLACK}, and each bogey's block with {@link #BOGEY_SLACK}.
     */
    static float[] bounds(CarriageShape shape) {
        float[] b = {Float.MAX_VALUE, Float.MAX_VALUE, Float.MAX_VALUE, -Float.MAX_VALUE, -Float.MAX_VALUE,
                -Float.MAX_VALUE};
        for (CarriageShape.Block k : shape.blocks())
            grow(b, k.x(), k.y(), k.z(), BLOCK_SLACK);
        for (CarriageShape.Bogey g : shape.bogeys())
            grow(b, g.x(), g.y(), g.z(), BOGEY_SLACK);
        return b;
    }

    private static void grow(float[] b, int x, int y, int z, float slack) {
        b[0] = Math.min(b[0], x - slack);
        b[1] = Math.min(b[1], y - slack);
        b[2] = Math.min(b[2], z - slack);
        b[3] = Math.max(b[3], x + 1 + slack);
        b[4] = Math.max(b[4], y + 1 + slack);
        b[5] = Math.max(b[5], z + 1 + slack);
    }

    /** The first vertex of {@code md} outside {@code b} or not finite, said in words; null when there is none. */
    static String outside(MeshData md, float[] b) {
        VertexFormat format = md.drawState().format();
        int stride = format.getVertexSize(), n = md.drawState().vertexCount();
        long at = MemoryUtil.memAddress(md.vertexBuffer()) + format.getOffset(VertexFormatElement.POSITION);
        for (int k = 0; k < n; k++, at += stride) {
            float x = MemoryUtil.memGetFloat(at), y = MemoryUtil.memGetFloat(at + 4), z = MemoryUtil.memGetFloat(at + 8);
            // Written so that NaN fails too
            if (!(x >= b[0] && x <= b[3] && y >= b[1] && y <= b[4] && z >= b[2] && z <= b[5]))
                return String.format(Locale.ROOT, "vertex %d of %d at %s %s %s, bounds %.1f %.1f %.1f to %.1f %.1f %.1f",
                        k, n, x, y, z, b[0], b[1], b[2], b[3], b[4], b[5]);
        }
        return null;
    }

    /** Frees meshes nobody drew for {@link #KEEP_NANOS} and forgets shapes nobody asked for in {@link #WANT_NANOS}. */
    private void sweep(long now) {
        for (var it = meshes.long2ObjectEntrySet().fastIterator(); it.hasNext(); ) {
            Long2ObjectMap.Entry<Mesh> e = it.next();
            if (now - e.getValue().used > KEEP_NANOS) {
                e.getValue().close();
                it.remove();
            }
        }
        wanted.long2LongEntrySet().removeIf(e -> now - e.getLongValue() > WANT_NANOS);
    }

    @Override
    public void clear() {
        if (job != null) {
            job.abandon();
            job = null;
        }
        for (Mesh m : meshes.values())
            m.close();
        meshes.clear();
        wanted.clear();
        resets.clear();
        failed.clear();
        BogeyMeshes.clear();
    }

    /**
     * The far feed started over (docs/choices-dh-feed.md DHT70): the meshes and asks of carriages no mover asks for
     * again within {@link FarLookReset#GRACE_NANOS} go then ({@link #freeUnasked}); the bogeys' meshes the prewarm
     * drew stay, so the next bake pays no cold first draw, as do the shapes that would not bake (a hash names its
     * blocks, so they would not bake again).
     */
    @Override
    public void reset() {
        resets.reset(clock);
    }

    /** A RESET's grace is over: frees the meshes and forgets the asks no mover asked for since. */
    private void freeUnasked() {
        for (var it = meshes.long2ObjectEntrySet().fastIterator(); it.hasNext(); ) {
            Long2ObjectMap.Entry<Mesh> e = it.next();
            if (resets.stale(e.getValue().used)) {
                e.getValue().close();
                it.remove();
            }
        }
        wanted.long2LongEntrySet().removeIf(e -> resets.stale(e.getLongValue()));
    }

    @Override
    public void prewarm() {
        BogeyMeshes.prewarm();
    }

    @Override
    public String stats() {
        long vertices = 0;
        for (Mesh m : meshes.values())
            vertices += m.vertices;
        String s = String.format(Locale.ROOT,
                "carriages: %d meshes (%d vertices, %.1f MiB), %d wanted, %d failed; %d bakes, %.2f ms each over %.1f frames, "
                        + "upload %.2f ms each, longest frame's bake %.2f ms, longest piece %.2f ms (%s); %d blocks left "
                        + "out; %d meshes outside their bounds; bogeys %d drawn in %.1f ms (%d outside their bounds)",
                meshes.size(), vertices, vertices * (double) DefaultVertexFormat.BLOCK.getVertexSize() / (1 << 20),
                wanted.size(), failed.size(), bakes, bakes == 0 ? 0.0 : bakeNanos / 1e6 / bakes,
                bakes == 0 ? 0.0 : bakeFrames / (double) bakes, bakes == 0 ? 0.0 : uploadNanos / 1e6 / bakes, maxStepMs,
                maxPieceMs, maxPieceWhat, badBlocks, badMeshes, BogeyMeshes.drawn, BogeyMeshes.drawNanos / 1e6,
                BogeyMeshes.outside);
        maxStepMs = 0;
        maxPieceMs = 0;
        maxPieceWhat = "none";
        return s;
    }

    static int layerOf(RenderType rt) {
        for (int i = 0; i < LAYERS.length; i++)
            if (LAYERS[i] == rt)
                return i;
        return rt.sortOnUpload() ? 3 : 2;
    }

    /** The rotation about +y taking the carriage's forward direction to +x. */
    static float forwardToX(Direction forward) {
        return switch (forward) {
            case WEST -> (float) Math.PI;
            case SOUTH -> (float) (Math.PI / 2);
            case NORTH -> (float) (-Math.PI / 2);
            default -> 0f;
        };
    }
}
