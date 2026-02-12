#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

info() { printf "[ci:mac] %s\n" "$*"; }
err() { printf "[ci:mac][error] %s\n" "$*" >&2; }

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    err "Missing required command: $1"
    exit 1
  fi
}

cd "$ROOT_DIR"

require_cmd node
require_cmd pnpm
require_cmd cargo

info "Repo: $ROOT_DIR"
info "Node: $(node -v)"
info "pnpm: $(pnpm -v)"
info "cargo: $(cargo -V)"

if [[ "${CI_RUN_ACT_FRONTEND:-}" == "1" ]]; then
  require_cmd act
  require_cmd docker
  info "Running ubuntu frontend job via act (OrbStack/Docker)…"
  act -W .github/workflows/ci.yml -j frontend
fi

if [[ "${CI_SKIP_INSTALL:-}" != "1" ]]; then
  info "Installing JS dependencies (frozen lockfile)…"
  pnpm install --frozen-lockfile
else
  info "Skipping pnpm install (CI_SKIP_INSTALL=1)"
fi

info "Running quality gate (typecheck + lint + frontend tests + backend tests)…"
pnpm quality-gate

info "Running Rust clippy (deny warnings)…"
(cd src-tauri && cargo clippy -- -D warnings)

info "Building frontend…"
pnpm build

info "Building Rust release (smoke)…"
(cd src-tauri && cargo build --release)

info "Done."
