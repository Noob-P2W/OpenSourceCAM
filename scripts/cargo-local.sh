#!/usr/bin/env bash
# Optional workspace-local Linux toolchain; does not change the user's shell setup.
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_HOME="$project_root/.tools/linux-cargo"
export RUSTUP_HOME="$project_root/.tools/linux-rustup"
export CARGO_TARGET_DIR="$project_root/target/linux"
export PATH="$CARGO_HOME/bin:$HOME/.local/bin:$PATH"
exec "$CARGO_HOME/bin/cargo" "$@"
