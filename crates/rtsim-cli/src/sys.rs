// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the bench measures about the process and the machine: heap bytes through a counting
//! allocator, thread CPU time, resident set size, and a description of the machine for the
//! results file.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts live heap bytes and their peak. One relaxed atomic add per allocation; the bench pays it
/// in every configuration alike, so it does not bend a slope.
pub struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }

    /// Forwarded, so zeroed memory stays lazily zeroed (calloc): the default would write zeros
    /// through every page and inflate the resident size the bench reports.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                let now = LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed)
                    + (new_size - layout.size());
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

pub fn heap_live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

pub fn heap_peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

pub fn reset_heap_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// CPU time of the calling thread, in nanoseconds. Unlike wall time it does not count time the
/// thread spent waiting for a core, so other load on the machine bends it less.
#[cfg(unix)]
pub fn thread_cpu_ns() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    (rc == 0).then(|| ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
}

#[cfg(not(unix))]
pub fn thread_cpu_ns() -> Option<u64> {
    None
}

/// (current, peak) resident set size in bytes, from /proc on Linux.
pub fn rss() -> (Option<u64>, Option<u64>) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return (None, None);
    };
    let field = |name: &str| {
        status.lines().find_map(|l| {
            let rest = l.strip_prefix(name)?;
            let kb: u64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
            Some(kb * 1024)
        })
    };
    (field("VmRSS:"), field("VmHWM:"))
}

/// The machine and its state, for the results file.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Machine {
    pub cpu: String,
    pub logical_cpus: usize,
    pub mem_total_bytes: Option<u64>,
    pub os: String,
    pub kernel: Option<String>,
    pub hostname: Option<String>,
    pub loadavg: Option<String>,
}

impl Machine {
    pub fn probe() -> Machine {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let cpu = cpuinfo
            .lines()
            .find_map(|l| {
                l.strip_prefix("model name")
                    .map(|r| r.trim_start_matches([' ', '\t', ':']).to_string())
            })
            .unwrap_or_else(|| std::env::consts::ARCH.to_string());
        let mem_total_bytes = std::fs::read_to_string("/proc/meminfo").ok().and_then(|m| {
            m.lines().find_map(|l| {
                let kb: u64 = l
                    .strip_prefix("MemTotal:")?
                    .trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse()
                    .ok()?;
                Some(kb * 1024)
            })
        });
        let read = |p: &str| {
            std::fs::read_to_string(p)
                .ok()
                .map(|s| s.trim().to_string())
        };
        Machine {
            cpu,
            logical_cpus: std::thread::available_parallelism().map_or(1, |n| n.get()),
            mem_total_bytes,
            os: std::env::consts::OS.to_string(),
            kernel: read("/proc/sys/kernel/osrelease"),
            hostname: read("/proc/sys/kernel/hostname"),
            loadavg: read("/proc/loadavg"),
        }
    }
}

pub fn loadavg() -> Option<String> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .map(|s| s.trim().to_string())
}
