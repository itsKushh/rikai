#!/usr/bin/env bash
# Environnement de développement portable ; la compilation de distribution se fait sur Windows.
set -euo pipefail
cd "$(dirname "$0")/.."
repo_root="$(pwd)"
tools_dir="${MEETINGNOTES_TOOLS:-/workspace/.meetingnotes-tools}"
export CARGO_HOME="$tools_dir/cargo" RUSTUP_HOME="$tools_dir/rustup"
export UV_CACHE_DIR="$tools_dir/uv-cache" UV_LINK_MODE=copy
export PATH="$CARGO_HOME/bin:$PATH"
mkdir -p "$tools_dir"
if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
  installer="$(mktemp)"
  trap 'rm -f "$installer"' EXIT
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$installer"
  sh "$installer" -y --no-modify-path --profile minimal --default-toolchain none
fi
rustup toolchain install 1.100.0 --profile minimal --component rustfmt
uv sync --inexact --directory "$repo_root/ai-worker" --locked --python 3.12
cargo test --locked --workspace
"$repo_root/ai-worker/.venv/bin/python" -m unittest discover -s ai-worker/tests -v

