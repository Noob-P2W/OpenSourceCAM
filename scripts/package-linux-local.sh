#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_HOME="$project_root/.tools/linux-cargo"
export RUSTUP_HOME="$project_root/.tools/linux-rustup"
export PATH="$CARGO_HOME/bin:$HOME/.local/bin:$PATH"
cd "$project_root"
exec python3 scripts/bundle-cad.py "$@"
