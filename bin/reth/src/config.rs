//! Configuration files.
use std::{collections::HashSet, sync::Arc};

use reth_db::database::Database;
use reth_network::{
    config::{mainnet_nodes, rng_secret_key},
    NetworkConfig,
};
use reth_primitives::{NodeRecord, H256};
use reth_provider::ProviderImpl;
use serde::{Deserialize, Serialize};

/// Configuration for the reth node.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    /// Configuration for each stage in the pipeline.
    // TODO(onbjerg): Can we make this easier to maintain when we add/remove stages?
    pub stages: StageConfig,
    /// Configuration for the discovery service.
    pub peers: PeersConfig,
}

impl Config {
    /// Initializes network config from read data
    pub fn network_config<DB: Database>(
        &self,
        db: Arc<DB>,
        chain_id: u64,
        genesis_hash: H256,
    ) -> NetworkConfig<ProviderImpl<DB>> {
        let peer_config = reth_network::PeersConfig::default()
            .with_trusted_nodes(self.peers.trusted_nodes.clone())
            .with_connect_trusted_nodes_only(self.peers.connect_trusted_nodes_only);
        NetworkConfig::builder(Arc::new(ProviderImpl::new(db)), rng_secret_key())
            .boot_nodes(mainnet_nodes())
            .peer_config(peer_config)
            .genesis_hash(genesis_hash)
            .chain_id(chain_id)
            .build()
    }
}

/// Configuration for each stage in the pipeline.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StageConfig {
    /// Header stage configuration.
    pub headers: HeadersConfig,
    /// Body stage configuration.
    pub bodies: BodiesConfig,
    /// Sender recovery stage configuration.
    pub sender_recovery: SenderRecoveryConfig,
}

/// Header stage configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeadersConfig {
    /// The maximum number of headers to download before committing progress to the database.
    pub commit_threshold: u64,
    /// The maximum number of headers to request from a peer at a time.
    pub downloader_batch_size: u64,
    /// The number of times to retry downloading a set of headers.
    pub downloader_retries: usize,
}

impl Default for HeadersConfig {
    fn default() -> Self {
        Self { commit_threshold: 10_000, downloader_batch_size: 1000, downloader_retries: 5 }
    }
}

/// Body stage configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BodiesConfig {
    /// The maximum number of bodies to download before committing progress to the database.
    pub commit_threshold: u64,
    /// The maximum number of bodies to request from a peer at a time.
    pub downloader_batch_size: usize,
    /// The number of times to retry downloading a set of bodies.
    pub downloader_retries: usize,
    /// The maximum number of body requests to have in flight at a time.
    ///
    /// The maximum number of bodies downloaded at the same time is `downloader_batch_size *
    /// downloader_concurrency`.
    pub downloader_concurrency: usize,
}

impl Default for BodiesConfig {
    fn default() -> Self {
        Self {
            commit_threshold: 5_000,
            downloader_batch_size: 200,
            downloader_retries: 5,
            downloader_concurrency: 10,
        }
    }
}

/// Sender recovery stage configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SenderRecoveryConfig {
    /// The maximum number of blocks to process before committing progress to the database.
    pub commit_threshold: u64,
    /// The maximum number of transactions to recover senders for concurrently.
    pub batch_size: usize,
}

impl Default for SenderRecoveryConfig {
    fn default() -> Self {
        Self { commit_threshold: 5_000, batch_size: 1000 }
    }
}

/// Configuration for peer managing.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PeersConfig {
    /// Trusted nodes to connect to.
    pub trusted_nodes: HashSet<NodeRecord>,
    /// Connect to trusted nodes only?
    pub connect_trusted_nodes_only: bool,
}
