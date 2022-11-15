use crate::consensus;
use reth_primitives::{rpc::BlockNumber, H256};
use thiserror::Error;

/// The downloader error type
#[derive(Error, Debug, Clone)]
pub enum DownloadError {
    /// Header validation failed
    #[error("Failed to validate header {hash}. Details: {error}.")]
    HeaderValidation {
        /// Hash of header failing validation
        hash: H256,
        /// The details of validation failure
        #[source]
        error: consensus::Error,
    },
    /// Timed out while waiting for request id response.
    #[error("Timed out while getting headers for request {request_id}.")]
    Timeout {
        /// The request id that timed out
        request_id: u64,
    },
    /// Error when checking that the current [`Header`] has the parent's hash as the parent_hash
    /// field, and that they have sequential block numbers.
    #[error("Headers did not match, current number: {header_number} / current hash: {header_hash}, parent number: {parent_number} / parent_hash: {parent_hash}")]
    MismatchedHeaders {
        /// The header number being evaluated
        header_number: BlockNumber,
        /// The header hash being evaluated
        header_hash: H256,
        /// The parent number being evaluated
        parent_number: BlockNumber,
        /// The parent hash being evaluated
        parent_hash: H256,
    },
}

impl DownloadError {
    /// Returns bool indicating whether this error is retryable or fatal, in the cases
    /// where the peer responds with no headers, or times out.
    pub fn is_retryable(&self) -> bool {
        matches!(self, DownloadError::Timeout { .. })
    }
}
