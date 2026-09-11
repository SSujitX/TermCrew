#!/usr/bin/env bash
# Keep backend/Cargo.toml and frontend/package.json on the root VERSION.
# Usage:
#   ./scripts/sync_version.sh           # write versions from VERSION
#   ./scripts/sync_version.sh check     # exit 1 if any file drifts
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(tr -d '[:space:]' < VERSION)"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Invalid VERSION (expected X.Y.Z): '$VERSION'" >&2
  exit 1
fi

# First ^version = "…" in Cargo.toml is the [package] line, not a dependency.
read_cargo_version() {
  awk '/^version = "/ { gsub(/"/, "", $3); print $3; exit }' backend/Cargo.toml
}

read_frontend_version() {
  node -p "require('./frontend/package.json').version"
}

write_cargo_version() {
  local ver="$1"
  awk -v ver="$ver" '
    /^version = "/ && !done {
      sub(/version = "[^"]+"/, "version = \"" ver "\"")
      done = 1
    }
    { print }
  ' backend/Cargo.toml > backend/Cargo.toml.tmp
  mv backend/Cargo.toml.tmp backend/Cargo.toml
}

write_frontend_version() {
  local ver="$1"
  node -e '
    const fs = require("fs");
    const path = "frontend/package.json";
    const pkg = JSON.parse(fs.readFileSync(path, "utf8"));
    pkg.version = process.argv[1];
    fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
  ' "$ver"
}

mode="${1:-sync}"

if [[ "$mode" == "check" ]]; then
  ok=1
  cargo_ver="$(read_cargo_version)"
  if [[ "$cargo_ver" != "$VERSION" ]]; then
    echo "backend/Cargo.toml is $cargo_ver, expected $VERSION" >&2
    ok=0
  fi
  pkg_ver="$(read_frontend_version)"
  if [[ "$pkg_ver" != "$VERSION" ]]; then
    echo "frontend/package.json is $pkg_ver, expected $VERSION" >&2
    ok=0
  fi
  if [[ "$ok" -ne 1 ]]; then
    exit 1
  fi
  echo "OK: package files match VERSION $VERSION"
  exit 0
fi

if [[ "$mode" != "sync" ]]; then
  echo "Usage: $0 [sync|check]" >&2
  exit 1
fi

echo "Syncing product version $VERSION"
write_cargo_version "$VERSION"
write_frontend_version "$VERSION"
echo "Done: package files now at $VERSION"
bash ./scripts/sync_version.sh check
