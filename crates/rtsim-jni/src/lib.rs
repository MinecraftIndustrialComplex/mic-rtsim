// SPDX-License-Identifier: AGPL-3.0-or-later
//! rtsim-jni: mic-rtsim's native link, the only crate that uses `jni` (PLAN.md §2, §4.5).
//!
//! It is a `mic_native` plugin (A6, A8): a cdylib the host loads and initialises
//! (`mic_native_rt::plugin!`), whose JNI functions the bridge's `NativeEngine` binds. It joins:
//!
//! - **the bridge** (`neoforge/`, mod `mic_rtsim`): the four boundary buffers of docs/boundary.md,
//!   allocated here ([`buffers`]), with their records ([`wire`]);
//! - **the host** (`mic_native`): each step runs on its background pool, page writes on its IO
//!   lane, and the plugin owns no threads ([`engine`]);
//! - **the kernel** (`rtsim-kernel`): observations go through `journal::Replay`, transfer
//!   commands through `journal::Outbox`, and the state into `page::PageStore` pages.
//!
//! What the engine simulates is a minimal world ([`world`]) of puppet agents and transfers, driven
//! by orders ([`orders`]) a test or `/rtsim` gives in place of the sim's own decisions. Phase 1
//! replaces the world; the link stays.

pub mod buffers;
pub mod engine;
pub mod jni_api;
pub mod orders;
pub mod wire;
pub mod world;

use mic_native_rt as rt;

/// The CPU level this library was compiled for: the x86-64-v3 level for the build made with
/// `-C target-cpu=x86-64-v3`, `baseline` otherwise. `tools/natives/build-natives.sh` checks that
/// each build carries the literal of its folder, as mic_native's host does.
pub const CPU_LEVEL: &str = if cfg!(all(
    target_arch = "x86_64",
    target_feature = "avx2",
    target_feature = "bmi1",
    target_feature = "bmi2",
    target_feature = "f16c",
    target_feature = "fma",
    target_feature = "lzcnt",
    target_feature = "movbe",
)) {
    "x86-64-v3"
} else {
    "baseline"
};

/// Keeps the CPU level literal in the library for the natives check.
#[used]
static CPU_LEVEL_TAG: &str = CPU_LEVEL;

fn init(host: &'static rt::Host) -> rt::Result<()> {
    rt::info(&format!(
        "[mic_rtsim] rtsim-jni {} ({}) loaded as plugin {} (id {})",
        env!("CARGO_PKG_VERSION"),
        CPU_LEVEL,
        host.plugin_name(),
        host.plugin_id()
    ));
    Ok(())
}

// Exports mic_native_plugin_init and installs the counting allocator that feeds the governor.
rt::plugin!(init);
