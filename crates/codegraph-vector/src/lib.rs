//! Stage 1 and Stage 2 vector funnel primitives.
//!
//! This crate implements the local 1-bit sieve from `MVP.md`: bit-packed
//! signatures, XOR + popcount Hamming distance, deterministic text-hash
//! signature generation, and top-k candidate reduction. It also defines the
//! Stage 2 compressed rerank interface with a deterministic local reranker.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryVectorError {
    ZeroDimensions,
    DimensionMismatch { expected: usize, actual: usize },
    EmptyVector,
    InvalidMatryoshkaPrefix(usize),
    BackendUnavailable(&'static str),
}

impl fmt::Display for BinaryVectorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDimensions => formatter.write_str("binary signature dimensions must be > 0"),
            Self::DimensionMismatch { expected, actual } => write!(
                formatter,
                "binary signature dimension mismatch: expected {expected}, got {actual}"
            ),
            Self::EmptyVector => formatter.write_str("compressed vector must not be empty"),
            Self::InvalidMatryoshkaPrefix(prefix) => write!(
                formatter,
                "invalid Matryoshka prefix dimension {prefix}; expected one of 32, 64, 128, 256"
            ),
            Self::BackendUnavailable(name) => {
                write!(formatter, "{name} vector backend adapter is a placeholder")
            }
        }
    }
}

impl Error for BinaryVectorError {}

pub type BinaryVectorResult<T> = Result<T, BinaryVectorError>;
pub type CompressedVectorResult<T> = Result<T, BinaryVectorError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinarySignature {
    dimensions: usize,
    words: Vec<u64>,
}

impl BinarySignature {
    pub fn from_bits(bits: &[bool]) -> BinaryVectorResult<Self> {
        if bits.is_empty() {
            return Err(BinaryVectorError::ZeroDimensions);
        }

        let mut signature = Self::zeros(bits.len())?;
        for (index, bit) in bits.iter().enumerate() {
            if *bit {
                signature.set_bit(index, true)?;
            }
        }
        Ok(signature)
    }

    pub fn from_text(text: &str, dimensions: usize) -> BinaryVectorResult<Self> {
        if dimensions == 0 {
            return Err(BinaryVectorError::ZeroDimensions);
        }

        let tokens = tokenize_for_signature(text);
        let token_refs = if tokens.is_empty() {
            vec![text.trim()]
        } else {
            tokens.iter().map(String::as_str).collect::<Vec<_>>()
        };
        let word_count = dimensions.div_ceil(64);
        let mut accumulators = vec![0i32; dimensions];

        for token in token_refs {
            if token.is_empty() {
                continue;
            }
            for word_index in 0..word_count {
                let hash = fnv1a64_with_seed(token.as_bytes(), word_index as u64);
                for bit_index in 0..64 {
                    let dimension = word_index * 64 + bit_index;
                    if dimension >= dimensions {
                        break;
                    }
                    if (hash >> bit_index) & 1 == 1 {
                        accumulators[dimension] += 1;
                    } else {
                        accumulators[dimension] -= 1;
                    }
                }
            }
        }

        let mut signature = Self::zeros(dimensions)?;
        for (index, score) in accumulators.iter().enumerate() {
            if *score >= 0 {
                signature.set_bit(index, true)?;
            }
        }
        Ok(signature)
    }

    pub fn zeros(dimensions: usize) -> BinaryVectorResult<Self> {
        if dimensions == 0 {
            return Err(BinaryVectorError::ZeroDimensions);
        }
        Ok(Self {
            dimensions,
            words: vec![0; dimensions.div_ceil(64)],
        })
    }

    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    pub fn words(&self) -> &[u64] {
        &self.words
    }

    pub fn to_bits(&self) -> Vec<bool> {
        (0..self.dimensions)
            .map(|index| self.bit(index).unwrap_or(false))
            .collect()
    }

    pub fn bit(&self, index: usize) -> BinaryVectorResult<bool> {
        if index >= self.dimensions {
            return Err(BinaryVectorError::DimensionMismatch {
                expected: self.dimensions,
                actual: index + 1,
            });
        }
        let word = self.words[index / 64];
        Ok(((word >> (index % 64)) & 1) == 1)
    }

    fn set_bit(&mut self, index: usize, value: bool) -> BinaryVectorResult<()> {
        if index >= self.dimensions {
            return Err(BinaryVectorError::DimensionMismatch {
                expected: self.dimensions,
                actual: index + 1,
            });
        }
        let mask = 1_u64 << (index % 64);
        let word = &mut self.words[index / 64];
        if value {
            *word |= mask;
        } else {
            *word &= !mask;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinarySearchHit {
    pub id: String,
    pub hamming_distance: u32,
    pub similarity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinarySieveCandidate {
    pub id: String,
    pub hamming_distance: Option<u32>,
    pub similarity: Option<i64>,
    pub exact_seed: bool,
}

impl BinarySieveCandidate {
    fn exact_seed(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            hamming_distance: None,
            similarity: None,
            exact_seed: true,
        }
    }

    fn from_hit(hit: BinarySearchHit, exact_seed: bool) -> Self {
        Self {
            id: hit.id,
            hamming_distance: Some(hit.hamming_distance),
            similarity: Some(hit.similarity),
            exact_seed,
        }
    }
}

pub trait BinaryVectorIndex {
    fn dimensions(&self) -> usize;
    fn upsert_signature(
        &mut self,
        id: impl Into<String>,
        signature: BinarySignature,
    ) -> BinaryVectorResult<()>;
    fn upsert_text(&mut self, id: impl Into<String>, text: &str) -> BinaryVectorResult<()> {
        let signature = BinarySignature::from_text(text, self.dimensions())?;
        self.upsert_signature(id, signature)
    }
    fn search_signature(
        &self,
        query: &BinarySignature,
        top_k: usize,
    ) -> BinaryVectorResult<Vec<BinarySearchHit>>;
    fn search_text(&self, query: &str, top_k: usize) -> BinaryVectorResult<Vec<BinarySearchHit>> {
        let signature = BinarySignature::from_text(query, self.dimensions())?;
        self.search_signature(&signature, top_k)
    }
    fn search_with_exact_seeds(
        &self,
        query: &BinarySignature,
        top_k: usize,
        exact_seed_ids: &[String],
    ) -> BinaryVectorResult<Vec<BinarySieveCandidate>> {
        union_exact_seeds(self.search_signature(query, top_k)?, exact_seed_ids)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatryoshkaPrefixDimension {
    D32,
    D64,
    D128,
    D256,
}

impl MatryoshkaPrefixDimension {
    pub const fn as_usize(self) -> usize {
        match self {
            Self::D32 => 32,
            Self::D64 => 64,
            Self::D128 => 128,
            Self::D256 => 256,
        }
    }
}

impl TryFrom<usize> for MatryoshkaPrefixDimension {
    type Error = BinaryVectorError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            32 => Ok(Self::D32),
            64 => Ok(Self::D64),
            128 => Ok(Self::D128),
            256 => Ok(Self::D256),
            other => Err(BinaryVectorError::InvalidMatryoshkaPrefix(other)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankConfig {
    pub matryoshka_prefix: MatryoshkaPrefixDimension,
    pub exact_seed_boost: f64,
    pub text_weight: f64,
    pub compressed_vector_weight: f64,
    pub stage1_weight: f64,
    pub metadata_weight: f64,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            matryoshka_prefix: MatryoshkaPrefixDimension::D128,
            exact_seed_boost: 10.0,
            text_weight: 0.35,
            compressed_vector_weight: 0.35,
            stage1_weight: 0.20,
            metadata_weight: 0.10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Int8Vector {
    values: Vec<i8>,
    scale_micros: i32,
}

impl Int8Vector {
    pub fn new(values: Vec<i8>, scale: f32) -> CompressedVectorResult<Self> {
        if values.is_empty() {
            return Err(BinaryVectorError::EmptyVector);
        }
        Ok(Self {
            values,
            scale_micros: (scale * 1_000_000.0).round() as i32,
        })
    }

    pub fn from_text(text: &str, dimensions: usize) -> CompressedVectorResult<Self> {
        if dimensions == 0 {
            return Err(BinaryVectorError::ZeroDimensions);
        }
        let tokens = tokenize_for_signature(text);
        let token_refs = if tokens.is_empty() {
            vec![text.trim()]
        } else {
            tokens.iter().map(String::as_str).collect::<Vec<_>>()
        };
        let mut values = vec![0i32; dimensions];
        for token in token_refs {
            if token.is_empty() {
                continue;
            }
            for (dimension, value) in values.iter_mut().enumerate() {
                let hash = fnv1a64_with_seed(token.as_bytes(), dimension as u64);
                let signed = i32::from((hash & 0xff) as u8) - 128;
                *value += signed;
            }
        }
        let quantized = values
            .into_iter()
            .map(|value| value.clamp(-127, 127) as i8)
            .collect::<Vec<_>>();
        Self::new(quantized, 1.0 / 127.0)
    }

    pub fn dimensions(&self) -> usize {
        self.values.len()
    }

    pub fn values(&self) -> &[i8] {
        &self.values
    }

    pub fn scale(&self) -> f32 {
        self.scale_micros as f32 / 1_000_000.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductQuantizedVector {
    pub codes: Vec<u8>,
    pub subvector_count: usize,
    pub codebook_id: Option<String>,
}

impl ProductQuantizedVector {
    pub fn new(
        codes: Vec<u8>,
        subvector_count: usize,
        codebook_id: Option<String>,
    ) -> CompressedVectorResult<Self> {
        if codes.is_empty() || subvector_count == 0 {
            return Err(BinaryVectorError::EmptyVector);
        }
        Ok(Self {
            codes,
            subvector_count,
            codebook_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatryoshkaVectorView {
    values: Vec<i8>,
    prefix: MatryoshkaPrefixDimension,
}

impl MatryoshkaVectorView {
    pub fn new(
        vector: &Int8Vector,
        prefix: MatryoshkaPrefixDimension,
    ) -> CompressedVectorResult<Self> {
        let prefix_dimensions = prefix.as_usize();
        ensure_dimensions_at_least(prefix_dimensions, vector.dimensions())?;
        Ok(Self {
            values: vector.values()[..prefix_dimensions].to_vec(),
            prefix,
        })
    }

    pub fn dimensions(&self) -> usize {
        self.values.len()
    }

    pub fn values(&self) -> &[i8] {
        &self.values
    }

    pub fn prefix(&self) -> MatryoshkaPrefixDimension {
        self.prefix
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankCandidate {
    pub id: String,
    pub text: String,
    pub exact_seed: bool,
    pub stage0_score: f64,
    pub stage1_similarity: Option<i64>,
    pub int8_vector: Option<Int8Vector>,
    pub product_quantized_vector: Option<ProductQuantizedVector>,
    pub metadata: BTreeMap<String, String>,
}

impl RerankCandidate {
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            exact_seed: false,
            stage0_score: 0.0,
            stage1_similarity: None,
            int8_vector: None,
            product_quantized_vector: None,
            metadata: BTreeMap::new(),
        }
    }

    pub fn exact_seed(mut self, exact_seed: bool) -> Self {
        self.exact_seed = exact_seed;
        self
    }

    pub fn stage0_score(mut self, score: f64) -> Self {
        self.stage0_score = score;
        self
    }

    pub fn stage1_similarity(mut self, similarity: i64) -> Self {
        self.stage1_similarity = Some(similarity);
        self
    }

    pub fn int8_vector(mut self, vector: Int8Vector) -> Self {
        self.int8_vector = Some(vector);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankQuery {
    pub text: String,
    pub int8_vector: Option<Int8Vector>,
}

impl RerankQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            int8_vector: None,
        }
    }

    pub fn with_int8_vector(mut self, vector: Int8Vector) -> Self {
        self.int8_vector = Some(vector);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankScore {
    pub id: String,
    pub score: f64,
    pub exact_seed: bool,
    pub components: BTreeMap<String, f64>,
}

pub trait CompressedVectorReranker {
    fn rerank(
        &self,
        query: &RerankQuery,
        candidates: &[RerankCandidate],
        top_n: usize,
    ) -> CompressedVectorResult<Vec<RerankScore>>;
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicCompressedReranker {
    config: RerankConfig,
}

impl DeterministicCompressedReranker {
    pub fn new(config: RerankConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RerankConfig {
        &self.config
    }
}

impl CompressedVectorReranker for DeterministicCompressedReranker {
    fn rerank(
        &self,
        query: &RerankQuery,
        candidates: &[RerankCandidate],
        top_n: usize,
    ) -> CompressedVectorResult<Vec<RerankScore>> {
        if top_n == 0 || candidates.is_empty() {
            return Ok(Vec::new());
        }

        let prefix = self.config.matryoshka_prefix;
        let query_vector = match &query.int8_vector {
            Some(vector) => vector.clone(),
            None => Int8Vector::from_text(&query.text, prefix.as_usize())?,
        };

        let mut scores = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let candidate_vector = match &candidate.int8_vector {
                Some(vector) => vector.clone(),
                None => Int8Vector::from_text(&candidate.text, prefix.as_usize())?,
            };
            let text_score = token_overlap_score(&query.text, &candidate.text);
            let compressed_vector_score =
                matryoshka_cosine_score(&query_vector, &candidate_vector, prefix)?;
            let stage1_score = normalize_stage1_similarity(candidate.stage1_similarity);
            let metadata_score = normalize_unit(candidate.stage0_score);
            let exact_boost = if candidate.exact_seed {
                self.config.exact_seed_boost
            } else {
                0.0
            };

            let mut components = BTreeMap::new();
            components.insert("text".to_string(), text_score);
            components.insert("compressed_vector".to_string(), compressed_vector_score);
            components.insert("stage1".to_string(), stage1_score);
            components.insert("metadata".to_string(), metadata_score);
            components.insert("exact_seed_boost".to_string(), exact_boost);

            let score = (self.config.text_weight * text_score)
                + (self.config.compressed_vector_weight * compressed_vector_score)
                + (self.config.stage1_weight * stage1_score)
                + (self.config.metadata_weight * metadata_score)
                + exact_boost;

            scores.push(RerankScore {
                id: candidate.id.clone(),
                score,
                exact_seed: candidate.exact_seed,
                components,
            });
        }

        scores.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| right.exact_seed.cmp(&left.exact_seed))
                .then_with(|| left.id.cmp(&right.id))
        });
        scores.truncate(top_n);
        Ok(scores)
    }
}

#[derive(Debug, Clone)]
pub struct InMemoryBinaryVectorIndex {
    dimensions: usize,
    entries: BTreeMap<String, BinarySignature>,
}

impl InMemoryBinaryVectorIndex {
    pub fn new(dimensions: usize) -> BinaryVectorResult<Self> {
        if dimensions == 0 {
            return Err(BinaryVectorError::ZeroDimensions);
        }
        Ok(Self {
            dimensions,
            entries: BTreeMap::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn remove_signature(&mut self, id: &str) -> bool {
        self.entries.remove(id).is_some()
    }
}

impl BinaryVectorIndex for InMemoryBinaryVectorIndex {
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn upsert_signature(
        &mut self,
        id: impl Into<String>,
        signature: BinarySignature,
    ) -> BinaryVectorResult<()> {
        ensure_dimensions(self.dimensions, signature.dimensions())?;
        self.entries.insert(id.into(), signature);
        Ok(())
    }

    fn search_signature(
        &self,
        query: &BinarySignature,
        top_k: usize,
    ) -> BinaryVectorResult<Vec<BinarySearchHit>> {
        ensure_dimensions(self.dimensions, query.dimensions())?;
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let mut hits = self
            .entries
            .iter()
            .map(|(id, signature)| {
                let hamming_distance = hamming_distance(query, signature)?;
                Ok(BinarySearchHit {
                    id: id.clone(),
                    hamming_distance,
                    similarity: similarity(query.dimensions(), hamming_distance),
                })
            })
            .collect::<BinaryVectorResult<Vec<_>>>()?;
        hits.sort_by(|left, right| {
            left.hamming_distance
                .cmp(&right.hamming_distance)
                .then_with(|| right.similarity.cmp(&left.similarity))
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(top_k);
        Ok(hits)
    }
}

pub fn hamming_distance(
    left: &BinarySignature,
    right: &BinarySignature,
) -> BinaryVectorResult<u32> {
    ensure_dimensions(left.dimensions(), right.dimensions())?;
    Ok(left
        .words()
        .iter()
        .zip(right.words())
        .map(|(left, right)| (left ^ right).count_ones())
        .sum())
}

pub fn similarity(dimensions: usize, hamming_distance: u32) -> i64 {
    dimensions as i64 - (2 * i64::from(hamming_distance))
}

pub fn union_exact_seeds(
    hits: Vec<BinarySearchHit>,
    exact_seed_ids: &[String],
) -> BinaryVectorResult<Vec<BinarySieveCandidate>> {
    let mut candidates = BTreeMap::<String, BinarySieveCandidate>::new();
    for seed in exact_seed_ids {
        if !seed.trim().is_empty() {
            candidates.insert(seed.clone(), BinarySieveCandidate::exact_seed(seed));
        }
    }

    for hit in hits {
        let id = hit.id.clone();
        let exact_seed = candidates
            .get(&id)
            .map(|candidate| candidate.exact_seed)
            .unwrap_or(false);
        candidates.insert(id, BinarySieveCandidate::from_hit(hit, exact_seed));
    }

    let mut values = candidates.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| match (left.exact_seed, right.exact_seed) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left
            .hamming_distance
            .unwrap_or(u32::MAX)
            .cmp(&right.hamming_distance.unwrap_or(u32::MAX))
            .then_with(|| {
                right
                    .similarity
                    .unwrap_or(i64::MIN)
                    .cmp(&left.similarity.unwrap_or(i64::MIN))
            })
            .then_with(|| left.id.cmp(&right.id)),
    });
    Ok(values)
}

fn ensure_dimensions(expected: usize, actual: usize) -> BinaryVectorResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(BinaryVectorError::DimensionMismatch { expected, actual })
    }
}

fn ensure_dimensions_at_least(expected: usize, actual: usize) -> BinaryVectorResult<()> {
    if actual >= expected {
        Ok(())
    } else {
        Err(BinaryVectorError::DimensionMismatch { expected, actual })
    }
}

fn matryoshka_cosine_score(
    query: &Int8Vector,
    candidate: &Int8Vector,
    prefix: MatryoshkaPrefixDimension,
) -> CompressedVectorResult<f64> {
    let query_view = MatryoshkaVectorView::new(query, prefix)?;
    let candidate_view = MatryoshkaVectorView::new(candidate, prefix)?;
    let mut dot = 0i64;
    let mut query_norm = 0i64;
    let mut candidate_norm = 0i64;

    for (left, right) in query_view.values().iter().zip(candidate_view.values()) {
        let left = i64::from(*left);
        let right = i64::from(*right);
        dot += left * right;
        query_norm += left * left;
        candidate_norm += right * right;
    }

    if query_norm == 0 || candidate_norm == 0 {
        return Ok(0.0);
    }

    let denominator = (query_norm as f64).sqrt() * (candidate_norm as f64).sqrt();
    let cosine = (dot as f64 / denominator).clamp(-1.0, 1.0);
    Ok((cosine + 1.0) / 2.0)
}

fn token_overlap_score(query: &str, candidate: &str) -> f64 {
    let query_tokens = tokenize_for_signature(query)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if query_tokens.is_empty() {
        return 0.0;
    }
    let candidate_tokens = tokenize_for_signature(candidate)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let overlap = query_tokens.intersection(&candidate_tokens).count();
    overlap as f64 / query_tokens.len() as f64
}

fn normalize_stage1_similarity(similarity: Option<i64>) -> f64 {
    similarity
        .map(|value| ((value as f64 / 512.0) + 0.5).clamp(0.0, 1.0))
        .unwrap_or(0.0)
}

fn normalize_unit(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn tokenize_for_signature(text: &str) -> Vec<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn fnv1a64_with_seed(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ (hash >> 32)
}

#[cfg(feature = "faiss")]
pub mod faiss {
    use super::{
        BinarySearchHit, BinarySignature, BinaryVectorError, BinaryVectorIndex, BinaryVectorResult,
        CompressedVectorReranker, CompressedVectorResult, RerankCandidate, RerankQuery,
        RerankScore,
    };

    #[derive(Debug, Clone, Default)]
    pub struct FaissBinaryVectorIndex;

    impl BinaryVectorIndex for FaissBinaryVectorIndex {
        fn dimensions(&self) -> usize {
            1
        }

        fn upsert_signature(
            &mut self,
            _id: impl Into<String>,
            _signature: BinarySignature,
        ) -> BinaryVectorResult<()> {
            Err(BinaryVectorError::BackendUnavailable("FAISS"))
        }

        fn search_signature(
            &self,
            _query: &BinarySignature,
            _top_k: usize,
        ) -> BinaryVectorResult<Vec<BinarySearchHit>> {
            Err(BinaryVectorError::BackendUnavailable("FAISS"))
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct FaissCompressedVectorReranker;

    impl CompressedVectorReranker for FaissCompressedVectorReranker {
        fn rerank(
            &self,
            _query: &RerankQuery,
            _candidates: &[RerankCandidate],
            _top_n: usize,
        ) -> CompressedVectorResult<Vec<RerankScore>> {
            Err(BinaryVectorError::BackendUnavailable("FAISS"))
        }
    }
}

#[cfg(feature = "qdrant")]
pub mod qdrant {
    use super::{
        BinarySearchHit, BinarySignature, BinaryVectorError, BinaryVectorIndex, BinaryVectorResult,
        CompressedVectorReranker, CompressedVectorResult, RerankCandidate, RerankQuery,
        RerankScore,
    };

    #[derive(Debug, Clone, Default)]
    pub struct QdrantBinaryVectorIndex;

    impl BinaryVectorIndex for QdrantBinaryVectorIndex {
        fn dimensions(&self) -> usize {
            1
        }

        fn upsert_signature(
            &mut self,
            _id: impl Into<String>,
            _signature: BinarySignature,
        ) -> BinaryVectorResult<()> {
            Err(BinaryVectorError::BackendUnavailable("Qdrant"))
        }

        fn search_signature(
            &self,
            _query: &BinarySignature,
            _top_k: usize,
        ) -> BinaryVectorResult<Vec<BinarySearchHit>> {
            Err(BinaryVectorError::BackendUnavailable("Qdrant"))
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct QdrantCompressedVectorReranker;

    impl CompressedVectorReranker for QdrantCompressedVectorReranker {
        fn rerank(
            &self,
            _query: &RerankQuery,
            _candidates: &[RerankCandidate],
            _top_n: usize,
        ) -> CompressedVectorResult<Vec<RerankScore>> {
            Err(BinaryVectorError::BackendUnavailable("Qdrant"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok(..), got Err({error:?})"),
        }
    }

    #[test]
    fn bit_packing_and_unpacking_round_trip() {
        let bits = (0..130)
            .map(|index| index % 3 == 0 || index == 129)
            .collect::<Vec<_>>();
        let signature = ok(BinarySignature::from_bits(&bits));

        assert_eq!(signature.dimensions(), 130);
        assert_eq!(signature.words().len(), 3);
        assert_eq!(signature.to_bits(), bits);
        assert!(ok(signature.bit(129)));
    }

    #[test]
    fn hamming_distance_uses_xor_popcount_correctly() {
        let left = ok(BinarySignature::from_bits(&[
            true, false, true, false, true, false, true, false,
        ]));
        let right = ok(BinarySignature::from_bits(&[
            true, true, false, false, true, false, false, true,
        ]));

        assert_eq!(ok(hamming_distance(&left, &right)), 4);
        assert_eq!(similarity(8, 4), 0);
    }

    #[test]
    fn deterministic_text_signatures_are_stable() {
        let left = ok(BinarySignature::from_text("AuthService login token", 128));
        let right = ok(BinarySignature::from_text("AuthService login token", 128));
        let different = ok(BinarySignature::from_text(
            "migration alters users table",
            128,
        ));

        assert_eq!(left, right);
        assert!(ok(hamming_distance(&left, &different)) > 0);
    }

    #[test]
    fn popcount_ranking_prefers_closer_signatures() {
        let query = ok(BinarySignature::from_bits(&[true, true, false, false]));
        let same = ok(BinarySignature::from_bits(&[true, true, false, false]));
        let near = ok(BinarySignature::from_bits(&[true, false, false, false]));
        let far = ok(BinarySignature::from_bits(&[false, false, true, true]));
        let mut index = ok(InMemoryBinaryVectorIndex::new(4));

        ok(index.upsert_signature("same", same));
        ok(index.upsert_signature("near", near));
        ok(index.upsert_signature("far", far));

        let hits = ok(index.search_signature(&query, 3));

        assert_eq!(
            hits.iter().map(|hit| hit.id.as_str()).collect::<Vec<_>>(),
            vec!["same", "near", "far"]
        );
        assert_eq!(hits[0].hamming_distance, 0);
        assert!(hits[0].similarity > hits[1].similarity);
    }

    #[test]
    fn top_k_retrieval_reduces_candidates() {
        let mut index = ok(InMemoryBinaryVectorIndex::new(96));
        ok(index.upsert_text("auth", "login token auth service"));
        ok(index.upsert_text("billing", "invoice payment ledger"));
        ok(index.upsert_text("profile", "user profile avatar"));

        let hits = ok(index.search_text("login auth token", 2));

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "auth");
    }

    #[test]
    fn exact_seeds_survive_stage_one_filtering() {
        let mut index = ok(InMemoryBinaryVectorIndex::new(128));
        ok(index.upsert_text("candidate-auth", "auth login token"));
        ok(index.upsert_text("candidate-billing", "billing invoice"));
        let query = ok(BinarySignature::from_text("auth login", 128));
        let exact_seeds = vec![
            "src/auth.ts".to_string(),
            "candidate-billing".to_string(),
            "missing-but-exact".to_string(),
        ];

        let candidates = ok(index.search_with_exact_seeds(&query, 1, &exact_seeds));

        for seed in &exact_seeds {
            assert!(
                candidates
                    .iter()
                    .any(|candidate| candidate.id == *seed && candidate.exact_seed),
                "missing exact seed {seed}"
            );
        }
        assert!(candidates
            .iter()
            .any(|candidate| candidate.id == "candidate-auth"));
    }

    #[test]
    fn large_synthetic_index_smoke_test_has_no_panics() {
        let mut index = ok(InMemoryBinaryVectorIndex::new(256));
        for item in 0..5_000 {
            ok(index.upsert_text(
                format!("item-{item:04}"),
                &format!("module {item} AuthService user token {}", item % 37),
            ));
        }

        let hits = ok(index.search_text("AuthService token user", 32));

        assert_eq!(index.len(), 5_000);
        assert_eq!(hits.len(), 32);
        assert!(hits
            .windows(2)
            .all(|window| { window[0].hamming_distance <= window[1].hamming_distance }));
    }

    #[test]
    fn reranker_orders_candidates_deterministically() {
        let reranker = DeterministicCompressedReranker::default();
        let query = RerankQuery::new("auth login token");
        let candidates = vec![
            RerankCandidate::new("billing", "invoice payment ledger")
                .stage0_score(0.8)
                .stage1_similarity(-80),
            RerankCandidate::new("auth", "auth login token service")
                .stage0_score(0.6)
                .stage1_similarity(120),
            RerankCandidate::new("profile", "user profile avatar")
                .stage0_score(0.4)
                .stage1_similarity(12),
        ];

        let first = ok(reranker.rerank(&query, &candidates, 3));
        let second = ok(reranker.rerank(&query, &candidates, 3));

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|score| score.id.as_str())
                .collect::<Vec<_>>(),
            vec!["auth", "billing", "profile"]
        );
        assert!(first[0].score > first[1].score);
    }

    #[test]
    fn matryoshka_prefix_dimension_validation_works() {
        assert_eq!(
            ok(MatryoshkaPrefixDimension::try_from(32)),
            MatryoshkaPrefixDimension::D32
        );
        assert!(matches!(
            MatryoshkaPrefixDimension::try_from(48),
            Err(BinaryVectorError::InvalidMatryoshkaPrefix(48))
        ));

        let short = ok(Int8Vector::new(vec![1; 16], 1.0 / 127.0));
        assert!(matches!(
            MatryoshkaVectorView::new(&short, MatryoshkaPrefixDimension::D32),
            Err(BinaryVectorError::DimensionMismatch {
                expected: 32,
                actual: 16
            })
        ));
    }

    #[test]
    fn rerank_output_size_limits_work() {
        let reranker = DeterministicCompressedReranker::default();
        let query = RerankQuery::new("auth token");
        let candidates = (0..8)
            .map(|index| {
                RerankCandidate::new(
                    format!("candidate-{index}"),
                    format!("auth token module {index}"),
                )
            })
            .collect::<Vec<_>>();

        let scores = ok(reranker.rerank(&query, &candidates, 3));

        assert_eq!(scores.len(), 3);
    }

    #[test]
    fn exact_seed_candidates_are_boosted_and_preserved() {
        let reranker = DeterministicCompressedReranker::default();
        let query = RerankQuery::new("auth login token");
        let candidates = vec![
            RerankCandidate::new("semantic-best", "auth login token service")
                .stage1_similarity(128),
            RerankCandidate::new("exact-seed", "unrelated migration text").exact_seed(true),
        ];

        let scores = ok(reranker.rerank(&query, &candidates, 1));

        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].id, "exact-seed");
        assert!(scores[0].exact_seed);
        assert!(scores[0]
            .components
            .get("exact_seed_boost")
            .is_some_and(|boost| *boost > 0.0));
    }

    #[test]
    fn reranker_uses_optional_int8_vectors() {
        let config = RerankConfig {
            text_weight: 0.0,
            compressed_vector_weight: 1.0,
            stage1_weight: 0.0,
            metadata_weight: 0.0,
            ..RerankConfig::default()
        };
        let reranker = DeterministicCompressedReranker::new(config);
        let auth_vector = ok(Int8Vector::from_text("auth login token", 128));
        let billing_vector = ok(Int8Vector::from_text("billing invoice ledger", 128));
        let query = RerankQuery::new("ignored").with_int8_vector(auth_vector.clone());
        let candidates = vec![
            RerankCandidate::new("billing", "ignored").int8_vector(billing_vector),
            RerankCandidate::new("auth", "ignored").int8_vector(auth_vector),
        ];

        let scores = ok(reranker.rerank(&query, &candidates, 2));

        assert_eq!(scores[0].id, "auth");
        assert!(scores[0].score > scores[1].score);
    }

    #[test]
    fn product_quantized_vector_placeholder_validates_shape() {
        assert!(ProductQuantizedVector::new(Vec::new(), 4, None).is_err());
        assert!(ProductQuantizedVector::new(vec![1, 2, 3], 0, None).is_err());
        assert!(ProductQuantizedVector::new(
            vec![1, 2, 3],
            2,
            Some("fixture-codebook".to_string())
        )
        .is_ok());
    }
}
