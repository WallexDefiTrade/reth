use reth_downloaders::{
    bodies::bodies::BodiesDownloaderBuilder,
    headers::reverse_headers::ReverseHeadersDownloaderBuilder,
};
use serde::{Deserialize, Serialize};

/// Configuration for each stage in the pipeline.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct StageConfig {
    /// Header stage configuration.
    pub headers: HeadersConfig,
    /// Total Difficulty stage configuration
    pub total_difficulty: TotalDifficultyConfig,
    /// Body stage configuration.
    pub bodies: BodiesConfig,
    /// Sender Recovery stage configuration.
    pub sender_recovery: SenderRecoveryConfig,
    /// Execution stage configuration.
    pub execution: ExecutionConfig,
    /// Account Hashing stage configuration.
    pub account_hashing: HashingConfig,
    /// Storage Hashing stage configuration.
    pub storage_hashing: HashingConfig,
    /// Merkle stage configuration.
    pub merkle: MerkleConfig,
    /// Transaction Lookup stage configuration.
    pub transaction_lookup: TransactionLookupConfig,
    /// Index Account History stage configuration.
    pub index_account_history: IndexHistoryConfig,
    /// Index Storage History stage configuration.
    pub index_storage_history: IndexHistoryConfig,
}

/// Header stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct HeadersConfig {
    /// The maximum number of requests to send concurrently.
    ///
    /// Default: 100
    pub downloader_max_concurrent_requests: usize,
    /// The minimum number of requests to send concurrently.
    ///
    /// Default: 5
    pub downloader_min_concurrent_requests: usize,
    /// Maximum amount of responses to buffer internally.
    /// The response contains multiple headers.
    pub downloader_max_buffered_responses: usize,
    /// The maximum number of headers to request from a peer at a time.
    pub downloader_request_limit: u64,
    /// The maximum number of headers to download before committing progress to the database.
    pub commit_threshold: u64,
}

impl Default for HeadersConfig {
    fn default() -> Self {
        Self {
            commit_threshold: 10_000,
            downloader_request_limit: 1_000,
            downloader_max_concurrent_requests: 100,
            downloader_min_concurrent_requests: 5,
            downloader_max_buffered_responses: 100,
        }
    }
}

impl From<HeadersConfig> for ReverseHeadersDownloaderBuilder {
    fn from(config: HeadersConfig) -> Self {
        ReverseHeadersDownloaderBuilder::default()
            .request_limit(config.downloader_request_limit)
            .min_concurrent_requests(config.downloader_min_concurrent_requests)
            .max_concurrent_requests(config.downloader_max_concurrent_requests)
            .max_buffered_responses(config.downloader_max_buffered_responses)
            .stream_batch_size(config.commit_threshold as usize)
    }
}

/// Total difficulty stage configuration
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct TotalDifficultyConfig {
    /// The maximum number of total difficulty entries to sum up before committing progress to the
    /// database.
    pub commit_threshold: u64,
}

impl Default for TotalDifficultyConfig {
    fn default() -> Self {
        Self { commit_threshold: 100_000 }
    }
}

/// Body stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct BodiesConfig {
    /// The batch size of non-empty blocks per one request
    ///
    /// Default: 200
    pub downloader_request_limit: u64,
    /// The maximum number of block bodies returned at once from the stream
    ///
    /// Default: 1_000
    pub downloader_stream_batch_size: usize,
    /// The size of the internal block buffer in bytes.
    ///
    /// Default: 2GB
    pub downloader_max_buffered_blocks_size_bytes: usize,
    /// The minimum number of requests to send concurrently.
    ///
    /// Default: 5
    pub downloader_min_concurrent_requests: usize,
    /// The maximum number of requests to send concurrently.
    /// This is equal to the max number of peers.
    ///
    /// Default: 100
    pub downloader_max_concurrent_requests: usize,
}

impl Default for BodiesConfig {
    fn default() -> Self {
        Self {
            downloader_request_limit: 200,
            downloader_stream_batch_size: 1_000,
            downloader_max_buffered_blocks_size_bytes: 2 * 1024 * 1024 * 1024, // ~2GB
            downloader_min_concurrent_requests: 5,
            downloader_max_concurrent_requests: 100,
        }
    }
}

impl From<BodiesConfig> for BodiesDownloaderBuilder {
    fn from(config: BodiesConfig) -> Self {
        BodiesDownloaderBuilder::default()
            .with_stream_batch_size(config.downloader_stream_batch_size)
            .with_request_limit(config.downloader_request_limit)
            .with_max_buffered_blocks_size_bytes(config.downloader_max_buffered_blocks_size_bytes)
            .with_concurrent_requests_range(
                config.downloader_min_concurrent_requests..=
                    config.downloader_max_concurrent_requests,
            )
    }
}

/// Sender recovery stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct SenderRecoveryConfig {
    /// The maximum number of transactions to process before committing progress to the database.
    pub commit_threshold: u64,
}

impl Default for SenderRecoveryConfig {
    fn default() -> Self {
        Self { commit_threshold: 5_000_000 }
    }
}

/// Execution stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct ExecutionConfig {
    /// The maximum number of blocks to process before the execution stage commits.
    pub max_blocks: Option<u64>,
    /// The maximum amount of state changes to keep in memory before the execution stage commits.
    pub max_changes: Option<u64>,
    /// The maximum gas to process before the execution stage commits.
    pub max_cumulative_gas: Option<u64>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_blocks: Some(500_000),
            max_changes: Some(5_000_000),
            // 50k full blocks of 30M gas
            max_cumulative_gas: Some(30_000_000 * 50_000),
        }
    }
}

/// Hashing stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct HashingConfig {
    /// The threshold (in number of blocks) for switching between
    /// incremental hashing and full hashing.
    pub clean_threshold: u64,
    /// The maximum number of entities to process before committing progress to the database.
    pub commit_threshold: u64,
}

impl Default for HashingConfig {
    fn default() -> Self {
        Self { clean_threshold: 500_000, commit_threshold: 100_000 }
    }
}

/// Merkle stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct MerkleConfig {
    /// The threshold (in number of blocks) for switching from incremental trie building of changes
    /// to whole rebuild.
    pub clean_threshold: u64,
}

impl Default for MerkleConfig {
    fn default() -> Self {
        Self { clean_threshold: 50_000 }
    }
}

/// Transaction Lookup stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct TransactionLookupConfig {
    /// The maximum number of transactions to process before committing progress to the database.
    pub commit_threshold: u64,
}

impl Default for TransactionLookupConfig {
    fn default() -> Self {
        Self { commit_threshold: 5_000_000 }
    }
}

/// History History stage configuration.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct IndexHistoryConfig {
    /// The maximum number of blocks to process before committing progress to the database.
    pub commit_threshold: u64,
}

impl Default for IndexHistoryConfig {
    fn default() -> Self {
        Self { commit_threshold: 100_000 }
    }
}
