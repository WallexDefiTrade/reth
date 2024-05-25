//! Formats OP receipt RPC response.   

use reth_evm::ConfigureEvm;
use reth_evm_optimism::RethL1BlockInfo;
use reth_network_api::NetworkInfo;
use reth_primitives::{BlockId, Receipt, TransactionMeta, TransactionSigned};
use reth_provider::{BlockReaderIdExt, ChainSpecProvider, EvmEnvProvider, StateProviderFactory};
use reth_rpc::{
    eth::{
        api::transactions::ReceiptResponseBuilder,
        error::{EthApiError, EthResult},
    },
    EthApi,
};
use reth_rpc_types::{AnyTransactionReceipt, OptimismTransactionReceiptFields};
use reth_transaction_pool::TransactionPool;

use crate::{error::OptimismEthApiError, transaction::OptimismTxMeta};

/// Helper function for `eth_getBlockReceipts`. Returns all transaction receipts in the block.
///
/// Returns `None` if the block wasn't found.
pub async fn block_receipts<Provider, Pool, Network, EvmConfig>(
    eth_api: &EthApi<Provider, Pool, Network, EvmConfig>,
    block_id: BlockId,
) -> EthResult<Option<Vec<AnyTransactionReceipt>>>
where
    Provider:
        BlockReaderIdExt + ChainSpecProvider + EvmEnvProvider + StateProviderFactory + 'static,
    Pool: TransactionPool + 'static,
    Network: NetworkInfo + 'static,
    EvmConfig: ConfigureEvm + 'static,
{
    if let Some((block, receipts)) = eth_api.load_block_and_receipts(block_id).await? {
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
                    build_op_tx_meta(eth_api, tx, l1_block_info.clone(), timestamp)?;

                ReceiptResponseBuilder::new(tx, meta, receipt, &receipts)
                    .map(|builder| op_fields(builder, tx, receipt, optimism_tx_meta).build())
            })
            .collect::<EthResult<Vec<_>>>();
        return receipts.map(Some)
    }

    Ok(None)
}

/// Helper function for `eth_getTransactionReceipt`
///
/// Returns the receipt
pub async fn build_transaction_receipt<Provider, Pool, Network, EvmConfig>(
    eth_api: &EthApi<Provider, Pool, Network, EvmConfig>,
    tx: TransactionSigned,
    meta: TransactionMeta,
    receipt: Receipt,
) -> EthResult<AnyTransactionReceipt>
where
    Provider: BlockReaderIdExt + ChainSpecProvider,
{
    let (block, receipts) = eth_api
        .cache()
        .get_block_and_receipts(meta.block_hash)
        .await?
        .ok_or(EthApiError::UnknownBlockNumber)?;

    let block = block.unseal();
    let l1_block_info = reth_evm_optimism::extract_l1_info(&block).ok();
    let optimism_tx_meta = build_op_tx_meta(eth_api, &tx, l1_block_info, block.timestamp)?;

    let resp_builder = ReceiptResponseBuilder::new(&tx, meta, &receipt, &receipts)?;
    let resp_builder = op_fields(resp_builder, &tx, &receipt, optimism_tx_meta);

    Ok(resp_builder.build())
}

/// Builds op metadata object using the provided [TransactionSigned], L1 block info and
/// `block_timestamp`. The L1BlockInfo is used to calculate the l1 fee and l1 data gas for the
/// transaction. If the L1BlockInfo is not provided, the meta info will be empty.
pub fn build_op_tx_meta<Provider, Pool, Network, EvmConfig>(
    eth_api: &EthApi<Provider, Pool, Network, EvmConfig>,
    tx: &TransactionSigned,
    l1_block_info: Option<revm::L1BlockInfo>,
    block_timestamp: u64,
) -> EthResult<OptimismTxMeta>
where
    Provider: BlockReaderIdExt + ChainSpecProvider,
{
    let Some(l1_block_info) = l1_block_info else { return Ok(OptimismTxMeta::default()) };

    let (l1_fee, l1_data_gas) = if !tx.is_deposit() {
        let envelope_buf = tx.envelope_encoded();

        let inner_l1_fee = l1_block_info
            .l1_tx_data_fee(
                &eth_api.provider().chain_spec(),
                block_timestamp,
                &envelope_buf,
                tx.is_deposit(),
            )
            .map_err(|_| OptimismEthApiError::L1BlockFeeError)?;
        let inner_l1_data_gas = l1_block_info
            .l1_data_gas(&eth_api.provider().chain_spec(), block_timestamp, &envelope_buf)
            .map_err(|_| OptimismEthApiError::L1BlockGasError)?;
        (
            Some(inner_l1_fee.saturating_to::<u128>()),
            Some(inner_l1_data_gas.saturating_to::<u128>()),
        )
    } else {
        (None, None)
    };

    Ok(OptimismTxMeta::new(Some(l1_block_info), l1_fee, l1_data_gas))
}

/// Applies OP specific fields to a receipts response.
pub fn op_fields(
    resp_builder: ReceiptResponseBuilder,
    tx: &TransactionSigned,
    receipt: &Receipt,
    optimism_tx_meta: OptimismTxMeta,
) -> ReceiptResponseBuilder {
    let mut op_fields = OptimismTransactionReceiptFields::default();

    if tx.is_deposit() {
        op_fields.deposit_nonce = receipt.deposit_nonce.map(reth_primitives::U64::from);
        op_fields.deposit_receipt_version =
            receipt.deposit_receipt_version.map(reth_primitives::U64::from);
    } else if let Some(l1_block_info) = optimism_tx_meta.l1_block_info {
        op_fields.l1_fee = optimism_tx_meta.l1_fee;
        op_fields.l1_gas_used = optimism_tx_meta.l1_data_gas.map(|dg| {
            dg + l1_block_info.l1_fee_overhead.unwrap_or_default().saturating_to::<u128>()
        });
        op_fields.l1_fee_scalar = Some(f64::from(l1_block_info.l1_base_fee_scalar) / 1_000_000.0);
        op_fields.l1_gas_price = Some(l1_block_info.l1_base_fee.saturating_to());
    }

    resp_builder.add_other_fields(op_fields.into())
}
