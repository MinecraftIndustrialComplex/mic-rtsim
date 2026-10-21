package com.minecraftindustrialcomplex.mic_rtsim.boundary;

/**
 * Command records: the engine writes them, Java reads and applies them. Kinds are 0x8000 and up.
 *
 * <p>Every command starts with a command sequence number at offset 8: unique and increasing per
 * world. Java remembers the last one it applied in the journal and skips any command at or below it
 * (PLAN.md §4.9). Ids of dimensions, items and body kinds in commands refer to the engine's own
 * dictionary, defined by {@link #DICT} commands; it is per session, so the engine sends it again
 * after every session start.
 */
public final class Cmd {
    private Cmd() {}

    public static final int DICT = 0x8001;
    public static final int SPAWN_GROUP = 0x8002;
    public static final int DESPAWN_GROUP = 0x8003;
    public static final int STEER_GROUP = 0x8004;
    public static final int TRANSFER_TO_CONTAINER = 0x8005;

    // Phase 1: caravans and markets (PLAN.md §8).
    public static final int MARKET = 0x8010;
    public static final int MERCHANT_OFFERS = 0x8011;
    public static final int WRITE_GOSSIP = 0x8012;
    public static final int SENTIMENT = 0x8013;
    public static final int PLACE_HOUSE = 0x8014;
    public static final int SPAWN_RESIDENT = 0x8015;

    /** u64. Command sequence number. */
    public static final int CMD_SEQ = 8;
    /** First byte of a command's body. */
    public static final int BODY = 16;

    /** DICT, aux = dictionary kind ({@link Obs#DICT_DIMENSION} and so on). */
    public static final class Dict {
        public static final int ID = 16;       // u32, from 1
        public static final int NAME_LEN = 20; // u32
        public static final int NAME = 24;     // UTF-8, zero-padded to 8

        public static int size(int nameBytes) {
            return Layout.align(NAME + nameBytes);
        }
    }

    /**
     * SPAWN_GROUP, aux = member count. Materialize a group as one unit (C25): every member in the same
     * tick, or none. Answered by MATERIALIZED or SPAWN_REJECTED.
     */
    public static final class SpawnGroup {
        public static final int AGENT = 16;      // u64
        public static final int EPOCH = 24;      // u32, the new materialization epoch
        public static final int GROUP_KIND = 28; // u32 engine dictionary id (DICT_BODY)
        public static final int DIM = 32;        // u32 engine dictionary id (DICT_DIMENSION)
        public static final int FLAGS = 36;      // u32, below
        public static final int X = 40;          // f64, the group's anchor
        public static final int Y = 48;          // f64
        public static final int Z = 56;          // f64
        public static final int MEMBERS = 64;    // aux x Member, each followed by its items

        /**
         * {@code Y} is the surface: each member stands on the highest motion-blocking, non-leaf block
         * at its own x, z (the anchor's x and z plus the member's slot offset), whatever {@code Y} says.
         */
        public static final int FLAG_SURFACE = 1;

        /** A member block: 24 bytes, then ITEM_COUNT x {@link Obs.Item} (items use engine ids). */
        public static final class Member {
            public static final int INDEX = 0;      // u32, position in the group kind
            public static final int ITEM_COUNT = 4; // u32
            public static final int HEALTH = 8;     // f32, 0 = the body's full health
            public static final int BODY_A = 12;    // i32, -1 = let the body roll it
            public static final int BODY_B = 16;    // i32, -1 = let the body roll it
            // 20: u32 reserved
            public static final int ITEMS = 24;

            public static int size(int items) {
                return ITEMS + items * Obs.Item.SIZE;
            }
        }
    }

    /** DESPAWN_GROUP. Capture every member and remove the group. Answered by DEMATERIALIZED. */
    public static final class DespawnGroup {
        public static final int AGENT = 16;  // u64
        public static final int EPOCH = 24;  // u32
        public static final int REASON = 28; // u32, engine-defined, echoed nowhere
        public static final int SIZE = 32;
    }

    /** STEER_GROUP. Point the group's leader at a position; the members follow it. */
    public static final class SteerGroup {
        public static final int AGENT = 16; // u64
        public static final int EPOCH = 24; // u32
        public static final int SPEED = 28; // f32, a multiplier; 0 = the body's own speed
        public static final int X = 32;     // f64
        public static final int Y = 40;     // f64
        public static final int Z = 48;     // f64
        public static final int SIZE = 56;
    }

    /**
     * TRANSFER_TO_CONTAINER. Put items into the container at a block position: a cross-store transfer
     * from the engine into Minecraft. Answered by COMMAND_RESULT. Java applies an op id at most once.
     */
    public static final class TransferToContainer {
        public static final int OP_ID = 16; // u64
        public static final int DIM = 24;   // u32 engine dictionary id
        public static final int ITEM = 28;  // u32 engine dictionary id
        public static final int X = 32;     // i32
        public static final int Y = 36;     // i32
        public static final int Z = 40;     // i32
        public static final int COUNT = 44; // i32
        public static final int SIZE = 48;
    }

    // ---- phase 1 --------------------------------------------------------------------------------

    /**
     * MARKET, aux = goods count. A village market's offers, ready-made (Ben's morning review,
     * "Prices": one source of truth, the engine's; G31: a player's standing is folded into them as
     * price levels). Per good, two ladders, the market's sales (the player buys) then its purchases:
     * for each price level, the bundles a run of trades meets one after another, each priced at the
     * market the trades before it left. The bridge only shows them: it finds where its own trades
     * since {@code AS_OF_OBS_SEQ} left each ladder, and uses the step there (choice PR2). Latest wins;
     * no answer. Units are rtsim-trade's: stock and treasury in thousandths.
     */
    public static final class Market {
        public static final int VILLAGE = 16;         // u64
        public static final int AS_OF_OBS_SEQ = 24;   // u64
        public static final int CURRENCY = 32;        // u32 engine dictionary (item)
        public static final int FLAGS = 36;           // u32, reserved
        public static final int TREASURY = 40;        // i64, thousandths of a coin
        public static final int TREASURY_TARGET = 48; // i64, thousandths (Mbar); above twice it the market refuses cash; 0 = never
        public static final int LEVELS = 56;          // u8, price levels (level 0 is fair)
        public static final int DEPTH = 57;           // u8, bundles per ladder
        public static final int THRESHOLDS = 64;      // MAX_LEVELS x i32: level k applies at standing <= it (level 0: 127)
        public static final int MAX_LEVELS = 8;
        public static final int MAX_DEPTH = 32;
        public static final int GOODS = 96;           // aux x good
        public static final int SIDE_SELLS = 0;
        public static final int SIDE_BUYS = 1;
        public static final int STEP = 8;             // i32 items, i32 coins; (0, 0) ends a ladder

        /** Good offsets, from the good's start. */
        public static final int G_ITEM = 0;    // u32 engine dictionary (item)
        public static final int G_FLAGS = 4;   // u32, reserved
        public static final int G_STOCK = 8;   // i64, thousandths: the stock the ladders start from
        public static final int G_LADDERS = 16;

        public static int good(int levels, int depth) {
            return G_LADDERS + 2 * levels * depth * STEP;
        }

        public static int size(int goods, int levels, int depth) {
            return GOODS + goods * good(levels, depth);
        }

        /** Step {@code j} of {@code side}'s ladder at level {@code k}, from the good's start. */
        public static int step(int levels, int depth, int side, int k, int j) {
            return G_LADDERS + ((side * levels + k) * depth + j) * STEP;
        }
    }

    /**
     * MERCHANT_OFFERS, aux = offer count. A puppet merchant's offers, as the engine prices them from
     * the news it carries (PLAN.md §8.5). Every result comes out of the member's cargo and every payment
     * goes into it (C21), so an offer's uses are capped by the cargo and the room for payment.
     */
    public static final class MerchantOffers {
        public static final int AGENT = 16;  // u64
        public static final int EPOCH = 24;  // u32
        public static final int MEMBER = 28; // u32
        public static final int OFFERS = 32; // aux x Offer
        public static final int OFFER = 40;

        public static int size(int offers) {
            return OFFERS + offers * OFFER;
        }

        public static final int O_COST_A_ITEM = 0;   // u32 engine dictionary
        public static final int O_COST_A_COUNT = 4;  // i32
        public static final int O_COST_B_ITEM = 8;   // u32, 0 = none
        public static final int O_COST_B_COUNT = 12; // i32
        public static final int O_RESULT_ITEM = 16;  // u32
        public static final int O_RESULT_COUNT = 20; // i32
        public static final int O_MAX_USES = 24;     // i32, 0 = the bridge's default
        public static final int O_XP = 28;           // i32
        public static final int O_PRICE_MULT = 32;   // f32
        public static final int O_FLAGS = 36;        // u32, below

        /** The merchant sells goods for its currency (TRADE side MERCHANT_SELLS). */
        public static final int O_FLAG_SELLS = 1;
        /** The merchant buys goods for its currency (TRADE side MERCHANT_BUYS). */
        public static final int O_FLAG_BUYS = 2;
    }

    /**
     * WRITE_GOSSIP. Write a slight or a fair trade into a vanilla village's gossip (C20) as the player's
     * entry, through {@code GossipContainer.add}. Each villager takes an op id at most once. Answered by
     * COMMAND_RESULT (APPLIED: written to the loaded residents and kept for the others' next census).
     */
    public static final class WriteGossip {
        public static final int OP_ID = 16;        // u64
        public static final int VILLAGE = 24;      // u64
        public static final int PLAYER_MOST = 32;  // u64
        public static final int PLAYER_LEAST = 40; // u64
        public static final int GOSSIP = 48;       // u32: 1 major_negative, 2 minor_negative, 3 minor_positive, 4 major_positive, 5 trading (rtsim-knowledge GossipType)
        public static final int AMOUNT = 52;       // i32
        public static final int REPORT = 56;       // u32 rtsim-knowledge ReportKind (1 assault ... 8 resident killing)
        public static final int FLAGS = 60;        // u32, below
        public static final int SIZE = 64;

        /** The village learned it first-hand (a witness), not as news. */
        public static final int FLAG_FIRST_HAND = 1;
    }

    /**
     * SENTIMENT, aux = entries. A holder's (a village's) feelings toward players, on rtsim-knowledge's
     * scale (-128 to 127), with the engine's thresholds (P18 tunables): the ribbit sorcerers leave out
     * a player at or below {@code DISLIKE}. Latest wins; no answer.
     */
    public static final class Sentiment {
        public static final int HOLDER = 16;  // u64, a village id
        public static final int DISLIKE = 24; // i32, at or below: disliked
        public static final int FRIEND = 28;  // i32, at or above: a friend
        public static final int FLAGS = 32;   // u32, below
        // 36: u32 reserved
        public static final int ENTRIES = 40; // aux x Entry
        public static final int ENTRY = 24;   // u64 most, u64 least, i32 sentiment, u32 reserved

        public static int size(int entries) {
            return ENTRIES + entries * ENTRY;
        }

        /** A full snapshot: players not listed are neutral. Without it the entries update the last one. */
        public static final int FLAG_REPLACE = 1;
    }

    /**
     * PLACE_HOUSE. Grow a village by one house from a structure template (C7, P17's recommendation),
     * on a free plot, under the block quota, never over player-placed blocks. Waits while the plot's
     * chunks are not loaded. Answered by HOUSE_PLACED then COMMAND_RESULT(APPLIED), or
     * COMMAND_RESULT(NOT_FREE or FAILED).
     */
    public static final class PlaceHouse {
        public static final int OP_ID = 16;      // u64
        public static final int VILLAGE = 24;    // u64
        public static final int DIM = 32;        // u32 engine dictionary
        public static final int TEMPLATE = 36;   // u32 engine dictionary (DICT_TEMPLATE)
        public static final int PROCESSORS = 40; // u32 engine dictionary (DICT_PROCESSORS), 0 = none
        public static final int X = 44;          // i32: the origin, or with SEARCH the centre of the search
        public static final int Y = 48;          // i32
        public static final int Z = 52;          // i32
        public static final int ROTATION = 56;   // u8, 0 to 3 quarter turns clockwise
        public static final int MIRROR = 57;     // u8, 0 none, 1 left-right, 2 front-back
        public static final int FLAGS = 58;      // u16, below
        public static final int RADIUS = 60;     // u32, with SEARCH: how far from the centre to look
        public static final int SEED = 64;       // u64, the placement's random (a resumed placement is identical)
        public static final int Y_OFFSET = 72;   // i32, added to the height the bridge picks
        // 76: u32 reserved
        public static final int SIZE = 80;

        /** Look for a free plot on rings around (x, z), out to the radius; else use (x, y, z) as is. */
        public static final int FLAG_SEARCH = 1;
        /** Take y from the village's base level (its start piece's floor), not from the command. */
        public static final int FLAG_Y_FROM_VILLAGE = 2;
    }

    /**
     * SPAWN_RESIDENT. A new resident for a village (C7 growth). Residents are ordinary entities, saved
     * by vanilla (C2: they are not puppets). Answered by COMMAND_RESULT; spawns at most once per op id.
     */
    public static final class SpawnResident {
        public static final int OP_ID = 16;      // u64
        public static final int VILLAGE = 24;    // u64
        public static final int DIM = 32;        // u32 engine dictionary
        public static final int BODY = 36;       // u32 engine dictionary (DICT_BODY): the entity type, e.g. ribbits:ribbit
        public static final int PROFESSION = 40; // u32 engine dictionary (DICT_PROFESSION), 0 = the body's default
        public static final int FLAGS = 44;      // u32, below
        public static final int X = 48;          // f64
        public static final int Y = 56;          // f64
        public static final int Z = 64;          // f64
        public static final int HOUSE = 72;      // u64, with AT_HOUSE: the PLACE_HOUSE op whose resident slot to use
        public static final int SIZE = 80;

        /** Spawn at the resident slot of house {@code HOUSE}; wait for the house if it is still being built. */
        public static final int FLAG_AT_HOUSE = 1;
        /** Spawn at the village's meeting point (a bought work's body: SINK.md's golems). */
        public static final int FLAG_AT_VILLAGE = 2;
    }
}
