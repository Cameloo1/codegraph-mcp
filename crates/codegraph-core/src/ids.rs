use crate::{EntityKind, RelationKind, SourceSpan};

pub fn normalize_repo_relative_path(path: impl AsRef<str>) -> String {
    let path = path.as_ref().trim().replace('\\', "/");
    let mut normalized = path.as_str();

    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped;
    }

    while let Some(stripped) = normalized.strip_prefix('/') {
        normalized = stripped;
    }

    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        parts.push(part);
    }

    parts.join("/")
}

pub fn stable_entity_id(
    repo_relative_path: impl AsRef<str>,
    semantic_identity: impl AsRef<str>,
) -> String {
    let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
    let semantic_identity = semantic_identity.as_ref().trim();
    format!(
        "repo://e/{}",
        stable_digest_128(["entity", &repo_relative_path, semantic_identity])
    )
}

pub fn stable_entity_id_for_kind(
    repo_relative_path: impl AsRef<str>,
    kind: EntityKind,
    name: impl AsRef<str>,
    signature_hash: Option<&str>,
) -> String {
    let mut semantic_identity = format!("{}:{}", kind.id_prefix(), name.as_ref().trim());
    if let Some(signature_hash) = signature_hash {
        semantic_identity.push('(');
        semantic_identity.push_str(signature_hash.trim());
        semantic_identity.push(')');
    }

    stable_entity_id(repo_relative_path, semantic_identity)
}

pub fn stable_edge_id(
    head_id: impl AsRef<str>,
    relation: RelationKind,
    tail_id: impl AsRef<str>,
    source_span: &SourceSpan,
) -> String {
    let relation = relation.to_string();
    let source_span = source_span.to_string();
    format!(
        "edge://{}",
        stable_digest_128([
            "edge",
            head_id.as_ref().trim(),
            &relation,
            tail_id.as_ref().trim(),
            &source_span,
        ])
    )
}

fn stable_digest_128<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(part.as_bytes());
        bytes.push(0);
    }

    let high = fnv64_with_seed(&bytes, 0xcbf29ce484222325);
    let low = fnv64_with_seed(&bytes, 0x9e3779b185ebca87);
    format!("{high:016x}{low:016x}")
}

fn fnv64_with_seed(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
