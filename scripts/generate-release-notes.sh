#!/usr/bin/env bash
# Build categorized GitHub Release notes from conventional commits.
# Usage:
#   ./scripts/generate-release-notes.sh [PREV_TAG] [NEW_TAG] [BUMP] [FROM_VER] [TO_VER]
# Writes markdown to stdout.
set -euo pipefail

PREV_TAG="${1:-}"
NEW_TAG="${2:-}"
BUMP="${3:-}"
FROM_VER="${4:-}"
TO_VER="${5:-}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

REPO_SLUG="${GITHUB_REPOSITORY:-SSujitX/TermCrew}"
SERVER_URL="${GITHUB_SERVER_URL:-https://github.com}"
COMMIT_BASE="${SERVER_URL}/${REPO_SLUG}/commit"
VER="${TO_VER:-${NEW_TAG#v}}"

if [[ -n "$PREV_TAG" ]]; then
  RANGE="${PREV_TAG}..HEAD"
else
  RANGE="HEAD"
fi

# subject\tshort\tfull  (newest first)
mapfile -t COMMITS < <(git log "$RANGE" --pretty=format:'%s%x09%h%x09%H' --no-merges)

feat=()
fix=()
perf=()
refactor=()
test=()
docs=()
build=()
ci=()
chore=()
other=()

for line in "${COMMITS[@]+"${COMMITS[@]}"}"; do
  [[ -n "$line" ]] || continue
  subject="${line%%$'\t'*}"
  rest="${line#*$'\t'}"
  short="${rest%%$'\t'*}"
  full="${rest#*$'\t'}"
  if [[ "$subject" =~ ^chore\(release\): ]]; then
    continue
  fi
  entry="- ${subject} ([${short}](${COMMIT_BASE}/${full}))"
  if [[ "$subject" =~ ^(feat)(\(.+\))?\!?: ]]; then
    feat+=("$entry")
  elif [[ "$subject" =~ ^(fix)(\(.+\))?\!?: ]]; then
    fix+=("$entry")
  elif [[ "$subject" =~ ^(perf)(\(.+\))?\!?: ]]; then
    perf+=("$entry")
  elif [[ "$subject" =~ ^(refactor)(\(.+\))?\!?: ]]; then
    refactor+=("$entry")
  elif [[ "$subject" =~ ^(test)(\(.+\))?\!?: ]]; then
    test+=("$entry")
  elif [[ "$subject" =~ ^(docs)(\(.+\))?\!?: ]]; then
    docs+=("$entry")
  elif [[ "$subject" =~ ^(build)(\(.+\))?\!?: ]]; then
    build+=("$entry")
  elif [[ "$subject" =~ ^(ci)(\(.+\))?\!?: ]]; then
    ci+=("$entry")
  elif [[ "$subject" =~ ^(chore|style)(\(.+\))?\!?: ]]; then
    chore+=("$entry")
  else
    other+=("$entry")
  fi
done

emit_section() {
  local title="$1"
  shift
  local -a items=("$@")
  if [[ "${#items[@]}" -eq 0 ]]; then
    return 0
  fi
  echo "## ${title}"
  echo
  printf '%s\n' "${items[@]}"
  echo
}

echo "## TermCrew ${NEW_TAG}"
echo
if [[ -n "$BUMP" && -n "$FROM_VER" && -n "$TO_VER" ]]; then
  if [[ "$FROM_VER" == "$TO_VER" ]]; then
    echo "Release **${TO_VER}** (\`${BUMP}\` bump — version unchanged)."
  else
    echo "Release **${TO_VER}** (\`${BUMP}\`: ${FROM_VER} → ${TO_VER})."
  fi
  echo
fi
if [[ -n "$PREV_TAG" ]]; then
  echo "Changes since \`${PREV_TAG}\`."
else
  echo "Initial tagged release."
fi
echo

emit_section "What's new" "${feat[@]+"${feat[@]}"}"
emit_section "Fixes" "${fix[@]+"${fix[@]}"}"
emit_section "Performance" "${perf[@]+"${perf[@]}"}"
emit_section "Refactors" "${refactor[@]+"${refactor[@]}"}"
emit_section "Tests" "${test[@]+"${test[@]}"}"
emit_section "Documentation" "${docs[@]+"${docs[@]}"}"
emit_section "Build" "${build[@]+"${build[@]}"}"
emit_section "CI" "${ci[@]+"${ci[@]}"}"
emit_section "Maintenance" "${chore[@]+"${chore[@]}"}"
emit_section "Other" "${other[@]+"${other[@]}"}"

echo "---"
echo
echo "### Run"
echo
echo "Download the \`termcrew\` binary for your OS (Windows / macOS / Linux). The UI is still the Vite app:"
echo
echo '```bash'
echo "git clone ${SERVER_URL}/${REPO_SLUG}.git"
echo "cd TermCrew"
echo "cd frontend && bun install && bun run dev"
echo '```'
echo
echo "On Windows you can also double-click \`start.bat\` (builds the backend with \`cargo run\`)."
echo
echo "Assets: \`termcrew-${VER}-windows-amd64.exe\`, \`termcrew-${VER}-darwin-arm64\`, \`termcrew-${VER}-linux-amd64\` + \`SHA256SUMS\`."
echo
echo "Backend listens on \`http://127.0.0.1:3001\`. Open \`http://localhost:5173\`."
