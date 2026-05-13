#!/usr/bin/env bash
set -euo pipefail

export PATH="/usr/bin:/bin:$PATH"

RUN_REPO_INDEX=0
SKIP_REPO_INDEX=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --run-repo-index) RUN_REPO_INDEX=1 ;;
    --skip-repo-index) SKIP_REPO_INDEX=1 ;;
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
log_root="reports/smoke/index"
run_log_dir="$log_root/linux_$run_id"
summary_path="$run_log_dir/summary.json"
steps_tsv="$run_log_dir/steps.tsv"

mkdir -p "$run_log_dir"
: > "$steps_tsv"

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

record_step() {
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" "$6" "$7" >> "$steps_tsv"
}

write_summary() {
  local status="$1"
  local failure="${2:-}"
  local first
  local row_name row_status row_exit_code row_duration_ms row_log_path row_command row_notes
  {
    printf '{\n'
    printf '  "schema_version": 1,\n'
    printf '  "status": "%s",\n' "$(json_escape "$status")"
    printf '  "failure": "%s",\n' "$(json_escape "$failure")"
    printf '  "run_id": "%s",\n' "$(json_escape "$run_id")"
    printf '  "generated_at": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "platform": "linux",\n'
    printf '  "repo_root": "%s",\n' "$(json_escape "$repo_root")"
    printf '  "fixture_repo": "fixtures/smoke/basic_repo",\n'
    printf '  "logs_dir": "%s",\n' "$(json_escape "$repo_root/$run_log_dir")"
    printf '  "fixture_index_mandatory": true,\n'
    printf '  "repo_index_local_only": true,\n'
    if [ "$SKIP_REPO_INDEX" -eq 0 ] && { [ -z "${CI:-}" ] || [ "$RUN_REPO_INDEX" -eq 1 ]; }; then
      printf '  "repo_index_requested": true,\n'
    else
      printf '  "repo_index_requested": false,\n'
    fi
    printf '  "requirements": {\n'
    printf '    "cgc_required": false,\n'
    printf '    "autoresearch_required": false,\n'
    printf '    "external_benchmark_artifacts_required": false,\n'
    printf '    "network_required": "only normal Cargo dependency resolution"\n'
    printf '  },\n'
    printf '  "steps": [\n'
    first=1
    while IFS='	' read -r row_name row_status row_exit_code row_duration_ms row_log_path row_command row_notes; do
      [ -n "$row_name" ] || continue
      if [ "$first" -eq 0 ]; then
        printf ',\n'
      fi
      first=0
      printf '    {"name":"%s","status":"%s","exit_code":%s,"duration_ms":%s,"log":"%s","command":"%s","notes":"%s"}' \
        "$(json_escape "$row_name")" \
        "$(json_escape "$row_status")" \
        "$row_exit_code" \
        "$row_duration_ms" \
        "$(json_escape "$row_log_path")" \
        "$(json_escape "$row_command")" \
        "$(json_escape "$row_notes")"
    done < "$steps_tsv"
    printf '\n  ]\n'
    printf '}\n'
  } > "$summary_path"
}

run_step() {
  local name="$1"
  local display="$2"
  local notes="$3"
  local log_path start exit_code end duration_ms
  shift 3
  log_path="$run_log_dir/$name.log"
  start=$(date +%s)
  {
    printf 'repo: %s\n' "$repo_root"
    printf 'command: %s\n\n' "$display"
  } > "$log_path"

  if (cd "$repo_root" && "$@") >> "$log_path" 2>&1; then
    exit_code=0
  else
    exit_code=$?
  fi

  end=$(date +%s)
  duration_ms=$(( (end - start) * 1000 ))
  record_step "$name" "completed" "$exit_code" "$duration_ms" "$log_path" "$display" "$notes"
  if [ "$exit_code" -ne 0 ]; then
    write_summary "fail" "$name failed with exit code $exit_code"
    echo "$name failed with exit code $exit_code. See $log_path" >&2
    exit "$exit_code"
  fi
}

skip_step() {
  local name="$1"
  local display="$2"
  local reason="$3"
  local log_path
  log_path="$run_log_dir/$name.log"
  {
    printf 'status: skipped\n'
    printf 'command: %s\n' "$display"
    printf 'reason: %s\n' "$reason"
  } > "$log_path"
  record_step "$name" "skipped" 0 0 "$log_path" "$display" "$reason"
}

binary="$repo_root/target/debug/codegraph-mcp"
if [ ! -x "$binary" ] && [ -x "$repo_root/target/debug/codegraph-mcp.exe" ]; then
  binary="$repo_root/target/debug/codegraph-mcp.exe"
fi
fixture_db="$repo_root/$run_log_dir/basic_repo.sqlite"
repo_db="$repo_root/$run_log_dir/repo_index.sqlite"

{
  printf 'repo_root=%s\n' "$repo_root"
  printf 'fixture_repo=fixtures/smoke/basic_repo\n'
  printf 'fixture_index_mandatory=true\n'
  printf 'repo_index_local_only=true\n'
  printf 'ci=%s\n' "${CI:-}"
  printf 'cargo=%s\n' "$(command -v cargo || true)"
} > "$run_log_dir/environment.txt"

run_step cargo_build "cargo build --workspace" "" cargo build --workspace
run_step fixture_index \
  "target/debug/codegraph-mcp index fixtures/smoke/basic_repo --db $fixture_db --profile --json" \
  "mandatory CI smoke; uses explicit --db to keep fixture source clean" \
  "$binary" index fixtures/smoke/basic_repo --db "$fixture_db" --profile --json

if [ "$SKIP_REPO_INDEX" -eq 1 ]; then
  skip_step repo_index "target/debug/codegraph-mcp index ." "skipped by --skip-repo-index"
elif [ -n "${CI:-}" ] && [ "$RUN_REPO_INDEX" -ne 1 ]; then
  skip_step repo_index "target/debug/codegraph-mcp index ." "repo index is local-only by default in CI; fixture index is the mandatory CI smoke"
else
  run_step repo_index \
    "target/debug/codegraph-mcp index . --db $repo_db --profile --json" \
    "local-only smoke; can be skipped in CI because full-repo indexing is larger than the deterministic fixture" \
    "$binary" index . --db "$repo_db" --profile --json
fi

write_summary "pass"
echo "index smoke passed"
echo "logs: $run_log_dir"
