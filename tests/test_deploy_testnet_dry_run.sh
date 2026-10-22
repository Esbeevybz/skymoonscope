#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT/scripts/deploy_testnet.sh"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIR"' EXIT
mkdir -p "$TEMP_DIR/bin"
export DRY_RUN_CALL_LOG="$TEMP_DIR/calls"

for command_name in cargo stellar; do
  cat >"$TEMP_DIR/bin/$command_name" <<'STUB'
#!/usr/bin/env bash
printf '%s %s\n' "$0" "$*" >>"$DRY_RUN_CALL_LOG"
STUB
  chmod +x "$TEMP_DIR/bin/$command_name"
done

output=$(PATH="$TEMP_DIR/bin:$PATH" "$SCRIPT" --dry-run --source-account test 2>&1)

printf '%s\n' "$output"

echo "$output" | grep -q 'DRY-RUN' || {
  echo "expected dry-run banner in output" >&2
  exit 1
}

echo "$output" | grep -q 'cargo build' || {
  echo "expected build command in dry-run output" >&2
  exit 1
}

echo "$output" | grep -q 'stellar contract deploy' || {
  echo "expected deploy command in dry-run output" >&2
  exit 1
}

echo "$output" | grep -q 'configured-account' || {
  echo "expected source account placeholder in dry-run output" >&2
  exit 1
}

if echo "$output" | grep -q -- '--source-account test'; then
  echo "dry-run unexpectedly exposed the configured source account" >&2
  exit 1
fi

[[ ! -e "$DRY_RUN_CALL_LOG" ]] || {
  echo "dry-run unexpectedly invoked cargo or stellar" >&2
  cat "$DRY_RUN_CALL_LOG" >&2
  exit 1
}

exit 0
