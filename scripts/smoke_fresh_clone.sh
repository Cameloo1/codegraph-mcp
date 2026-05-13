#!/usr/bin/env bash
set -euo pipefail

export PATH="/usr/bin:/bin:$PATH"

KEEP_TEMP=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --keep-temp) KEEP_TEMP=1 ;;
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
run_id="$(date +%Y%m%d_%H%M%S)_$$"
log_root="reports/smoke/fresh_clone"
run_log_dir="$log_root/linux_$run_id"
temp_base="${TMPDIR:-/tmp}"
temp_parent="$temp_base/codegraph fresh clone $run_id"
clone_root="$temp_parent/repo under test"
summary_path="$run_log_dir/summary.json"
steps_tsv="$run_log_dir/steps.tsv"

mkdir -p "$run_log_dir" "$clone_root"
: > "$steps_tsv"

case "$clone_root" in
  *" "*) path_with_spaces_tested=true ;;
  *) path_with_spaces_tested=false ;;
esac

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_summary() {
  local status="$1"
  local failure="${2:-}"
  local first
  local row_name row_exit_code row_duration_ms row_log_path
  {
    printf '{\n'
    printf '  "schema_version": 1,\n'
    printf '  "status": "%s",\n' "$(json_escape "$status")"
    printf '  "failure": "%s",\n' "$(json_escape "$failure")"
    printf '  "run_id": "%s",\n' "$(json_escape "$run_id")"
    printf '  "generated_at": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "platform": "linux",\n'
    printf '  "repo_root": "%s",\n' "$(json_escape "$repo_root")"
    printf '  "temp_parent": "%s",\n' "$(json_escape "$temp_parent")"
    printf '  "clone_root": "%s",\n' "$(json_escape "$clone_root")"
    printf '  "copy_mode": "filesystem_snapshot",\n'
    printf '  "path_with_spaces_tested": %s,\n' "$path_with_spaces_tested"
    printf '  "logs_dir": "%s",\n' "$(json_escape "$repo_root/$run_log_dir")"
    printf '  "requirements": {\n'
    printf '    "cgc_required": false,\n'
    printf '    "autoresearch_required": false,\n'
    printf '    "external_benchmark_artifacts_required": false,\n'
    printf '    "network_required": "only normal Cargo dependency resolution"\n'
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

cleanup() {
  if [ "$KEEP_TEMP" -ne 1 ] && [ -d "$temp_parent" ]; then
    rm -rf "$temp_parent"
  fi
}
trap cleanup EXIT

record_step() {
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >> "$steps_tsv"
}

run_step() {
  local name="$1"
  local log_path start exit_code end duration_ms arg
  shift
  log_path="$run_log_dir/$name.log"
  start=$(date +%s)
  {
    printf 'repo: %s\n' "$clone_root"
    printf 'command:'
    for arg in "$@"; do
      printf ' %s' "$arg"
    done
    printf '\n\n'
  } > "$log_path"

  if (cd "$clone_root" && "$@") >> "$log_path" 2>&1; then
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

run_cargo_metadata_step() {
  local name="cargo_metadata"
  local log_path start exit_code end duration_ms
  log_path="$run_log_dir/$name.log"
  start=$(date +%s)
  {
    printf 'repo: %s\n' "$clone_root"
    printf 'command: cargo metadata --workspace\n\n'
  } > "$log_path"

  if (cd "$clone_root" && cargo metadata --workspace) >> "$log_path" 2>&1; then
    exit_code=0
  else
    exit_code=$?
    if grep -q "unexpected argument .*--workspace" "$log_path"; then
      {
        printf '\ncompatibility fallback: cargo metadata --format-version 1\n'
        printf 'reason: cargo metadata --workspace is unsupported by this Cargo; metadata is workspace-scoped by default\n\n'
      } >> "$log_path"
      if (cd "$clone_root" && cargo metadata --format-version 1) >> "$log_path" 2>&1; then
        exit_code=0
      else
        exit_code=$?
      fi
    fi
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

copy_log="$run_log_dir/copy.log"
if ! command -v tar >/dev/null 2>&1; then
  write_summary "fail" "tar is required for the filesystem snapshot copy"
  echo "tar is required for the filesystem snapshot copy" >&2
  exit 1
fi

start_copy=$(date +%s)
if (
  cd "$repo_root"
  tar \
    --exclude='./.git' \
    --exclude='./target' \
    --exclude='./.codegraph' \
    --exclude='./.codegraph-index' \
    --exclude='./.codegraph-competitors' \
    --exclude='./.codegraph-bench-cache' \
    --exclude='./.codex-tools' \
    --exclude='./.tools' \
    --exclude='./reports/smoke/fresh_clone/windows_*' \
    --exclude='./reports/smoke/fresh_clone/linux_*' \
    --exclude='./reports/smoke/index/windows_*' \
    --exclude='./reports/smoke/index/linux_*' \
    --exclude='./reports/smoke/docker/run_*' \
    --exclude='./reports/audit' \
    --exclude='./reports/final/artifacts' \
    --exclude='./reports/comparison/cgc_recovery' \
    --exclude='node_modules' \
    --exclude='.next' \
    --exclude='.nuxt' \
    --exclude='coverage' \
    --exclude='*.db' \
    --exclude='*.db-shm' \
    --exclude='*.db-wal' \
    --exclude='*.sqlite' \
    --exclude='*.sqlite-shm' \
    --exclude='*.sqlite-wal' \
    --exclude='*.sqlite3' \
    --exclude='*.sqlite3-shm' \
    --exclude='*.sqlite3-wal' \
    --exclude='*.log' \
    --exclude='*.cgc-bundle' \
    --exclude='*.cgc-bundle.tmp' \
    -cf - . 2> "$copy_log"
) | (cd "$clone_root" && tar -xf - 2>> "$copy_log"); then
  copy_exit=0
else
  copy_exit=$?
fi
end_copy=$(date +%s)
record_step "copy_worktree" "$copy_exit" "$(( (end_copy - start_copy) * 1000 ))" "$copy_log"
if [ "$copy_exit" -ne 0 ]; then
  write_summary "fail" "filesystem snapshot failed"
  echo "filesystem snapshot failed. See $copy_log" >&2
  exit "$copy_exit"
fi

{
  printf 'repo_root=%s\n' "$repo_root"
  printf 'clone_root=%s\n' "$clone_root"
  printf 'path_with_spaces_tested=%s\n' "$path_with_spaces_tested"
  printf 'copy_mode=filesystem_snapshot\n'
  printf 'cargo=%s\n' "$(command -v cargo || true)"
} > "$run_log_dir/environment.txt"

run_cargo_metadata_step
run_step cargo_build cargo build --workspace
run_step cargo_test cargo test --workspace
run_step codegraph_mcp_help cargo run --bin codegraph-mcp -- --help

write_summary "pass"
echo "fresh clone smoke passed"
echo "logs: $run_log_dir"
echo "clone path: $clone_root"
