// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the bench measures about the process and the machine: heap bytes through the counting
//! allocator, thread CPU time, resident set size, and a description of the machine for the
//! results file.

use std::sync::atomic::Ordering;

use mic_native_rt::memory::GAUGE;

// Heap bytes are counted by the global allocator mic_native's runtime installs in rtsim-jni (linked
// here for `run`): one relaxed atomic add per allocation, paid in every configuration alike, so it
// does not bend a slope. A binary can have one global allocator only, so the bench reads that one.

pub fn heap_live() -> usize {
    GAUGE.in_use.load(Ordering::Relaxed).max(0) as usize
}

pub fn heap_peak() -> usize {
    GAUGE.peak.load(Ordering::Relaxed).max(0) as usize
}

pub fn reset_heap_peak() {
    GAUGE
        .peak
        .store(GAUGE.in_use.load(Ordering::Relaxed), Ordering::Relaxed);
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

/// CPU time of the whole process (every thread), in nanoseconds.
#[cfg(unix)]
pub fn process_cpu_ns() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
    (rc == 0).then(|| ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
}

#[cfg(not(unix))]
pub fn process_cpu_ns() -> Option<u64> {
    None
}

/// Hands freed heap memory back to the OS (glibc's `malloc_trim`), so the resident size shows what
/// is live, not what a build phase left in the allocator's free lists. A no-op elsewhere.
pub fn trim_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only releases free memory; it has no preconditions.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Resets the peak resident size (`VmHWM`) to the current one (Linux 4.0 and later), so the peak
/// covers only what follows. Returns whether it worked.
pub fn reset_rss_peak() -> bool {
    std::fs::write("/proc/self/clear_refs", "5").is_ok()
}

/// The logical CPUs this process may run on, as Linux lists them (`taskset` shows here).
pub fn allowed_cpus() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|l| {
        l.strip_prefix("Cpus_allowed_list:")
            .map(|r| r.trim().to_string())
    })
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
