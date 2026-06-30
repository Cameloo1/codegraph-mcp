#!/usr/bin/env bash
set -euo pipefail

KEEP_TEMP=0
TARGET_REPO=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --keep-temp) KEEP_TEMP=1 ;;
    --target-repo)
      shift
      if [ "$#" -eq 0 ]; then
        echo "--target-repo requires a path" >&2
        exit 2
      fi
      TARGET_REPO="$1"
      ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

script_path="${BASH_SOURCE[0]:-$0}"
case "$script_path" in
  */*) script_parent="${script_path%/*}" ;;
  *) script_parent="." ;;
esac
script_dir=$(CDPATH= cd -- "$script_parent" && pwd -P)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
cd "$repo_root"

if [ ! -d /mnt/c ]; then
  echo "This smoke is intended for WSL2 on Windows and expects /mnt/c to exist." >&2
  exit 2
fi

for tool in cargo rustc git; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required in the WSL2 distro" >&2
    exit 2
  fi
done

run_id="$(date +%Y%m%d_%H%M%S)_$$"
log_root="reports/smoke/wsl2_agent_use"
run_log_dir="$log_root/$run_id"
temp_parent="${TMPDIR:-/tmp}/codegraph-wsl2-agent-use-$run_id"
agent_data_root="$temp_parent/agent-use-data"
summary_path="$run_log_dir/summary.json"
steps_tsv="$run_log_dir/steps.tsv"

mkdir -p "$run_log_dir" "$temp_parent" "$agent_data_root"
: > "$steps_tsv"

cleanup() {
  if [ "$KEEP_TEMP" -ne 1 ] && [ -d "$temp_parent" ]; then
    rm -rf "$temp_parent"
  fi
}
trap cleanup EXIT

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

record_step() {
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >> "$steps_tsv"
}

write_summary() {
  local status="$1"
  local failure="${2:-}"
  local first row_name row_exit_code row_duration_ms row_log_path
  {
    printf '{\n'
    printf '  "schema_version": 1,\n'
    printf '  "status": "%s",\n' "$(json_escape "$status")"
    printf '  "failure": "%s",\n' "$(json_escape "$failure")"
    printf '  "run_id": "%s",\n' "$(json_escape "$run_id")"
    printf '  "generated_at": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "platform": "wsl2",\n'
    printf '  "repo_root": "%s",\n' "$(json_escape "$repo_root")"
    printf '  "target_repo": "%s",\n' "$(json_escape "$target_repo")"
    printf '  "cargo_target_dir": "%s",\n' "$(json_escape "$CARGO_TARGET_DIR")"
    printf '  "agent_use_data_root": "%s",\n' "$(json_escape "$CODEGRAPH_AGENT_USE_DATA_ROOT")"
    printf '  "repo_local_codegraph_created": %s,\n' "$repo_local_codegraph_created"
    printf '  "target_local_codegraph_created": %s,\n' "$target_local_codegraph_created"
    printf '  "logs_dir": "%s",\n' "$(json_escape "$repo_root/$run_log_dir")"
    printf '  "requirements": {\n'
    printf '    "wsl2_required": true,\n'
    printf '    "network_required": "only normal Cargo dependency resolution",\n'
    printf '    "repo_local_codegraph_allowed": false\n'
    printf '  },\n'
    printf '  "steps": [\n'
    first=1
    while IFS='	' read -r row_name row_exit_code row_duration_ms row_log_path; do
      [ -n "$row_name" ] || continue
      if [ "$first" -eq 0 ]; then
        printf ',\n'
      fi
      first=0
      printf '    {"name":"%s","exit_code":%s,"duration_ms":%s,"log":"%s"}' \
        "$(json_escape "$row_name")" \
        "$row_exit_code" \
        "$row_duration_ms" \
        "$(json_escape "$row_log_path")"
    done < "$steps_tsv"
    printf '\n  ]\n'
    printf '}\n'
  } > "$summary_path"
}

run_step() {
  local name="$1"
  local log_path start exit_code end duration_ms arg
  shift
  log_path="$run_log_dir/$name.log"
  start=$(date +%s)
  {
    printf 'repo_root: %s\n' "$repo_root"
    printf 'target_repo: %s\n' "$target_repo"
    printf 'agent_use_data_root: %s\n' "$CODEGRAPH_AGENT_USE_DATA_ROOT"
    printf 'cargo_target_dir: %s\n' "$CARGO_TARGET_DIR"
    printf 'command:'
    for arg in "$@"; do
      printf ' %s' "$arg"
    done
    printf '\n\n'
  } > "$log_path"

  if (cd "$repo_root" && "$@") >> "$log_path" 2>&1; then
    exit_code=0
  else
    exit_code=$?
  fi

  end=$(date +%s)
  duration_ms=$(( (end - start) * 1000 ))
  record_step "$name" "$exit_code" "$duration_ms" "$log_path"
  if [ "$exit_code" -ne 0 ]; then
    write_summary "fail" "$name failed with exit code $exit_code"
    echo "$name failed with exit code $exit_code. See $log_path" >&2
    exit "$exit_code"
  fi
}

assert_no_local_codegraph() {
  local label="$1"
  local path="$2"
  if [ -e "$path/.codegraph" ]; then
    write_summary "fail" "$label created or contains repo-local .codegraph at $path/.codegraph"
    echo "$label created or contains repo-local .codegraph at $path/.codegraph" >&2
    exit 1
  fi
}

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/cg-target/codegraph-mcp}"
export CODEGRAPH_AGENT_USE_DATA_ROOT="${CODEGRAPH_AGENT_USE_DATA_ROOT:-$agent_data_root}"

if [ -n "$TARGET_REPO" ]; then
  target_repo=$(CDPATH= cd -- "$TARGET_REPO" && pwd -P)
else
  target_repo="$temp_parent/target repo"
  mkdir -p "$target_repo/src"
  cat > "$target_repo/Cargo.toml" <<'EOF'
[package]
name = "codegraph-wsl2-smoke-target"
version = "0.0.0"
edition = "2021"
publish = false
EOF
  cat > "$target_repo/src/lib.rs" <<'EOF'
pub fn greet(name: &str) -> String {
    format!("hello {name}")
}

pub fn greet_world() -> String {
    greet("world")
}
EOF
  (cd "$target_repo" && git init -q && git add Cargo.toml src/lib.rs)
fi

repo_local_codegraph_created=false
target_local_codegraph_created=false
assert_no_local_codegraph "codegraph checkout before smoke" "$repo_root"
assert_no_local_codegraph "target repo before smoke" "$target_repo"

{
  printf 'repo_root=%s\n' "$repo_root"
  printf 'target_repo=%s\n' "$target_repo"
  printf 'cargo_target_dir=%s\n' "$CARGO_TARGET_DIR"
  printf 'agent_use_data_root=%s\n' "$CODEGRAPH_AGENT_USE_DATA_ROOT"
  printf 'cargo=%s\n' "$(command -v cargo)"
  printf 'rustc=%s\n' "$(rustc --version)"
  printf 'git=%s\n' "$(git --version)"
  printf 'uname=%s\n' "$(uname -a)"
} > "$run_log_dir/environment.txt"

binary="$CARGO_TARGET_DIR/release/codegraph-mcp"

run_step cargo_build_release cargo build --release --bin codegraph-mcp
run_step version "$binary" --json --version
run_step agent_use_status_missing "$binary" agent-use status --repo "$target_repo" --json
run_step agent_use_mcp_config "$binary" agent-use mcp-config --repo "$target_repo" --json
run_step agent_use_index "$binary" agent-use index --repo "$target_repo" --json
run_step agent_use_status_indexed "$binary" agent-use status --repo "$target_repo" --json
run_step agent_use_query_symbols "$binary" agent-use query symbols greet --repo "$target_repo" --limit 5 --agent-json
run_step agent_use_validate_edit "$binary" agent-use validate-edit --repo "$target_repo" --changed src/lib.rs --agent-json

if [ -e "$repo_root/.codegraph" ]; then
  repo_local_codegraph_created=true
fi
if [ -e "$target_repo/.codegraph" ]; then
  target_local_codegraph_created=true
fi
assert_no_local_codegraph "codegraph checkout after smoke" "$repo_root"
assert_no_local_codegraph "target repo after smoke" "$target_repo"

write_summary "pass"
echo "WSL2 agent-use smoke passed"
echo "logs: $run_log_dir"
echo "target repo: $target_repo"
echo "agent-use data root: $CODEGRAPH_AGENT_USE_DATA_ROOT"
