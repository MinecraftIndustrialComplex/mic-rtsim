#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# The determinism gate (PLAN.md §4.7, SCALE.md §5): runs the parallel step on the same world at
# 1 and 4 workers, resident and paged, and fails when any run ends in a different state hash
# (crates/rtsim-cli/src/bench/bsp.rs). The unit test bsp::tests::same_state_at_any_worker_count_and_budget
# checks the same on a smaller world.
#
# Run it inside the dev shell, under the shared heavy-job lock on Ben's laptop:
#   nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 tools/ci/determinism-check.sh
# Results go to bench/scratch/ (gitignored); committed results live in bench/results/.
set -euo pipefail
cd "$(dirname "$0")/../.."
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" cargo build --release --quiet -p rtsim-cli
exec ./target/release/rtsim-cli bench bsp --quick --check --out bench/scratch "$@"
