#!/usr/bin/env bash
set -euo pipefail

export PATH="/usr/bin:/bin:$PATH"

script_path="${BASH_SOURCE[0]:-$0}"
case "$script_path" in
  */*) script_parent="${script_path%/*}" ;;
  *) script_parent="." ;;
esac
script_dir=$(CDPATH= cd -- "$script_parent" && pwd -P)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
cd "$repo_root"
run_id="$(date +%Y%m%d_%H%M%S)_$$"
log_root="reports/smoke/docker"
run_log_dir="$log_root/run_$run_id"
summary_path="$run_log_dir/summary.json"
steps_tsv="$run_log_dir/steps.tsv"
image_tag="codegraph-mcp:smoke-$run_id"

mkdir -p "$run_log_dir"
: > "$steps_tsv"

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

record_step() {
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" "$6" >> "$steps_tsv"
}

write_summary() {
  local status="$1"
  local failure="${2:-}"
  local first
  local row_name row_exit_code row_duration_ms row_log_path row_command row_notes
  {
    printf '{\n'
    printf '  "schema_version": 1,\n'
    printf '  "status": "%s",\n' "$(json_escape "$status")"
    printf '  "failure": "%s",\n' "$(json_escape "$failure")"
    printf '  "run_id": "%s",\n' "$(json_escape "$run_id")"
    printf '  "generated_at": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "repo_root": "%s",\n' "$(json_escape "$repo_root")"
    printf '  "image_tag": "%s",\n' "$(json_escape "$image_tag")"
    printf '  "logs_dir": "%s",\n' "$(json_escape "$repo_root/$run_log_dir")"
    printf '  "requirements": {\n'
    printf '    "cgc_required": false,\n'
    printf '    "autoresearch_required": false,\n'
    printf '    "external_benchmark_artifacts_required": false,\n'
    printf '    "network_required": "Docker base image plus normal Cargo dependency resolution"\n'
    printf '  },\n'
    printf '  "steps": [\n'
    first=1
    while IFS='	' read -r row_name row_exit_code row_duration_ms row_log_path row_command row_notes; do
      [ -n "$row_name" ] || continue
      if [ "$first" -eq 0 ]; then
        printf ',\n'
      fi
      first=0
      printf '    {"name":"%s","exit_code":%s,"duration_ms":%s,"log":"%s","command":"%s","notes":"%s"}' \
        "$(json_escape "$row_name")" \
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
  local notes="$2"
  local log_path command start exit_code end duration_ms
  shift 2
  log_path="$run_log_dir/$name.log"
  command="$*"
  start=$(date +%s)
  {
    printf 'repo: %s\n' "$repo_root"
    printf 'command: %s\n\n' "$command"
  } > "$log_path"

  if (cd "$repo_root" && "$@") >> "$log_path" 2>&1; then
    exit_code=0
  else
    exit_code=$?
  fi

  end=$(date +%s)
  duration_ms=$(( (end - start) * 1000 ))
  record_step "$name" "$exit_code" "$duration_ms" "$log_path" "$command" "$notes"
  if [ "$exit_code" -ne 0 ]; then
    write_summary "fail" "$name failed with exit code $exit_code"
    echo "$name failed with exit code $exit_code. See $log_path" >&2
    exit "$exit_code"
  fi
}

{
  printf 'repo_root=%s\n' "$repo_root"
  printf 'image_tag=%s\n' "$image_tag"
  printf 'docker=%s\n' "$(command -v docker || true)"
} > "$run_log_dir/environment.txt"

if ! command -v docker >/dev/null 2>&1; then
  write_summary "fail" "docker CLI not found"
  echo "docker CLI not found" >&2
  exit 1
fi

run_step docker_build "builds the workspace and runs fixture index during Dockerfile build" \
  docker build --progress=plain -t "$image_tag" .

run_step docker_help "default command runs codegraph-mcp --help" \
  docker run --rm "$image_tag"

run_step docker_fixture_index "container can run fixture index smoke after build" \
  docker run --rm "$image_tag" ./target/debug/codegraph-mcp index fixtures/smoke/basic_repo --db /tmp/codegraph-basic-repo-runtime.sqlite --profile --json

write_summary "pass"
echo "docker smoke passed"
echo "logs: $run_log_dir"
