use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use codegraph_core::{
    Edge, EdgeClass, EdgeContext, Exactness, FileRecord, RelationKind, SourceSpan,
};
use codegraph_index::{scope_policy_hash, IndexScopeOptions, StorageMode};
use codegraph_mcp_server::{McpServer, McpServerConfig};
use codegraph_store::{
    DbPassport, GraphStore, SqliteGraphStore, DB_PASSPORT_VERSION, SCHEMA_VERSION,
};
use serde_json::{json, Value};

fn run_codegraph(args: &[&str]) -> Output {
    run_codegraph_in(Path::new(env!("CARGO_MANIFEST_DIR")), args)
}

fn run_codegraph_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codegraph-mcp"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("failed to run codegraph-mcp")
}

fn stdout_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout JSON")
}

fn stderr_json(output: &Output) -> Value {
    assert!(
        !output.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stderr).expect("stderr JSON")
}

fn mcp_server_for_repo(repo: &Path) -> McpServer {
    McpServer::new(McpServerConfig::for_repo(repo).without_trace())
}

fn mcp_ok(result: Result<Value, impl std::fmt::Debug>) -> Value {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected MCP Ok(..), got Err({error:?})"),
    }
}

fn assert_hits_present(value: &Value, label: &str) {
    assert!(
        !value["hits"].as_array().expect("hits").is_empty(),
        "expected hits for {label}: {value:?}"
    );
}

fn assert_hits_empty(value: &Value, label: &str) {
    assert!(
        value["hits"].as_array().expect("hits").is_empty(),
        "expected no hits for {label}: {value:?}"
    );
}

fn passport_scope_hash(value: &Value) -> &str {
    value["db_lifecycle_read"]["passport_scope_hash"]
        .as_str()
        .expect("passport scope hash")
}

#[test]
fn help_smoke() {
    let output = run_codegraph(&["--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CodeGraph Memory Layer CLI"));
    assert!(stdout.contains("serve-mcp"));
    assert!(stdout.contains("bench"));
    assert!(stdout.contains("trace"));
    assert!(stdout.contains("audit"));
    assert!(stdout.contains("languages"));
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("--repo <path>"));
}

#[test]
fn version_and_config_metadata_are_schema_stable() {
    let version = stdout_json(&run_codegraph(&["--json", "--version"]));
    assert_eq!(version["schema_version"].as_u64(), Some(1));
    assert_eq!(version["name"].as_str(), Some("codegraph-mcp"));
    assert!(version["target"].is_object());
    assert!(version["feature_flags"].is_array());

    let metadata = stdout_json(&run_codegraph(&["config", "release-metadata", "--json"]));
    assert_eq!(metadata["status"].as_str(), Some("ok"));
    assert_eq!(metadata["release"]["schema_version"].as_u64(), Some(1));
    assert!(metadata["release"]["archives"]
        .as_array()
        .expect("archives")
        .iter()
        .any(|archive| archive["name"].as_str() == Some("linux-x64")));
}

#[test]
fn release_packaging_templates_are_present_and_dry_run_safe() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let manifest_path = workspace_root.join("dist").join("archive-manifest.json");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("archive manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["schema_version"].as_u64(), Some(1));
    assert!(manifest["archives"]
        .as_array()
        .expect("archives")
        .iter()
        .any(|archive| archive["name"].as_str() == Some("windows-x64")));

    let install_ps1 =
        fs::read_to_string(workspace_root.join("install").join("install.ps1")).expect("ps1");
    let install_sh =
        fs::read_to_string(workspace_root.join("install").join("install.sh")).expect("sh");
    let release_workflow = fs::read_to_string(
        workspace_root
            .join(".github")
            .join("workflows")
            .join("release.yml"),
    )
    .expect("release workflow");

    assert!(install_ps1.contains("DryRun"));
    assert!(install_ps1.contains("network = \"not used in dry run\""));
    assert!(install_sh.contains("--dry-run"));
    assert!(install_sh.contains("network\":\"not used in dry run"));
    assert!(release_workflow.contains("package metadata dry run"));
    assert!(workspace_root
        .join("packaging")
        .join("homebrew")
        .join("codegraph-mcp.rb")
        .exists());
    assert!(workspace_root
        .join("dist")
        .join("cargo-binstall.example.toml")
        .exists());
}

#[test]
fn index_profile_and_doctor_json_outputs_are_structured() {
    let repo = fixture_repo();
    let index = stdout_json(&run_codegraph(&[
        "index",
        repo.to_str().expect("repo path"),
        "--profile",
        "--json",
    ]));
    assert_eq!(index["files_indexed"].as_u64(), Some(1));
    assert!(index["profile"]["file_discovery_ms"].is_u64());
    assert!(
        index["profile"]["worker_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );

    let doctor = stdout_json(&run_codegraph(&[
        "doctor",
        repo.to_str().expect("repo path"),
        "--json",
    ]));
    assert_eq!(doctor["status"].as_str(), Some("ok"));
    assert!(doctor["checks"].as_array().expect("checks").len() >= 5);

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn bench_synthetic_index_generates_profiled_repo() {
    let output_dir = empty_repo();
    fs::remove_dir_all(&output_dir).expect("start absent output dir");

    let result = stdout_json(&run_codegraph(&[
        "bench",
        "synthetic-index",
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--files",
        "6",
    ]));
    assert_eq!(result["status"].as_str(), Some("benchmarked"));
    assert_eq!(result["kind"].as_str(), Some("synthetic_index"));
    assert!(Path::new(result["manifest"].as_str().expect("manifest")).exists());
    assert_eq!(result["index_summary"]["files_indexed"].as_u64(), Some(6));
    assert!(result["index_summary"]["profile"].is_object());

    fs::remove_dir_all(output_dir).expect("cleanup synthetic benchmark");
}

#[test]
fn bench_update_integrity_harness_repeats_updates_and_writes_reports() {
    let output_dir = empty_repo();
    let out_json = output_dir.join("update-integrity.json");
    let out_md = output_dir.join("update-integrity.md");
    let workdir = output_dir.join("work");

    let result = stdout_json(&run_codegraph(&[
        "bench",
        "update-integrity",
        "--mode",
        "update-fast",
        "--iterations",
        "2",
        "--workers",
        "2",
        "--medium-files",
        "4",
        "--skip-autoresearch",
        "--workdir",
        workdir.to_str().expect("workdir path"),
        "--out-json",
        out_json.to_str().expect("json path"),
        "--out-md",
        out_md.to_str().expect("markdown path"),
    ]));

    assert_eq!(result["status"].as_str(), Some("passed"));
    assert_eq!(result["benchmark"].as_str(), Some("update_integrity"));
    assert!(out_json.exists());
    assert!(out_md.exists());
    let report: Value = serde_json::from_str(&fs::read_to_string(&out_json).expect("report json"))
        .expect("valid update-integrity report");
    assert_eq!(report["status"].as_str(), Some("passed"));
    assert_eq!(report["update_mode"].as_str(), Some("update-fast"));
    assert!(report["repos"].as_array().expect("repos").len() >= 2);
    for repo in report["repos"].as_array().expect("repos") {
        assert_eq!(repo["update_mode"].as_str(), Some("update-fast"));
        assert_eq!(repo["all_integrity_checks_passed"].as_bool(), Some(true));
        assert_eq!(
            repo["graph_fact_hash_stable_on_repeat"].as_bool(),
            Some(true)
        );
        assert_eq!(
            repo["changed_file_updates_graph_fact_hash"].as_bool(),
            Some(true)
        );
        assert_eq!(
            repo["restore_returns_to_repeat_graph_fact_hash"].as_bool(),
            Some(true)
        );
        for iteration in repo["iteration_results"].as_array().expect("iterations") {
            let update = &iteration["update"];
            assert_eq!(update["mode"].as_str(), Some("update-fast"));
            assert_eq!(update["global_hash_check_ran"].as_bool(), Some(false));
            assert_eq!(update["graph_counts_ran"].as_bool(), Some(false));
            assert_eq!(
                update["graph_digest_kind"].as_str(),
                Some("incremental_graph_digest")
            );
        }
    }

    fs::remove_dir_all(output_dir).expect("cleanup update-integrity benchmark");
}

#[test]
fn bench_update_integrity_repeat_loop_and_partial_artifacts_are_explicit() {
    let output_dir = empty_repo();
    let out_json = output_dir.join("repeat-loop.json");
    let out_md = output_dir.join("repeat-loop.md");
    let workdir = output_dir.join("work");

    let result = stdout_json(&run_codegraph(&[
        "bench",
        "update-integrity",
        "--mode",
        "update-fast",
        "--loop-kind",
        "repeat-fast",
        "--iterations",
        "2",
        "--workers",
        "2",
        "--medium-files",
        "4",
        "--skip-autoresearch",
        "--workdir",
        workdir.to_str().expect("workdir path"),
        "--out-json",
        out_json.to_str().expect("json path"),
        "--out-md",
        out_md.to_str().expect("markdown path"),
    ]));

    assert_eq!(result["status"].as_str(), Some("passed"));
    let report: Value = serde_json::from_str(&fs::read_to_string(&out_json).expect("report json"))
        .expect("valid repeat-loop report");
    assert_eq!(report["loop_kind"].as_str(), Some("repeat-fast"));
    for repo in report["repos"].as_array().expect("repos") {
        assert_eq!(repo["loop_kind"].as_str(), Some("repeat-fast"));
        assert_eq!(
            repo["repeat_iterations"].as_array().expect("repeats").len(),
            2
        );
        assert!(repo["iteration_results"]
            .as_array()
            .expect("updates")
            .is_empty());
    }

    let timeout_json = output_dir.join("timeout.json");
    let timeout_md = output_dir.join("timeout.md");
    let timeout_work = output_dir.join("timeout-work");
    let timeout = stdout_json(&run_codegraph(&[
        "bench",
        "update-integrity",
        "--mode",
        "update-fast",
        "--loop-kind",
        "repeat-fast",
        "--iterations",
        "2",
        "--timeout-ms",
        "1",
        "--skip-autoresearch",
        "--workdir",
        timeout_work.to_str().expect("timeout work"),
        "--out-json",
        timeout_json.to_str().expect("timeout json"),
        "--out-md",
        timeout_md.to_str().expect("timeout md"),
    ]));
    assert!(matches!(
        timeout["status"].as_str(),
        Some("timeout") | Some("failed")
    ));
    assert!(timeout_json.exists());
    let timeout_report: Value =
        serde_json::from_str(&fs::read_to_string(&timeout_json).expect("timeout report"))
            .expect("valid timeout report");
    assert!(matches!(
        timeout_report["status"].as_str(),
        Some("timeout") | Some("failed")
    ));

    fs::remove_dir_all(output_dir).expect("cleanup update-integrity repeat benchmark");
}

#[test]
fn bench_update_integrity_errors_emit_json_artifact() {
    let output_dir = empty_repo();
    let out_json = output_dir.join("failed-update-integrity.json");
    let out_md = output_dir.join("failed-update-integrity.md");

    let result = stdout_json(&run_codegraph(&[
        "bench",
        "update-integrity",
        "--only-autoresearch",
        "--autoresearch-repo",
        output_dir
            .join("missing-repo")
            .to_str()
            .expect("missing repo"),
        "--out-json",
        out_json.to_str().expect("json path"),
        "--out-md",
        out_md.to_str().expect("markdown path"),
    ]));

    assert_eq!(result["status"].as_str(), Some("failed"));
    assert!(out_json.exists());
    assert!(out_md.exists());
    let report: Value = serde_json::from_str(&fs::read_to_string(&out_json).expect("report json"))
        .expect("valid failed report");
    assert_eq!(report["status"].as_str(), Some("failed"));
    assert!(report["error"]
        .as_str()
        .unwrap_or("")
        .contains("did not find"));

    fs::remove_dir_all(output_dir).expect("cleanup failed update-integrity benchmark");
}

#[test]
fn languages_command_reports_frontend_tiers_and_exactness() {
    let table = run_codegraph(&["languages"]);
    assert!(table.status.success());
    let stdout = String::from_utf8_lossy(&table.stdout);
    assert!(stdout.contains("Language"));
    assert!(stdout.contains("typescript"));
    assert!(stdout.contains("python"));
    assert!(stdout.contains("static_heuristic"));

    let json = stdout_json(&run_codegraph(&["languages", "--json"]));
    assert_eq!(json["status"].as_str(), Some("ok"));
    let frontends = json["frontends"].as_array().expect("frontends");
    assert!(frontends.iter().any(
        |frontend| frontend["language_id"].as_str() == Some("python")
            && frontend["support_tier"].as_str() == Some("tier3_calls_caller_callee")
    ));
    assert!(frontends.iter().any(|frontend| {
        frontend["language_id"].as_str() == Some("typescript")
            && frontend["compiler_resolver_available"].as_bool() == Some(true)
    }));
}

#[test]
fn bench_command_outputs_machine_readable_report() {
    let value = stdout_json(&run_codegraph(&["bench", "--baseline", "graph-only"]));

    assert_eq!(value["schema_version"].as_u64(), Some(1));
    assert_eq!(
        value["generated_by"].as_str(),
        Some("codegraph-bench phase 20")
    );
    assert!(!value["results"].as_array().expect("results").is_empty());
    assert!(value["aggregate"].get("graph_only").is_some());
}

#[test]
fn bench_graph_truth_gate_runs_adversarial_fixtures_and_writes_reports() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let cases = workspace_root
        .join("benchmarks")
        .join("graph_truth")
        .join("fixtures");
    let output_dir = empty_repo().join("graph-truth-output");
    let out_json = output_dir.join("report.json");
    let out_md = output_dir.join("report.md");

    let value = stdout_json(&run_codegraph(&[
        "bench",
        "graph-truth",
        "--cases",
        cases.to_str().expect("cases path"),
        "--fixture-root",
        workspace_root.to_str().expect("workspace root"),
        "--out-json",
        out_json.to_str().expect("json path"),
        "--out-md",
        out_md.to_str().expect("markdown path"),
        "--fail-on-forbidden",
        "--fail-on-missing-source-span",
        "--fail-on-unresolved-exact",
        "--fail-on-derived-without-provenance",
        "--fail-on-test-mock-production-leak",
        "--update-mode",
    ]));

    assert_eq!(value["gate"].as_str(), Some("graph_truth"));
    assert_eq!(value["cases_total"].as_u64(), Some(11));
    assert!(out_json.exists());
    assert!(out_md.exists());

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&out_json).expect("graph truth JSON"))
            .expect("graph truth report is valid JSON");
    assert_eq!(report["gate"].as_str(), Some("graph_truth"));
    assert_eq!(report["cases_total"].as_u64(), Some(11));
    assert!(report["cases"].as_array().expect("cases").len() >= 2);
    let first_case = &report["cases"].as_array().expect("cases")[0];
    for field in [
        "case_id",
        "status",
        "failures",
        "missing_entities",
        "missing_edges",
        "forbidden_edges_found",
        "missing_paths",
        "forbidden_paths_found",
        "source_span_failures",
        "context_symbol_failures",
        "expected_test_failures",
        "mutation_failures",
        "timing",
        "index_time",
        "query_time",
        "entity_count",
        "edge_count",
        "source_span_count",
    ] {
        assert!(
            first_case.get(field).is_some(),
            "graph truth case result missing {field}"
        );
    }
    assert!(
        report["totals"]["forbidden_edges"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(fs::read_to_string(&out_md)
        .expect("graph truth markdown")
        .contains("Graph Truth Gate"));

    fs::remove_dir_all(output_dir.parent().expect("temp root"))
        .expect("cleanup graph truth output");
}

#[test]
fn bench_context_packet_gate_runs_adversarial_fixtures_and_writes_reports() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let cases = workspace_root
        .join("benchmarks")
        .join("graph_truth")
        .join("fixtures");
    let output_dir = empty_repo().join("context-packet-output");
    let out_json = output_dir.join("report.json");
    let out_md = output_dir.join("report.md");

    let value = stdout_json(&run_codegraph(&[
        "bench",
        "context-packet",
        "--cases",
        cases.to_str().expect("cases path"),
        "--fixture-root",
        workspace_root.to_str().expect("workspace root"),
        "--out-json",
        out_json.to_str().expect("json path"),
        "--out-md",
        out_md.to_str().expect("markdown path"),
        "--top-k",
        "10",
    ]));

    assert_eq!(value["gate"].as_str(), Some("context_packet"));
    assert_eq!(value["cases_total"].as_u64(), Some(11));
    assert!(value["distractor_ratio"].is_number());
    assert!(value["useful_facts_per_byte"].is_number());
    assert!(out_json.exists());
    assert!(out_md.exists());

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&out_json).expect("context packet JSON"))
            .expect("context packet report is valid JSON");
    assert_eq!(report["gate"].as_str(), Some("context_packet"));
    assert_eq!(report["cases_total"].as_u64(), Some(11));
    assert!(report["metrics"]["distractor_ratio"].is_number());
    assert!(report["metrics"]["useful_facts_per_byte"].is_number());
    assert!(fs::read_to_string(&out_md)
        .expect("context packet markdown")
        .contains("Context Packet Gate"));

    fs::remove_dir_all(output_dir.parent().expect("temp root"))
        .expect("cleanup context packet output");
}

#[test]
fn bench_retrieval_ablation_reports_stage0_and_full_funnel_separately() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let cases = workspace_root
        .join("benchmarks")
        .join("graph_truth")
        .join("fixtures");
    let output_dir = empty_repo().join("retrieval-ablation-output");
    let out_json = output_dir.join("report.json");
    let out_md = output_dir.join("report.md");

    let value = stdout_json(&run_codegraph(&[
        "bench",
        "retrieval-ablation",
        "--cases",
        cases.to_str().expect("cases path"),
        "--fixture-root",
        workspace_root.to_str().expect("workspace root"),
        "--out-json",
        out_json.to_str().expect("json path"),
        "--out-md",
        out_md.to_str().expect("markdown path"),
        "--mode",
        "stage0_exact_only",
        "--mode",
        "full_context_packet",
        "--top-k",
        "5",
    ]));

    assert_eq!(value["benchmark"].as_str(), Some("retrieval_ablation"));
    assert_eq!(value["cases_total"].as_u64(), Some(11));
    assert!(out_json.exists());
    assert!(out_md.exists());

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&out_json).expect("ablation JSON"))
            .expect("ablation report is valid JSON");
    let modes = report["modes"].as_array().expect("modes");
    assert!(modes
        .iter()
        .any(|mode| mode["mode"].as_str() == Some("stage0_exact_only")));
    assert!(modes
        .iter()
        .any(|mode| mode["mode"].as_str() == Some("full_context_packet")));
    assert!(modes.iter().all(|mode| {
        mode["mode"].as_str() != Some("stage0_exact_only")
            || mode["proof_grade_path_success_claimed"].as_bool() == Some(false)
    }));
    assert!(fs::read_to_string(&out_md)
        .expect("ablation markdown")
        .contains("Retrieval Stage Ablation"));

    fs::remove_dir_all(output_dir.parent().expect("temp root")).expect("cleanup ablation output");
}

#[test]
fn bench_cgc_comparison_skips_missing_competitor_and_writes_reports() {
    let output_dir = empty_repo().join("reports").join("cgc-comparison");
    let missing_bin = output_dir.join("missing-cgc.exe");
    let value = stdout_json(&run_codegraph(&[
        "bench",
        "cgc-comparison",
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--competitor-bin",
        missing_bin.to_str().expect("missing bin path"),
        "--timeout-ms",
        "25",
    ]));

    assert_eq!(value["status"].as_str(), Some("benchmarked"));
    assert_eq!(value["phase"].as_str(), Some("21.1"));
    assert_eq!(
        value["benchmark_id"].as_str(),
        Some("codegraphcontext-external-comparison")
    );
    assert!(output_dir.join("run.json").exists());
    assert!(output_dir.join("per_task.jsonl").exists());
    assert!(output_dir.join("summary.md").exists());
    assert!(value["aggregate"].get("codegraphcontext_cli").is_some());

    let workspace = output_dir
        .parent()
        .and_then(Path::parent)
        .expect("temp workspace");
    fs::remove_dir_all(workspace).expect("cleanup cgc comparison workspace");
}

#[test]
fn bench_gaps_writes_scoreboard_and_skips_missing_competitor() {
    let output_dir = empty_repo().join("reports").join("gaps");
    let missing_bin = output_dir.join("missing-cgc.exe");
    let value = stdout_json(&run_codegraph(&[
        "bench",
        "gaps",
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--competitor-bin",
        missing_bin.to_str().expect("missing bin path"),
        "--timeout-ms",
        "25",
    ]));

    assert_eq!(value["status"].as_str(), Some("reported"));
    assert_eq!(value["phase"].as_str(), Some("26"));
    assert!(output_dir.join("summary.json").exists());
    assert!(output_dir.join("summary.md").exists());
    assert!(output_dir.join("per_task.jsonl").exists());
    assert!(output_dir
        .join("external-codegraphcontext")
        .join("run.json")
        .exists());

    let summary: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("summary.json")).expect("summary"),
    )
    .expect("summary JSON");
    assert!(summary["dimensions"]
        .as_array()
        .expect("dimensions")
        .iter()
        .any(|dimension| dimension["id"].as_str() == Some("symbol_search_recall_at_k")));
    assert!(summary["competitor_metadata"]["executable_used"]
        .as_str()
        .expect("executable")
        .contains("skipped"));

    let workspace = output_dir
        .parent()
        .and_then(Path::parent)
        .expect("temp workspace");
    fs::remove_dir_all(workspace).expect("cleanup gap scoreboard workspace");
}

#[test]
fn bench_final_gate_reports_compact_mvp_verdict_and_unknown_cgc() {
    let output_dir = empty_repo().join("reports").join("final-gate");
    let missing_bin = output_dir.join("missing-cgc.exe");
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let value = stdout_json(&run_codegraph(&[
        "bench",
        "final-gate",
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--workspace-root",
        workspace_root.to_str().expect("workspace path"),
        "--competitor-bin",
        missing_bin.to_str().expect("missing bin path"),
        "--timeout-ms",
        "25",
    ]));

    assert_eq!(value["status"].as_str(), Some("reported"));
    assert_eq!(value["gate"].as_str(), Some("final_compact_mvp_acceptance"));
    assert_eq!(value["internal_verdict"].as_str(), Some("pass"));
    assert_eq!(value["verdict"].as_str(), Some("unknown"));
    assert_eq!(value["cgc_status"].as_str(), Some("skipped"));
    assert!(output_dir.join("summary.json").exists());
    assert!(output_dir.join("summary.md").exists());

    let summary: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("summary.json")).expect("summary"),
    )
    .expect("summary JSON");
    assert_eq!(
        summary["indexing"]["counts_equivalent"].as_bool(),
        Some(true)
    );
    assert!(
        summary["storage"]["db_size_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        summary["storage"]["source_span_count"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        summary["storage"]["relation_counts"]["CALLS"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(
        summary["storage"]["proof_object_counts"]["path_evidence_generated"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    assert!(summary["mvp_proof_checks"]
        .as_array()
        .expect("proof checks")
        .iter()
        .any(
            |check| check["id"].as_str() == Some("source_spans_preserved")
                && check["status"].as_str() == Some("pass")
        ));
    assert!(summary["functionality_checks"]
        .as_array()
        .expect("functionality checks")
        .iter()
        .any(|check| check["id"].as_str() == Some("context_packet")
            && check["status"].as_str() == Some("pass")));
    assert_eq!(
        summary["storage"]["size_targets"]["smaller_than_cgc_same_repo"].as_str(),
        Some("not_comparable_incomplete_cgc")
    );
    assert!(summary["cgc_comparison"]["executable_path"]
        .as_str()
        .expect("cgc executable")
        .contains("skipped"));

    let workspace = output_dir
        .parent()
        .and_then(Path::parent)
        .expect("temp workspace");
    fs::remove_dir_all(workspace).expect("cleanup final gate workspace");
}

#[test]
fn bench_comprehensive_writes_machine_json_and_human_markdown() {
    let workspace = empty_repo();
    let repo = fixture_repo();
    let output_dir = workspace.join("reports").join("final");
    let artifact_dir = workspace.join("artifacts");
    fs::create_dir_all(&artifact_dir).expect("create artifacts");

    let proof_storage_path = artifact_dir.join("proof_storage.json");
    fs::write(
        &proof_storage_path,
        serde_json::to_string_pretty(&json!({
            "file_family": { "total_bytes": 300_000_000u64 },
            "integrity_check": { "status": "ok" },
            "objects": [
                { "name": "template_entities", "object_type": "table", "row_count": 10, "total_bytes": 1000 },
                { "name": "template_edges", "object_type": "table", "row_count": 5, "total_bytes": 500 },
                { "name": "symbol_dict", "object_type": "table", "row_count": 10, "total_bytes": 300 },
                { "name": "qname_prefix_dict", "object_type": "table", "row_count": 2, "total_bytes": 200 },
                { "name": "entities", "object_type": "table", "row_count": 4, "total_bytes": 400 },
                { "name": "edges", "object_type": "table", "row_count": 3, "total_bytes": 300 },
                { "name": "file_source_spans", "object_type": "table", "row_count": 3, "total_bytes": 300 },
                { "name": "path_evidence", "object_type": "table", "row_count": 1, "total_bytes": 100 },
                { "name": "callsites", "object_type": "table", "row_count": 1, "total_bytes": 100 },
                { "name": "callsite_args", "object_type": "table", "row_count": 2, "total_bytes": 100 },
                { "name": "files", "object_type": "table", "row_count": 1, "total_bytes": 100 },
                { "name": "source_content_template", "object_type": "table", "row_count": 1, "total_bytes": 100 },
                { "name": "idx_template_edges_head_relation", "object_type": "index", "row_count": null, "total_bytes": 50 }
            ],
            "aggregate_metrics": {
                "average_database_bytes_per_edge": 100_000.0,
                "average_edge_table_plus_index_bytes_per_edge": 100.0
            },
            "fts_storage": { "stores_source_snippets": false },
            "table_row_metrics": [
                { "table": "entities", "average_total_bytes_per_row": 100.0 },
                { "table": "template_entities", "average_total_bytes_per_row": 100.0 },
                { "table": "template_edges", "average_total_bytes_per_row": 100.0 },
                { "table": "file_source_spans", "average_total_bytes_per_row": 100.0 },
                { "table": "path_evidence", "average_total_bytes_per_row": 100.0 }
            ]
        }))
        .expect("storage JSON"),
    )
    .expect("write storage");

    let baseline_path = workspace.join("baseline.json");
    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&json!({
            "storage_summary": {
                "proof_file_family_mib": 286.1,
                "top_storage_contributors": [
                    { "object": "template_entities", "rows": 10, "bytes": 1000 }
                ]
            }
        }))
        .expect("baseline JSON"),
    )
    .expect("write baseline");

    let gate_path = workspace.join("gate.json");
    fs::write(
        &gate_path,
        serde_json::to_string_pretty(&json!({
            "artifacts": {
                "proof_storage_json": proof_storage_path.to_string_lossy()
            },
            "gates": {
                "graph_truth": {
                    "cases_total": 11,
                    "cases_passed": 11,
                    "expected_entities": 2,
                    "matched_entities": 2,
                    "expected_edges": 2,
                    "matched_expected_edges": 2,
                    "expected_paths": 1,
                    "matched_expected_paths": 1,
                    "matched_forbidden_edges": 0,
                    "matched_forbidden_paths": 0,
                    "source_span_failures": 0,
                    "unresolved_exact_violations": 0,
                    "derived_without_provenance_violations": 0,
                    "test_mock_production_leakage": 0,
                    "stale_failures": 0
                },
                "context_packet": {
                    "cases_total": 11,
                    "cases_passed": 11,
                    "critical_symbol_recall": 1.0,
                    "proof_path_coverage": 1.0,
                    "source_span_coverage": 1.0,
                    "expected_test_recall": 1.0,
                    "distractor_ratio": 0.0
                }
            },
            "storage": {
                "proof": {
                    "file_family_bytes": 300_000_000u64,
                    "file_family_mib": 286.1,
                    "wal_bytes": 0,
                    "path_evidence_rows": 1,
                    "physical_edge_rows": 3,
                    "integrity_status": "ok"
                },
                "audit": {
                    "file_family_bytes": 0,
                    "audit_only_sidecar_rows": 0
                }
            },
            "autoresearch": {
                "proof_build": {
                    "wall_ms": 61_000,
                    "db_write_ms": 55_000,
                    "integrity_check_ms": 100,
                    "files_walked": 1,
                    "files_parsed": 1,
                    "duplicate_local_analyses_skipped": 0
                }
            },
            "relation_sampler": {
                "stored_path_evidence_count": 1,
                "generated_path_evidence_count": 0
            },
            "update_path": {
                "repeat_unchanged": {
                    "profile_wall_ms": 100,
                    "files_walked": 1,
                    "files_read": 0,
                    "files_hashed": 0,
                    "files_parsed": 0,
                    "entities_inserted": 0,
                    "edges_inserted": 0,
                    "integrity_status": "ok"
                },
                "single_file_update": {
                    "wall_ms": 100,
                    "entities_inserted": 1,
                    "edges_inserted": 1,
                    "dirty_path_evidence_count": 1,
                    "integrity_status": "ok"
                }
            },
            "query_latency": {
                "context_pack": {
                    "p95_shell_ms": 100,
                    "note": "fixture smoke"
                }
            }
        }))
        .expect("gate JSON"),
    )
    .expect("write gate");

    let value = stdout_json(&run_codegraph(&[
        "bench",
        "comprehensive",
        "--baseline",
        baseline_path.to_str().expect("baseline path"),
        "--compact-gate-json",
        gate_path.to_str().expect("gate path"),
        "--repo",
        repo.to_str().expect("repo path"),
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--timestamp",
        "fixture",
        "--allow-debug-timing",
        "--no-previous",
        "--allow-debug-timing",
    ]));

    assert_eq!(value["status"].as_str(), Some("reported"));
    assert_eq!(value["benchmark"].as_str(), Some("comprehensive"));
    assert!(matches!(
        value["verdict"].as_str(),
        Some("pass") | Some("fail")
    ));
    assert!(output_dir
        .join("comprehensive_benchmark_latest.json")
        .exists());
    assert!(output_dir
        .join("comprehensive_benchmark_latest.md")
        .exists());
    assert!(output_dir
        .join("comprehensive_benchmark_fixture.json")
        .exists());
    assert!(output_dir
        .join("comprehensive_benchmark_fixture.md")
        .exists());

    let summary: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("comprehensive_benchmark_latest.json"))
            .expect("summary JSON file"),
    )
    .expect("valid comprehensive JSON");
    assert_eq!(summary["schema_version"].as_u64(), Some(1));
    assert_eq!(
        summary["execution_mode"].as_str(),
        Some("fresh_proof_build")
    );
    assert_eq!(
        summary["debug_assertions"].as_bool(),
        Some(cfg!(debug_assertions))
    );
    assert_eq!(
        summary["binary_profile"].as_str(),
        Some(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
    );
    assert_eq!(
        summary["claimable_for_thresholds"].as_bool(),
        Some(!cfg!(debug_assertions))
    );
    assert_eq!(
        summary["diagnostic_only"].as_bool(),
        Some(cfg!(debug_assertions))
    );
    assert_eq!(
        summary["binary_metadata"]["exact_command"],
        summary["exact_command"]
    );
    assert!(summary["exact_command"]
        .as_str()
        .expect("exact command")
        .contains("bench comprehensive"));
    assert_eq!(
        summary["artifact_freshness"]["freshly_built"].as_bool(),
        Some(true)
    );
    assert_eq!(
        summary["artifact_freshness"]["artifact_reuse"].as_bool(),
        Some(false)
    );
    let metadata_path = summary["artifact_freshness"]["artifact_metadata_path"]
        .as_str()
        .expect("artifact metadata path");
    assert!(Path::new(metadata_path).exists());
    assert_eq!(
        summary["artifact_freshness"]["debug_assertions"].as_bool(),
        Some(cfg!(debug_assertions))
    );
    assert_eq!(
        summary["artifact_freshness"]["binary_profile"].as_str(),
        Some(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
    );
    assert_eq!(
        summary["artifact_freshness"]["claimable_for_thresholds"].as_bool(),
        Some(!cfg!(debug_assertions))
    );
    assert!(summary["artifact_freshness"]["exact_command"]
        .as_str()
        .expect("artifact exact command")
        .contains("bench comprehensive"));
    assert!(
        summary["sections"]["executive_verdict"]["exact_passed_targets"]
            .as_array()
            .expect("passed targets")
            .iter()
            .any(|target| target.as_str() == Some("storage_result_claimable"))
    );
    assert!(
        summary["sections"]["cold_proof_build_profile"]["mode_distinction"]
            .as_array()
            .expect("cold mode distinction")
            .iter()
            .any(|mode| mode["mode"].as_str() == Some("proof-build-only"))
    );
    assert!(summary["sections"]["cold_proof_build_profile"]["waterfall"]
        .as_array()
        .expect("cold waterfall")
        .iter()
        .any(|stage| stage["stage"].as_str()
            == Some("production_persistence_and_global_reduction_bucket")));
    if cfg!(debug_assertions) {
        assert_eq!(
            summary["timing_separation"]["proof_build_only_ms"].as_str(),
            Some("non_claimable_debug_timing")
        );
        assert!(summary["artifact_freshness"]["diagnostic_debug_proof_build_only_ms"].is_number());
        assert_eq!(
            summary["artifact_freshness"]["cold_build_result_claimable"].as_bool(),
            Some(false)
        );
        let failed = summary["sections"]["executive_verdict"]["exact_failed_targets"]
            .as_array()
            .expect("failed targets");
        assert!(failed
            .iter()
            .any(|target| target.as_str() == Some("cold_build_result_claimable")));
        assert!(failed.iter().any(|target| {
            target.as_str() == Some("proof_build_timing_claimable_for_thresholds")
        }));
        let cold_metrics = summary["sections"]["cold_proof_build_profile"]["metrics"]
            .as_array()
            .expect("cold metrics");
        let claim_metric = cold_metrics
            .iter()
            .find(|metric| {
                metric["id"].as_str() == Some("proof_build_timing_claimable_for_thresholds")
            })
            .expect("claimable metric");
        assert_eq!(claim_metric["status"].as_str(), Some("fail"));
        let wall_metric = cold_metrics
            .iter()
            .find(|metric| metric["id"].as_str() == Some("cold_proof_build_total_wall_ms"))
            .expect("wall metric");
        assert_eq!(wall_metric["status"].as_str(), Some("unknown"));
        assert_eq!(
            wall_metric["observed"].as_str(),
            Some("non_claimable_debug_timing")
        );
    } else {
        assert!(summary["timing_separation"]["proof_build_only_ms"].is_number());
    }
    assert!(summary["timing_separation"]["validation_ms"].is_number());
    assert!(summary["timing_separation"]["audit_ms"].is_number());
    assert!(summary["timing_separation"]["report_generation_ms"].is_number());
    assert!(summary["timing_separation"]["comprehensive_total_ms"].is_number());
    assert_eq!(
        summary["sections"]["timing_separation"]["proof_build_only_ms"],
        summary["timing_separation"]["proof_build_only_ms"]
    );
    assert_eq!(
        summary["artifact_freshness"]["artifact_validation"]["validated"].as_bool(),
        Some(false),
        "fresh comprehensive artifacts are proof-build-only plus separate audit, not validation-mode artifacts"
    );
    assert_eq!(
        summary["artifact_freshness"]["binary_profile"].as_str(),
        Some(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
    );
    assert_eq!(
        summary["artifact_freshness"]["claimable_for_thresholds"].as_bool(),
        Some(!cfg!(debug_assertions))
    );
    assert_eq!(
        summary["timing_separation"]["claimable_for_thresholds"].as_bool(),
        Some(!cfg!(debug_assertions))
    );
    if cfg!(debug_assertions) {
        let failed = summary["sections"]["executive_verdict"]["exact_failed_targets"]
            .as_array()
            .expect("failed targets");
        assert!(failed
            .iter()
            .any(|target| target.as_str() == Some("proof_build_timing_claimable_for_thresholds")));
        assert!(failed
            .iter()
            .any(|target| target.as_str() == Some("cold_build_result_claimable")));
    }
    let markdown =
        fs::read_to_string(output_dir.join("comprehensive_benchmark_latest.md")).expect("markdown");
    assert!(markdown.contains("Section 1 - Executive Verdict"));
    assert!(markdown.contains("Section 4A - Proof Artifact Freshness"));
    assert!(markdown.contains("Section 4B - Timing Separation"));
    assert!(markdown.contains("Section 6 - Storage Contributors"));
    assert!(markdown.contains("Cold Build Mode Distinction"));
    if cfg!(debug_assertions) {
        assert!(markdown.contains("non_claimable_debug_timing"));
        assert!(markdown.contains("binary_profile"));
    }

    fs::remove_dir_all(workspace).expect("cleanup comprehensive workspace");
    fs::remove_dir_all(repo).expect("cleanup comprehensive repo");
}

#[test]
fn bench_comprehensive_debug_requires_explicit_diagnostic_flag() {
    if !cfg!(debug_assertions) {
        return;
    }

    let workspace = empty_repo();
    let repo = fixture_repo();
    let output_dir = workspace.join("reports").join("final");
    let (baseline_path, gate_path) = write_minimal_comprehensive_inputs(&workspace);
    let output = run_codegraph(&[
        "bench",
        "comprehensive",
        "--baseline",
        baseline_path.to_str().expect("baseline path"),
        "--compact-gate-json",
        gate_path.to_str().expect("gate path"),
        "--repo",
        repo.to_str().expect("repo path"),
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--timestamp",
        "debug_refusal",
        "--no-previous",
    ]);

    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).expect("structured benchmark error");
    assert_eq!(error["error"].as_str(), Some("bench_failed"));
    assert_eq!(error["binary_profile"].as_str(), Some("debug"));
    assert_eq!(error["debug_assertions"].as_bool(), Some(true));
    assert_eq!(error["claimable_for_thresholds"].as_bool(), Some(false));
    assert!(error["message"]
        .as_str()
        .expect("message")
        .contains("--allow-debug-timing"));

    fs::remove_dir_all(workspace).expect("cleanup comprehensive debug refusal workspace");
    fs::remove_dir_all(repo).expect("cleanup comprehensive debug refusal repo");
}

#[test]
fn bench_proof_build_only_has_clean_contract_and_metadata() {
    let repo = fixture_repo();
    let db = repo.join("proof-only.sqlite");
    let result = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "bench",
            "proof-build-only",
            "--repo",
            repo.to_str().expect("repo path"),
            "--db",
            db.to_str().expect("db path"),
            "--allow-debug-timing",
        ],
    ));

    assert_eq!(result["benchmark"].as_str(), Some("proof_build_only"));
    assert_eq!(result["mode"].as_str(), Some("proof-build-only"));
    assert_eq!(
        result["debug_assertions"].as_bool(),
        Some(cfg!(debug_assertions))
    );
    assert_eq!(
        result["binary_profile"].as_str(),
        Some(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
    );
    assert_eq!(
        result["claimable_for_thresholds"].as_bool(),
        Some(!cfg!(debug_assertions))
    );
    assert_eq!(
        result["diagnostic_only"].as_bool(),
        Some(cfg!(debug_assertions))
    );
    assert!(result["current_exe"]
        .as_str()
        .expect("current exe")
        .contains("codegraph-mcp"));
    assert!(result["exact_command"]
        .as_str()
        .expect("exact command")
        .contains("bench proof-build-only"));
    assert_eq!(
        result["binary_metadata"]["exact_command"],
        result["exact_command"]
    );
    if cfg!(debug_assertions) {
        assert_eq!(
            result["proof_build_only_ms"].as_str(),
            Some("non_claimable_debug_timing")
        );
        assert!(
            result["diagnostic_debug_proof_build_only_ms"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
    } else {
        assert!(result["proof_build_only_ms"].as_u64().unwrap_or(0) > 0);
        assert!(result["diagnostic_debug_proof_build_only_ms"].is_null());
    }
    assert_eq!(result["validation_ms"].as_f64(), Some(0.0));
    assert_eq!(result["audit_ms"].as_u64(), Some(0));
    assert!(result["report_generation_ms"].as_u64().is_some());
    assert!(result["comprehensive_total_ms"].is_null());
    assert_eq!(
        result["binary_profile"].as_str(),
        Some(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
    );
    assert_eq!(
        result["claimable_for_thresholds"].as_bool(),
        Some(!cfg!(debug_assertions))
    );
    assert_eq!(
        result["timing_classification"].as_str(),
        Some(if cfg!(debug_assertions) {
            "diagnostic_only"
        } else {
            "production"
        })
    );

    let metadata_path = Path::new(
        result["artifact_metadata_path"]
            .as_str()
            .expect("metadata path"),
    );
    assert!(metadata_path.exists());
    let metadata: Value =
        serde_json::from_str(&fs::read_to_string(metadata_path).expect("metadata JSON"))
            .expect("valid metadata");
    assert_eq!(metadata["build_mode"].as_str(), Some("proof-build-only"));
    assert!(metadata["build_duration_ms"].as_u64().unwrap_or(0) > 0);
    assert_eq!(
        metadata["debug_assertions"].as_bool(),
        Some(cfg!(debug_assertions))
    );
    assert_eq!(
        metadata["binary_profile"].as_str(),
        result["binary_profile"].as_str()
    );
    assert_eq!(
        metadata["claimable_for_thresholds"].as_bool(),
        Some(!cfg!(debug_assertions))
    );
    assert!(metadata["exact_command"]
        .as_str()
        .expect("metadata exact command")
        .contains("bench proof-build-only"));
    if cfg!(debug_assertions) {
        assert_eq!(
            metadata["proof_build_only_ms"].as_str(),
            Some("non_claimable_debug_timing")
        );
        assert!(
            metadata["diagnostic_debug_proof_build_only_ms"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
    }
    assert_eq!(
        metadata["artifact_validation"]["validated"].as_bool(),
        Some(false)
    );
    assert_eq!(
        metadata["artifact_validation"]["claimable_as_validated"].as_bool(),
        Some(false)
    );

    let contract = &result["mode_separation"]["prohibited_operations_ran"];
    for key in [
        "storage_audit",
        "dbstat",
        "relation_sampler",
        "path_evidence_sampler",
        "cgc_comparison",
        "manual_precision_summary",
        "readme_or_report_generation",
        "full_comprehensive_benchmark_generation",
        "repeated_build_attempts",
        "artifact_compression",
        "vacuum",
        "analyze",
        "fresh_repeat_update_loops",
    ] {
        assert_eq!(contract[key].as_bool(), Some(false), "{key}");
    }
    assert_eq!(
        result["mode_separation"]["post_build_checks"]["full_integrity_check_ran"].as_bool(),
        Some(false)
    );
    assert_eq!(
        result["mode_separation"]["post_build_checks"]["quick_check_ran"].as_bool(),
        Some(true)
    );

    let spans = result["summary"]["profile"]["spans"]
        .as_array()
        .expect("profile spans");
    assert!(spans
        .iter()
        .any(|span| span["name"].as_str() == Some("quick_check")));
    assert!(!spans.iter().any(|span| {
        span["name"].as_str() == Some("integrity_check") && span["count"].as_u64().unwrap_or(0) > 0
    }));
    for prohibited_span in [
        "storage_audit",
        "relation_sampler",
        "path_evidence_sampler",
        "cgc_comparison",
        "repeat_unchanged_index",
        "single_file_update",
    ] {
        assert!(
            !spans.iter().any(|span| {
                span["name"].as_str() == Some(prohibited_span)
                    && span["count"].as_u64().unwrap_or(0) > 0
            }),
            "{prohibited_span} should not execute in proof-build-only"
        );
    }
    assert!(!repo.join("reports").exists());

    fs::remove_dir_all(repo).expect("cleanup proof-build-only repo");
}

#[test]
fn bench_proof_build_only_debug_requires_explicit_diagnostic_flag() {
    if !cfg!(debug_assertions) {
        return;
    }

    let repo = fixture_repo();
    let db = repo.join("proof-only.sqlite");
    let output = run_codegraph_in(
        &repo,
        &[
            "bench",
            "proof-build-only",
            "--repo",
            repo.to_str().expect("repo path"),
            "--db",
            db.to_str().expect("db path"),
        ],
    );

    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr).expect("structured benchmark error");
    assert_eq!(error["error"].as_str(), Some("bench_failed"));
    assert_eq!(error["binary_profile"].as_str(), Some("debug"));
    assert_eq!(error["debug_assertions"].as_bool(), Some(true));
    assert_eq!(error["claimable_for_thresholds"].as_bool(), Some(false));
    assert_eq!(error["diagnostic_only"].as_bool(), Some(true));
    assert!(error["exact_command"]
        .as_str()
        .expect("exact command")
        .contains("bench proof-build-only"));
    assert!(error["message"]
        .as_str()
        .expect("message")
        .contains("--allow-debug-timing"));

    fs::remove_dir_all(repo).expect("cleanup debug refusal repo");
}

#[test]
fn bench_proof_build_validated_marks_artifact_validated_only_after_integrity_gate() {
    let repo = fixture_repo();
    let db = repo.join("validated.sqlite");
    let result = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "bench",
            "proof-build-validated",
            "--repo",
            repo.to_str().expect("repo path"),
            "--db",
            db.to_str().expect("db path"),
            "--allow-debug-timing",
        ],
    ));

    assert_eq!(result["benchmark"].as_str(), Some("proof_build_validated"));
    assert_eq!(result["mode"].as_str(), Some("proof-build-plus-validation"));
    assert!(result["proof_build_only_ms"].as_u64().unwrap_or(0) > 0);
    assert!(result["validation_ms"].as_f64().unwrap_or(0.0) > 0.0);
    assert_eq!(
        result["mode_separation"]["post_build_checks"]["full_integrity_check_ran"].as_bool(),
        Some(true)
    );
    let metadata_path = Path::new(
        result["artifact_metadata_path"]
            .as_str()
            .expect("metadata path"),
    );
    let metadata: Value =
        serde_json::from_str(&fs::read_to_string(metadata_path).expect("metadata JSON"))
            .expect("valid metadata");
    assert_eq!(
        metadata["artifact_validation"]["validated"].as_bool(),
        Some(true)
    );
    assert_eq!(
        metadata["artifact_validation"]["claimable_as_validated"].as_bool(),
        Some(true)
    );
    assert!(
        metadata["artifact_validation"]["integrity_check_count"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );

    fs::remove_dir_all(repo).expect("cleanup proof-build-validated repo");
}

#[test]
fn bench_comprehensive_debug_timing_fails_without_explicit_allowance() {
    let workspace = empty_repo();
    let output_dir = workspace.join("reports").join("final");
    let repo = fixture_repo();
    let (baseline_path, gate_path) = write_minimal_comprehensive_inputs(&workspace);

    let output = run_codegraph(&[
        "bench",
        "comprehensive",
        "--baseline",
        baseline_path.to_str().expect("baseline path"),
        "--compact-gate-json",
        gate_path.to_str().expect("gate path"),
        "--repo",
        repo.to_str().expect("repo path"),
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--timestamp",
        "debug_guard",
        "--no-previous",
    ]);

    if cfg!(debug_assertions) {
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("debug binary"));
        assert!(stderr.contains("--allow-debug-timing"));
    } else {
        assert!(output.status.success());
    }

    fs::remove_dir_all(workspace).expect("cleanup comprehensive debug guard workspace");
    fs::remove_dir_all(repo).expect("cleanup comprehensive debug guard repo");
}

#[test]
fn bench_proof_build_modes_reject_contaminating_overrides() {
    let repo = fixture_repo();
    let audit_storage = run_codegraph_in(
        &repo,
        &[
            "bench",
            "proof-build-only",
            "--repo",
            repo.to_str().expect("repo path"),
            "--allow-debug-timing",
            "--storage-mode",
            "audit",
        ],
    );
    assert!(!audit_storage.status.success());
    assert!(
        String::from_utf8_lossy(&audit_storage.stderr).contains("requires --storage-mode proof")
    );

    let validation_override = run_codegraph_in(
        &repo,
        &[
            "bench",
            "proof-build-only",
            "--repo",
            repo.to_str().expect("repo path"),
            "--allow-debug-timing",
            "--build-mode",
            "proof-build-plus-validation",
        ],
    );
    assert!(!validation_override.status.success());
    assert!(String::from_utf8_lossy(&validation_override.stderr)
        .contains("cannot be run with --build-mode"));

    fs::remove_dir_all(repo).expect("cleanup contaminated override repo");
}

#[test]
fn bench_comprehensive_explicit_reuse_is_marked_with_claimable_metadata() {
    let workspace = empty_repo();
    let output_dir = workspace.join("reports").join("final");
    let (baseline_path, gate_path) = write_minimal_comprehensive_inputs(&workspace);
    let (db_path, schema_version) = create_passported_empty_codegraph_db(&workspace);
    let metadata_path = workspace.join("artifact.metadata.json");
    write_artifact_metadata(
        &db_path,
        &metadata_path,
        schema_version,
        schema_version,
        "proof",
        123,
    );

    let value = stdout_json(&run_codegraph(&[
        "bench",
        "comprehensive",
        "--use-existing-artifact",
        db_path.to_str().expect("db path"),
        "--artifact-metadata",
        metadata_path.to_str().expect("metadata path"),
        "--baseline",
        baseline_path.to_str().expect("baseline path"),
        "--compact-gate-json",
        gate_path.to_str().expect("gate path"),
        "--repo",
        workspace.to_str().expect("repo path"),
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--timestamp",
        "reuse",
        "--no-previous",
    ]));
    assert_eq!(value["status"].as_str(), Some("reported"));

    let summary: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("comprehensive_benchmark_latest.json"))
            .expect("summary JSON file"),
    )
    .expect("valid comprehensive JSON");
    assert_eq!(
        summary["execution_mode"].as_str(),
        Some("explicit_artifact_reuse")
    );
    assert_eq!(
        summary["artifact_freshness"]["artifact_reuse"].as_bool(),
        Some(true)
    );
    assert_eq!(
        summary["artifact_freshness"]["stale"].as_bool(),
        Some(false)
    );
    let passed = summary["sections"]["executive_verdict"]["exact_passed_targets"]
        .as_array()
        .expect("passed targets");
    assert!(passed
        .iter()
        .any(|target| target.as_str() == Some("storage_result_claimable")));
    assert!(passed
        .iter()
        .any(|target| target.as_str() == Some("cold_build_result_claimable")));

    fs::remove_dir_all(workspace).expect("cleanup reuse workspace");
}

#[test]
fn bench_query_surface_measures_default_compact_proof_queries() {
    let workspace = empty_repo();
    let repo = fixture_repo();
    let out_json = workspace
        .join("reports")
        .join("audit")
        .join("default_query_surface.json");
    let out_md = workspace
        .join("reports")
        .join("audit")
        .join("default_query_surface.md");

    let value = stdout_json(&run_codegraph(&[
        "bench",
        "query-surface",
        "--fresh",
        "--repo",
        repo.to_str().expect("repo path"),
        "--iterations",
        "2",
        "--out-json",
        out_json.to_str().expect("json path"),
        "--out-md",
        out_md.to_str().expect("md path"),
    ]));

    assert_eq!(value["status"].as_str(), Some("passed"));
    assert!(out_json.exists());
    assert!(out_md.exists());
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&out_json).expect("query-surface JSON file"))
            .expect("valid query-surface JSON");
    let queries = report["queries"].as_array().expect("queries");
    for id in [
        "entity_name_lookup",
        "symbol_lookup",
        "qname_lookup",
        "text_fts_query",
        "relation_query_calls",
        "relation_query_reads_writes",
        "path_evidence_lookup",
        "source_snippet_batch_load",
        "context_pack_normal",
        "unresolved_calls_paginated",
    ] {
        let query = queries
            .iter()
            .find(|query| query["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("missing query metric {id}"));
        assert_eq!(query["status"].as_str(), Some("pass"), "{id}");
        assert!(query["observed"]["p95_ms"].is_number(), "{id} p95");
        assert!(query.get("sql").is_some(), "{id} sql");
        assert!(query["explain_query_plan"].is_array(), "{id} plan");
    }
    let unresolved = queries
        .iter()
        .find(|query| query["id"].as_str() == Some("unresolved_calls_paginated"))
        .expect("unresolved metric");
    assert_eq!(
        unresolved["db_lifecycle_read"]["operation_kind"].as_str(),
        Some("benchmark_inspection")
    );
    assert_eq!(
        unresolved["db_lifecycle_read"]["safe_to_read"].as_bool(),
        Some(true)
    );
    assert_eq!(
        unresolved["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        report["db_path"].as_str()
    );

    fs::remove_dir_all(workspace).expect("cleanup query surface workspace");
    fs::remove_dir_all(repo).expect("cleanup query surface repo");
}

#[test]
fn bench_comprehensive_stale_artifact_fails_when_requested() {
    let workspace = empty_repo();
    let output_dir = workspace.join("reports").join("final");
    let (baseline_path, gate_path) = write_minimal_comprehensive_inputs(&workspace);
    let (db_path, _) = create_empty_codegraph_db(&workspace);

    let output = run_codegraph(&[
        "bench",
        "comprehensive",
        "--use-existing-artifact",
        db_path.to_str().expect("db path"),
        "--fail-on-stale-artifact",
        "--baseline",
        baseline_path.to_str().expect("baseline path"),
        "--compact-gate-json",
        gate_path.to_str().expect("gate path"),
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--timestamp",
        "stale",
        "--no-previous",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stale artifact refused"));
    assert!(stderr.contains("missing freshness metadata"));

    fs::remove_dir_all(workspace).expect("cleanup stale workspace");
}

#[test]
fn bench_comprehensive_schema_mismatch_fails_when_requested() {
    let workspace = empty_repo();
    let output_dir = workspace.join("reports").join("final");
    let (baseline_path, gate_path) = write_minimal_comprehensive_inputs(&workspace);
    let (db_path, schema_version) = create_empty_codegraph_db(&workspace);
    let metadata_path = workspace.join("artifact.metadata.json");
    write_artifact_metadata(
        &db_path,
        &metadata_path,
        schema_version.saturating_sub(1),
        schema_version,
        "proof",
        123,
    );

    let output = run_codegraph(&[
        "bench",
        "comprehensive",
        "--use-existing-artifact",
        db_path.to_str().expect("db path"),
        "--artifact-metadata",
        metadata_path.to_str().expect("metadata path"),
        "--fail-on-stale-artifact",
        "--baseline",
        baseline_path.to_str().expect("baseline path"),
        "--compact-gate-json",
        gate_path.to_str().expect("gate path"),
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--timestamp",
        "schema_mismatch",
        "--no-previous",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("schema mismatch"));

    fs::remove_dir_all(workspace).expect("cleanup schema mismatch workspace");
}

#[test]
fn bench_comprehensive_stale_artifact_cannot_claim_storage_win() {
    let workspace = empty_repo();
    let output_dir = workspace.join("reports").join("final");
    let (baseline_path, gate_path) = write_minimal_comprehensive_inputs(&workspace);
    let (db_path, schema_version) = create_empty_codegraph_db(&workspace);
    let metadata_path = workspace.join("artifact.metadata.json");
    write_artifact_metadata(
        &db_path,
        &metadata_path,
        schema_version.saturating_sub(1),
        schema_version,
        "proof",
        123,
    );

    let value = stdout_json(&run_codegraph(&[
        "bench",
        "comprehensive",
        "--use-existing-artifact",
        db_path.to_str().expect("db path"),
        "--artifact-metadata",
        metadata_path.to_str().expect("metadata path"),
        "--baseline",
        baseline_path.to_str().expect("baseline path"),
        "--compact-gate-json",
        gate_path.to_str().expect("gate path"),
        "--output-dir",
        output_dir.to_str().expect("output path"),
        "--timestamp",
        "stale_reported",
        "--no-previous",
    ]));
    assert_eq!(value["status"].as_str(), Some("reported"));

    let summary: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("comprehensive_benchmark_latest.json"))
            .expect("summary JSON file"),
    )
    .expect("valid comprehensive JSON");
    assert_eq!(summary["artifact_freshness"]["stale"].as_bool(), Some(true));
    assert_eq!(
        summary["artifact_freshness"]["storage_result_claimable"].as_bool(),
        Some(false)
    );
    let failed = summary["sections"]["executive_verdict"]["exact_failed_targets"]
        .as_array()
        .expect("failed targets");
    assert!(failed
        .iter()
        .any(|target| target.as_str() == Some("storage_result_claimable")));
    let markdown =
        fs::read_to_string(output_dir.join("comprehensive_benchmark_latest.md")).expect("markdown");
    assert!(markdown.contains("stale artifact; storage result not claimable"));

    fs::remove_dir_all(workspace).expect("cleanup stale reported workspace");
}

#[test]
fn init_dry_run_reports_plan_without_writing() {
    let repo = fixture_repo();

    let output = run_codegraph_in(
        &repo,
        &[
            "init",
            "--dry-run",
            "--with-codex-config",
            "--with-agents",
            "--with-skills",
            "--with-hooks",
            "--index",
        ],
    );
    let value = stdout_json(&output);

    assert_eq!(value["status"].as_str(), Some("dry_run"));
    assert!(value["actions"].as_array().expect("actions").len() >= 5);
    assert!(!repo.join(".codegraph").exists());

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn init_generates_codex_skill_hook_and_agents_templates() {
    let repo = empty_repo();

    let value = stdout_json(&run_codegraph_in(
        &repo,
        &["init", "--with-templates", "--with-codex-config"],
    ));
    assert_eq!(value["status"].as_str(), Some("initialized"));

    let agents = fs::read_to_string(repo.join("AGENTS.md")).expect("read AGENTS template");
    assert_template_guardrails(&agents);

    for skill in EXPECTED_SKILLS {
        let path = repo
            .join(".codex")
            .join("skills")
            .join(skill)
            .join("SKILL.md");
        let contents = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("read generated skill template {}: {error}", path.display())
        });
        assert_template_guardrails(&contents);
        assert!(contents.contains(&format!("name: {skill}")));
        assert!(contents.contains("description:"));
        assert!(contents.contains("codegraph.find_auth_paths"));
        assert!(contents.contains("codegraph.find_mutations"));
    }

    let hooks_dir = repo.join(".codex").join("hooks");
    let hook_config_path = hooks_dir.join("codegraph-hooks.json");
    let hook_config_contents =
        fs::read_to_string(&hook_config_path).expect("read generated hook config");
    assert_template_guardrails(&hook_config_contents);
    let hook_config: Value =
        serde_json::from_str(&hook_config_contents).expect("hook config is valid JSON");
    let hooks = hook_config["hooks"].as_array().expect("hooks array");
    for hook in EXPECTED_HOOKS {
        assert!(
            hooks
                .iter()
                .any(|entry| entry["event"].as_str() == Some(hook)),
            "missing hook event {hook}"
        );
        let contents = fs::read_to_string(hooks_dir.join(format!("{hook}.md")))
            .unwrap_or_else(|error| panic!("read generated hook template {hook}: {error}"));
        assert_template_guardrails(&contents);
        if matches!(*hook, "PostToolUse" | "Stop") {
            assert!(contents.contains("codegraph-mcp trace append"));
        }
        if *hook == "Stop" {
            assert!(contents.contains("codegraph-mcp trace replay"));
        }
    }

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn index_status_and_query_commands_work_on_fixture_repo() {
    let repo = fixture_repo();

    let index = stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--profile", "--workers", "1"],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));
    assert_eq!(index["files_indexed"].as_u64(), Some(1));
    assert_eq!(index["profile"]["worker_count"].as_u64(), Some(1));
    assert!(repo.join(".codegraph").join("codegraph.sqlite").exists());
    let db_path = repo.join(".codegraph").join("codegraph.sqlite");

    let status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(status["files"].as_u64(), Some(1));
    assert_eq!(
        status["storage_policy"].as_str(),
        Some("proof:compact-proof-graph")
    );
    assert!(status["db_size_bytes"].as_u64().unwrap_or_default() > 0);
    assert!(status["source_spans"].as_u64().unwrap_or_default() > 0);
    assert!(!status["storage_accounting"]
        .as_array()
        .expect("storage accounting")
        .is_empty());
    let relation_counts = status["relation_counts"]
        .as_object()
        .expect("relation counts");
    for relation in [
        "CONTAINS",
        "DEFINED_IN",
        "CALLS",
        "CALLEE",
        "ARGUMENT_0",
        "RETURNS_TO",
    ] {
        assert!(
            relation_counts
                .get(relation)
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0,
            "missing relation {relation}: {relation_counts:?}"
        );
    }

    let schema_check = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "schema-check",
            "--db",
            db_path.to_str().expect("db path"),
        ],
    ));
    assert_eq!(schema_check["status"].as_str(), Some("ok"));
    assert_eq!(schema_check["failure_count"].as_u64(), Some(0));

    let symbols = stdout_json(&run_codegraph_in(&repo, &["query", "symbols", "login"]));
    assert_eq!(symbols["status"].as_str(), Some("ok"));
    assert!(!symbols["hits"].as_array().expect("hits").is_empty());

    let text = stdout_json(&run_codegraph_in(&repo, &["query", "text", "sanitize"]));
    assert_eq!(text["status"].as_str(), Some("ok"));
    assert!(!text["hits"].as_array().expect("hits").is_empty());

    let files = stdout_json(&run_codegraph_in(&repo, &["query", "files", "auth"]));
    assert_eq!(files["status"].as_str(), Some("ok"));
    assert!(!files["hits"].as_array().expect("hits").is_empty());

    let definitions = stdout_json(&run_codegraph_in(&repo, &["query", "definitions", "login"]));
    assert_eq!(definitions["status"].as_str(), Some("ok"));
    assert!(!definitions["definitions"]
        .as_array()
        .expect("definitions")
        .is_empty());

    let references = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "references", "sanitize"],
    ));
    assert_eq!(references["status"].as_str(), Some("ok"));
    assert!(!references["references"]
        .as_array()
        .expect("references")
        .is_empty());

    let callers = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "callers", "--fuzzy", "sanitize"],
    ));
    assert_eq!(callers["status"].as_str(), Some("ok"));
    assert!(!callers["callers"].as_array().expect("callers").is_empty());

    let callees = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "callees", "--fuzzy", "login"],
    ));
    assert_eq!(callees["status"].as_str(), Some("ok"));
    assert!(!callees["callees"].as_array().expect("callees").is_empty());

    let chain = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "chain", "login", "saveUser"],
    ));
    assert_eq!(chain["status"].as_str(), Some("ok"));
    assert!(!chain["paths"].as_array().expect("paths").is_empty());

    let unresolved = stdout_json(&run_codegraph_in(&repo, &["query", "unresolved-calls"]));
    assert_eq!(unresolved["status"].as_str(), Some("ok"));
    assert!(unresolved["calls"].as_array().expect("calls").is_empty());

    let path = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "path", "login", "sanitize"],
    ));
    assert_eq!(path["status"].as_str(), Some("ok"));
    assert!(!path["paths"].as_array().expect("paths").is_empty());

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn audit_storage_micro_tiny_run_writes_reports_and_uses_external_dbs() {
    let workspace = empty_repo();
    let output_dir = workspace.join("storage micro output");
    let out_json = output_dir.join("summary.json");
    let out_md = output_dir.join("summary.md");

    let value = stdout_json(&run_codegraph(&[
        "audit",
        "storage-micro",
        "--out",
        output_dir.to_str().expect("output path"),
        "--cases",
        "simple",
        "--batch-sizes",
        "1",
        "--keep-artifacts",
        "--no-context-pack",
        "--json",
        out_json.to_str().expect("json path"),
        "--markdown",
        out_md.to_str().expect("markdown path"),
    ]));

    assert_eq!(value["status"].as_str(), Some("ok"));
    assert_eq!(value["audit"].as_str(), Some("storage_micro"));
    assert_eq!(value["normal_codegraph_db_created"].as_bool(), Some(false));
    assert!(out_json.exists());
    assert!(out_md.exists());

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&out_json).expect("storage micro JSON"))
            .expect("storage micro report JSON validates");
    assert_eq!(report["status"].as_str(), Some("ok"));
    assert_eq!(
        report["command"].as_str(),
        Some("codegraph-mcp audit storage-micro")
    );
    assert_eq!(report["artifacts_kept"].as_bool(), Some(true));
    assert_eq!(
        report["safety"]["normal_codegraph_db_created"].as_bool(),
        Some(false)
    );
    assert_eq!(report["context_pack_checks"]["ran"].as_bool(), Some(false));
    assert_eq!(
        report["storage_budget"]["policy_version"].as_str(),
        Some("storage-budget-v1")
    );
    assert_eq!(
        report["storage_budget"]["budget_status"].as_str(),
        Some("ok")
    );
    assert_eq!(
        report["storage_budget"]["diagnostic_only"].as_bool(),
        Some(true)
    );
    assert_eq!(report["storage_budget"]["claimable"].as_bool(), Some(false));
    let cases = report["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 2);
    assert!(cases.iter().any(|case| {
        case["case"].as_str() == Some("schema_baseline_empty_repo")
            && case["db_path_is_absolute"].as_bool() == Some(true)
    }));
    let n1 = cases
        .iter()
        .find(|case| case["case"].as_str() == Some("n1_simple_2kb_files"))
        .expect("n1 case");
    assert_eq!(n1["db_path_is_absolute"].as_bool(), Some(true));
    assert!(n1["source_bytes"].as_u64().unwrap_or_default() >= 1536);
    assert!(Path::new(
        n1["storage_audit_json"]
            .as_str()
            .expect("storage audit JSON")
    )
    .exists());
    assert!(Path::new(n1["case_log"].as_str().expect("case log")).exists());
    assert!(!Path::new(n1["repo_path"].as_str().expect("repo path"))
        .join(".codegraph")
        .exists());
    assert!(fs::read_to_string(&out_md)
        .expect("storage micro markdown")
        .contains("Storage Micro Diagnostic"));

    fs::remove_dir_all(workspace).expect("cleanup storage micro workspace");
}

#[test]
fn storage_budget_refuses_linux_stress_without_extended() {
    let repo = fixture_repo();
    for corpus in ["linux", "buildroot"] {
        let output = run_codegraph(&[
            "index",
            repo.to_str().expect("repo path"),
            "--stress-corpus",
            corpus,
            "--json",
        ]);
        let error = stderr_json(&output);
        assert_eq!(error["error"].as_str(), Some("storage_budget_refused"));
        assert_eq!(
            error["storage_budget"]["stress_corpus"].as_str(),
            Some(corpus)
        );
        assert_eq!(
            error["storage_budget"]["budget_status"].as_str(),
            Some("refused")
        );
        assert!(error["message"]
            .as_str()
            .expect("message")
            .contains("--extended"));
    }

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn storage_budget_refuses_linux_stress_without_explicit_external_db() {
    let repo = fixture_repo();
    let output = run_codegraph(&[
        "index",
        repo.to_str().expect("repo path"),
        "--extended",
        "--stress-corpus",
        "linux",
        "--json",
    ]);
    let error = stderr_json(&output);
    assert_eq!(error["error"].as_str(), Some("storage_budget_refused"));
    assert_eq!(
        error["storage_budget"]["external_db_required"].as_bool(),
        Some(true)
    );
    assert!(error["message"]
        .as_str()
        .expect("message")
        .contains("explicit external --db"));

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn storage_budget_refuses_stress_output_under_codegraph_or_reports_final() {
    let workspace = empty_repo();
    let under_codegraph = workspace.join(".codegraph").join("stress");
    let codegraph_error = stderr_json(&run_codegraph_in(
        &workspace,
        &[
            "audit",
            "storage-micro",
            "--out",
            under_codegraph.to_str().expect("output path"),
            "--extended",
            "--stress-corpus",
            "linux",
            "--no-context-pack",
        ],
    ));
    assert_eq!(
        codegraph_error["storage_budget"]["default_codegraph_refused"].as_bool(),
        Some(true)
    );

    let under_reports_final = workspace.join("reports").join("final").join("stress");
    let reports_final_error = stderr_json(&run_codegraph_in(
        &workspace,
        &[
            "audit",
            "storage-micro",
            "--out",
            under_reports_final.to_str().expect("output path"),
            "--extended",
            "--stress-corpus",
            "linux",
            "--no-context-pack",
        ],
    ));
    assert_eq!(
        reports_final_error["storage_budget"]["reports_final_refused"].as_bool(),
        Some(true)
    );

    fs::remove_dir_all(workspace).expect("cleanup storage budget workspace");
}

#[test]
fn storage_budget_accepts_buildroot_medium_corpus_with_external_output() {
    let workspace = empty_repo();
    let output_dir = workspace.join("buildroot medium output");
    let value = stdout_json(&run_codegraph_in(
        &workspace,
        &[
            "audit",
            "storage-micro",
            "--out",
            output_dir.to_str().expect("output path"),
            "--cases",
            "simple",
            "--batch-sizes",
            "1",
            "--no-context-pack",
            "--json",
            "--extended",
            "--stress-corpus",
            "buildroot",
        ],
    ));
    assert_eq!(value["status"].as_str(), Some("ok"));
    assert_eq!(
        value["storage_budget"]["stress_corpus"].as_str(),
        Some("buildroot")
    );
    assert_eq!(
        value["storage_budget"]["budget_status"].as_str(),
        Some("ok")
    );
    assert!(
        value["storage_budget"]["min_free_disk_gib"]
            .as_f64()
            .unwrap_or_default()
            >= 5.0
    );

    fs::remove_dir_all(workspace).expect("cleanup buildroot storage budget workspace");
}

#[test]
fn storage_budget_reports_postflight_db_and_artifact_overruns() {
    let db_workspace = empty_repo();
    let db_output = db_workspace.join("db budget");
    let db_error = stderr_json(&run_codegraph(&[
        "audit",
        "storage-micro",
        "--out",
        db_output.to_str().expect("output path"),
        "--cases",
        "simple",
        "--batch-sizes",
        "1",
        "--keep-artifacts",
        "--no-context-pack",
        "--max-db-mib",
        "0.000001",
    ]));
    assert_eq!(
        db_error["storage_budget"]["budget_status"].as_str(),
        Some("over_budget")
    );
    assert_eq!(
        db_error["storage_budget"]["claimable"].as_bool(),
        Some(false)
    );
    assert!(
        db_error["storage_budget"]["db_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    fs::remove_dir_all(db_workspace).expect("cleanup db budget workspace");

    let artifact_workspace = empty_repo();
    let artifact_output = artifact_workspace.join("artifact budget");
    let artifact_error = stderr_json(&run_codegraph(&[
        "audit",
        "storage-micro",
        "--out",
        artifact_output.to_str().expect("output path"),
        "--cases",
        "simple",
        "--batch-sizes",
        "1",
        "--keep-artifacts",
        "--no-context-pack",
        "--max-artifacts-mib",
        "0.000001",
    ]));
    assert_eq!(
        artifact_error["storage_budget"]["budget_status"].as_str(),
        Some("over_budget")
    );
    assert!(
        artifact_error["storage_budget"]["artifact_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
    fs::remove_dir_all(artifact_workspace).expect("cleanup artifact budget workspace");
}

#[test]
fn storage_budget_min_free_disk_refusal_is_structured() {
    let workspace = empty_repo();
    let output_dir = workspace.join("free disk budget");
    let error = stderr_json(&run_codegraph(&[
        "audit",
        "storage-micro",
        "--out",
        output_dir.to_str().expect("output path"),
        "--cases",
        "simple",
        "--batch-sizes",
        "1",
        "--no-context-pack",
        "--min-free-disk-gib",
        "999999999",
    ]));
    assert_eq!(error["error"].as_str(), Some("storage_budget_refused"));
    assert_eq!(
        error["storage_budget"]["budget_status"].as_str(),
        Some("disk_check_failed")
    );
    assert_eq!(error["storage_budget"]["claimable"].as_bool(), Some(false));

    fs::remove_dir_all(workspace).expect("cleanup min free disk workspace");
}

#[test]
fn status_missing_external_db_is_structured_and_read_only() {
    let repo = empty_repo();
    let workspace = empty_repo();
    let db_parent = workspace.join("external profile");
    fs::create_dir_all(&db_parent).expect("create DB parent");
    let db_path = db_parent.join("production-agent-use.sqlite");
    let repo_arg = repo.to_str().expect("repo path");
    let db_arg = db_path.to_str().expect("db path");

    let status = stdout_json(&run_codegraph(&[
        "--repo", repo_arg, "--db", db_arg, "--json", "status", repo_arg,
    ]));

    assert_eq!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(status["db_problem_kind"].as_str(), Some("db_missing"));
    assert_eq!(status["path_access_status"].as_str(), Some("db_missing"));
    assert_eq!(
        status["db_lifecycle_read"]["claimable"].as_bool(),
        Some(false)
    );
    assert!(status["next_command"]
        .as_str()
        .expect("next command")
        .contains("index"));
    assert!(!db_path.exists(), "status must not create missing DB");
    assert!(
        !sqlite_sidecar_path(&db_path, "wal").exists()
            && !sqlite_sidecar_path(&db_path, "shm").exists(),
        "status must not create SQLite sidecars"
    );

    fs::remove_dir_all(repo).expect("cleanup repo");
    fs::remove_dir_all(workspace).expect("cleanup workspace");
}

#[test]
fn status_missing_external_db_parent_is_structured_and_read_only() {
    let repo = empty_repo();
    let workspace = empty_repo();
    let db_path = workspace
        .join("missing parent")
        .join("production-agent-use.sqlite");
    let repo_arg = repo.to_str().expect("repo path");
    let db_arg = db_path.to_str().expect("db path");

    let status = stdout_json(&run_codegraph(&[
        "--repo", repo_arg, "--db", db_arg, "--json", "status", repo_arg,
    ]));

    assert_eq!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(status["db_problem_kind"].as_str(), Some("db_missing"));
    assert_eq!(status["path_access_status"].as_str(), Some("db_missing"));
    assert!(!db_path.parent().expect("db parent").exists());
    assert!(!db_path.exists(), "status must not create missing DB");
    assert!(
        !sqlite_sidecar_path(&db_path, "wal").exists()
            && !sqlite_sidecar_path(&db_path, "shm").exists(),
        "status must not create SQLite sidecars"
    );

    fs::remove_dir_all(repo).expect("cleanup repo");
    fs::remove_dir_all(workspace).expect("cleanup workspace");
}

#[test]
fn status_valid_external_db_uses_read_only_open_without_sidecars() {
    let repo = fixture_repo();
    let workspace = empty_repo();
    let db_parent = workspace.join("external profile");
    fs::create_dir_all(&db_parent).expect("create DB parent");
    let db_path = db_parent.join("production-agent-use.sqlite");
    let repo_arg = repo.to_str().expect("repo path");
    let db_arg = db_path.to_str().expect("db path");

    stdout_json(&run_codegraph(&[
        "index", repo_arg, "--db", db_arg, "--fresh", "--json",
    ]));
    let _ = fs::remove_file(sqlite_sidecar_path(&db_path, "wal"));
    let _ = fs::remove_file(sqlite_sidecar_path(&db_path, "shm"));

    let status = stdout_json(&run_codegraph(&[
        "--repo", repo_arg, "--db", db_arg, "--json", "status", repo_arg,
    ]));

    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(status["path_access_status"].as_str(), Some("ok"));
    assert_eq!(
        status["db_lifecycle_read"]["claimable"].as_bool(),
        Some(true)
    );
    assert!(
        !sqlite_sidecar_path(&db_path, "wal").exists()
            && !sqlite_sidecar_path(&db_path, "shm").exists(),
        "read-only status must not create SQLite sidecars"
    );

    fs::remove_dir_all(repo).expect("cleanup repo");
    fs::remove_dir_all(workspace).expect("cleanup workspace");
}

#[test]
fn query_and_context_pack_missing_external_db_still_fail_closed() {
    let repo = fixture_repo();
    let workspace = empty_repo();
    let db_parent = workspace.join("external profile");
    fs::create_dir_all(&db_parent).expect("create DB parent");
    let db_path = db_parent.join("missing.sqlite");
    let repo_arg = repo.to_str().expect("repo path");
    let db_arg = db_path.to_str().expect("db path");

    let query = run_codegraph(&[
        "--repo", repo_arg, "--db", db_arg, "--json", "query", "symbols", "login",
    ]);

    assert!(!query.status.success(), "query should fail closed");
    let error: Value = serde_json::from_slice(&query.stderr).expect("query error JSON");
    assert_eq!(error["status"].as_str(), Some("error"));
    assert!(error["message"]
        .as_str()
        .expect("message")
        .contains("db_missing"));
    assert!(!db_path.exists(), "query must not create missing DB");
    assert!(
        !sqlite_sidecar_path(&db_path, "wal").exists()
            && !sqlite_sidecar_path(&db_path, "shm").exists(),
        "query must not create SQLite sidecars"
    );

    let context_pack = run_codegraph(&[
        "--repo",
        repo_arg,
        "--db",
        db_arg,
        "--json",
        "context-pack",
        "--task",
        "inspect login",
        "--seed",
        "login",
        "--agent-json",
    ]);

    assert!(
        !context_pack.status.success(),
        "context-pack should fail closed"
    );
    let error: Value =
        serde_json::from_slice(&context_pack.stderr).expect("context-pack error JSON");
    assert_eq!(error["status"].as_str(), Some("error"));
    assert!(error["message"]
        .as_str()
        .expect("message")
        .contains("db_missing"));
    assert!(!db_path.exists(), "context-pack must not create missing DB");
    assert!(
        !sqlite_sidecar_path(&db_path, "wal").exists()
            && !sqlite_sidecar_path(&db_path, "shm").exists(),
        "context-pack must not create SQLite sidecars"
    );

    fs::remove_dir_all(repo).expect("cleanup repo");
    fs::remove_dir_all(workspace).expect("cleanup workspace");
}

#[cfg(windows)]
#[test]
fn profile_wrapper_prod_agent_status_missing_db_is_actionable() {
    let repo = empty_repo();
    let workspace = empty_repo();
    let local_app_data = workspace.join("localapp");
    let db_path = local_app_data
        .join("CodeGraphMCP")
        .join("agent-indexes")
        .join(repo.file_name().expect("repo name"))
        .join("production-agent-use.sqlite");
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");

    let output = Command::new("powershell")
        .current_dir(workspace_root)
        .env("LOCALAPPDATA", &local_app_data)
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            ".\\scripts\\codegraph-profile.ps1",
            "-Profile",
            "prod-agent",
            "-Action",
            "status",
            "-Repo",
            repo.to_str().expect("repo path"),
            "-Binary",
            env!("CARGO_BIN_EXE_codegraph-mcp"),
        ])
        .output()
        .expect("run profile wrapper");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout.find('{').expect("JSON status in stdout");
    let status: Value = serde_json::from_str(&stdout[json_start..]).expect("status JSON");
    assert_eq!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(status["db_problem_kind"].as_str(), Some("db_missing"));
    assert!(!db_path.exists(), "profile status must not create DB");
    assert!(
        !sqlite_sidecar_path(&db_path, "wal").exists()
            && !sqlite_sidecar_path(&db_path, "shm").exists(),
        "profile status must not create SQLite sidecars"
    );

    fs::remove_dir_all(repo).expect("cleanup repo");
    fs::remove_dir_all(workspace).expect("cleanup workspace");
}

#[test]
fn unresolved_calls_query_is_bounded_and_instrumented() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--storage-mode", "audit"],
    ));

    let first_page = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "query",
            "unresolved-calls",
            "--limit",
            "1",
            "--json",
            "--no-snippets",
        ],
    ));
    assert_eq!(first_page["status"].as_str(), Some("ok"));
    assert_eq!(
        first_page["db_lifecycle_read"]["decision"].as_str(),
        Some("read_reuse")
    );
    assert_eq!(first_page["claimable"].as_bool(), Some(true));
    assert_eq!(first_page["diagnostic_only"].as_bool(), Some(false));
    assert!(first_page["exact_db_path_checked"].is_string());
    assert_eq!(
        first_page["pagination"]["effective_limit"].as_u64(),
        Some(1)
    );
    assert_eq!(first_page["pagination"]["offset"].as_u64(), Some(0));
    let calls = first_page["calls"].as_array().expect("calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(first_page["row_counts"]["returned"].as_u64(), Some(1));
    assert_eq!(
        first_page["row_counts"]["total_matching_counted"].as_bool(),
        Some(false)
    );
    assert!(first_page["instrumentation"]["sql"]["page_query"]
        .as_str()
        .expect("page query")
        .contains("FROM heuristic_edges e"));
    assert!(!first_page["instrumentation"]["explain_query_plan"]
        .as_array()
        .expect("query plan")
        .is_empty());
    assert_eq!(
        first_page["instrumentation"]["snippets"]["requested"].as_bool(),
        Some(false)
    );
    assert_eq!(calls[0]["source_snippet"]["loaded"].as_bool(), Some(false));

    let second_page = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "query",
            "unresolved-calls",
            "--limit",
            "1",
            "--offset",
            "1",
            "--json",
            "--no-snippets",
        ],
    ));
    assert_eq!(second_page["status"].as_str(), Some("ok"));
    assert_eq!(second_page["pagination"]["offset"].as_u64(), Some(1));

    let counted = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "query",
            "unresolved-calls",
            "--limit",
            "2",
            "--json",
            "--no-snippets",
            "--count-total",
        ],
    ));
    assert_eq!(
        counted["row_counts"]["total_matching_counted"].as_bool(),
        Some(true)
    );
    assert!(
        counted["row_counts"]["total_matching"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );

    let with_snippet = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "query",
            "unresolved-calls",
            "--limit",
            "1",
            "--json",
            "--include-snippets",
        ],
    ));
    let snippet = &with_snippet["calls"].as_array().expect("snippet calls")[0]["source_snippet"];
    assert_eq!(snippet["requested"].as_bool(), Some(true));
    assert_eq!(snippet["loaded"].as_bool(), Some(true));
    assert!(snippet["text"].as_str().expect("snippet text").trim().len() > 0);

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn unresolved_calls_custom_db_preflights_exact_path() {
    let repo = fixture_repo();
    let custom_db = repo.join("custom-unresolved.sqlite");
    let custom_db_arg = custom_db.to_string_lossy().to_string();
    let index = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--db",
            &custom_db_arg,
            "--storage-mode",
            "audit",
            "--json",
            "--workers",
            "1",
        ],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));
    assert!(
        !repo.join(".codegraph").join("codegraph.sqlite").exists(),
        "test expects no default DB so unresolved-calls must guard its inner --db path"
    );

    let query = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "query",
            "unresolved-calls",
            "--db",
            &custom_db_arg,
            "--limit",
            "1",
            "--json",
            "--no-snippets",
        ],
    ));
    assert_eq!(query["status"].as_str(), Some("ok"));
    assert_eq!(
        query["db_lifecycle_read"]["decision"].as_str(),
        Some("read_reuse")
    );
    assert_eq!(query["claimable"].as_bool(), Some(true));
    assert_eq!(query["diagnostic_only"].as_bool(), Some(false));
    assert_eq!(
        query["exact_db_path_checked"].as_str(),
        Some(custom_db_arg.as_str())
    );
    assert_eq!(
        query["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(custom_db_arg.as_str())
    );

    fs::remove_dir_all(repo).expect("cleanup custom unresolved fixture");
}

#[test]
fn unresolved_calls_custom_mismatched_db_fails_without_diagnostic_flag() {
    let repo_a = fixture_repo();
    let repo_b = fixture_repo();
    let repo_a_db = repo_a.join("repo-a-unresolved.sqlite");
    let repo_a_db_arg = repo_a_db.to_string_lossy().to_string();
    stdout_json(&run_codegraph_in(
        &repo_a,
        &[
            "index",
            ".",
            "--db",
            &repo_a_db_arg,
            "--storage-mode",
            "audit",
            "--json",
            "--workers",
            "1",
        ],
    ));

    let blocked = run_codegraph_in(
        &repo_b,
        &[
            "query",
            "unresolved-calls",
            "--db",
            &repo_a_db_arg,
            "--limit",
            "1",
            "--json",
            "--no-snippets",
        ],
    );
    assert!(
        !blocked.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&blocked.stdout)
    );
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(stderr.contains("repo root mismatch"), "stderr={stderr}");
    let blocked_error: Value =
        serde_json::from_slice(&blocked.stderr).expect("blocked stderr JSON");
    assert!(
        blocked_error["message"]
            .as_str()
            .is_some_and(|message| message.contains(&repo_a_db_arg)),
        "stderr should mention exact checked DB path: {blocked_error:?}"
    );

    let diagnostic = stdout_json(&run_codegraph_in(
        &repo_b,
        &[
            "query",
            "unresolved-calls",
            "--db",
            &repo_a_db_arg,
            "--limit",
            "1",
            "--json",
            "--no-snippets",
            "--allow-stale-read",
        ],
    ));
    assert_eq!(diagnostic["status"].as_str(), Some("ok"));
    assert_eq!(diagnostic["claimable"].as_bool(), Some(false));
    assert_eq!(diagnostic["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(
        diagnostic["db_lifecycle_read"]["decision"].as_str(),
        Some("diagnostic_stale_reuse")
    );
    assert_eq!(
        diagnostic["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(repo_a_db_arg.as_str())
    );
    assert!(diagnostic["db_lifecycle_read"]["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .any(|warning| warning
            .as_str()
            .is_some_and(|text| text.contains("foreign-repo blocker"))));

    fs::remove_dir_all(repo_a).expect("cleanup repo A");
    fs::remove_dir_all(repo_b).expect("cleanup repo B");
}

#[test]
fn audit_commands_write_storage_samples_and_relation_counts() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    let db = repo.join(".codegraph").join("codegraph.sqlite");
    let db_arg = db.to_string_lossy().to_string();
    let artifacts = repo.join("audit-artifacts");
    let storage_json = artifacts.join("storage.json");
    let storage_md = artifacts.join("storage.md");
    let counts_json = artifacts.join("relation-counts.json");
    let counts_md = artifacts.join("relation-counts.md");
    let experiments_json = artifacts.join("storage-experiments.json");
    let experiments_md = artifacts.join("storage-experiments.md");
    let experiments_workdir = artifacts.join("storage-experiment-workdir");
    let sample_a_json = artifacts.join("sample-a.json");
    let sample_a_md = artifacts.join("sample-a.md");
    let sample_b_json = artifacts.join("sample-b.json");
    let sample_paths_json = artifacts.join("sample-paths.json");
    let sample_paths_md = artifacts.join("sample-paths.md");
    let manual_edge_md = artifacts.join("manual-edge-labels.md");
    let manual_path_md = artifacts.join("manual-path-labels.md");
    let labels_json = artifacts.join("manual-labels.json");
    let labels_md = artifacts.join("manual-labels.md");
    let label_summary_json = artifacts.join("manual-label-summary.json");
    let label_summary_md = artifacts.join("manual-label-summary.md");
    let missing_json = artifacts.join("missing-source.json");

    let storage_json_arg = storage_json.to_string_lossy().to_string();
    let storage_md_arg = storage_md.to_string_lossy().to_string();
    let storage = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "storage",
            "--db",
            &db_arg,
            "--json",
            &storage_json_arg,
            "--markdown",
            &storage_md_arg,
        ],
    ));
    assert_eq!(storage["status"].as_str(), Some("ok"));
    let storage_file: Value =
        serde_json::from_str(&fs::read_to_string(&storage_json).expect("storage json"))
            .expect("storage JSON is valid");
    assert!(storage_file["objects"].as_array().expect("objects").len() > 1);
    assert!(storage_file["aggregate_metrics"]["average_database_bytes_per_edge"].is_number());
    assert!(storage_file["table_row_metrics"]
        .as_array()
        .expect("table row metrics")
        .iter()
        .any(|row| row["table"].as_str() == Some("edges")
            && row["average_total_bytes_per_row"].is_number()));
    assert!(storage_file["index_usage"]
        .as_array()
        .expect("index usage")
        .iter()
        .any(
            |index| index["name"].as_str() == Some("idx_edges_head_relation")
                && index["default_query_usage"].is_array()
        ));
    assert!(storage_file["core_query_plans"]
        .as_array()
        .expect("core query plans")
        .iter()
        .any(
            |query| query["name"].as_str() == Some("unresolved_calls_paginated")
                && query["explain_query_plan"].is_array()
        ));
    assert!(fs::read_to_string(&storage_md)
        .expect("storage markdown")
        .contains("Index Usage Report"));

    let counts_json_arg = counts_json.to_string_lossy().to_string();
    let counts_md_arg = counts_md.to_string_lossy().to_string();
    let counts = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "relation-counts",
            "--db",
            &db_arg,
            "--json",
            &counts_json_arg,
            "--markdown",
            &counts_md_arg,
        ],
    ));
    assert_eq!(counts["status"].as_str(), Some("ok"));
    let counts_file: Value =
        serde_json::from_str(&fs::read_to_string(&counts_json).expect("counts json"))
            .expect("counts JSON is valid");
    let calls = counts_file["relations"]
        .as_array()
        .expect("relations")
        .iter()
        .find(|row| row["relation"].as_str() == Some("CALLS"))
        .expect("CALLS relation row");
    assert!(calls["edge_count"].as_u64().unwrap_or_default() > 0);
    assert!(calls["duplicate_edge_count"].is_u64());
    assert!(calls["missing_source_span_rows"].is_u64());
    assert!(fs::read_to_string(&counts_md)
        .expect("counts markdown")
        .contains("Relation Counts"));

    let experiments_json_arg = experiments_json.to_string_lossy().to_string();
    let experiments_md_arg = experiments_md.to_string_lossy().to_string();
    let experiments_workdir_arg = experiments_workdir.to_string_lossy().to_string();
    let experiments = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "storage-experiments",
            "--db",
            &db_arg,
            "--workdir",
            &experiments_workdir_arg,
            "--json",
            &experiments_json_arg,
            "--markdown",
            &experiments_md_arg,
        ],
    ));
    assert_eq!(experiments["status"].as_str(), Some("ok"));
    let experiments_file: Value = serde_json::from_str(
        &fs::read_to_string(&experiments_json).expect("storage experiments json"),
    )
    .expect("storage experiments JSON is valid");
    assert!(experiments_file["experiments"]
        .as_array()
        .expect("experiments")
        .iter()
        .any(|experiment| experiment["name"].as_str() == Some("drop_recreate_edge_indexes")));
    assert!(fs::read_to_string(&experiments_md)
        .expect("storage experiments markdown")
        .contains("Storage Experiments"));

    let sample_a_json_arg = sample_a_json.to_string_lossy().to_string();
    let sample_a_md_arg = sample_a_md.to_string_lossy().to_string();
    let sample_b_json_arg = sample_b_json.to_string_lossy().to_string();
    let sample_a = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "sample-edges",
            "--db",
            &db_arg,
            "--relation",
            "CALLS",
            "--limit",
            "5",
            "--seed",
            "42",
            "--json",
            &sample_a_json_arg,
            "--markdown",
            &sample_a_md_arg,
            "--include-snippets",
        ],
    ));
    assert_eq!(sample_a["status"].as_str(), Some("ok"));
    stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "sample-edges",
            "--db",
            &db_arg,
            "--relation",
            "CALLS",
            "--limit",
            "5",
            "--seed",
            "42",
            "--json",
            &sample_b_json_arg,
            "--include-snippets",
        ],
    ));
    let sample_a_file: Value =
        serde_json::from_str(&fs::read_to_string(&sample_a_json).expect("sample a json"))
            .expect("sample JSON is valid");
    let sample_b_file: Value =
        serde_json::from_str(&fs::read_to_string(&sample_b_json).expect("sample b json"))
            .expect("sample JSON is valid");
    assert_eq!(sample_a_file["samples"], sample_b_file["samples"]);
    assert!(sample_a_file["samples"]
        .as_array()
        .expect("samples")
        .iter()
        .all(|sample| sample["relation"].as_str() == Some("CALLS")));
    assert!(sample_a_file["samples"]
        .as_array()
        .expect("samples")
        .iter()
        .all(
            |sample| sample["relation_direction"].as_str() == Some("head_to_tail")
                && sample["fact_classification"].as_str().is_some()
                && sample["missing_metadata"].is_array()
        ));
    assert!(sample_a_file["samples"]
        .as_array()
        .expect("samples")
        .iter()
        .any(|sample| sample["span_loaded"].as_bool() == Some(true)));
    assert!(fs::read_to_string(&sample_a_md)
        .expect("sample markdown")
        .contains("derived_missing_provenance:"));

    let sample_paths_json_arg = sample_paths_json.to_string_lossy().to_string();
    let sample_paths_md_arg = sample_paths_md.to_string_lossy().to_string();
    let paths = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "sample-paths",
            "--db",
            &db_arg,
            "--limit",
            "3",
            "--seed",
            "42",
            "--json",
            &sample_paths_json_arg,
            "--markdown",
            &sample_paths_md_arg,
            "--include-snippets",
        ],
    ));
    assert_eq!(paths["status"].as_str(), Some("ok"));
    let sample_paths_file: Value =
        serde_json::from_str(&fs::read_to_string(&sample_paths_json).expect("paths json"))
            .expect("path sample JSON is valid");
    assert!(sample_paths_file["samples"]
        .as_array()
        .expect("path samples")
        .iter()
        .all(|sample| sample["edge_list"].is_array()
            && sample["relation_sequence"].is_array()
            && sample["missing_metadata"].is_array()
            && sample["production_test_mock_context"].as_str().is_some()));
    assert!(fs::read_to_string(&sample_paths_md)
        .expect("path sample markdown")
        .contains("PathEvidence Sample Audit"));

    fs::write(
        &manual_edge_md,
        "## Sample 1\n\n- true_positive: yes\n\n## Sample 2\n\n- false_positive: yes\n- wrong_target: yes\n- false_positive_cause: same-name collision\n",
    )
    .expect("write manual edge labels");
    fs::write(
        &manual_path_md,
        "## Path Sample 1\n\n- unsupported: yes\n- unsupported_pattern: generated fallback path\n",
    )
    .expect("write manual path labels");
    let labels_json_arg = labels_json.to_string_lossy().to_string();
    let labels_md_arg = labels_md.to_string_lossy().to_string();
    let manual_edge_md_arg = manual_edge_md.to_string_lossy().to_string();
    let manual_path_md_arg = manual_path_md.to_string_lossy().to_string();
    let labels = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "label-samples",
            "--edges-json",
            &sample_a_json_arg,
            "--edges-md",
            &manual_edge_md_arg,
            "--paths-json",
            &sample_paths_json_arg,
            "--paths-md",
            &manual_path_md_arg,
            "--json",
            &labels_json_arg,
            "--markdown",
            &labels_md_arg,
        ],
    ));
    assert_eq!(labels["status"].as_str(), Some("ok"));
    assert_eq!(labels["labeled_samples"].as_u64(), Some(3));
    let labels_file: Value =
        serde_json::from_str(&fs::read_to_string(&labels_json).expect("labels json"))
            .expect("labels JSON is valid");
    let calls_precision = labels_file["summary"]["relation_precision"]
        .as_array()
        .expect("relation precision")
        .iter()
        .find(|row| row["relation"].as_str() == Some("CALLS"))
        .expect("CALLS precision");
    assert_eq!(calls_precision["precision"].as_f64(), Some(0.5));
    assert!(labels_file["summary"]["unsupported_pattern_taxonomy"]
        .as_array()
        .expect("unsupported taxonomy")
        .iter()
        .any(|row| row["category"].as_str() == Some("generated fallback path")));
    assert!(fs::read_to_string(&labels_md)
        .expect("labels markdown")
        .contains("Precision By Relation"));

    let label_summary_json_arg = label_summary_json.to_string_lossy().to_string();
    let label_summary_md_arg = label_summary_md.to_string_lossy().to_string();
    let summary = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "summarize-labels",
            "--labels",
            &labels_json_arg,
            "--json",
            &label_summary_json_arg,
            "--markdown",
            &label_summary_md_arg,
        ],
    ));
    assert_eq!(summary["status"].as_str(), Some("ok"));
    let summary_file: Value =
        serde_json::from_str(&fs::read_to_string(&label_summary_json).expect("summary json"))
            .expect("summary JSON is valid");
    assert_eq!(summary_file["summary"]["labeled_samples"].as_u64(), Some(3));
    assert!(fs::read_to_string(&label_summary_md)
        .expect("summary markdown")
        .contains("Source-Span Precision"));

    fs::remove_file(repo.join("src").join("auth.ts")).expect("remove source to test missing span");
    let missing_json_arg = missing_json.to_string_lossy().to_string();
    stdout_json(&run_codegraph_in(
        &repo,
        &[
            "audit",
            "sample-edges",
            "--db",
            &db_arg,
            "--relation",
            "CALLS",
            "--limit",
            "1",
            "--seed",
            "42",
            "--json",
            &missing_json_arg,
            "--include-snippets",
        ],
    ));
    let missing_file: Value =
        serde_json::from_str(&fs::read_to_string(&missing_json).expect("missing json"))
            .expect("missing-source JSON is valid");
    assert!(missing_file["samples"]
        .as_array()
        .expect("missing samples")
        .iter()
        .any(|sample| sample["span_loaded"].as_bool() == Some(false)));

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn mixed_language_repo_indexes_broader_frontends() {
    let repo = empty_repo();
    fs::create_dir_all(repo.join("src")).expect("create fixture directories");
    fs::write(
        repo.join("src").join("app.ts"),
        "export function tsEntry(input: string) { return input.trim(); }\n",
    )
    .expect("write ts fixture");
    fs::write(
        repo.join("src").join("worker.py"),
        "import os\n\ndef py_entry(value):\n    local = value\n    return local\n",
    )
    .expect("write python fixture");
    fs::write(
        repo.join("src").join("worker.go"),
        "package worker\n\nimport \"fmt\"\nfunc GoEntry(value string) string { return fmt.Sprint(value) }\n",
    )
    .expect("write go fixture");
    fs::write(
        repo.join("src").join("worker.rs"),
        "use std::fmt;\npub fn rust_entry(value: String) -> String { value }\n",
    )
    .expect("write rust fixture");

    let index = stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    assert_eq!(index["status"].as_str(), Some("indexed"));
    assert_eq!(index["files_indexed"].as_u64(), Some(4));

    let status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    let languages = status["languages"].as_array().expect("languages");
    for expected in ["typescript", "python", "go", "rust"] {
        assert!(
            languages
                .iter()
                .any(|language| language.as_str() == Some(expected)),
            "missing indexed language {expected}: {languages:?}"
        );
    }

    let py_symbol = stdout_json(&run_codegraph_in(&repo, &["query", "symbols", "py_entry"]));
    assert!(!py_symbol["hits"].as_array().expect("hits").is_empty());

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn context_pack_and_impact_return_evidence() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));

    let context = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "context-pack",
            "--task",
            "Change login email handling",
            "--seed",
            "login",
            "--budget",
            "1200",
            "--mode",
            "impact",
        ],
    ));
    assert_eq!(context["status"].as_str(), Some("ok"));
    assert!(context["packet"]["verified_paths"].is_array());

    let impact = stdout_json(&run_codegraph_in(&repo, &["impact", "login"]));
    assert_eq!(impact["status"].as_str(), Some("ok"));
    assert!(impact["blast_radius"]["callers_callees"].is_array());
    assert!(impact["blast_radius"]["mutations_dataflow"].is_array());
    assert!(impact["blast_radius"]["db_schema_tables_columns"].is_array());
    assert!(impact["blast_radius"]["apis_auth_security"].is_array());
    assert!(impact["blast_radius"]["events_messages"].is_array());
    assert!(impact["blast_radius"]["tests_assertions_mocks_stubs"].is_array());

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn watch_once_reindexes_changed_file_and_prunes_stale_facts() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    let old_symbol = stdout_json(&run_codegraph_in(&repo, &["query", "symbols", "login"]));
    assert!(!old_symbol["hits"].as_array().expect("hits").is_empty());

    fs::write(
        repo.join("src").join("auth.ts"),
        "export function register(req: any) {\n  const email = req.body.email.trim();\n  return email;\n}\n",
    )
    .expect("rewrite fixture source");

    let update = stdout_json(&run_codegraph_in(
        &repo,
        &["watch", "--once", "--changed", "src/auth.ts"],
    ));
    assert_eq!(update["status"].as_str(), Some("updated"));
    assert_eq!(update["files_seen"].as_u64(), Some(1));
    assert_eq!(update["files_indexed"].as_u64(), Some(1));
    assert!(update["binary_signatures_updated"].as_u64().unwrap_or(0) > 0);
    assert!(update["adjacency_edges"].as_u64().unwrap_or(0) > 0);

    let stale = stdout_json(&run_codegraph_in(&repo, &["query", "symbols", "login"]));
    assert!(stale["hits"].as_array().expect("hits").is_empty());

    let fresh = stdout_json(&run_codegraph_in(&repo, &["query", "symbols", "register"]));
    assert!(!fresh["hits"].as_array().expect("hits").is_empty());

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn watch_once_uses_external_db_and_does_not_create_default_db() {
    let repo = fixture_repo();
    let external_db = repo.join("watch-external.sqlite");
    let external_db_arg = external_db.to_string_lossy().to_string();
    stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--db",
            &external_db_arg,
            "--json",
            "--workers",
            "1",
        ],
    ));
    let default_db = repo.join(".codegraph").join("codegraph.sqlite");
    assert!(
        !default_db.exists(),
        "external index should not create default DB"
    );

    fs::write(
        repo.join("src").join("auth.ts"),
        "export function watchedExternalDb(req: any) {\n  return req.body.email;\n}\n",
    )
    .expect("rewrite fixture source");

    let update = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "watch",
            "--once",
            "--db",
            &external_db_arg,
            "--changed",
            "src/auth.ts",
        ],
    ));
    assert_eq!(update["status"].as_str(), Some("updated"));
    assert_eq!(update["db_path"].as_str(), Some(external_db_arg.as_str()));
    assert_eq!(
        update["watch_db"]["requested_db_path"].as_str(),
        Some(external_db_arg.as_str())
    );
    assert_eq!(
        update["watch_db"]["actual_db_path_opened"].as_str(),
        Some(external_db_arg.as_str())
    );
    assert_eq!(
        update["watch_db"]["lifecycle_status"].as_str(),
        Some("safe_to_write")
    );
    assert_eq!(
        update["watch_db"]["auto_index_enabled"].as_bool(),
        Some(false)
    );
    assert!(
        !default_db.exists(),
        "watch --once --db must not fall back to default DB"
    );

    fs::remove_dir_all(repo).expect("cleanup external watch fixture");
}

#[test]
fn context_pack_modes_filter_production_and_allow_test_impact_edges() {
    let repo = empty_repo();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::create_dir_all(repo.join("tests")).expect("create tests");
    let checkout = [
        "import { sendEmail } from './service';",
        "",
        "export function checkout() {",
        "  return sendEmail(\"receipt\");",
        "}",
    ]
    .join("\n");
    let test_source = [
        "import { checkout } from '../src/checkout';",
        "import { sendEmail } from '../src/service';",
        "",
        "vi.mock(\"../src/service\", () => ({ sendEmail: vi.fn() }));",
        "vi.stubEnv(\"EMAIL_GATE\", \"off\");",
        "test(\"checkout sends receipt\", () => {",
        "  checkout();",
        "  expect(sendEmail).toHaveBeenCalled();",
        "});",
    ]
    .join("\n");
    fs::write(repo.join("src").join("checkout.ts"), &checkout).expect("write checkout");
    fs::write(repo.join("tests").join("checkout.test.ts"), &test_source).expect("write test");

    let db_dir = repo.join(".codegraph");
    fs::create_dir_all(&db_dir).expect("create db dir");
    let store = SqliteGraphStore::open(db_dir.join("codegraph.sqlite")).expect("open store");
    store
        .upsert_file(&file_record("src/checkout.ts", checkout.len() as u64))
        .expect("upsert checkout file");
    store
        .upsert_file(&file_record(
            "tests/checkout.test.ts",
            test_source.len() as u64,
        ))
        .expect("upsert test file");
    store
        .upsert_file_text("src/checkout.ts", &checkout)
        .expect("upsert checkout text");
    store
        .upsert_file_text("tests/checkout.test.ts", &test_source)
        .expect("upsert test text");
    for edge in [
        test_edge(
            "src/checkout.checkout",
            RelationKind::Calls,
            "src/service.sendEmail",
            SourceSpan::with_columns("src/checkout.ts", 4, 10, 4, 30),
        ),
        test_edge(
            "src/checkout.checkout",
            RelationKind::Calls,
            "tests/checkout.test#mocked_sendEmail",
            SourceSpan::with_columns("src/checkout.ts", 4, 10, 4, 30),
        ),
        test_edge(
            "tests/checkout.test",
            RelationKind::Tests,
            "src/service.sendEmail",
            SourceSpan::with_columns("tests/checkout.test.ts", 6, 1, 9, 3),
        ),
        test_edge(
            "tests/checkout.test",
            RelationKind::Asserts,
            "src/service.sendEmail",
            SourceSpan::with_columns("tests/checkout.test.ts", 8, 3, 8, 39),
        ),
        test_edge(
            "tests/checkout.test",
            RelationKind::Mocks,
            "src/service.sendEmail",
            SourceSpan::with_columns("tests/checkout.test.ts", 4, 1, 4, 59),
        ),
        test_edge(
            "tests/checkout.test#factory",
            RelationKind::Stubs,
            "src/service.sendEmail",
            SourceSpan::with_columns("tests/checkout.test.ts", 4, 34, 4, 56),
        ),
    ] {
        store.upsert_edge(&edge).expect("upsert edge");
    }
    store
        .quick_integrity_gate()
        .expect("manual DB integrity gate");
    upsert_test_passport(&store, &repo, StorageMode::Proof, 2, 2);
    drop(store);

    let production = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "context-pack",
            "--task",
            "Find checkout production email call",
            "--mode",
            "impact",
            "--seed",
            "src/checkout.checkout",
            "--budget",
            "4000",
        ],
    ));
    let production_paths = production["packet"]["verified_paths"]
        .as_array()
        .expect("production paths");
    assert!(production_paths.iter().any(|path| {
        path["target"].as_str() == Some("src/service.sendEmail")
            && path["metadata"]["path_context"].as_str() == Some("production")
            && path["metadata"]["production_proof_eligible"].as_bool() == Some(true)
    }));
    assert!(production_paths
        .iter()
        .all(|path| path["target"].as_str() != Some("tests/checkout.test#mocked_sendEmail")));

    let test_impact = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "context-pack",
            "--task",
            "Find tests and mocks for sendEmail",
            "--mode",
            "test-impact",
            "--seed",
            "src/service.sendEmail",
            "--budget",
            "4000",
        ],
    ));
    let relation_names = test_impact["packet"]["verified_paths"]
        .as_array()
        .expect("test impact paths")
        .iter()
        .flat_map(|path| path["metapath"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    for relation in ["TESTS", "ASSERTS", "MOCKS", "STUBS"] {
        assert!(
            relation_names.contains(&relation),
            "missing {relation}: {relation_names:?}"
        );
    }

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn bundle_export_import_round_trip() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    let bundle = repo.join("repo.cgc-bundle");
    let bundle_arg = bundle.to_str().expect("bundle path");

    let export = stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "export", "--output", bundle_arg],
    ));
    assert_eq!(export["status"].as_str(), Some("exported"));
    assert!(bundle.exists());
    assert_eq!(export["manifest"]["schema_version"].as_u64(), Some(2));
    assert!(export["manifest"]["repo_identity"].as_str().is_some());
    assert!(export["manifest"]["canonical_repo_root"].as_str().is_some());
    assert!(export["manifest"]["scope_hash"].as_str().is_some());
    assert!(export["manifest"]["scope_policy_json"].as_str().is_some());
    assert_eq!(export["manifest"]["storage_mode"].as_str(), Some("proof"));
    assert_eq!(
        export["manifest"]["db_schema_version"].as_u64(),
        Some(SCHEMA_VERSION as u64)
    );
    assert!(export["manifest"]["graph_digest"].as_str().is_some());

    fs::remove_dir_all(repo.join(".codegraph")).expect("remove existing DB before fresh import");
    let import = stdout_json(&run_codegraph_in(&repo, &["bundle", "import", bundle_arg]));
    assert_eq!(import["status"].as_str(), Some("imported"));
    assert_eq!(import["mode"].as_str(), Some("fresh"));
    assert_eq!(
        import["target_state_before_import"].as_str(),
        Some("missing")
    );
    assert_eq!(import["claimable"].as_bool(), Some(true));
    assert_eq!(import["diagnostic_only"].as_bool(), Some(false));

    let status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(status["entities"], export["manifest"]["entity_count"]);
    assert_eq!(status["edges"], export["manifest"]["edge_count"]);
    assert_eq!(
        status["db_lifecycle_read"]["passport_status"].as_str(),
        Some("valid")
    );

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn bundle_import_refuses_non_empty_db_without_replace_or_merge() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    let bundle = repo.join("repo.cgc-bundle");
    let bundle_arg = bundle.to_str().expect("bundle path");
    stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "export", "--output", bundle_arg],
    ));

    let output = run_codegraph_in(&repo, &["bundle", "import", bundle_arg]);
    let error = stderr_json(&output);

    assert_eq!(error["status"].as_str(), Some("error"));
    assert!(error["message"]
        .as_str()
        .is_some_and(|message| message.contains("refuses to write into non_empty DB")));

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn bundle_import_replace_is_atomic_and_replaces_non_empty_db() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    let bundle = repo.join("repo.cgc-bundle");
    let bundle_arg = bundle.to_str().expect("bundle path");
    let export = stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "export", "--output", bundle_arg],
    ));
    fs::write(
        repo.join("src").join("extra.ts"),
        "export function imported_extra_symbol() { return 42; }\n",
    )
    .expect("write extra source");
    stdout_json(&run_codegraph_in(&repo, &["index", ".", "--fresh"]));
    let changed_status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert!(
        changed_status["entities"].as_u64().unwrap_or_default()
            > export["manifest"]["entity_count"]
                .as_u64()
                .unwrap_or_default()
    );

    let import = stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "import", bundle_arg, "--replace"],
    ));
    assert_eq!(import["status"].as_str(), Some("imported"));
    assert_eq!(import["mode"].as_str(), Some("replace"));
    assert_eq!(import["old_db_replaced"].as_bool(), Some(true));
    assert_eq!(import["atomic_publish"].as_bool(), Some(true));
    let status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert_eq!(status["entities"], export["manifest"]["entity_count"]);
    assert_eq!(status["edges"], export["manifest"]["edge_count"]);
    assert_no_bundle_import_backups(&repo.join(".codegraph"));

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn bundle_import_rejects_foreign_repo_by_default() {
    let repo_a = fixture_repo();
    stdout_json(&run_codegraph_in(&repo_a, &["index", "."]));
    let bundle = repo_a.join("repo.cgc-bundle");
    let bundle_arg = bundle.to_str().expect("bundle path");
    stdout_json(&run_codegraph_in(
        &repo_a,
        &["bundle", "export", "--output", bundle_arg],
    ));
    let repo_b = empty_repo();

    let output = run_codegraph_in(&repo_b, &["bundle", "import", bundle_arg]);
    let error = stderr_json(&output);

    assert!(error["message"]
        .as_str()
        .is_some_and(|message| message.contains("repo identity mismatch")));
    assert!(
        !repo_b.join(".codegraph").join("codegraph.sqlite").exists(),
        "foreign failed import must not create a DB"
    );

    fs::remove_dir_all(repo_a).expect("cleanup repo A");
    fs::remove_dir_all(repo_b).expect("cleanup repo B");
}

#[test]
fn bundle_import_failure_leaves_old_db_untouched() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    let bundle = repo.join("repo.cgc-bundle");
    let bundle_arg = bundle.to_str().expect("bundle path");
    stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "export", "--output", bundle_arg],
    ));
    let before = stdout_json(&run_codegraph_in(&repo, &["status"]));
    let mut bundle_json: Value =
        serde_json::from_str(&fs::read_to_string(&bundle).expect("bundle JSON")).expect("JSON");
    bundle_json["manifest"]["graph_digest"] = json!("tampered-graph-digest");
    fs::write(
        &bundle,
        serde_json::to_string_pretty(&bundle_json).expect("encode tampered bundle"),
    )
    .expect("write tampered bundle");

    let output = run_codegraph_in(&repo, &["bundle", "import", bundle_arg, "--replace"]);
    let error = stderr_json(&output);
    let after = stdout_json(&run_codegraph_in(&repo, &["status"]));

    assert!(error["message"]
        .as_str()
        .is_some_and(|message| message.contains("graph digest mismatch")));
    assert_eq!(after["status"].as_str(), Some("ok"));
    assert_eq!(after["entities"], before["entities"]);
    assert_eq!(after["edges"], before["edges"]);

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn bundle_import_merge_is_explicit_diagnostic_noop() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(&repo, &["index", "."]));
    let bundle = repo.join("repo.cgc-bundle");
    let bundle_arg = bundle.to_str().expect("bundle path");
    stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "export", "--output", bundle_arg],
    ));
    let before = stdout_json(&run_codegraph_in(&repo, &["status"]));

    let merge = stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "import", bundle_arg, "--merge"],
    ));
    let after = stdout_json(&run_codegraph_in(&repo, &["status"]));

    assert_eq!(merge["status"].as_str(), Some("merge_refused"));
    assert_eq!(merge["mode"].as_str(), Some("merge"));
    assert_eq!(merge["claimable"].as_bool(), Some(false));
    assert_eq!(merge["diagnostic_only"].as_bool(), Some(true));
    assert_eq!(merge["database_mutated"].as_bool(), Some(false));
    assert_eq!(after["entities"], before["entities"]);
    assert_eq!(after["edges"], before["edges"]);

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn bundle_import_rejects_schema_mismatch() {
    let repo = empty_repo();
    let bundle = repo.join("bad.cgc-bundle");
    fs::write(
        &bundle,
        serde_json::to_string(&json!({
            "manifest": {
                "schema_version": 999,
                "created_by": "test",
                "created_at_unix_ms": 1,
                "repo_root": ".",
                "file_count": 0,
                "entity_count": 0,
                "edge_count": 0
            },
            "files": [],
            "entities": [],
            "edges": []
        }))
        .expect("encode bundle"),
    )
    .expect("write bundle");

    let output = run_codegraph_in(&repo, &["bundle", "import", "bad.cgc-bundle"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("schema mismatch"), "{stderr}");

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
}

#[test]
fn serve_mcp_starts_and_exits_on_closed_stdin() {
    let output = run_codegraph(&["serve-mcp"]);

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn non_index_commands_do_not_create_default_index_db() {
    let repo = empty_repo();
    let db_dir = repo.join(".codegraph");

    let help = run_codegraph_in(&repo, &["--help"]);
    assert!(
        help.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&help.stderr)
    );
    assert!(!db_dir.exists(), "--help created default index state");

    let status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert_eq!(status["status"].as_str(), Some("not_indexed"));
    assert_eq!(
        status["next_command"].as_str(),
        Some("codegraph-mcp index .")
    );
    assert!(!db_dir.exists(), "status created default index state");

    let server = run_codegraph_in(&repo, &["serve-mcp"]);
    assert!(
        server.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&server.stderr)
    );
    assert!(
        !db_dir.exists(),
        "serve-mcp cold-indexed or created default index state"
    );

    fs::remove_dir_all(repo).expect("cleanup non-index command workspace");
}

#[test]
fn index_status_and_query_surface_db_lifecycle_evidence() {
    let repo = fixture_repo();

    let index = stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--json", "--workers", "1"],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));
    assert_eq!(
        index["db_lifecycle"]["decision"].as_str(),
        Some("fresh_rebuild")
    );
    assert_eq!(index["db_lifecycle"]["claimable"].as_bool(), Some(true));

    let status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(
        status["db_health"]["passport_status"].as_str(),
        Some("valid")
    );
    assert_eq!(status["db_health"]["valid"].as_bool(), Some(true));

    let query = stdout_json(&run_codegraph_in(&repo, &["query", "symbols", "sanitize"]));
    assert_eq!(query["status"].as_str(), Some("ok"));
    assert_eq!(
        query["db_lifecycle_read"]["decision"].as_str(),
        Some("read_reuse")
    );
    assert_eq!(
        query["db_lifecycle_read"]["claimable"].as_bool(),
        Some(true)
    );

    fs::remove_dir_all(repo).expect("cleanup lifecycle fixture");
}

#[test]
fn global_flag_placement_is_targeted_and_literal_query_flags_can_escape() {
    let repo = fixture_repo();
    let db_path = repo.join("flag-placement.sqlite");
    let repo_arg = repo.to_str().expect("repo path");
    let db_arg = db_path.to_str().expect("db path");
    let resolved_repo_arg = fs::canonicalize(&repo)
        .expect("canonical repo")
        .to_string_lossy()
        .to_string();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));

    stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--db", db_arg, "--fresh", "--json"],
    ));

    let before = stdout_json(&run_codegraph_in(
        workspace,
        &[
            "--repo",
            repo_arg,
            "--db",
            db_arg,
            "query",
            "symbols",
            "login",
            "--agent-json",
        ],
    ));
    assert_eq!(before["status"].as_str(), Some("ok"));
    assert_eq!(before["query"]["text"].as_str(), Some("login"));
    assert_eq!(
        before["resolved_repo"].as_str(),
        Some(resolved_repo_arg.as_str())
    );
    assert_eq!(before["resolved_db"].as_str(), Some(db_arg));
    assert_eq!(before["repo_source"].as_str(), Some("global --repo"));
    assert_eq!(before["db_source"].as_str(), Some("global --db"));
    assert_eq!(before["lifecycle_status"].as_str(), Some("read_reuse"));

    let status_with_global_db = stdout_json(&run_codegraph_in(
        workspace,
        &["--repo", repo_arg, "--db", db_arg, "status", "."],
    ));
    assert_eq!(status_with_global_db["status"].as_str(), Some("ok"));
    assert_eq!(status_with_global_db["db_path"].as_str(), Some(db_arg));

    let doctor_with_global_db = stdout_json(&run_codegraph_in(
        workspace,
        &["--repo", repo_arg, "--db", db_arg, "doctor", ".", "--json"],
    ));
    assert_eq!(doctor_with_global_db["status"].as_str(), Some("ok"));
    assert_eq!(doctor_with_global_db["db_path"].as_str(), Some(db_arg));

    let relative_db_name = "flag-placement-relative.sqlite";
    let relative_db_path = PathBuf::from(&resolved_repo_arg).join(relative_db_name);
    let relative_db_arg = relative_db_path.to_str().expect("relative DB path");
    stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--db", relative_db_name, "--fresh", "--json"],
    ));
    let relative_db_query = stdout_json(&run_codegraph_in(
        workspace,
        &[
            "--repo",
            repo_arg,
            "--db",
            relative_db_name,
            "query",
            "symbols",
            "login",
            "--agent-json",
        ],
    ));
    let observed_relative_db = relative_db_query["resolved_db"]
        .as_str()
        .expect("resolved relative DB");
    assert_eq!(
        observed_relative_db.trim_start_matches(r"\\?\"),
        relative_db_arg.trim_start_matches(r"\\?\"),
        "observed_relative_db={observed_relative_db}, expected={relative_db_arg}"
    );
    assert!(relative_db_path.exists());

    let misplaced_db = run_codegraph_in(&repo, &["query", "symbols", "login", "--db", db_arg]);
    let misplaced_db_json = stderr_json(&misplaced_db);
    assert_eq!(misplaced_db_json["error"].as_str(), Some("query_failed"));
    assert!(misplaced_db_json["message"]
        .as_str()
        .expect("message")
        .contains("--db is a global flag"));

    let misplaced_repo =
        run_codegraph_in(&repo, &["query", "symbols", "login", "--repo", repo_arg]);
    let misplaced_repo_json = stderr_json(&misplaced_repo);
    assert!(misplaced_repo_json["message"]
        .as_str()
        .expect("message")
        .contains("--repo is a global flag"));

    let json_after_command = stdout_json(&run_codegraph_in(
        workspace,
        &[
            "--repo", repo_arg, "--db", db_arg, "query", "symbols", "login", "--json",
        ],
    ));
    assert_eq!(json_after_command["status"].as_str(), Some("ok"));
    assert_eq!(json_after_command["query"].as_str(), Some("login"));

    let literal_flag_query = stdout_json(&run_codegraph_in(
        workspace,
        &[
            "--repo",
            repo_arg,
            "--db",
            db_arg,
            "query",
            "text",
            "--agent-json",
            "--",
            "--db",
        ],
    ));
    assert_eq!(literal_flag_query["status"].as_str(), Some("ok"));
    assert_eq!(literal_flag_query["query"]["text"].as_str(), Some("--db"));

    let doctor_db = run_codegraph_in(&repo, &["doctor", "--db", db_arg]);
    let doctor_db_json = stderr_json(&doctor_db);
    assert_eq!(doctor_db_json["error"].as_str(), Some("doctor_failed"));
    assert!(doctor_db_json["message"]
        .as_str()
        .expect("message")
        .contains("--db is a global flag"));

    let other_repo = fixture_repo();
    let other_repo_arg = other_repo.to_str().expect("other repo path");
    let mismatched_db = run_codegraph_in(
        workspace,
        &[
            "--repo",
            other_repo_arg,
            "--db",
            db_arg,
            "query",
            "symbols",
            "login",
            "--agent-json",
        ],
    );
    assert!(
        !mismatched_db.status.success(),
        "mismatched repo/db should be lifecycle-blocked"
    );
    let mismatched_stderr = String::from_utf8_lossy(&mismatched_db.stderr);
    assert!(
        mismatched_stderr.contains("repo_root_mismatch")
            || mismatched_stderr.contains("not safe to read"),
        "stderr={mismatched_stderr}"
    );
    fs::remove_dir_all(other_repo).expect("cleanup mismatched repo fixture");

    fs::remove_dir_all(repo).expect("cleanup flag placement fixture");
}

#[test]
fn index_incremental_rejects_corrupt_explicit_db() {
    let repo = fixture_repo();
    fs::write(repo.join("named.sqlite"), "not sqlite").expect("write corrupt DB");

    let output = run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--db",
            "named.sqlite",
            "--incremental",
            "--json",
        ],
    );
    assert!(
        !output.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DB lifecycle preflight failed"),
        "stderr={stderr}"
    );
    assert_eq!(
        fs::read_to_string(repo.join("named.sqlite")).expect("corrupt DB preserved"),
        "not sqlite"
    );

    fs::remove_dir_all(repo).expect("cleanup corrupt explicit fixture");
}

#[test]
fn query_refuses_db_passport_from_another_repo() {
    let repo_a = fixture_repo();
    let repo_b = fixture_repo();
    let index = stdout_json(&run_codegraph_in(
        &repo_a,
        &["index", ".", "--json", "--workers", "1"],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));

    let source_db = repo_a.join(".codegraph").join("codegraph.sqlite");
    let target_db_dir = repo_b.join(".codegraph");
    fs::create_dir_all(&target_db_dir).expect("create target DB dir");
    fs::copy(&source_db, target_db_dir.join("codegraph.sqlite")).expect("copy DB");

    let output = run_codegraph_in(&repo_b, &["query", "symbols", "sanitize"]);
    assert!(
        !output.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("repo root mismatch"), "stderr={stderr}");

    let diagnostic = stdout_json(&run_codegraph_in(
        &repo_b,
        &["query", "symbols", "sanitize", "--allow-stale-read"],
    ));
    assert_eq!(diagnostic["status"].as_str(), Some("ok"));
    assert_eq!(
        diagnostic["db_lifecycle_read"]["decision"].as_str(),
        Some("diagnostic_stale_reuse")
    );
    assert_eq!(
        diagnostic["db_lifecycle_read"]["claimable"].as_bool(),
        Some(false)
    );

    fs::remove_dir_all(repo_a).expect("cleanup repo A");
    fs::remove_dir_all(repo_b).expect("cleanup repo B");
}

#[test]
fn passport_non_default_scope_query_symbols_uses_passport_scope() {
    let repo = non_default_scope_cli_repo();
    let index = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--json",
            "--include-ignored",
            "--workers",
            "1",
        ],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));

    let db = repo.join(".codegraph").join("codegraph.sqlite");
    let store = SqliteGraphStore::open(&db).expect("open DB");
    let passport = store
        .get_db_passport()
        .expect("read passport")
        .expect("passport");
    let expected_scope = IndexScopeOptions {
        include_ignored: true,
        ..IndexScopeOptions::default()
    };
    let expected_scope_hash = scope_policy_hash(&expected_scope).expect("non-default scope hash");
    assert_eq!(passport.index_scope_policy_hash, expected_scope_hash);
    assert_ne!(
        passport.index_scope_policy_hash,
        scope_policy_hash(&IndexScopeOptions::default()).expect("default scope hash")
    );
    drop(store);

    let query = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "symbols", "ignored_scope_symbol"],
    ));
    assert_eq!(query["status"].as_str(), Some("ok"));
    assert!(!query["hits"].as_array().expect("hits").is_empty());
    assert_eq!(
        query["db_lifecycle_read"]["scope_source"].as_str(),
        Some("passport")
    );
    assert_eq!(
        query["db_lifecycle_read"]["decision"].as_str(),
        Some("read_reuse")
    );
    assert_eq!(
        query["db_lifecycle_read"]["passport_scope_hash"].as_str(),
        Some(expected_scope_hash.as_str())
    );
    assert!(query["db_lifecycle_read"]["explicit_scope_hash"].is_null());

    let explicit_query = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "query",
            "symbols",
            "ignored_scope_symbol",
            "--include-ignored",
        ],
    ));
    assert_eq!(explicit_query["status"].as_str(), Some("ok"));
    assert_eq!(
        explicit_query["db_lifecycle_read"]["scope_source"].as_str(),
        Some("explicit")
    );
    assert_eq!(
        explicit_query["db_lifecycle_read"]["passport_scope_hash"].as_str(),
        Some(expected_scope_hash.as_str())
    );
    assert_eq!(
        explicit_query["db_lifecycle_read"]["explicit_scope_hash"].as_str(),
        Some(expected_scope_hash.as_str())
    );

    fs::remove_dir_all(repo).expect("cleanup non-default scope query fixture");
}

#[test]
fn passport_non_default_scope_context_pack_uses_passport_scope() {
    let repo = non_default_scope_cli_repo();
    stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--json",
            "--include-ignored",
            "--workers",
            "1",
        ],
    ));
    let expected_scope_hash = scope_policy_hash(&IndexScopeOptions {
        include_ignored: true,
        ..IndexScopeOptions::default()
    })
    .expect("non-default scope hash");

    let context = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "context-pack",
            "--task",
            "Change ignored-scope symbol safely",
            "--seed",
            "ignored_scope_symbol",
            "--budget",
            "1200",
            "--mode",
            "impact",
        ],
    ));
    assert_eq!(context["status"].as_str(), Some("ok"));
    assert_eq!(
        context["db_lifecycle_read"]["scope_source"].as_str(),
        Some("passport")
    );
    assert_eq!(
        context["db_lifecycle_read"]["passport_scope_hash"].as_str(),
        Some(expected_scope_hash.as_str())
    );
    assert!(context["packet"]["verified_paths"].is_array());

    let explicit_context = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "context-pack",
            "--task",
            "Change ignored-scope symbol safely",
            "--seed",
            "ignored_scope_symbol",
            "--budget",
            "1200",
            "--mode",
            "impact",
            "--include-ignored",
        ],
    ));
    assert_eq!(
        explicit_context["db_lifecycle_read"]["scope_source"].as_str(),
        Some("explicit")
    );
    assert_eq!(
        explicit_context["db_lifecycle_read"]["explicit_scope_hash"].as_str(),
        Some(expected_scope_hash.as_str())
    );

    fs::remove_dir_all(repo).expect("cleanup non-default scope context fixture");
}

#[test]
fn passport_explicit_incompatible_scope_read_flags_reject_mismatch() {
    let repo = non_default_scope_cli_repo();
    stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--json",
            "--include-ignored",
            "--workers",
            "1",
        ],
    ));

    let output = run_codegraph_in(
        &repo,
        &[
            "query",
            "symbols",
            "ignored_scope_symbol",
            "--exclude",
            "ignored.ts",
        ],
    );
    assert!(
        !output.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DB passport scope does not match explicit requested scope")
            && stderr.contains("passport_scope_hash")
            && stderr.contains("explicit_scope_hash"),
        "stderr={stderr}"
    );

    fs::remove_dir_all(repo).expect("cleanup incompatible scope fixture");
}

#[test]
fn lifecycle_integration_default_cli_query_mcp_status_and_search_share_passport() {
    let repo = fixture_repo();
    let index = stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--json", "--workers", "1"],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));

    let cli_query = stdout_json(&run_codegraph_in(&repo, &["query", "symbols", "sanitize"]));
    assert_eq!(cli_query["status"].as_str(), Some("ok"));
    assert_hits_present(&cli_query, "CLI sanitize query");

    let server = mcp_server_for_repo(&repo);
    let repo_arg = repo.to_str().expect("repo path");
    let mcp_status = mcp_ok(server.call_tool("codegraph.status", &json!({"repo": repo_arg})));
    let mcp_search = mcp_ok(server.call_tool(
        "codegraph.search",
        &json!({"repo": repo_arg, "query": "sanitize"}),
    ));

    assert_eq!(mcp_status["status"].as_str(), Some("ok"));
    assert_eq!(mcp_status["safe_to_query"].as_bool(), Some(true));
    assert_eq!(mcp_search["status"].as_str(), Some("ok"));
    assert_hits_present(&mcp_search, "MCP sanitize search");

    let scope_hash = passport_scope_hash(&cli_query);
    assert_eq!(
        mcp_status["passport_summary"]["passport_scope_hash"].as_str(),
        Some(scope_hash)
    );
    assert_eq!(
        mcp_status["db_lifecycle_read"]["passport_scope_hash"].as_str(),
        Some(scope_hash)
    );
    assert_eq!(
        mcp_search["db_lifecycle_read"]["passport_scope_hash"].as_str(),
        Some(scope_hash)
    );

    fs::remove_dir_all(repo).expect("cleanup lifecycle default fixture");
}

#[test]
fn lifecycle_integration_non_default_scope_cli_and_mcp_use_passport_scope() {
    let repo = non_default_scope_cli_repo();
    let index = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--json",
            "--include-ignored",
            "--workers",
            "1",
        ],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));

    let cli_query = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "symbols", "ignored_scope_symbol"],
    ));
    assert_eq!(cli_query["status"].as_str(), Some("ok"));
    assert_hits_present(&cli_query, "CLI ignored-scope query");
    assert_eq!(
        cli_query["db_lifecycle_read"]["scope_source"].as_str(),
        Some("passport")
    );

    let server = mcp_server_for_repo(&repo);
    let repo_arg = repo.to_str().expect("repo path");
    let mcp_status = mcp_ok(server.call_tool("codegraph.status", &json!({"repo": repo_arg})));
    let mcp_search = mcp_ok(server.call_tool(
        "codegraph.search",
        &json!({"repo": repo_arg, "query": "ignored_scope_symbol"}),
    ));

    assert_eq!(mcp_status["status"].as_str(), Some("ok"));
    assert_eq!(mcp_status["safe_to_query"].as_bool(), Some(true));
    assert_eq!(mcp_status["scope_source"].as_str(), Some("passport"));
    assert_eq!(
        mcp_search["db_lifecycle_read"]["scope_source"].as_str(),
        Some("passport")
    );
    assert_hits_present(&mcp_search, "MCP ignored-scope search");
    assert_eq!(
        mcp_status["passport_summary"]["passport_scope_hash"].as_str(),
        Some(passport_scope_hash(&cli_query))
    );

    fs::remove_dir_all(repo).expect("cleanup lifecycle non-default fixture");
}

#[test]
fn lifecycle_integration_explicit_incompatible_scope_rejected_by_cli_and_mcp() {
    let repo = non_default_scope_cli_repo();
    stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--json",
            "--include-ignored",
            "--workers",
            "1",
        ],
    ));

    let cli = run_codegraph_in(
        &repo,
        &[
            "query",
            "symbols",
            "ignored_scope_symbol",
            "--exclude",
            "ignored.ts",
        ],
    );
    assert!(!cli.status.success());
    let cli_stderr = String::from_utf8_lossy(&cli.stderr);
    assert!(cli_stderr.contains("scope_mismatch"), "{cli_stderr}");
    assert!(
        cli_stderr.contains("DB passport scope does not match explicit requested scope"),
        "{cli_stderr}"
    );

    let server = mcp_server_for_repo(&repo);
    let repo_arg = repo.to_str().expect("repo path");
    let mcp_search = server
        .call_tool(
            "codegraph.search",
            &json!({
                "repo": repo_arg,
                "query": "ignored_scope_symbol",
                "exclude": ["ignored.ts"]
            }),
        )
        .expect_err("MCP explicit incompatible scope must fail");
    assert_eq!(mcp_search.code, "scope_mismatch");
    assert!(mcp_search.message.contains("passport_scope_hash"));
    assert!(mcp_search.message.contains("explicit_scope_hash"));

    let mcp_status = mcp_ok(server.call_tool(
        "codegraph.status",
        &json!({"repo": repo_arg, "exclude": ["ignored.ts"]}),
    ));
    assert_eq!(mcp_status["status"].as_str(), Some("db_problem"));
    assert_eq!(mcp_status["problem"].as_str(), Some("scope_mismatch"));
    assert_eq!(mcp_status["safe_to_query"].as_bool(), Some(false));
    assert_eq!(
        mcp_status["db_lifecycle_read"]["scope_status"].as_str(),
        Some("mismatched")
    );
    assert!(mcp_status.get("files").is_none());

    fs::remove_dir_all(repo).expect("cleanup lifecycle incompatible scope fixture");
}

#[test]
fn lifecycle_integration_newly_ignored_cleanup_removes_cli_and_mcp_hits() {
    let repo = empty_repo();
    fs::create_dir_all(repo.join("generated")).expect("create generated");
    fs::write(
        repo.join("generated").join("now_ignored.ts"),
        "export function stale_generated_symbol() { return 1; }\n",
    )
    .expect("write generated source");
    stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--json", "--workers", "1"],
    ));

    let before_cli = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "symbols", "stale_generated_symbol"],
    ));
    assert_hits_present(&before_cli, "CLI stale symbol before ignore");
    let server = mcp_server_for_repo(&repo);
    let repo_arg = repo.to_str().expect("repo path");
    let before_mcp = mcp_ok(server.call_tool(
        "codegraph.search",
        &json!({"repo": repo_arg, "query": "stale_generated_symbol"}),
    ));
    assert_hits_present(&before_mcp, "MCP stale symbol before ignore");

    fs::write(repo.join(".gitignore"), "generated/\n").expect("write ignore");
    let update = stdout_json(&run_codegraph_in(
        &repo,
        &["watch", "--once", "--changed", "generated/now_ignored.ts"],
    ));
    assert_eq!(update["status"].as_str(), Some("updated"));
    assert_eq!(update["ignored_paths_seen"].as_u64(), Some(1));
    assert_eq!(
        update["ignored_paths_with_existing_facts"].as_u64(),
        Some(1)
    );
    assert_eq!(
        update["stale_facts_deleted_for_ignored_paths"].as_u64(),
        Some(1)
    );

    let after_cli = stdout_json(&run_codegraph_in(
        &repo,
        &["query", "symbols", "stale_generated_symbol"],
    ));
    assert_hits_empty(&after_cli, "CLI stale symbol after ignore cleanup");
    let after_mcp = mcp_ok(server.call_tool(
        "codegraph.search",
        &json!({"repo": repo_arg, "query": "stale_generated_symbol"}),
    ));
    assert_hits_empty(&after_mcp, "MCP stale symbol after ignore cleanup");

    let mcp_status = mcp_ok(server.call_tool("codegraph.status", &json!({"repo": repo_arg})));
    assert_eq!(mcp_status["status"].as_str(), Some("ok"));
    assert_eq!(mcp_status["safe_to_query"].as_bool(), Some(true));

    fs::remove_dir_all(repo).expect("cleanup lifecycle ignored cleanup fixture");
}

#[test]
fn lifecycle_integration_include_aware_pruning_audit_prunes_unrelated_dirs() {
    let repo = empty_repo();
    for dir in ["src", "target/debug", "node_modules/pkg", ".git", "dist"] {
        fs::create_dir_all(repo.join(dir)).expect("create fixture dir");
    }
    fs::write(
        repo.join("src").join("keep.ts"),
        "export function keep_me() { return 1; }\n",
    )
    .expect("write keep source");
    fs::write(
        repo.join("target").join("debug").join("junk.ts"),
        "export function target_junk() { return 1; }\n",
    )
    .expect("write target junk");
    fs::write(
        repo.join("node_modules").join("pkg").join("junk.js"),
        "export function dependency_junk() { return 1; }\n",
    )
    .expect("write node_modules junk");
    fs::write(repo.join(".git").join("config"), "[core]\n").expect("write git config");
    fs::write(repo.join(".gitignore"), "dist/\n").expect("write ignore config");
    fs::write(
        repo.join("dist").join("bundle.js"),
        "export function dist_junk() { return 1; }\n",
    )
    .expect("write dist junk");

    let index = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--json",
            "--include",
            "src/keep.ts",
            "--explain-scope",
            "--workers",
            "1",
        ],
    ));
    assert_eq!(index["status"].as_str(), Some("indexed"));
    assert_eq!(index["files_indexed"].as_u64(), Some(1));
    for directory in ["target", "node_modules", ".git", "dist"] {
        let decision = scope_directory_prune_decision(&index, directory);
        assert_eq!(decision["pruned"].as_bool(), Some(true), "{decision:?}");
        assert_eq!(
            decision["could_include_descendant"].as_bool(),
            Some(false),
            "{decision:?}"
        );
        assert!(
            decision["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("excluded_directory_pruned")),
            "{decision:?}"
        );
    }

    fs::remove_dir_all(repo).expect("cleanup lifecycle pruning fixture");
}

#[test]
fn lifecycle_integration_unsafe_db_rejected_consistently_by_cli_mcp_and_status() {
    let repo_a = fixture_repo();
    let repo_b = fixture_repo();
    stdout_json(&run_codegraph_in(
        &repo_a,
        &["index", ".", "--json", "--workers", "1"],
    ));
    let source_db = repo_a.join(".codegraph").join("codegraph.sqlite");
    let target_db_dir = repo_b.join(".codegraph");
    fs::create_dir_all(&target_db_dir).expect("create target DB dir");
    fs::copy(&source_db, target_db_dir.join("codegraph.sqlite")).expect("copy DB");

    let cli = run_codegraph_in(&repo_b, &["query", "symbols", "sanitize"]);
    assert!(!cli.status.success());
    let cli_stderr = String::from_utf8_lossy(&cli.stderr);
    assert!(cli_stderr.contains("repo root mismatch"), "{cli_stderr}");

    let server = mcp_server_for_repo(&repo_b);
    let repo_arg = repo_b.to_str().expect("repo path");
    let mcp_search = server
        .call_tool(
            "codegraph.search",
            &json!({"repo": repo_arg, "query": "sanitize"}),
        )
        .expect_err("MCP search must reject unsafe DB");
    assert_eq!(mcp_search.code, "repo_root_mismatch");
    assert!(mcp_search.message.contains("repo root mismatch"));

    let mcp_status = mcp_ok(server.call_tool("codegraph.status", &json!({"repo": repo_arg})));
    assert_eq!(mcp_status["status"].as_str(), Some("db_problem"));
    assert_eq!(mcp_status["problem"].as_str(), Some("repo_root_mismatch"));
    assert_eq!(
        mcp_status["db_lifecycle_read"]["db_problem_kind"].as_str(),
        Some("repo_root_mismatch")
    );
    assert_eq!(mcp_status["safe_to_query"].as_bool(), Some(false));
    assert!(mcp_status.get("files").is_none());
    assert!(mcp_status.get("entities").is_none());
    assert!(mcp_status.get("edges").is_none());

    fs::remove_dir_all(repo_a).expect("cleanup unsafe repo A");
    fs::remove_dir_all(repo_b).expect("cleanup unsafe repo B");
}

#[test]
fn lifecycle_regression_suite_cli_query_diagnostic_and_unresolved_db_guards() {
    let repo_a = fixture_repo();
    let repo_b = fixture_repo();
    stdout_json(&run_codegraph_in(
        &repo_a,
        &["index", ".", "--json", "--workers", "1"],
    ));

    let valid_query = stdout_json(&run_codegraph_in(
        &repo_a,
        &["query", "symbols", "sanitize"],
    ));
    assert_eq!(valid_query["status"].as_str(), Some("ok"));
    assert_eq!(
        valid_query["db_lifecycle_read"]["decision"].as_str(),
        Some("read_reuse")
    );
    assert_hits_present(&valid_query, "valid normal query");

    let repo_a_db = repo_a.join(".codegraph").join("codegraph.sqlite");
    let repo_b_db = repo_b.join(".codegraph").join("codegraph.sqlite");
    fs::create_dir_all(repo_b_db.parent().expect("repo B db dir")).expect("create repo B db dir");
    fs::copy(&repo_a_db, &repo_b_db).expect("copy mismatched lifecycle DB");

    let blocked = run_codegraph_in(&repo_b, &["query", "symbols", "sanitize"]);
    assert!(!blocked.status.success());
    let blocked_error = stderr_json(&blocked);
    assert_eq!(blocked_error["error"].as_str(), Some("query_failed"));
    assert!(blocked_error["message"]
        .as_str()
        .is_some_and(|message| message.contains("repo root mismatch")));

    let diagnostic = stdout_json(&run_codegraph_in(
        &repo_b,
        &["query", "symbols", "sanitize", "--allow-stale-read"],
    ));
    assert_eq!(diagnostic["status"].as_str(), Some("ok"));
    assert_eq!(
        diagnostic["db_lifecycle_read"]["decision"].as_str(),
        Some("diagnostic_stale_reuse")
    );
    assert_eq!(
        diagnostic["db_lifecycle_read"]["claimable"].as_bool(),
        Some(false)
    );
    assert_eq!(
        diagnostic["db_lifecycle_read"]["contaminated"].as_bool(),
        Some(true)
    );
    assert!(diagnostic["db_lifecycle_read"]["blockers"]
        .as_array()
        .expect("diagnostic blockers")
        .iter()
        .any(|blocker| blocker
            .as_str()
            .is_some_and(|message| message.contains("repo root mismatch"))));
    assert_hits_present(&diagnostic, "diagnostic foreign-repo query");

    let custom_db = repo_a.join("suite-unresolved.sqlite");
    let custom_db_arg = custom_db.to_string_lossy().to_string();
    stdout_json(&run_codegraph_in(
        &repo_a,
        &[
            "index",
            ".",
            "--db",
            &custom_db_arg,
            "--storage-mode",
            "audit",
            "--json",
            "--workers",
            "1",
        ],
    ));
    let unresolved = stdout_json(&run_codegraph_in(
        &repo_a,
        &[
            "query",
            "unresolved-calls",
            "--db",
            &custom_db_arg,
            "--limit",
            "1",
            "--json",
            "--no-snippets",
        ],
    ));
    assert_eq!(unresolved["status"].as_str(), Some("ok"));
    assert_eq!(
        unresolved["exact_db_path_checked"].as_str(),
        Some(custom_db_arg.as_str())
    );
    assert_eq!(
        unresolved["db_lifecycle_read"]["exact_db_path_checked"].as_str(),
        Some(custom_db_arg.as_str())
    );
    assert_eq!(unresolved["claimable"].as_bool(), Some(true));

    let blocked_unresolved = run_codegraph_in(
        &repo_b,
        &[
            "query",
            "unresolved-calls",
            "--db",
            &custom_db_arg,
            "--limit",
            "1",
            "--json",
            "--no-snippets",
        ],
    );
    assert!(!blocked_unresolved.status.success());
    let blocked_unresolved_error = stderr_json(&blocked_unresolved);
    assert!(blocked_unresolved_error["message"]
        .as_str()
        .is_some_and(|message| message.contains(&custom_db_arg)));
    assert!(blocked_unresolved_error["message"]
        .as_str()
        .is_some_and(|message| message.contains("repo root mismatch")));

    fs::remove_dir_all(repo_a).expect("cleanup lifecycle suite repo A");
    fs::remove_dir_all(repo_b).expect("cleanup lifecycle suite repo B");
}

#[test]
fn lifecycle_regression_suite_watch_once_external_db_uses_exact_path() {
    let repo = fixture_repo();
    let external_db = repo.join("suite-watch-once.sqlite");
    let external_db_arg = external_db.to_string_lossy().to_string();
    stdout_json(&run_codegraph_in(
        &repo,
        &[
            "index",
            ".",
            "--db",
            &external_db_arg,
            "--json",
            "--workers",
            "1",
        ],
    ));
    let default_db = repo.join(".codegraph").join("codegraph.sqlite");
    assert!(
        !default_db.exists(),
        "external suite index should not create default DB"
    );

    fs::write(
        repo.join("src").join("auth.ts"),
        "export function lifecycleWatchOnceExternal(req: any) {\n  return req.body.email;\n}\n",
    )
    .expect("rewrite fixture source");

    let update = stdout_json(&run_codegraph_in(
        &repo,
        &[
            "watch",
            "--once",
            "--db",
            &external_db_arg,
            "--changed",
            "src/auth.ts",
        ],
    ));
    assert_eq!(update["status"].as_str(), Some("updated"));
    assert_eq!(update["db_path"].as_str(), Some(external_db_arg.as_str()));
    assert_eq!(
        update["watch_db"]["requested_db_path"].as_str(),
        Some(external_db_arg.as_str())
    );
    assert_eq!(
        update["watch_db"]["actual_db_path_opened"].as_str(),
        Some(external_db_arg.as_str())
    );
    assert_eq!(
        update["watch_db"]["lifecycle_status"].as_str(),
        Some("safe_to_write")
    );
    assert_eq!(
        update["watch_db"]["auto_index_enabled"].as_bool(),
        Some(false)
    );
    assert!(
        !default_db.exists(),
        "watch --once --db must not create or mutate the default DB"
    );

    fs::remove_dir_all(repo).expect("cleanup lifecycle suite watch repo");
}

#[test]
fn lifecycle_regression_suite_bundle_import_contracts_are_explicit() {
    let repo = fixture_repo();
    stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--json", "--workers", "1"],
    ));
    let bundle = repo.join("suite.cgc-bundle");
    let bundle_arg = bundle.to_str().expect("bundle path");
    let export = stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "export", "--output", bundle_arg],
    ));

    let non_empty = run_codegraph_in(&repo, &["bundle", "import", bundle_arg]);
    let non_empty_error = stderr_json(&non_empty);
    assert!(non_empty_error["message"]
        .as_str()
        .is_some_and(|message| message.contains("refuses to write into non_empty DB")));

    let foreign_repo = empty_repo();
    let foreign = run_codegraph_in(&foreign_repo, &["bundle", "import", bundle_arg]);
    let foreign_error = stderr_json(&foreign);
    assert!(foreign_error["message"]
        .as_str()
        .is_some_and(|message| message.contains("repo identity mismatch")));
    assert!(
        !foreign_repo
            .join(".codegraph")
            .join("codegraph.sqlite")
            .exists(),
        "foreign failed import must not publish a DB"
    );

    fs::write(
        repo.join("src").join("replacement_delta.ts"),
        "export function replacement_delta_symbol() { return 7; }\n",
    )
    .expect("write replacement delta source");
    stdout_json(&run_codegraph_in(
        &repo,
        &["index", ".", "--fresh", "--json", "--workers", "1"],
    ));
    let changed_status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert!(
        changed_status["entities"].as_u64().unwrap_or_default()
            > export["manifest"]["entity_count"]
                .as_u64()
                .unwrap_or_default(),
        "replacement setup should make the current DB observably different"
    );

    let replace = stdout_json(&run_codegraph_in(
        &repo,
        &["bundle", "import", bundle_arg, "--replace"],
    ));
    assert_eq!(replace["status"].as_str(), Some("imported"));
    assert_eq!(replace["mode"].as_str(), Some("replace"));
    assert_eq!(replace["atomic_publish"].as_bool(), Some(true));
    assert_eq!(replace["old_db_replaced"].as_bool(), Some(true));
    let replaced_status = stdout_json(&run_codegraph_in(&repo, &["status"]));
    assert_eq!(
        replaced_status["entities"],
        export["manifest"]["entity_count"]
    );
    assert_eq!(replaced_status["edges"], export["manifest"]["edge_count"]);
    assert_no_bundle_import_backups(&repo.join(".codegraph"));

    fs::remove_dir_all(repo).expect("cleanup lifecycle suite bundle repo");
    fs::remove_dir_all(foreign_repo).expect("cleanup lifecycle suite foreign repo");
}

#[test]
fn command_help_is_successful() {
    for command in [
        "init",
        "index",
        "status",
        "query",
        "impact",
        "context-pack",
        "context",
        "bundle",
        "watch",
        "serve-mcp",
        "mcp",
        "serve-ui",
        "ui",
        "bench",
        "audit",
        "languages",
        "doctor",
        "config",
    ] {
        let output = run_codegraph(&[command, "--help"]);

        assert!(output.status.success(), "{command} --help failed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Usage:"), "{command} --help missing usage");
    }
}

#[test]
fn bench_real_repo_corpus_and_parity_report_commands_are_structured() {
    let corpus = stdout_json(&run_codegraph(&["bench", "real-repo-corpus"]));
    assert_eq!(corpus["status"].as_str(), Some("ok"));
    assert_eq!(corpus["corpus"]["schema_version"].as_u64(), Some(1));
    assert_eq!(
        corpus["corpus"]["repos"].as_array().expect("repos").len(),
        5
    );
    assert_eq!(corpus["replay"]["status"].as_str(), Some("unavailable"));

    let output_dir = empty_repo().join("phase30-parity");
    let output_dir_arg = output_dir.to_string_lossy().to_string();
    let report = stdout_json(&run_codegraph(&[
        "bench",
        "parity-report",
        "--output-dir",
        &output_dir_arg,
    ]));
    assert_eq!(report["status"].as_str(), Some("reported"));
    assert_eq!(report["proof"].as_str(), Some("Final parity report records unknown/skipped fields explicitly and makes no SOTA claim."));
    assert!(output_dir.join("summary.json").exists());
    assert!(output_dir.join("summary.md").exists());
    assert!(output_dir.join("per_task.jsonl").exists());
    let markdown = fs::read_to_string(output_dir.join("summary.md")).expect("parity markdown");
    assert!(markdown.contains("## Evidence Sections"));
    assert!(markdown.contains("Fake-agent dry run"));

    fs::remove_dir_all(output_dir.parent().expect("temp root")).expect("cleanup parity report");
}

#[test]
fn real_repo_replay_cache_path_is_ignored() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let gitignore = fs::read_to_string(workspace_root.join(".gitignore")).expect(".gitignore");

    assert!(
        gitignore.contains(".codegraph-bench-cache"),
        "real-repo replay cache must stay ignored"
    );
}

#[test]
fn index_command_requires_repo_argument() {
    let output = run_codegraph(&["index"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"error\":\"index_failed\""), "{stderr}");
}

#[test]
fn trace_append_and_replay_cli_helpers_are_replayable() {
    let repo = fixture_repo();
    let trace_root = repo.join("trace-root");
    let trace_root_arg = trace_root.to_str().expect("trace root path");
    let repo_arg = repo.to_str().expect("repo path");

    let appended = stdout_json(&run_codegraph(&[
        "trace",
        "append",
        "--repo",
        repo_arg,
        "--trace-root",
        trace_root_arg,
        "--run-id",
        "cli-trace-run",
        "--task-id",
        "cli-trace-task",
        "--event-type",
        "file_edit",
        "--trace-id",
        "edit-1",
        "--tool",
        "apply_patch",
        "--status",
        "ok",
        "--edited-file",
        "src/auth.ts",
        "--evidence-ref",
        "codegraph://source-span/src/auth.ts:1-3",
        "--input-json",
        "{\"edited_files\":[\"src/auth.ts\"]}",
    ]));
    assert_eq!(appended["status"].as_str(), Some("traced"));

    stdout_json(&run_codegraph(&[
        "trace",
        "append",
        "--repo",
        repo_arg,
        "--trace-root",
        trace_root_arg,
        "--run-id",
        "cli-trace-run",
        "--task-id",
        "cli-trace-task",
        "--event-type",
        "test_run",
        "--trace-id",
        "test-1",
        "--tool",
        "cargo",
        "--status",
        "passed",
        "--test-command",
        "cargo test -p codegraph-trace",
        "--test-status",
        "passed",
    ]));

    let events = trace_root.join("cli-trace-run").join("events.jsonl");
    let replay = stdout_json(&run_codegraph(&[
        "trace",
        "replay",
        "--events",
        events.to_str().expect("events path"),
    ]));
    assert_eq!(replay["status"].as_str(), Some("ok"));
    assert!(replay["answers"]["files_edited"]
        .as_array()
        .expect("files")
        .contains(&json!("src/auth.ts")));
    assert!(replay["answers"]["tests_run"]
        .as_array()
        .expect("tests")
        .contains(&json!("cargo test -p codegraph-trace")));
    assert!(replay["answers"]["context_evidence_used"]
        .as_array()
        .expect("evidence")
        .contains(&json!("codegraph://source-span/src/auth.ts:1-3")));

    fs::remove_dir_all(repo).expect("cleanup trace fixture");
}

fn file_record(path: &str, size_bytes: u64) -> FileRecord {
    FileRecord {
        repo_relative_path: path.to_string(),
        file_hash: format!("hash-{path}"),
        language: Some("typescript".to_string()),
        size_bytes,
        indexed_at_unix_ms: None,
        metadata: Default::default(),
    }
}

fn test_edge(head: &str, relation: RelationKind, tail: &str, span: SourceSpan) -> Edge {
    Edge {
        id: format!(
            "edge://{}-{}-{}",
            head.replace(['/', '#', ':'], "-"),
            relation,
            tail.replace(['/', '#', ':'], "-")
        ),
        head_id: head.to_string(),
        relation,
        tail_id: tail.to_string(),
        source_span: span,
        repo_commit: None,
        file_hash: Some("hash".to_string()),
        extractor: "cli-smoke-fixture".to_string(),
        confidence: 1.0,
        exactness: Exactness::ParserVerified,
        edge_class: EdgeClass::BaseExact,
        context: EdgeContext::Production,
        derived: false,
        provenance_edges: Vec::new(),
        metadata: Default::default(),
    }
}

fn upsert_test_passport(
    store: &SqliteGraphStore,
    repo: &Path,
    storage_mode: StorageMode,
    files_seen: u64,
    files_indexed: u64,
) {
    let scope = IndexScopeOptions::default();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let canonical_repo_root = fs::canonicalize(repo).expect("canonical repo");
    let passport = DbPassport {
        passport_version: DB_PASSPORT_VERSION,
        codegraph_schema_version: SCHEMA_VERSION,
        storage_mode: storage_mode.as_str().to_string(),
        index_scope_policy_hash: scope_policy_hash(&scope).expect("scope policy hash"),
        scope_policy_json: serde_json::to_string(&scope).expect("scope policy json"),
        canonical_repo_root: canonical_repo_root.display().to_string(),
        git_remote: None,
        worktree_root: Some(canonical_repo_root.display().to_string()),
        repo_head: None,
        source_discovery_policy_version: "scope-policy-v1".to_string(),
        codegraph_build_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        last_successful_index_timestamp: Some(now),
        last_completed_run_id: Some(format!("test-{now}-{}", std::process::id())),
        last_run_status: "completed".to_string(),
        integrity_gate_result: "ok".to_string(),
        files_seen,
        files_indexed,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    };
    store
        .upsert_db_passport(&passport)
        .expect("upsert test passport");
}

fn fixture_repo() -> PathBuf {
    let repo = empty_repo();
    fs::create_dir_all(repo.join("src")).expect("create fixture directories");
    fs::write(
        repo.join("src").join("auth.ts"),
        "export function sanitize(input: string) {\n  return input.trim();\n}\n\nexport function saveUser(email: string) {\n  return email;\n}\n\nexport function login(req: any) {\n  const email = sanitize(req.body.email);\n  saveUser(email);\n  auditLogin(req.user);\n  return email;\n}\n",
    )
    .expect("write fixture source");
    repo
}

fn non_default_scope_cli_repo() -> PathBuf {
    let repo = empty_repo();
    fs::write(repo.join(".gitignore"), "ignored.ts\n").expect("write ignore config");
    fs::write(
        repo.join("ignored.ts"),
        "export function ignored_scope_symbol() {\n  return 1;\n}\n",
    )
    .expect("write ignored source");
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(
        repo.join("src").join("visible.ts"),
        "export function visible_scope_symbol() {\n  return ignored_scope_symbol();\n}\n",
    )
    .expect("write visible source");
    repo
}

fn write_minimal_comprehensive_inputs(workspace: &Path) -> (PathBuf, PathBuf) {
    let baseline_path = workspace.join("baseline.json");
    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&json!({
            "storage_summary": {
                "proof_file_family_mib": 0.0,
                "top_storage_contributors": []
            }
        }))
        .expect("baseline JSON"),
    )
    .expect("write baseline");

    let gate_path = workspace.join("gate.json");
    fs::write(
        &gate_path,
        serde_json::to_string_pretty(&json!({
            "artifacts": {},
            "gates": {
                "graph_truth": {
                    "cases_total": 11,
                    "cases_passed": 11,
                    "expected_entities": 0,
                    "matched_entities": 0,
                    "expected_edges": 0,
                    "matched_expected_edges": 0,
                    "expected_paths": 0,
                    "matched_expected_paths": 0,
                    "matched_forbidden_edges": 0,
                    "matched_forbidden_paths": 0,
                    "source_span_failures": 0,
                    "unresolved_exact_violations": 0,
                    "derived_without_provenance_violations": 0,
                    "test_mock_production_leakage": 0,
                    "stale_failures": 0
                },
                "context_packet": {
                    "cases_total": 11,
                    "cases_passed": 11,
                    "critical_symbol_recall": 1.0,
                    "proof_path_coverage": 1.0,
                    "source_span_coverage": 1.0,
                    "expected_test_recall": 1.0,
                    "distractor_ratio": 0.0
                }
            },
            "storage": {
                "proof": {
                    "file_family_bytes": 0,
                    "file_family_mib": 0.0,
                    "wal_bytes": 0,
                    "path_evidence_rows": 0,
                    "physical_edge_rows": 0,
                    "integrity_status": "ok"
                },
                "audit": {
                    "file_family_bytes": 0,
                    "audit_only_sidecar_rows": 0
                }
            },
            "autoresearch": {
                "proof_build": {
                    "wall_ms": 0,
                    "db_write_ms": 0,
                    "integrity_check_ms": 0,
                    "files_walked": 0,
                    "files_parsed": 0,
                    "duplicate_local_analyses_skipped": 0
                }
            },
            "relation_sampler": {
                "stored_path_evidence_count": 0,
                "generated_path_evidence_count": 0
            },
            "update_path": {
                "repeat_unchanged": {
                    "profile_wall_ms": 0,
                    "files_walked": 0,
                    "files_read": 0,
                    "files_hashed": 0,
                    "files_parsed": 0,
                    "entities_inserted": 0,
                    "edges_inserted": 0,
                    "integrity_status": "ok"
                },
                "single_file_update": {
                    "wall_ms": 0,
                    "entities_inserted": 0,
                    "edges_inserted": 0,
                    "dirty_path_evidence_count": 0,
                    "integrity_status": "ok"
                }
            },
            "query_latency": {
                "context_pack": {
                    "p95_shell_ms": 100,
                    "note": "fixture smoke"
                }
            }
        }))
        .expect("gate JSON"),
    )
    .expect("write gate");

    (baseline_path, gate_path)
}

fn create_empty_codegraph_db(workspace: &Path) -> (PathBuf, u32) {
    let db_path = workspace.join("artifact.sqlite");
    let store = SqliteGraphStore::open(&db_path).expect("open fixture DB");
    let schema_version = store.schema_version().expect("schema version");
    drop(store);
    (db_path, schema_version)
}

fn create_passported_empty_codegraph_db(workspace: &Path) -> (PathBuf, u32) {
    let (db_path, schema_version) = create_empty_codegraph_db(workspace);
    let store = SqliteGraphStore::open(&db_path).expect("open fixture DB for passport");
    upsert_test_passport(&store, workspace, StorageMode::Proof, 0, 0);
    drop(store);
    (db_path, schema_version)
}

fn write_artifact_metadata(
    db_path: &Path,
    metadata_path: &Path,
    schema_version: u32,
    migration_version: u32,
    storage_mode: &str,
    build_duration_ms: u64,
) {
    let size = sqlite_family_size_for_test(db_path);
    let current_exe = "C:\\fixture\\target\\release\\codegraph-mcp.exe";
    let exact_command = format!(
        "{current_exe} bench proof-build-only --repo fixture --db {}",
        db_path.to_string_lossy()
    );
    fs::write(
        metadata_path,
        serde_json::to_string_pretty(&json!({
            "artifact_path": db_path.to_string_lossy(),
            "artifact_created_at": 1u64,
            "git_commit": "unknown",
            "schema_version": schema_version,
            "migration_version": migration_version,
            "storage_mode": storage_mode,
            "build_command": "fixture build",
            "benchmark_command": "fixture benchmark",
            "build_duration_ms": build_duration_ms,
            "proof_build_only_ms": build_duration_ms,
            "db_size_bytes": size,
            "integrity_status": "ok",
            "benchmark_run_id": "fixture",
            "current_exe": current_exe,
            "debug_assertions": false,
            "binary_profile": "release",
            "exact_command": exact_command,
            "claimable_for_thresholds": true,
            "diagnostic_only": false,
            "binary_metadata": {
                "current_exe": current_exe,
                "debug_assertions": false,
                "binary_profile": "release",
                "exact_command": exact_command,
                "claimable_for_thresholds": true,
                "diagnostic_only": false
            }
        }))
        .expect("metadata JSON"),
    )
    .expect("write metadata");
}

fn sqlite_family_size_for_test(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn assert_no_bundle_import_backups(codegraph_dir: &Path) {
    let leftovers = fs::read_dir(codegraph_dir)
        .expect("read codegraph dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains("bundle-import-backup"))
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "bundle import backup files should be cleaned after atomic publish: {leftovers:?}"
    );
}

fn empty_repo() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codegraph-cli-fixture-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create fixture workspace");
    path
}

fn sqlite_sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}-{suffix}", db_path.display()))
}

fn scope_directory_prune_decision<'a>(summary: &'a Value, directory: &str) -> &'a Value {
    summary["scope"]["directory_prune_decisions"]
        .as_array()
        .expect("directory prune decisions")
        .iter()
        .find(|decision| {
            decision["directory"]
                .as_str()
                .map(|value| value.replace('\\', "/") == directory)
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("missing prune decision for {directory}: {summary:?}"))
}

const EXPECTED_SKILLS: &[&str] = &[
    "large-codebase-investigate",
    "impact-analysis",
    "trace-dataflow",
    "security-auth-review",
    "api-contract-change",
    "event-flow-debug",
    "schema-migration-impact",
    "test-impact-analysis",
    "refactor-safety-check",
];

const EXPECTED_HOOKS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
];

fn assert_template_guardrails(contents: &str) {
    let normalized = contents.to_ascii_lowercase();

    assert!(contents.contains("MVP.md"), "{contents}");
    assert!(normalized.contains("do not use subagents"), "{contents}");
    assert!(contents.contains("codegraph.context_pack"), "{contents}");
    assert!(
        normalized.contains("source spans") || normalized.contains("source-span"),
        "{contents}"
    );
    assert!(
        contents.contains("codegraph.update_changed_files"),
        "{contents}"
    );
    assert!(normalized.contains("recommended tests"), "{contents}");
    assert_no_subagent_recommendations(contents);
}

fn assert_no_subagent_recommendations(contents: &str) {
    let normalized = contents.to_ascii_lowercase();
    for forbidden in [
        "spawn_agent",
        "sub-agent",
        "parallel agents",
        "delegate to subagents",
        "use subagents to",
        "use subagents for",
        "launch subagents",
    ] {
        assert!(
            !normalized.contains(forbidden),
            "forbidden subagent instruction {forbidden:?} in {contents}"
        );
    }
}
