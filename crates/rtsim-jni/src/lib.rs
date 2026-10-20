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

fn init(host: &'static rt::Host) -> rt::Result<()> {
    rt::info(&format!(
        "[mic_rtsim] rtsim-jni {} loaded as plugin {} (id {})",
        env!("CARGO_PKG_VERSION"),
        host.plugin_name(),
        host.plugin_id()
    ));
    Ok(())
}

// Exports mic_native_plugin_init and installs the counting allocator that feeds the governor.
rt::plugin!(init);
