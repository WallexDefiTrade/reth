use crate::{provider, provider::BlockProvider};
use reth_primitives::{rpc::BlockId, Block, BlockNumber, H256, U256};

/// Supports various api interfaces for testing purposes.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TestApi;

/// Noop implementation for testing purposes
impl BlockProvider for TestApi {
    fn chain_info(&self) -> crate::Result<provider::ChainInfo> {
        Ok(provider::ChainInfo {
            best_hash: Default::default(),
            best_number: 0,
            last_finalized: None,
            safe_finalized: None,
        })
    }

    fn block(&self, _id: BlockId) -> crate::Result<Option<Block>> {
        Ok(None)
    }

    fn block_number(&self, _hash: H256) -> crate::Result<Option<BlockNumber>> {
        Ok(None)
    }

    fn block_hash(&self, _number: U256) -> crate::Result<Option<H256>> {
        Ok(None)
    }
}
