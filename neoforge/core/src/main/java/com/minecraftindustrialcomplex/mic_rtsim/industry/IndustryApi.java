package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * The industry entry point for plugins (INDUSTRY.md §12): register an {@link IndustryPlugin} and the
 * handlers of industry commands the core does not apply itself (IND_CARRIAGE_DELTA is Create's). Call
 * the registration methods during mod construction.
 */
public final class IndustryApi {
    private IndustryApi() {}

    static final List<IndustryPlugin> PLUGINS = new CopyOnWriteArrayList<>();
    static final Map<Integer, CommandHandler> COMMANDS = new ConcurrentHashMap<>();

    /** Applies one industry command (kind 0x8400 to 0x84FF) on the server thread. */
    public interface CommandHandler {
        /**
         * The command's record is at {@code at} in {@code b} (little-endian), {@code size} bytes with its
         * header. Return false to leave it, and everything after it, for a later tick (a quota is spent);
         * true when it is done with. A handler answers its own op id (COMMAND_RESULT and its result
         * record) through {@link Industry}.
         */
        boolean accept(Industry industry, ByteBuffer b, int at, int size, int aux, long cmdSeq);
    }

    public static void register(IndustryPlugin plugin) {
        PLUGINS.add(plugin);
    }

    public static void registerCommand(int kind, CommandHandler handler) {
        if (!IndRecords.isIndustryCommand(kind))
            throw new IllegalArgumentException("not an industry command kind: 0x" + Integer.toHexString(kind));
        if (COMMANDS.putIfAbsent(kind, handler) != null)
            throw new IllegalStateException("industry command 0x" + Integer.toHexString(kind) + " already has a handler");
    }

    public static List<IndustryPlugin> plugins() {
        return PLUGINS;
    }

    /** This server's industry module, or null (no session, the bridge failed, or the module is off). */
    public static Industry current() {
        RtsimSession s = RtsimSession.current();
        if (s == null || s.failed())
            return null;
        Industry i = s.industry;
        return i != null && i.enabled() ? i : null;
    }
}
