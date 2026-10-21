// SPDX-License-Identifier: AGPL-3.0-or-later
//! The JNI entry points of `com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine`.
//!
//! Java loads this library twice over: mic_native `dlopen`s it as a plugin and runs its init
//! (`MicNative.loadPlugin("mic_rtsim", "rtsim_jni")`), then `NativeEngine` calls `System.load` on
//! the same path so these functions bind to the mod's own class loader. Both reach the same
//! instance of the library.
//!
//! Every entry point runs inside `EnvUnowned::with_env`, which catches panics; a panic or a JNI
//! error becomes a `RuntimeException`. Failures a caller can expect return a negative status with
//! the message in `nLastError()`, as mic_native's own `HostNative` does.
//!
//! **Handles.** `nOpen` returns a boxed `Arc<Engine>` as a `long`; every other call borrows it and
//! `nClose` frees the box. A step job holds its own `Arc`, so the engine lives until a running step
//! ends even after `nClose`. Java must not use a handle after `nClose` (`NativeEngine` makes every
//! call and the close one critical section). The buffer memory Java's direct `ByteBuffer`s point
//! into has holds of its own (`nHoldBuffers`), released by a `Cleaner` per buffer, so it outlives
//! any Java view of it.
//!
//! Only game threads call these. Pool threads never call into Java (PLAN.md §2).

use std::sync::Arc;
use std::time::Duration;

use jni::errors::{Error as JniError, ThrowRuntimeExAndDefault};
use jni::objects::{JByteArray, JByteBuffer, JClass, JDoubleArray, JLongArray, JString};
use jni::sys::{jboolean, jint, jlong};
use jni::{Env, EnvUnowned};
use mic_native_rt::abi;
use rtsim_kernel::page::Durability;

use crate::engine::{
    Config, Engine, Exec, FLAG_INDUSTRY_READ_ONLY, FLAG_NO_FSYNC, FLAG_TEST, HABITAT_INFO,
    HERD_INFO, PHASE1_STATS, STATS_LEN, last_error, set_last_error,
};
use crate::orders;

/// Borrows the engine behind a handle.
///
/// # Safety
/// `h` is 0 or came from `nOpen` and was not yet passed to `nClose` (the Java side's contract).
unsafe fn engine<'a>(h: jlong) -> Result<&'a Arc<Engine>, JniError> {
    if h == 0 {
        return Err(JniError::NullPtr("engine handle"));
    }
    // SAFETY: the caller's contract: h is a live Box<Arc<Engine>>.
    Ok(unsafe { &*(h as usize as *const Arc<Engine>) })
}

fn jstr(env: &Env<'_>, s: &JString<'_>) -> Result<String, JniError> {
    if s.is_null() {
        return Err(JniError::NullPtr("string argument"));
    }
    s.try_to_string(env)
}

/// `static native long nOpen(String pagesDir, int capacity, int flags)`: a handle, or 0 with the
/// reason in `nLastError()`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nOpen<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    pages_dir: JString<'l>,
    capacity: jint,
    flags: jint,
) -> jlong {
    env.with_env(|env| -> Result<jlong, JniError> {
        let dir = jstr(env, &pages_dir)?;
        let flags = flags as u32;
        let mut cfg = Config::new(dir);
        if capacity > 0 {
            cfg.capacity = capacity as usize;
        }
        cfg.test = flags & FLAG_TEST != 0;
        if flags & FLAG_NO_FSYNC != 0 {
            cfg.durability = Durability::NoSync;
        }
        cfg.industry_read_only = flags & FLAG_INDUSTRY_READ_ONLY != 0;
        if mic_native_rt::try_host().is_none() {
            // No fallback without the host (A8): the library was System.load-ed before loadPlugin.
            set_last_error(
                "rtsim-jni has no mic_native host: load it with MicNative.loadPlugin first",
            );
            return Ok(0);
        }
        cfg.exec = Exec::Host;
        Ok(match Engine::open(cfg) {
            Ok(e) => Box::into_raw(Box::new(e)) as usize as jlong,
            Err(m) => {
                set_last_error(m);
                0
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native ByteBuffer nBuffer(long handle, int which)`: 0, 1 observations; 2, 3 commands.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nBuffer<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    which: jint,
) -> JByteBuffer<'l> {
    env.with_env(|env| -> Result<JByteBuffer<'l>, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let b = e
            .buffer(which.max(0) as usize)
            .ok_or(JniError::NullPtr("buffer index out of range"))?;
        // SAFETY: the memory lives as long as the engine, which outlives every use Java makes of
        // the buffer (it drops its views when the session closes).
        unsafe { env.new_direct_byte_buffer(b.base(), b.total()) }
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native long nHoldBuffers(long handle)`: a hold on the engine's buffer memory for one
/// Java `ByteBuffer`; `nReleaseBuffers` gives it back (from a `Cleaner`, once that buffer and every
/// view of it are unreachable). The memory lives until the engine and every hold are gone.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nHoldBuffers<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jlong {
    env.with_env(|_env| -> Result<jlong, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        Ok(e.hold_buffers() as jlong)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native void nReleaseBuffers(long token)`: gives back one `nHoldBuffers` hold.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nReleaseBuffers<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    token: jlong,
) {
    env.with_env(|_env| -> Result<(), JniError> {
        // SAFETY: Java releases each token once (one Cleaner action per hold).
        unsafe { Engine::release_buffers(token as usize) };
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native int nStart(long handle, long lastAppliedCmdSeq, long[] out)`: out[0] the
/// observation sequence the pages include, out[1] the next command sequence.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nStart<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    last_applied: jlong,
    out: JLongArray<'l>,
) -> jint {
    env.with_env(|env| -> Result<jint, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        if out.is_null() || out.len(env)? < 2 {
            set_last_error("nStart needs a long[2]");
            return Ok(abi::MIC_ERR_INVALID_ARGUMENT);
        }
        match e.start(last_applied as u64) {
            Ok((pages, next)) => {
                out.set_region(env, 0, &[pages as jlong, next as jlong])?;
                Ok(abi::MIC_OK)
            }
            Err(m) => {
                set_last_error(m);
                Ok(abi::MIC_ERR_INTERNAL)
            }
        }
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native int nSubmit(long handle, long handover)`: queues the step, returns at once.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nSubmit<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    handover: jlong,
) -> jint {
    env.with_env(|_env| -> Result<jint, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        Ok(match e.submit(handover as u64) {
            Ok(()) => abi::MIC_OK,
            Err(m) => {
                set_last_error(m);
                abi::MIC_ERR_INTERNAL
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native void nRequestSave(long handle, long throughObsSeq)`: a signal, never blocks.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nRequestSave<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    through: jlong,
) {
    env.with_env(|_env| -> Result<(), JniError> {
        // SAFETY: Java's contract on handles.
        unsafe { engine(handle) }?.request_save(through.max(0) as u64);
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native int nClose(long handle, boolean save, int timeoutMs)`: ends the session and
/// frees the handle. 0, or a negative status with the reason in `nLastError()` (the handle is
/// freed either way).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nClose<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    save: jboolean,
    timeout_ms: jint,
) -> jint {
    env.with_env(|_env| -> Result<jint, JniError> {
        if handle == 0 {
            return Ok(abi::MIC_ERR_INVALID_ARGUMENT);
        }
        // SAFETY: Java's contract: a live handle, closed once. The box is ours again.
        let e = unsafe { Box::from_raw(handle as usize as *mut Arc<Engine>) };
        let r = e.close(save, Duration::from_millis(timeout_ms.max(0) as u64));
        drop(e);
        Ok(match r {
            Ok(()) => abi::MIC_OK,
            Err(m) => {
                set_last_error(m);
                abi::MIC_ERR_INTERNAL
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native String nDescribe(long handle)`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nDescribe<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> JString<'l> {
    env.with_env(|env| -> Result<JString<'l>, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        JString::from_str(env, e.describe())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native String nLastError()`: this thread's last failure message ("" if none).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nLastError<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
) -> JString<'l> {
    env.with_env(|env| JString::from_str(env, last_error()))
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native int nOrder(long handle, byte[] orders)`: one unit of orders (`orders` module).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nOrder<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    bytes: JByteArray<'l>,
) -> jint {
    env.with_env(|env| -> Result<jint, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        if bytes.is_null() {
            set_last_error("nOrder: null orders");
            return Ok(abi::MIC_ERR_INVALID_ARGUMENT);
        }
        let raw = env.convert_byte_array(&bytes)?;
        let unit = match orders::decode(&raw) {
            Ok(u) => u,
            Err(m) => {
                set_last_error(m);
                return Ok(abi::MIC_ERR_INVALID_ARGUMENT);
            }
        };
        Ok(match e.order(unit) {
            Ok(()) => abi::MIC_OK,
            Err(m) => {
                set_last_error(m);
                abi::MIC_ERR_SHUTDOWN
            }
        })
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native byte[] nDrainReceived(long handle)`: entries of u64 handover, u64 obsSeq,
/// u32 flags (1: skipped), u32 length, then the record.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nDrainReceived<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> JByteArray<'l> {
    env.with_env(|env| -> Result<JByteArray<'l>, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let received = e.drain_received();
        let mut out = Vec::with_capacity(received.iter().map(|r| 24 + r.bytes.len()).sum());
        for r in received {
            out.extend_from_slice(&r.handover.to_le_bytes());
            out.extend_from_slice(&r.obs_seq.to_le_bytes());
            out.extend_from_slice(&u32::from(r.skipped).to_le_bytes());
            out.extend_from_slice(&(r.bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&r.bytes);
        }
        env.byte_array_from_slice(&out)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native void nStats(long handle, long[] out)`: the counters (`STATS_LEN` of them).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nStats<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    out: JLongArray<'l>,
) {
    env.with_env(|env| -> Result<(), JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let stats = e.stats_array();
        let n = out.len(env)?.min(STATS_LEN);
        let vals: Vec<jlong> = stats[..n].iter().map(|v| *v as jlong).collect();
        out.set_region(env, 0, &vals)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native void nPhase1Stats(long handle, long[] out)`: phase 1's counters
/// (`PHASE1_STATS` of them, `engine::phase1::stat`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nPhase1Stats<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    out: JLongArray<'l>,
) {
    env.with_env(|env| -> Result<(), JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let stats = e.phase1_stats();
        let n = out.len(env)?.min(PHASE1_STATS);
        let vals: Vec<jlong> = stats[..n].iter().map(|v| *v as jlong).collect();
        out.set_region(env, 0, &vals)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native boolean nHerdInfo(long handle, long agent, long[] out)`: one herd by its
/// group's agent id (`Engine::herd_info`, `HERD_INFO` values, as of the herds' present); false
/// when the agent is no herd's.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nHerdInfo<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    agent: jlong,
    out: JLongArray<'l>,
) -> jboolean {
    env.with_env(|env| -> Result<jboolean, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let Some(info) = e.herd_info(agent as u64) else {
            return Ok(false);
        };
        let n = out.len(env)?.min(HERD_INFO);
        let vals: Vec<jlong> = info[..n].iter().map(|v| *v as jlong).collect();
        out.set_region(env, 0, &vals)?;
        Ok(true)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native long nDragonsVillageRisk(long handle, long village)`: a phase 1 village's fear
/// to the dragons, its golems' included (`Engine::dragons_village_risk`); -1 for a village the
/// world does not know. Between steps; tests.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nDragonsVillageRisk<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    village: jlong,
) -> jlong {
    env.with_env(|_env| -> Result<jlong, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        Ok(e.dragons_village_risk(village as u64))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native void nDragonsTribute(long handle, long[] out)`: the dragons' tribute
/// (`Engine::dragons_tribute`: seasons paid, coins placed in chests, coins waiting for a chest,
/// stacks on their way). Between steps; tests.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nDragonsTribute<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    out: JLongArray<'l>,
) {
    env.with_env(|env| -> Result<(), JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let info = e.dragons_tribute();
        let n = out.len(env)?.min(info.len());
        out.set_region(env, 0, &info[..n])?;
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native double[] nGlassReport(long handle, long district)`: the district's glass report
/// (`Engine::glass_report`, the layout of `rtsim_industry::glass::report::Report::to_f64s`); null
/// while it has no complete compile. Between steps; tests and operators.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nGlassReport<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    district: jlong,
) -> JDoubleArray<'l> {
    env.with_env(|env| -> Result<JDoubleArray<'l>, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        match e.glass_report(district as u64) {
            Some(v) => doubles(env, &v),
            None => Ok(JDoubleArray::null()),
        }
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native double[] nGlassOffline(byte[] records, long district)`: the glass check of a
/// record stream on a fresh industry engine (`engine::glass_offline`, the stub engine's
/// GameTests); null when the district has no complete compile in it. No handle: pure.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nGlassOffline<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    records: JByteArray<'l>,
    district: jlong,
) -> JDoubleArray<'l> {
    env.with_env(|env| -> Result<JDoubleArray<'l>, JniError> {
        if records.is_null() {
            return Err(JniError::NullPtr("records"));
        }
        let raw = env.convert_byte_array(&records)?;
        match crate::engine::glass_offline(&raw, district as u64) {
            Ok(Some(v)) => doubles(env, &v),
            Ok(None) => Ok(JDoubleArray::null()),
            Err(m) => {
                set_last_error(&m);
                Err(JniError::JniCall(jni::errors::JniError::InvalidArguments))
            }
        }
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

fn doubles<'l>(env: &mut Env<'l>, v: &[f64]) -> Result<JDoubleArray<'l>, JniError> {
    let arr = env.new_double_array(v.len())?;
    arr.set_region(env, 0, v)?;
    Ok(arr)
}

/// `static native void nHabitatInfo(long handle, int dim, int x, int z, long[] out)`: the world
/// model at block (x, z) of Java's dimension `dim` (`Engine::habitat_info`, `HABITAT_INFO`
/// values: the cell's masks, the counts, the region's founded herd). Between steps; tests.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nHabitatInfo<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    dim: jint,
    x: jint,
    z: jint,
    out: JLongArray<'l>,
) {
    env.with_env(|env| -> Result<(), JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let info = e.habitat_info(dim as u32, x, z);
        let n = out.len(env)?.min(HABITAT_INFO);
        let vals: Vec<jlong> = info[..n].to_vec();
        out.set_region(env, 0, &vals)?;
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native int nPhase1Sentiment(long handle, long village, long most, long least)`: a
/// phase 1 village's sentiment toward a player as of the last step (TEST mode), 0 when it feels
/// nothing, `Integer.MIN_VALUE` when the engine knows no such village or is not in TEST mode.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nPhase1Sentiment<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    village: jlong,
    most: jlong,
    least: jlong,
) -> jint {
    env.with_env(|_env| -> Result<jint, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        Ok(
            e.phase1_sentiment(village as u64, (most as u64, least as u64))
                .unwrap_or(jint::MIN),
        )
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native boolean nPhase1Works(long handle, long village, long[] out)`: a phase 1
/// village's works as of the last step (TEST mode; `phase1::works_view`, `phase1::WORKS_VIEW` of
/// them): false when the engine knows no such village, it has no works state, or the engine is
/// not in TEST mode.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nPhase1Works<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    village: jlong,
    out: JLongArray<'l>,
) -> jboolean {
    env.with_env(|env| -> Result<jboolean, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        let Some(view) = e.phase1_works(village as u64) else {
            return Ok(false);
        };
        let n = out.len(env)?.min(view.len());
        let vals: Vec<jlong> = view[..n].to_vec();
        out.set_region(env, 0, &vals)?;
        Ok(true)
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native long nLedger(long handle, long store)` (TEST mode; 0 otherwise).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nLedger<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    store: jlong,
) -> jlong {
    env.with_env(|_env| -> Result<jlong, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        Ok(e.inspect()
            .ledger
            .get(&(store as u64))
            .copied()
            .unwrap_or(0))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native boolean nOpApplied(long handle, long opId)` (TEST mode; false otherwise).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nOpApplied<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
    op: jlong,
) -> jboolean {
    env.with_env(|_env| -> Result<jboolean, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        Ok(e.inspect().op_applied(op as u64))
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native String nProtocolError(long handle)`: null when none.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minecraftindustrialcomplex_mic_1rtsim_engine_NativeEngine_nProtocolError<
    'l,
>(
    mut env: EnvUnowned<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> JString<'l> {
    env.with_env(|env| -> Result<JString<'l>, JniError> {
        // SAFETY: Java's contract on handles.
        let e = unsafe { engine(handle) }?;
        match e.protocol_error() {
            Some(m) => JString::from_str(env, m),
            None => Ok(JString::null()),
        }
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}
