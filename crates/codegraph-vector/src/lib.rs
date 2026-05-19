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
    DimensionMismatch {
        expected: usize,
        actual: usize,
    },
    EmptyVector,
    InvalidMatryoshkaPrefix(usize),
    BackendUnavailable(&'static str),
    InputTooLarge {
        max_bytes: usize,
        actual_bytes: usize,
    },
    InputTooManyTokens {
        max_tokens: usize,
        actual_tokens: usize,
    },
    ExternalProviderRequiresExplicitOptIn,
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
            Self::InputTooLarge {
                max_bytes,
                actual_bytes,
            } => write!(
                formatter,
                "embedding input exceeds provider byte limit: max {max_bytes}, got {actual_bytes}"
            ),
            Self::InputTooManyTokens {
                max_tokens,
                actual_tokens,
            } => write!(
                formatter,
                "embedding input exceeds provider token limit: max {max_tokens}, got {actual_tokens}"
            ),
            Self::ExternalProviderRequiresExplicitOptIn => {
                formatter.write_str("external embedding provider requires explicit opt-in")
            }
        }
    }
}

impl Error for BinaryVectorError {}

pub type BinaryVectorResult<T> = Result<T, BinaryVectorError>;
pub type CompressedVectorResult<T> = Result<T, BinaryVectorError>;
pub type TestEmbeddingResult<T> = Result<T, BinaryVectorError>;

pub const DETERMINISTIC_TEST_EMBEDDING_PROVIDER_ID: &str = "codegraph-deterministic-test";
pub const DETERMINISTIC_TEST_EMBEDDING_MODEL_ID: &str =
    "codegraph-deterministic-token-projection-v1";
pub const DETERMINISTIC_TEST_EMBEDDING_VERSION: &str = "deterministic-test-embedding-v1";
pub const DETERMINISTIC_TEST_EMBEDDING_NORMALIZATION: &str = "l2";
pub const DETERMINISTIC_TEST_EMBEDDING_MAX_INPUT_BYTES: usize = 8 * 1024;
pub const DETERMINISTIC_TEST_EMBEDDING_MAX_INPUT_TOKENS: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestEmbeddingEnablement {
    Test,
    Dev,
    Diagnostic,
    Explicit,
}

impl TestEmbeddingEnablement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Dev => "dev",
            Self::Diagnostic => "diagnostic",
            Self::Explicit => "explicit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingProviderCapability {
    Local,
    Deterministic,
    Batch,
    External,
    RequiresApiKey,
}

impl EmbeddingProviderCapability {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Deterministic => "deterministic",
            Self::Batch => "batch",
            Self::External => "external",
            Self::RequiresApiKey => "requires_api_key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingPrivacyMode {
    LocalOnly,
    ExternalOptIn,
}

impl EmbeddingPrivacyMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalOnly => "local_only",
            Self::ExternalOptIn => "external_opt_in",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingProviderMetadata {
    pub provider_id: String,
    pub model_id: String,
    pub dimension: usize,
    pub max_input_bytes: usize,
    pub max_input_tokens: usize,
    pub normalization: String,
    pub version: String,
    pub capabilities: Vec<EmbeddingProviderCapability>,
    pub privacy_mode: EmbeddingPrivacyMode,
    pub enablement: TestEmbeddingEnablement,
    pub source_leaves_machine: bool,
    pub production_semantic_quality: bool,
    pub external_network: bool,
    pub requires_api_key: bool,
}

pub type TestEmbeddingProviderMetadata = EmbeddingProviderMetadata;

#[derive(Debug, Clone, PartialEq)]
pub struct TestEmbeddingVector {
    values: Vec<f32>,
}

impl TestEmbeddingVector {
    pub fn dimensions(&self) -> usize {
        self.values.len()
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

pub trait EmbeddingProvider {
    fn metadata(&self) -> &EmbeddingProviderMetadata;

    fn embed(&self, text: &str) -> TestEmbeddingResult<TestEmbeddingVector>;

    fn embed_batch(&self, texts: &[&str]) -> TestEmbeddingResult<Vec<TestEmbeddingVector>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicTestEmbeddingProvider {
    metadata: TestEmbeddingProviderMetadata,
}

impl DeterministicTestEmbeddingProvider {
    pub fn new(dimension: usize, enablement: TestEmbeddingEnablement) -> TestEmbeddingResult<Self> {
        if dimension == 0 {
            return Err(BinaryVectorError::ZeroDimensions);
        }
        Ok(Self {
            metadata: TestEmbeddingProviderMetadata {
                provider_id: DETERMINISTIC_TEST_EMBEDDING_PROVIDER_ID.to_string(),
                model_id: DETERMINISTIC_TEST_EMBEDDING_MODEL_ID.to_string(),
                dimension,
                max_input_bytes: DETERMINISTIC_TEST_EMBEDDING_MAX_INPUT_BYTES,
                max_input_tokens: DETERMINISTIC_TEST_EMBEDDING_MAX_INPUT_TOKENS,
                normalization: DETERMINISTIC_TEST_EMBEDDING_NORMALIZATION.to_string(),
                version: DETERMINISTIC_TEST_EMBEDDING_VERSION.to_string(),
                capabilities: vec![
                    EmbeddingProviderCapability::Local,
                    EmbeddingProviderCapability::Deterministic,
                    EmbeddingProviderCapability::Batch,
                ],
                privacy_mode: EmbeddingPrivacyMode::LocalOnly,
                enablement,
                source_leaves_machine: false,
                production_semantic_quality: false,
                external_network: false,
                requires_api_key: false,
            },
        })
    }

    pub fn for_tests(dimension: usize) -> TestEmbeddingResult<Self> {
        Self::new(dimension, TestEmbeddingEnablement::Test)
    }

    pub fn metadata(&self) -> &TestEmbeddingProviderMetadata {
        &self.metadata
    }

    pub fn embed(&self, text: &str) -> TestEmbeddingResult<TestEmbeddingVector> {
        check_embedding_input_limits(&self.metadata, text)?;
        let dimension = self.metadata.dimension;
        let mut values = vec![0.0f32; dimension];
        let tokens = tokenize_for_embedding(text);
        let token_refs = if tokens.is_empty() {
            vec![text.trim().to_ascii_lowercase()]
        } else {
            tokens
        };

        for token in token_refs.iter().filter(|token| !token.is_empty()) {
            project_embedding_token(token, &mut values);
            for subtoken in token.split('_').filter(|part| part.len() >= 2) {
                project_embedding_token(subtoken, &mut values);
            }
        }

        normalize_l2(&mut values);
        Ok(TestEmbeddingVector { values })
    }

    pub fn embed_batch(&self, texts: &[&str]) -> TestEmbeddingResult<Vec<TestEmbeddingVector>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }
}

impl EmbeddingProvider for DeterministicTestEmbeddingProvider {
    fn metadata(&self) -> &EmbeddingProviderMetadata {
        &self.metadata
    }

    fn embed(&self, text: &str) -> TestEmbeddingResult<TestEmbeddingVector> {
        DeterministicTestEmbeddingProvider::embed(self, text)
    }

    fn embed_batch(&self, texts: &[&str]) -> TestEmbeddingResult<Vec<TestEmbeddingVector>> {
        DeterministicTestEmbeddingProvider::embed_batch(self, texts)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEmbeddingProviderConfig {
    pub provider_id: String,
    pub model_id: String,
    pub dimension: usize,
    pub max_input_bytes: usize,
    pub max_input_tokens: usize,
    pub normalization: String,
    pub api_key_env_var: Option<String>,
    pub source_leaves_machine: bool,
    pub explicit_opt_in: bool,
}

impl ExternalEmbeddingProviderConfig {
    pub fn metadata(&self) -> EmbeddingProviderMetadata {
        EmbeddingProviderMetadata {
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            dimension: self.dimension,
            max_input_bytes: self.max_input_bytes,
            max_input_tokens: self.max_input_tokens,
            normalization: self.normalization.clone(),
            version: "external-provider-plan-v1".to_string(),
            capabilities: vec![
                EmbeddingProviderCapability::External,
                EmbeddingProviderCapability::Batch,
                EmbeddingProviderCapability::RequiresApiKey,
            ],
            privacy_mode: EmbeddingPrivacyMode::ExternalOptIn,
            enablement: TestEmbeddingEnablement::Explicit,
            source_leaves_machine: self.source_leaves_machine,
            production_semantic_quality: false,
            external_network: true,
            requires_api_key: self.api_key_env_var.is_some(),
        }
    }

    pub fn safe_log_summary(&self, api_key_present: bool) -> String {
        format!(
            "provider_id={} model_id={} dimension={} normalization={} privacy_mode={} source_leaves_machine={} explicit_opt_in={} api_key_env_var={} api_key_present={}",
            self.provider_id,
            self.model_id,
            self.dimension,
            self.normalization,
            EmbeddingPrivacyMode::ExternalOptIn.as_str(),
            self.source_leaves_machine,
            self.explicit_opt_in,
            self.api_key_env_var.as_deref().unwrap_or("none"),
            api_key_present
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingProviderConfig {
    DeterministicTest {
        dimension: usize,
        enablement: TestEmbeddingEnablement,
    },
    External(ExternalEmbeddingProviderConfig),
}

impl EmbeddingProviderConfig {
    pub const fn deterministic_for_tests(dimension: usize) -> Self {
        Self::DeterministicTest {
            dimension,
            enablement: TestEmbeddingEnablement::Test,
        }
    }

    pub fn select(self) -> TestEmbeddingResult<EmbeddingProviderSelection> {
        match self {
            Self::DeterministicTest {
                dimension,
                enablement,
            } => DeterministicTestEmbeddingProvider::new(dimension, enablement)
                .map(EmbeddingProviderSelection::Deterministic),
            Self::External(config) => {
                if !config.explicit_opt_in {
                    return Err(BinaryVectorError::ExternalProviderRequiresExplicitOptIn);
                }
                Ok(EmbeddingProviderSelection::ExternalPlan(
                    ExternalEmbeddingProviderPlan {
                        metadata: config.metadata(),
                    },
                ))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingProviderSelection {
    Deterministic(DeterministicTestEmbeddingProvider),
    ExternalPlan(ExternalEmbeddingProviderPlan),
}

impl EmbeddingProviderSelection {
    pub fn metadata(&self) -> &EmbeddingProviderMetadata {
        match self {
            Self::Deterministic(provider) => provider.metadata(),
            Self::ExternalPlan(plan) => &plan.metadata,
        }
    }

    pub fn deterministic_provider(&self) -> Option<&DeterministicTestEmbeddingProvider> {
        match self {
            Self::Deterministic(provider) => Some(provider),
            Self::ExternalPlan(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEmbeddingProviderPlan {
    metadata: EmbeddingProviderMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorIndexProviderMetadata {
    pub provider_id: String,
    pub model_id: String,
    pub dimension: usize,
    pub normalization: String,
    pub provider_version: String,
    pub privacy_mode: EmbeddingPrivacyMode,
}

impl VectorIndexProviderMetadata {
    pub fn from_provider(metadata: &EmbeddingProviderMetadata) -> Self {
        Self {
            provider_id: metadata.provider_id.clone(),
            model_id: metadata.model_id.clone(),
            dimension: metadata.dimension,
            normalization: metadata.normalization.clone(),
            provider_version: metadata.version.clone(),
            privacy_mode: metadata.privacy_mode,
        }
    }

    pub fn incompatibility_reason(&self, metadata: &EmbeddingProviderMetadata) -> Option<String> {
        if self.provider_id != metadata.provider_id {
            return Some("provider_id changed".to_string());
        }
        if self.model_id != metadata.model_id {
            return Some("model_id changed".to_string());
        }
        if self.dimension != metadata.dimension {
            return Some("dimension changed".to_string());
        }
        if self.normalization != metadata.normalization {
            return Some("normalization changed".to_string());
        }
        if self.provider_version != metadata.version {
            return Some("provider_version changed".to_string());
        }
        if self.privacy_mode != metadata.privacy_mode {
            return Some("privacy_mode changed".to_string());
        }
        None
    }

    pub fn is_compatible_with(&self, metadata: &EmbeddingProviderMetadata) -> bool {
        self.incompatibility_reason(metadata).is_none()
    }
}

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
    pub rare_token_weight: f64,
    pub identifier_signature_weight: f64,
    pub text_evidence_weight: f64,
    pub source_role_weight: f64,
    pub graph_verification_weight: f64,
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
            rare_token_weight: 0.20,
            identifier_signature_weight: 0.20,
            text_evidence_weight: 0.08,
            source_role_weight: 0.06,
            graph_verification_weight: 0.06,
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
            let rare_token_match = rare_token_feature_score(query, candidate);
            let identifier_signature_match = identifier_signature_feature_score(query, candidate);
            let text_evidence_match = text_evidence_feature_score(candidate);
            let source_role_compatibility = source_role_compatibility_score(query, candidate);
            let graph_verification_availability =
                metadata_flag(&candidate.metadata, "graph_verification_available");
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
            components.insert("rare_token_match".to_string(), rare_token_match);
            components.insert(
                "identifier_signature_match".to_string(),
                identifier_signature_match,
            );
            components.insert("text_evidence_match".to_string(), text_evidence_match);
            components.insert(
                "source_role_compatibility".to_string(),
                source_role_compatibility,
            );
            components.insert(
                "graph_verification_availability".to_string(),
                graph_verification_availability,
            );
            components.insert("exact_seed_boost".to_string(), exact_boost);

            let score = (self.config.text_weight * text_score)
                + (self.config.compressed_vector_weight * compressed_vector_score)
                + (self.config.stage1_weight * stage1_score)
                + (self.config.metadata_weight * metadata_score)
                + (self.config.rare_token_weight * rare_token_match)
                + (self.config.identifier_signature_weight * identifier_signature_match)
                + (self.config.text_evidence_weight * text_evidence_match)
                + (self.config.source_role_weight * source_role_compatibility)
                + (self.config.graph_verification_weight * graph_verification_availability)
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

fn rare_token_feature_score(query: &RerankQuery, candidate: &RerankCandidate) -> f64 {
    if metadata_bool(&candidate.metadata, "rare_token_match") {
        return 1.0;
    }

    let query_rare = tokenize_for_signature(&query.text)
        .into_iter()
        .filter(|token| is_rerank_rare_token(token))
        .collect::<BTreeSet<_>>();
    if query_rare.is_empty() {
        return 0.0;
    }

    let candidate_text = candidate_feature_text(candidate);
    let candidate_tokens = tokenize_for_signature(&candidate_text)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let overlap = query_rare.intersection(&candidate_tokens).count();
    overlap as f64 / query_rare.len() as f64
}

fn identifier_signature_feature_score(query: &RerankQuery, candidate: &RerankCandidate) -> f64 {
    if metadata_bool(&candidate.metadata, "identifier_signature_match") {
        return 1.0;
    }

    let identifier_text = [
        candidate.id.as_str(),
        metadata_value(&candidate.metadata, "identifier"),
        metadata_value(&candidate.metadata, "symbol"),
        metadata_value(&candidate.metadata, "signature"),
        metadata_value(&candidate.metadata, "path"),
        metadata_value(&candidate.metadata, "title"),
        metadata_value(&candidate.metadata, "config_key"),
        metadata_value(&candidate.metadata, "route_literal"),
        metadata_value(&candidate.metadata, "test_name"),
        metadata_value(&candidate.metadata, "matched_tokens"),
    ]
    .join(" ");
    token_overlap_score(&query.text, &identifier_text).clamp(0.0, 1.0)
}

fn text_evidence_feature_score(candidate: &RerankCandidate) -> f64 {
    if metadata_bool(&candidate.metadata, "text_evidence_match") {
        return 1.0;
    }

    let metadata_text = candidate_metadata_text(candidate).to_ascii_lowercase();
    if metadata_text.contains("text_evidence")
        || metadata_text.contains("stage0_text")
        || metadata_text.contains("docs")
        || metadata_text.contains("config.in")
        || metadata_text.contains(".mk")
        || metadata_text.contains("readme")
    {
        return 1.0;
    }
    0.0
}

fn source_role_compatibility_score(query: &RerankQuery, candidate: &RerankCandidate) -> f64 {
    if metadata_bool(&candidate.metadata, "source_role_compatible") {
        return 1.0;
    }

    let query_text = query.text.to_ascii_lowercase();
    let candidate_text = candidate_feature_text(candidate).to_ascii_lowercase();
    let mut matched = 0usize;
    let mut expected = 0usize;

    for (query_terms, candidate_terms) in [
        (
            ["test", "spec", "failing", "assert"].as_slice(),
            ["test", "spec", "__tests__", ".test.", ".spec."].as_slice(),
        ),
        (
            ["config", "buildroot", "package"].as_slice(),
            ["config.in", ".mk", "br2_", "generic-package"].as_slice(),
        ),
        (
            ["route", "endpoint", "api"].as_slice(),
            ["/api", "route", "endpoint"].as_slice(),
        ),
        (
            ["auth", "admin", "role", "permission"].as_slice(),
            ["auth", "admin", "role", "permission", "security"].as_slice(),
        ),
    ] {
        if query_terms.iter().any(|term| query_text.contains(term)) {
            expected += 1;
            if candidate_terms
                .iter()
                .any(|term| candidate_text.contains(term))
            {
                matched += 1;
            }
        }
    }

    if expected == 0 {
        0.0
    } else {
        matched as f64 / expected as f64
    }
}

fn candidate_feature_text(candidate: &RerankCandidate) -> String {
    format!(
        "{} {} {}",
        candidate.id,
        candidate.text,
        candidate_metadata_text(candidate)
    )
}

fn candidate_metadata_text(candidate: &RerankCandidate) -> String {
    candidate
        .metadata
        .iter()
        .map(|(key, value)| format!("{key} {value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn metadata_value<'a>(metadata: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    metadata.get(key).map(String::as_str).unwrap_or("")
}

fn metadata_flag(metadata: &BTreeMap<String, String>, key: &str) -> f64 {
    metadata_bool(metadata, key) as u8 as f64
}

fn metadata_bool(metadata: &BTreeMap<String, String>, key: &str) -> bool {
    metadata
        .get(key)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(
                value.as_str(),
                "1" | "true" | "yes" | "match" | "matched" | "available"
            )
        })
        .unwrap_or(false)
}

fn is_rerank_rare_token(token: &str) -> bool {
    token.len() >= 6
        || token.chars().any(|ch| ch.is_ascii_digit())
        || token.contains('_')
        || token.contains('-')
        || token.contains('.')
}

fn check_embedding_input_limits(
    metadata: &EmbeddingProviderMetadata,
    text: &str,
) -> TestEmbeddingResult<()> {
    let actual_bytes = text.len();
    if actual_bytes > metadata.max_input_bytes {
        return Err(BinaryVectorError::InputTooLarge {
            max_bytes: metadata.max_input_bytes,
            actual_bytes,
        });
    }
    let actual_tokens = tokenize_for_embedding(text).len();
    if actual_tokens > metadata.max_input_tokens {
        return Err(BinaryVectorError::InputTooManyTokens {
            max_tokens: metadata.max_input_tokens,
            actual_tokens,
        });
    }
    Ok(())
}

fn tokenize_for_embedding(text: &str) -> Vec<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn project_embedding_token(token: &str, values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    for projection in 0..6u64 {
        let hash = fnv1a64_with_seed(
            token.as_bytes(),
            0xd1b5_4a32_d192_ed03_u64.wrapping_add(projection),
        );
        let index = (hash as usize) % values.len();
        let sign = if (hash >> 63) == 0 { 1.0 } else { -1.0 };
        let magnitude = 1.0 + (((hash >> 8) & 0xff) as f32 / 1024.0);
        values[index] += sign * magnitude;
    }
}

fn normalize_l2(values: &mut [f32]) {
    let norm = values
        .iter()
        .map(|value| *value * *value)
        .sum::<f32>()
        .sqrt();
    if norm > 0.0 {
        for value in values {
            *value /= norm;
        }
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
    fn deterministic_test_embedding_same_text_same_vector() {
        let provider = ok(DeterministicTestEmbeddingProvider::for_tests(128));

        let left = ok(provider.embed("config BR2_PACKAGE_FOO generic-package"));
        let right = ok(provider.embed("config BR2_PACKAGE_FOO generic-package"));

        assert_eq!(left, right);
        assert_eq!(left.dimensions(), 128);
        assert_eq!(provider.metadata().dimension, 128);
    }

    #[test]
    fn deterministic_test_embedding_buildroot_tokens_are_separable() {
        let provider = ok(DeterministicTestEmbeddingProvider::for_tests(128));
        let openssl =
            ok(provider.embed("config BR2_PACKAGE_OPENSSL OPENSSL_VERSION TLS crypto library"));
        let busybox =
            ok(provider.embed("config BR2_PACKAGE_BUSYBOX BUSYBOX_CONFIG shell applets init"));
        let openssl_again =
            ok(provider.embed("config BR2_PACKAGE_OPENSSL OPENSSL_VERSION TLS crypto library"));

        assert!(cosine_f32(openssl.values(), openssl_again.values()) > 0.999);
        assert!(cosine_f32(openssl.values(), busybox.values()) < 0.95);
    }

    #[test]
    fn deterministic_test_embedding_metadata_records_local_test_boundaries() {
        let provider = ok(DeterministicTestEmbeddingProvider::new(
            64,
            TestEmbeddingEnablement::Diagnostic,
        ));
        let metadata = provider.metadata();

        assert_eq!(
            metadata.provider_id,
            DETERMINISTIC_TEST_EMBEDDING_PROVIDER_ID
        );
        assert_eq!(metadata.model_id, DETERMINISTIC_TEST_EMBEDDING_MODEL_ID);
        assert_eq!(metadata.dimension, 64);
        assert_eq!(
            metadata.max_input_bytes,
            DETERMINISTIC_TEST_EMBEDDING_MAX_INPUT_BYTES
        );
        assert_eq!(
            metadata.max_input_tokens,
            DETERMINISTIC_TEST_EMBEDDING_MAX_INPUT_TOKENS
        );
        assert_eq!(
            metadata.normalization,
            DETERMINISTIC_TEST_EMBEDDING_NORMALIZATION
        );
        assert_eq!(metadata.version, DETERMINISTIC_TEST_EMBEDDING_VERSION);
        assert!(metadata
            .capabilities
            .contains(&EmbeddingProviderCapability::Deterministic));
        assert!(metadata
            .capabilities
            .contains(&EmbeddingProviderCapability::Batch));
        assert_eq!(metadata.privacy_mode, EmbeddingPrivacyMode::LocalOnly);
        assert_eq!(metadata.enablement, TestEmbeddingEnablement::Diagnostic);
        assert_eq!(metadata.enablement.as_str(), "diagnostic");
        assert!(!metadata.source_leaves_machine);
        assert!(!metadata.production_semantic_quality);
        assert!(!metadata.external_network);
        assert!(!metadata.requires_api_key);
    }

    #[test]
    fn embedding_provider_config_selects_deterministic_provider_for_tests() {
        let selected = ok(EmbeddingProviderConfig::deterministic_for_tests(96).select());
        let metadata = selected.metadata();

        assert_eq!(
            metadata.provider_id,
            DETERMINISTIC_TEST_EMBEDDING_PROVIDER_ID
        );
        assert_eq!(metadata.dimension, 96);
        assert_eq!(metadata.enablement, TestEmbeddingEnablement::Test);
        assert_eq!(metadata.privacy_mode, EmbeddingPrivacyMode::LocalOnly);
        assert!(!metadata.external_network);
        let provider = selected
            .deterministic_provider()
            .expect("deterministic provider");
        let vectors = ok(provider.embed_batch(&["BR2_PACKAGE_FOO", "generic-package"]));
        assert_eq!(vectors.len(), 2);
        assert!(vectors.iter().all(|vector| vector.dimensions() == 96));
    }

    #[test]
    fn external_provider_without_explicit_opt_in_does_not_block_normal_tests() {
        let normal = ok(EmbeddingProviderConfig::deterministic_for_tests(32).select());
        assert!(!normal.metadata().source_leaves_machine);

        let external = ExternalEmbeddingProviderConfig {
            provider_id: "example-external".to_string(),
            model_id: "example-model".to_string(),
            dimension: 1536,
            max_input_bytes: 16 * 1024,
            max_input_tokens: 4096,
            normalization: "l2".to_string(),
            api_key_env_var: Some("CODEGRAPH_EMBEDDING_API_KEY".to_string()),
            source_leaves_machine: true,
            explicit_opt_in: false,
        };
        assert!(matches!(
            EmbeddingProviderConfig::External(external).select(),
            Err(BinaryVectorError::ExternalProviderRequiresExplicitOptIn)
        ));
    }

    #[test]
    fn provider_metadata_is_included_in_vector_index_metadata() {
        let provider = ok(DeterministicTestEmbeddingProvider::for_tests(128));
        let index_metadata = VectorIndexProviderMetadata::from_provider(provider.metadata());

        assert_eq!(
            index_metadata.provider_id,
            DETERMINISTIC_TEST_EMBEDDING_PROVIDER_ID
        );
        assert_eq!(
            index_metadata.model_id,
            DETERMINISTIC_TEST_EMBEDDING_MODEL_ID
        );
        assert_eq!(index_metadata.dimension, 128);
        assert_eq!(index_metadata.normalization, "l2");
        assert_eq!(
            index_metadata.provider_version,
            DETERMINISTIC_TEST_EMBEDDING_VERSION
        );
        assert!(index_metadata.is_compatible_with(provider.metadata()));
    }

    #[test]
    fn provider_model_mismatch_invalidates_vector_index_metadata() {
        let provider = ok(DeterministicTestEmbeddingProvider::for_tests(128));
        let index_metadata = VectorIndexProviderMetadata::from_provider(provider.metadata());
        let mut changed = provider.metadata().clone();
        changed.model_id = "different-model".to_string();

        assert_eq!(
            index_metadata.incompatibility_reason(&changed).as_deref(),
            Some("model_id changed")
        );
        assert!(!index_metadata.is_compatible_with(&changed));
    }

    #[test]
    fn external_provider_log_summary_excludes_secret_values() {
        let secret_value = "sk-test-secret-value";
        let external = ExternalEmbeddingProviderConfig {
            provider_id: "example-external".to_string(),
            model_id: "example-model".to_string(),
            dimension: 1536,
            max_input_bytes: 16 * 1024,
            max_input_tokens: 4096,
            normalization: "l2".to_string(),
            api_key_env_var: Some("CODEGRAPH_EMBEDDING_API_KEY".to_string()),
            source_leaves_machine: true,
            explicit_opt_in: true,
        };

        let summary = external.safe_log_summary(!secret_value.is_empty());

        assert!(summary.contains("provider_id=example-external"));
        assert!(summary.contains("model_id=example-model"));
        assert!(summary.contains("source_leaves_machine=true"));
        assert!(summary.contains("api_key_env_var=CODEGRAPH_EMBEDDING_API_KEY"));
        assert!(summary.contains("api_key_present=true"));
        assert!(!summary.contains(secret_value));
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
    fn reranker_uses_deterministic_nuance_features() {
        let config = RerankConfig {
            text_weight: 0.0,
            compressed_vector_weight: 0.0,
            stage1_weight: 0.0,
            metadata_weight: 0.0,
            rare_token_weight: 1.0,
            identifier_signature_weight: 1.0,
            text_evidence_weight: 0.5,
            source_role_weight: 0.5,
            graph_verification_weight: 0.5,
            ..RerankConfig::default()
        };
        let reranker = DeterministicCompressedReranker::new(config);
        let query = RerankQuery::new("Trace ZephyrAlphaToken route auth test");
        let mut target =
            RerankCandidate::new("ZephyrAlphaTokenGuard", "tiny body").stage1_similarity(-128);
        target
            .metadata
            .insert("matched_tokens".to_string(), "zephyralphatoken".to_string());
        target
            .metadata
            .insert("path".to_string(), "tests/auth_route.test.ts".to_string());
        target
            .metadata
            .insert("text_evidence_match".to_string(), "true".to_string());
        target.metadata.insert(
            "graph_verification_available".to_string(),
            "true".to_string(),
        );
        let noise = RerankCandidate::new("generic", "trace route auth test").stage1_similarity(128);

        let scores = ok(reranker.rerank(&query, &[noise, target], 1));

        assert_eq!(scores[0].id, "ZephyrAlphaTokenGuard");
        for component in [
            "rare_token_match",
            "identifier_signature_match",
            "text_evidence_match",
            "source_role_compatibility",
            "graph_verification_availability",
        ] {
            assert!(
                scores[0]
                    .components
                    .get(component)
                    .is_some_and(|value| *value > 0.0),
                "missing positive component {component}"
            );
        }
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

    fn cosine_f32(left: &[f32], right: &[f32]) -> f32 {
        let mut dot = 0.0f32;
        let mut left_norm = 0.0f32;
        let mut right_norm = 0.0f32;
        for (left, right) in left.iter().zip(right) {
            dot += *left * *right;
            left_norm += *left * *left;
            right_norm += *right * *right;
        }
        if left_norm == 0.0 || right_norm == 0.0 {
            return 0.0;
        }
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}
