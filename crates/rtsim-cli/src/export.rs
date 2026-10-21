// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli export <world> <dir>`: a world's rtsim state copied into a new fork through the
//! read-only open (VIEWER.md §12.1; rtsim_host::fork). Safe on a running server.

use std::path::Path;
use std::process::ExitCode;

use rtsim_host::fork::{self, ForkInfo};

pub fn cli(args: &[&str]) -> ExitCode {
    let [world, dir] = args else {
        eprintln!("usage: rtsim-cli export <world> <dir>");
        return ExitCode::from(64);
    };
    match fork::export(Path::new(world), Path::new(dir)) {
        Ok(info) => {
            print_info(&info, Path::new(dir));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("export: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn print_info(info: &ForkInfo, dir: &Path) {
    println!(
        "fork {} from {}: {} pages ({:.1} MiB), the world's generation {} through observation {}; journal {}..{}; game time {}; Deep Time {}; {} archive files; {} Surveyor files; {} cop{}",
        dir.display(),
        info.source,
        info.pages,
        info.page_bytes as f64 / f64::from(1 << 20),
        info.pages_save_seq,
        info.pages_obs_seq,
        info.journal_first_obs,
        info.journal_next_obs,
        info.game_time
            .map_or_else(|| "unknown (no level.dat)".to_string(), |t| t.to_string()),
        info.deep_time_folder.as_deref().unwrap_or("none"),
        info.archive_files,
        info.surveyor_files,
        info.attempts,
        if info.attempts == 1 { "y" } else { "ies" },
    );
    for n in &info.notes {
        println!("  note: {n}");
    }
}
