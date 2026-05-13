use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use codegraph_core::{
    Edge, EdgeClass, EdgeContext, Exactness, FileRecord, RelationKind, SourceSpan,
};
use codegraph_store::{GraphStore, SqliteGraphStore};
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
        "--no-previous",
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
    let markdown =
        fs::read_to_string(output_dir.join("comprehensive_benchmark_latest.md")).expect("markdown");
    assert!(markdown.contains("Section 1 - Executive Verdict"));
    assert!(markdown.contains("Section 4A - Proof Artifact Freshness"));
    assert!(markdown.contains("Section 6 - Storage Contributors"));
    assert!(markdown.contains("Cold Build Mode Distinction"));

    fs::remove_dir_all(workspace).expect("cleanup comprehensive workspace");
    fs::remove_dir_all(repo).expect("cleanup comprehensive repo");
}

#[test]
fn bench_comprehensive_explicit_reuse_is_marked_with_claimable_metadata() {
    let workspace = empty_repo();
    let output_dir = workspace.join("reports").join("final");
    let (baseline_path, gate_path) = write_minimal_comprehensive_inputs(&workspace);
    let (db_path, schema_version) = create_empty_codegraph_db(&workspace);
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

    let callers = stdout_json(&run_codegraph_in(&repo, &["query", "callers", "sanitize"]));
    assert_eq!(callers["status"].as_str(), Some("ok"));
    assert!(!callers["callers"].as_array().expect("callers").is_empty());

    let callees = stdout_json(&run_codegraph_in(&repo, &["query", "callees", "login"]));
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

    let import_repo = empty_repo();
    let import = stdout_json(&run_codegraph_in(
        &import_repo,
        &["bundle", "import", bundle_arg],
    ));
    assert_eq!(import["status"].as_str(), Some("imported"));

    let status = stdout_json(&run_codegraph_in(&import_repo, &["status"]));
    assert_eq!(status["status"].as_str(), Some("ok"));
    assert_eq!(status["entities"], export["manifest"]["entity_count"]);
    assert_eq!(status["edges"], export["manifest"]["edge_count"]);

    fs::remove_dir_all(repo).expect("cleanup fixture workspace");
    fs::remove_dir_all(import_repo).expect("cleanup import workspace");
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

fn write_artifact_metadata(
    db_path: &Path,
    metadata_path: &Path,
    schema_version: u32,
    migration_version: u32,
    storage_mode: &str,
    build_duration_ms: u64,
) {
    let size = sqlite_family_size_for_test(db_path);
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
            "build_duration_ms": build_duration_ms,
            "db_size_bytes": size,
            "integrity_status": "ok",
            "benchmark_run_id": "fixture"
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
