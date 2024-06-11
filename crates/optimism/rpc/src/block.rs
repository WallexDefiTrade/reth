//! Loads and formats OP block RPC response.   

use reth_evm::ConfigureEvm;
use reth_primitives::TransactionMeta;
use reth_provider::{
    BlockReaderIdExt, ChainSpecProvider, EvmEnvProvider, HeaderProvider, StateProviderFactory,
};
use reth_rpc::eth::{
    api::{
        EthBlocks, LoadBlock, LoadPendingBlock, LoadReceipt, PendingBlock, ReceiptBuilder,
        SpawnBlocking,
    },
    cache::EthStateCache,
    error::EthResult,
};
use reth_rpc_types::{AnyTransactionReceipt, BlockId};
use reth_transaction_pool::TransactionPool;
use tokio::sync::Mutex;

use crate::receipt::op_receipt_fields;

use super::OptimismApi;

impl<Provider, Pool, Network, EvmConfig> EthBlocks
    for OptimismApi<Provider, Pool, Network, EvmConfig>
where
    Self: LoadBlock,
    Provider: BlockReaderIdExt + ChainSpecProvider + HeaderProvider,
{
    fn provider(&self) -> impl HeaderProvider {
        self.inner.provider()
    }

    async fn block_receipts(
        &self,
        block_id: BlockId,
    ) -> EthResult<Option<Vec<AnyTransactionReceipt>>>
    where
        Self: LoadReceipt,
    {
        if let Some((block, receipts)) = self.load_block_and_receipts(block_id).await? {
            let block_number = block.number;
            let base_fee = block.base_fee_per_gas;
            let block_hash = block.hash();
            let excess_blob_gas = block.excess_blob_gas;
            let timestamp = block.timestamp;
            let block = block.unseal();

            let l1_block_info = reth_evm_optimism::extract_l1_info(&block).ok();

            let receipts = block
                .body
                .into_iter()
                .zip(receipts.iter())
                .enumerate()
                .map(|(idx, (ref tx, receipt))| {
                    let meta = TransactionMeta {
                        tx_hash: tx.hash,
                        index: idx as u64,
                        block_hash,
                        block_number,
                        base_fee,
                        excess_blob_gas,
                        timestamp,
                    };

                    let optimism_tx_meta =
                        self.build_op_tx_meta(tx, l1_block_info.clone(), timestamp)?;

                    ReceiptBuilder::new(tx, meta, receipt, &receipts).map(|builder| {
                        op_receipt_fields(builder, tx, receipt, optimism_tx_meta).build()
                    })
                })
                .collect::<EthResult<Vec<_>>>();
            return receipts.map(Some)
        }

        Ok(None)
    }
}

impl<Provider, Pool, Network, EvmConfig> LoadBlock
    for OptimismApi<Provider, Pool, Network, EvmConfig>
where
    Self: LoadPendingBlock + SpawnBlocking,
    Provider: BlockReaderIdExt,
{
    #[inline]
    fn provider(&self) -> impl BlockReaderIdExt {
        self.inner.provider()
    }

    #[inline]
    fn cache(&self) -> &EthStateCache {
        self.inner.cache()
    }
}

impl<Provider, Pool, Network, EvmConfig> LoadPendingBlock
    for OptimismApi<Provider, Pool, Network, EvmConfig>
where
    Provider: BlockReaderIdExt + EvmEnvProvider + ChainSpecProvider + StateProviderFactory,
    Pool: TransactionPool,
    EvmConfig: ConfigureEvm,
{
    #[inline]
    fn provider(
        &self,
    ) -> impl BlockReaderIdExt + EvmEnvProvider + ChainSpecProvider + StateProviderFactory {
        self.inner.provider()
    }

    #[inline]
    fn pool(&self) -> impl TransactionPool {
        self.inner.pool()
    }

    #[inline]
    fn pending_block(&self) -> &Mutex<Option<PendingBlock>> {
        self.inner.pending_block()
    }

    #[inline]
    fn evm_config(&self) -> &impl ConfigureEvm {
        self.inner.evm_config()
    }
}
