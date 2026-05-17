use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

use serde_json::{json, Value};

pub(crate) const STORAGE_BUDGET_POLICY_VERSION: &str = "storage-budget-v1";
pub(crate) const FIXTURE_SMOKE_MAX_DB_MIB: f64 = 10.0;
pub(crate) const FIXTURE_SMOKE_MAX_ARTIFACTS_MIB: f64 = 10.0;
pub(crate) const NORMAL_SELF_USE_DB_TARGET_MIB: f64 = 250.0;
pub(crate) const EXTENDED_OPT_IN_THRESHOLD_MIB: f64 = 500.0;
pub(crate) const MEDIUM_CORPUS_WARNING_THRESHOLD_GIB: f64 = 1.0;
pub(crate) const BUILDROOT_STRESS_MIN_FREE_DISK_GIB: f64 = 5.0;
pub(crate) const LINUX_STRESS_MIN_FREE_DISK_GIB: f64 = 20.0;

const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone)]
pub(crate) struct StorageBudgetOptions {
    pub(crate) max_db_mib: f64,
    pub(crate) max_artifacts_mib: f64,
    pub(crate) min_free_disk_gib: f64,
    pub(crate) extended: bool,
    pub(crate) stress_corpus: Option<String>,
    pub(crate) keep_artifacts: bool,
}

impl StorageBudgetOptions {
    pub(crate) fn fixture_smoke() -> Self {
        Self {
            max_db_mib: FIXTURE_SMOKE_MAX_DB_MIB,
            max_artifacts_mib: FIXTURE_SMOKE_MAX_ARTIFACTS_MIB,
            min_free_disk_gib: 0.0,
            extended: false,
            stress_corpus: None,
            keep_artifacts: false,
        }
    }

    pub(crate) fn normal_self_use() -> Self {
        Self {
            max_db_mib: EXTENDED_OPT_IN_THRESHOLD_MIB,
            max_artifacts_mib: FIXTURE_SMOKE_MAX_ARTIFACTS_MIB,
            min_free_disk_gib: 0.0,
            extended: false,
            stress_corpus: None,
            keep_artifacts: false,
        }
    }

    pub(crate) fn stress_corpus_normalized(&self) -> Option<String> {
        self.stress_corpus
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StorageBudgetContext {
    pub(crate) command: String,
    pub(crate) repo_root: Option<PathBuf>,
    pub(crate) db_path: Option<PathBuf>,
    pub(crate) out_path: Option<PathBuf>,
    pub(crate) explicit_db: bool,
    pub(crate) explicit_out: bool,
    pub(crate) diagnostic_only: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct StorageBudgetReport {
    pub(crate) command: String,
    pub(crate) budget_status: String,
    pub(crate) max_db_mib: f64,
    pub(crate) max_artifacts_mib: f64,
    pub(crate) min_free_disk_gib: f64,
    pub(crate) free_disk_bytes: Option<u64>,
    pub(crate) db_bytes: Option<u64>,
    pub(crate) artifact_bytes: Option<u64>,
    pub(crate) extended: bool,
    pub(crate) stress_corpus: Option<String>,
    pub(crate) external_db_required: bool,
    pub(crate) external_out_required: bool,
    pub(crate) default_codegraph_refused: bool,
    pub(crate) reports_final_refused: bool,
    pub(crate) claimable: bool,
    pub(crate) diagnostic_only: bool,
    pub(crate) refusal_reason: Option<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) db_path: Option<PathBuf>,
    pub(crate) out_path: Option<PathBuf>,
    pub(crate) repo_root: Option<PathBuf>,
}

impl StorageBudgetReport {
    pub(crate) fn is_refused(&self) -> bool {
        matches!(
            self.budget_status.as_str(),
            "refused" | "over_budget" | "disk_check_failed"
        )
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "policy_version": STORAGE_BUDGET_POLICY_VERSION,
            "budget_status": self.budget_status,
            "max_db_mib": self.max_db_mib,
            "max_artifacts_mib": self.max_artifacts_mib,
            "normal_self_use_target_mib": NORMAL_SELF_USE_DB_TARGET_MIB,
            "extended_opt_in_threshold_mib": EXTENDED_OPT_IN_THRESHOLD_MIB,
            "medium_corpus_warning_threshold_gib": MEDIUM_CORPUS_WARNING_THRESHOLD_GIB,
            "buildroot_stress_min_free_disk_gib": BUILDROOT_STRESS_MIN_FREE_DISK_GIB,
            "linux_stress_min_free_disk_gib": LINUX_STRESS_MIN_FREE_DISK_GIB,
            "min_free_disk_gib": self.min_free_disk_gib,
            "free_disk_bytes": self.free_disk_bytes,
            "db_bytes": self.db_bytes,
            "artifact_bytes": self.artifact_bytes,
            "extended": self.extended,
            "stress_corpus": self.stress_corpus,
            "external_db_required": self.external_db_required,
            "external_out_required": self.external_out_required,
            "default_codegraph_refused": self.default_codegraph_refused,
            "reports_final_refused": self.reports_final_refused,
            "claimable": self.claimable,
            "diagnostic_only": self.diagnostic_only,
            "refusal_reason": self.refusal_reason,
            "warnings": self.warnings,
            "command": self.command,
            "repo_root": self.repo_root.as_ref().map(path_string),
            "db_path": self.db_path.as_ref().map(path_string),
            "out_path": self.out_path.as_ref().map(path_string),
        })
    }
}

pub(crate) fn parse_storage_budget_flag(
    args: &[String],
    index: &mut usize,
    options: &mut StorageBudgetOptions,
) -> Result<bool, String> {
    match args[*index].as_str() {
        "--max-db-mib" => {
            *index += 1;
            options.max_db_mib = parse_positive_f64(args, *index, "--max-db-mib")?;
            Ok(true)
        }
        "--max-artifacts-mib" => {
            *index += 1;
            options.max_artifacts_mib = parse_positive_f64(args, *index, "--max-artifacts-mib")?;
            Ok(true)
        }
        "--min-free-disk-gib" => {
            *index += 1;
            options.min_free_disk_gib =
                parse_non_negative_f64(args, *index, "--min-free-disk-gib")?;
            Ok(true)
        }
        "--extended" => {
            options.extended = true;
            Ok(true)
        }
        "--stress-corpus" => {
            *index += 1;
            let raw = args
                .get(*index)
                .ok_or_else(|| "--stress-corpus requires a name".to_string())?;
            options.stress_corpus = Some(raw.to_string());
            Ok(true)
        }
        "--keep-artifacts" | "--keep_artifacts" => {
            options.keep_artifacts = true;
            Ok(false)
        }
        _ => Ok(false),
    }
}

pub(crate) fn storage_budget_preflight(
    options: &StorageBudgetOptions,
    context: StorageBudgetContext,
) -> StorageBudgetReport {
    let stress_corpus = options.stress_corpus_normalized();
    let stress_requested = stress_corpus.is_some();
    let external_db_required = stress_requested && context.db_path.is_some();
    let external_out_required = stress_requested && context.out_path.is_some();
    let mut report = StorageBudgetReport {
        command: context.command,
        budget_status: "ok".to_string(),
        max_db_mib: options.max_db_mib,
        max_artifacts_mib: options.max_artifacts_mib,
        min_free_disk_gib: options.min_free_disk_gib,
        free_disk_bytes: None,
        db_bytes: None,
        artifact_bytes: None,
        extended: options.extended,
        stress_corpus,
        external_db_required,
        external_out_required,
        default_codegraph_refused: false,
        reports_final_refused: false,
        claimable: !context.diagnostic_only,
        diagnostic_only: context.diagnostic_only,
        refusal_reason: None,
        warnings: Vec::new(),
        db_path: context.db_path.as_ref().map(|path| absolutize_lossy(path)),
        out_path: context.out_path.as_ref().map(|path| absolutize_lossy(path)),
        repo_root: context
            .repo_root
            .as_ref()
            .map(|path| absolutize_lossy(path)),
    };

    if let Some(corpus) = report.stress_corpus.clone() {
        if !is_supported_stress_corpus(&corpus) {
            refuse(
                &mut report,
                format!(
                    "unsupported stress corpus `{corpus}`; supported stress corpus names: buildroot, linux"
                ),
            );
            return report;
        }
    }

    if stress_requested && !options.extended {
        refuse(
            &mut report,
            "stress corpus runs require --extended; request them as --extended --stress-corpus <buildroot|linux>".to_string(),
        );
        return report;
    }

    if (options.max_db_mib > EXTENDED_OPT_IN_THRESHOLD_MIB
        || options.max_artifacts_mib > EXTENDED_OPT_IN_THRESHOLD_MIB)
        && !options.extended
    {
        refuse(
            &mut report,
            format!("budgets above {EXTENDED_OPT_IN_THRESHOLD_MIB:.0} MiB require --extended"),
        );
        return report;
    }

    if stress_requested {
        if context.db_path.is_some() && !context.explicit_db {
            refuse(
                &mut report,
                "stress corpus runs require an explicit external --db path".to_string(),
            );
            return report;
        }
        if context.out_path.is_some() && !context.explicit_out {
            refuse(
                &mut report,
                "stress corpus runs require an explicit external --out/--output-dir path"
                    .to_string(),
            );
            return report;
        }
        match report.stress_corpus.as_deref() {
            Some("buildroot") if report.min_free_disk_gib < BUILDROOT_STRESS_MIN_FREE_DISK_GIB => {
                report.min_free_disk_gib = BUILDROOT_STRESS_MIN_FREE_DISK_GIB;
            }
            Some("linux") if report.min_free_disk_gib < LINUX_STRESS_MIN_FREE_DISK_GIB => {
                report.min_free_disk_gib = LINUX_STRESS_MIN_FREE_DISK_GIB;
            }
            _ => {}
        }
    }

    if (options.extended || stress_requested) && path_under_default_codegraph(&report) {
        report.default_codegraph_refused = true;
        refuse(
            &mut report,
            "extended/stress runs refuse default in-repo .codegraph paths; pass an explicit external --db/--out path".to_string(),
        );
        return report;
    }

    if (options.extended || stress_requested) && path_under_reports_final(&report) {
        report.reports_final_refused = true;
        refuse(
            &mut report,
            "extended/stress raw artifacts must not be written under reports/final; use reports/audit/artifacts or an external output directory".to_string(),
        );
        return report;
    }

    if report.min_free_disk_gib > 0.0 {
        let disk_path = report
            .out_path
            .as_ref()
            .or(report.db_path.as_ref())
            .or(report.repo_root.as_ref())
            .cloned()
            .unwrap_or_else(|| absolutize_lossy(Path::new(".")));
        report.free_disk_bytes = available_disk_bytes(&disk_path);
        match report.free_disk_bytes {
            Some(bytes) if (bytes as f64) < report.min_free_disk_gib * BYTES_PER_GIB => {
                let min_free_disk_gib = report.min_free_disk_gib;
                refuse(
                    &mut report,
                    format!(
                        "available disk space is below required minimum: free={} bytes, required={:.0} GiB",
                        bytes, min_free_disk_gib
                    ),
                );
                report.budget_status = "disk_check_failed".to_string();
                return report;
            }
            Some(_) => {}
            None => {
                refuse(
                    &mut report,
                    "free disk detection unavailable for this path; stress and explicit min-free-disk runs are non-claimable until disk space can be checked".to_string(),
                );
                report.budget_status = "disk_check_failed".to_string();
                return report;
            }
        }
    }

    report
}

pub(crate) fn storage_budget_postflight(
    mut report: StorageBudgetReport,
    db_paths: &[PathBuf],
    artifact_paths: &[PathBuf],
) -> StorageBudgetReport {
    let db_bytes = db_paths
        .iter()
        .map(|path| sqlite_family_size_bytes(path).unwrap_or(0))
        .sum::<u64>();
    let artifact_bytes = artifact_paths
        .iter()
        .map(|path| path_size_bytes(path).unwrap_or(0))
        .sum::<u64>();
    report.db_bytes = Some(db_bytes);
    report.artifact_bytes = Some(artifact_bytes);

    let max_db_bytes = mib_to_bytes(report.max_db_mib);
    let max_artifact_bytes = mib_to_bytes(report.max_artifacts_mib);
    if db_bytes > max_db_bytes {
        let max_db_mib = report.max_db_mib;
        refuse(
            &mut report,
            format!(
                "DB budget exceeded: db_bytes={} > max_db_mib={}",
                db_bytes, max_db_mib
            ),
        );
        report.budget_status = "over_budget".to_string();
    } else if artifact_bytes > max_artifact_bytes {
        let max_artifacts_mib = report.max_artifacts_mib;
        refuse(
            &mut report,
            format!(
                "artifact budget exceeded: artifact_bytes={} > max_artifacts_mib={}",
                artifact_bytes, max_artifacts_mib
            ),
        );
        report.budget_status = "over_budget".to_string();
    }
    report
}

pub(crate) fn storage_budget_error_value(command: &str, report: &StorageBudgetReport) -> Value {
    json!({
        "status": "error",
        "error": "storage_budget_refused",
        "message": report.refusal_reason.clone().unwrap_or_else(|| "storage budget refused".to_string()),
        "command": command,
        "storage_budget": report.to_json(),
    })
}

pub(crate) fn structured_error_string(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_else(|error| {
        format!(
            "{{\"status\":\"error\",\"error\":\"storage_budget_refused\",\"message\":\"failed to serialize storage budget error: {error}\"}}"
        )
    })
}

pub(crate) fn path_size_bytes(path: &Path) -> std::io::Result<u64> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

fn parse_positive_f64(args: &[String], index: usize, flag: &str) -> Result<f64, String> {
    let raw = args
        .get(index)
        .ok_or_else(|| format!("{flag} requires a positive number"))?;
    let value = raw
        .parse::<f64>()
        .map_err(|_| format!("{flag} requires a number, got {raw}"))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(value)
}

fn is_supported_stress_corpus(corpus: &str) -> bool {
    matches!(corpus, "buildroot" | "linux")
}

fn parse_non_negative_f64(args: &[String], index: usize, flag: &str) -> Result<f64, String> {
    let raw = args
        .get(index)
        .ok_or_else(|| format!("{flag} requires a non-negative number"))?;
    let value = raw
        .parse::<f64>()
        .map_err(|_| format!("{flag} requires a number, got {raw}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{flag} must be zero or greater"));
    }
    Ok(value)
}

fn refuse(report: &mut StorageBudgetReport, reason: String) {
    report.budget_status = "refused".to_string();
    report.claimable = false;
    report.refusal_reason = Some(reason);
}

fn mib_to_bytes(mib: f64) -> u64 {
    (mib * BYTES_PER_MIB).ceil().max(0.0) as u64
}

fn sqlite_family_size_bytes(path: &Path) -> std::io::Result<u64> {
    let mut total = metadata_len(path)?;
    total = total.saturating_add(metadata_len(&PathBuf::from(format!(
        "{}-wal",
        path.to_string_lossy()
    )))?);
    total = total.saturating_add(metadata_len(&PathBuf::from(format!(
        "{}-shm",
        path.to_string_lossy()
    )))?);
    Ok(total)
}

fn metadata_len(path: &Path) -> std::io::Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn path_under_default_codegraph(report: &StorageBudgetReport) -> bool {
    let Some(repo_root) = &report.repo_root else {
        return false;
    };
    let codegraph_root = normalize_path(&repo_root.join(".codegraph"));
    report
        .db_path
        .iter()
        .chain(report.out_path.iter())
        .any(|path| path_has_prefix(path, &codegraph_root))
}

fn path_under_reports_final(report: &StorageBudgetReport) -> bool {
    let Some(repo_root) = &report.repo_root else {
        return false;
    };
    let reports_final = normalize_path(&repo_root.join("reports").join("final"));
    report
        .db_path
        .iter()
        .chain(report.out_path.iter())
        .any(|path| path_has_prefix(path, &reports_final))
}

fn path_has_prefix(path: &Path, prefix: &Path) -> bool {
    let path = normalize_path(path);
    let prefix = normalize_path(prefix);
    #[cfg(windows)]
    {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .starts_with(&prefix.to_string_lossy().to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.starts_with(prefix)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn absolutize_lossy(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    normalize_path(&absolute)
}

fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut candidate = absolutize_lossy(path);
    loop {
        if candidate.exists() {
            return Some(candidate);
        }
        if !candidate.pop() {
            return None;
        }
    }
}

fn available_disk_bytes(path: &Path) -> Option<u64> {
    let existing = nearest_existing_ancestor(path)?;
    #[cfg(windows)]
    {
        available_disk_bytes_windows(&existing)
    }
    #[cfg(not(windows))]
    {
        available_disk_bytes_unix(&existing)
    }
}

#[cfg(windows)]
fn available_disk_bytes_windows(path: &Path) -> Option<u64> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "$resolved = Resolve-Path -LiteralPath $env:CODEGRAPH_DISK_PATH; $root = [System.IO.Path]::GetPathRoot($resolved.ProviderPath); $drive = New-Object System.IO.DriveInfo($root); [Console]::Out.Write($drive.AvailableFreeSpace)",
        ])
        .env("CODEGRAPH_DISK_PATH", path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()
}

#[cfg(not(windows))]
fn available_disk_bytes_unix(path: &Path) -> Option<u64> {
    let output = Command::new("df").args(["-Pk"]).arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().nth(1)?;
    let available_kib = line.split_whitespace().nth(3)?.parse::<u64>().ok()?;
    Some(available_kib.saturating_mul(1024))
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
