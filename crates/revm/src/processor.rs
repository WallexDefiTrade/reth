use crate::{
    database::State,
    env::{fill_cfg_and_block_env, fill_tx_env},
    eth_dao_fork::{DAO_HARDFORK_BENEFICIARY, DAO_HARDKFORK_ACCOUNTS},
    into_reth_log,
    stack::{InspectorStack, InspectorStackConfig},
    state_change::post_block_balance_increments,
};
use reth_interfaces::{
    executor::{BlockExecutionError, BlockValidationError},
    Error,
};
use reth_primitives::{
    Address, Block, BlockNumber, Bloom, ChainSpec, Hardfork, Header, PruneModes, Receipt,
    ReceiptWithBloom, TransactionSigned, H256, U256,
};
use reth_provider::{change::BundleState, BlockExecutor, BlockExecutorStats, StateProvider};
use revm::{
    primitives::ResultAndState, DatabaseCommit, State as RevmState,
    StateBuilder as RevmStateBuilder, EVM,
};
use std::{sync::Arc, time::Instant};
use tracing::{debug, trace};

/// Main block executor
pub struct EVMProcessor<'a> {
    /// The configured chain-spec
    pub chain_spec: Arc<ChainSpec>,
    evm: EVM<RevmState<'a, Error>>,
    stack: InspectorStack,
    receipts: Vec<Vec<Receipt>>,
    /// First block will be initialized to ZERO
    /// and be set to the block number of first block executed.
    first_block: BlockNumber,
    /// The maximum known block .
    tip: Option<BlockNumber>,
    /// Pruning configuration.
    prune_modes: PruneModes,
    /// Execution stats
    stats: BlockExecutorStats,
}

impl<'a> From<Arc<ChainSpec>> for EVMProcessor<'a> {
    /// Instantiates a new executor from the chainspec. Must call
    /// `with_db` to set the database before executing.
    fn from(chain_spec: Arc<ChainSpec>) -> Self {
        let evm = EVM::new();
        EVMProcessor {
            chain_spec,
            evm,
            stack: InspectorStack::new(InspectorStackConfig::default()),
            receipts: Vec::new(),
            first_block: 0,
            tip: None,
            prune_modes: PruneModes::none(),
            stats: BlockExecutorStats::default(),
        }
    }
}

impl<'a> EVMProcessor<'a> {
    /// Creates a new executor from the given chain spec and database.
    pub fn new<DB: StateProvider + 'a>(chain_spec: Arc<ChainSpec>, db: State<DB>) -> Self {
        let revm_state =
            RevmStateBuilder::default().with_database(Box::new(db)).without_state_clear().build();
        EVMProcessor::new_with_state(chain_spec, revm_state)
    }

    /// Create new EVM processor with a given revm state.
    pub fn new_with_state(chain_spec: Arc<ChainSpec>, revm_state: RevmState<'a, Error>) -> Self {
        let mut evm = EVM::new();
        evm.database(revm_state);
        EVMProcessor {
            chain_spec,
            evm,
            stack: InspectorStack::new(InspectorStackConfig::default()),
            receipts: Vec::new(),
            first_block: 0,
            tip: None,
            prune_modes: PruneModes::default(),
            stats: BlockExecutorStats::default(),
        }
    }

    /// Configures the executor with the given inspectors.
    pub fn set_stack(&mut self, stack: InspectorStack) {
        self.stack = stack;
    }

    /// Gives a reference to the database
    pub fn db(&mut self) -> &mut RevmState<'a, Error> {
        self.evm.db().expect("db to not be moved")
    }

    fn recover_senders(
        &mut self,
        body: &[TransactionSigned],
        senders: Option<Vec<Address>>,
    ) -> Result<Vec<Address>, BlockExecutionError> {
        if let Some(senders) = senders {
            if body.len() == senders.len() {
                Ok(senders)
            } else {
                Err(BlockValidationError::SenderRecoveryError.into())
            }
        } else {
            let time = Instant::now();
            let ret = TransactionSigned::recover_signers(body, body.len())
                .ok_or(BlockValidationError::SenderRecoveryError.into());
            self.stats.sender_recovery_duration += time.elapsed();
            ret
        }
    }

    /// Initializes the config and block env.
    fn init_env(&mut self, header: &Header, total_difficulty: U256) {
        //set state clear flag
        self.evm.db.as_mut().unwrap().set_state_clear_flag(
            self.chain_spec.fork(Hardfork::SpuriousDragon).active_at_block(header.number),
        );

        fill_cfg_and_block_env(
            &mut self.evm.env.cfg,
            &mut self.evm.env.block,
            &self.chain_spec,
            header,
            total_difficulty,
        );
    }

    /// Post execution state change that include block reward, withdrawals and iregular DAO hardfork
    /// state change.
    pub fn post_execution_state_change(
        &mut self,
        block: &Block,
        total_difficulty: U256,
    ) -> Result<(), BlockExecutionError> {
        let mut balance_increments = post_block_balance_increments(
            &self.chain_spec,
            block.number,
            block.difficulty,
            block.beneficiary,
            block.timestamp,
            total_difficulty,
            &block.ommers,
            block.withdrawals.as_deref(),
        );

        // Irregular state change at Ethereum DAO hardfork
        if self.chain_spec.fork(Hardfork::Dao).transitions_at_block(block.number) {
            // drain balances from hardcoded addresses.
            let drained_balance: u128 = self
                .db()
                .drain_balances(DAO_HARDKFORK_ACCOUNTS)
                .map_err(|_| BlockValidationError::IncrementBalanceFailed)?
                .into_iter()
                .sum();

            // return balance to DAO beneficiary.
            *balance_increments.entry(DAO_HARDFORK_BENEFICIARY).or_default() += drained_balance;
        }
        // increment balances
        self.db()
            .increment_balances(balance_increments.into_iter().map(|(k, v)| (k, v)))
            .map_err(|_| BlockValidationError::IncrementBalanceFailed)?;

        Ok(())
    }

    /// Runs a single transaction in the configured environment and proceeds
    /// to return the result and state diff (without applying it).
    ///
    /// Assumes the rest of the block environment has been filled via `init_block_env`.
    pub fn transact(
        &mut self,
        transaction: &TransactionSigned,
        sender: Address,
    ) -> Result<ResultAndState, BlockExecutionError> {
        // Fill revm structure.
        fill_tx_env(&mut self.evm.env.tx, transaction, sender);

        let hash = transaction.hash();
        let out = if self.stack.should_inspect(&self.evm.env, hash) {
            // execution with inspector.
            let output = self.evm.inspect(&mut self.stack);
            tracing::trace!(
                target: "evm",
                ?hash, ?output, ?transaction, env = ?self.evm.env,
                "Executed transaction"
            );
            output
        } else {
            // main execution.
            self.evm.transact()
        };
        out.map_err(|e| BlockValidationError::EVM { hash, message: format!("{e:?}") }.into())
    }

    /// Runs the provided transactions and commits their state to the run-time database.
    ///
    /// The returned [PostState] can be used to persist the changes to disk, and contains the
    /// changes made by each transaction.
    ///
    /// The changes in [PostState] have a transition ID associated with them: there is one
    /// transition ID for each transaction (with the first executed tx having transition ID 0, and
    /// so on).
    ///
    /// The second returned value represents the total gas used by this block of transactions.
    pub fn execute_transactions(
        &mut self,
        block: &Block,
        total_difficulty: U256,
        senders: Option<Vec<Address>>,
    ) -> Result<u64, BlockExecutionError> {
        // perf: do not execute empty blocks
        if block.body.is_empty() {
            self.receipts.push(Vec::new());
            return Ok(0)
        }
        let senders = self.recover_senders(&block.body, senders)?;

        self.init_env(&block.header, total_difficulty);

        let mut cumulative_gas_used = 0;
        let mut receipts = Vec::with_capacity(block.body.len());
        for (transaction, sender) in block.body.iter().zip(senders) {
            let time = Instant::now();
            // The sum of the transaction’s gas limit, Tg, and the gas utilised in this block prior,
            // must be no greater than the block’s gasLimit.
            let block_available_gas = block.header.gas_limit - cumulative_gas_used;
            if transaction.gas_limit() > block_available_gas {
                return Err(BlockValidationError::TransactionGasLimitMoreThanAvailableBlockGas {
                    transaction_gas_limit: transaction.gas_limit(),
                    block_available_gas,
                }
                .into())
            }
            // Execute transaction.
            let ResultAndState { result, state } = self.transact(transaction, sender)?;
            trace!(
                target: "evm",
                ?transaction, ?result, ?state,
                "Executed transaction"
            );
            self.stats.execution_duration += time.elapsed();
            let time = Instant::now();

            self.db().commit(state);

            self.stats.apply_state_duration += time.elapsed();

            // append gas used
            cumulative_gas_used += result.gas_used();

            // Push transaction changeset and calculate header bloom filter for receipt.
            receipts.push(Receipt {
                tx_type: transaction.tx_type(),
                // Success flag was added in `EIP-658: Embedding transaction status code in
                // receipts`.
                success: result.is_success(),
                cumulative_gas_used,
                // convert to reth log
                logs: result.into_logs().into_iter().map(into_reth_log).collect(),
            });
        }
        self.receipts.push(receipts);

        Ok(cumulative_gas_used)
    }
}

impl<'a> BlockExecutor for EVMProcessor<'a> {
    fn execute(
        &mut self,
        block: &Block,
        total_difficulty: U256,
        senders: Option<Vec<Address>>,
    ) -> Result<(), BlockExecutionError> {
        let cumulative_gas_used = self.execute_transactions(block, total_difficulty, senders)?;

        // Check if gas used matches the value set in header.
        if block.gas_used != cumulative_gas_used {
            return Err(BlockValidationError::BlockGasUsed {
                got: cumulative_gas_used,
                expected: block.gas_used,
                receipts: self
                    .receipts
                    .last()
                    .map(|block_r| {
                        block_r
                            .iter()
                            .enumerate()
                            .map(|(id, tx_r)| (id as u64, tx_r.cumulative_gas_used))
                            .collect()
                    })
                    .unwrap_or_default(),
            }
            .into())
        }
        let time = Instant::now();
        self.post_execution_state_change(block, total_difficulty)?;
        self.stats.apply_post_execution_changes_duration += time.elapsed();

        let time = Instant::now();
        let with_reverts = self.tip.map_or(true, |tip| {
            !self.prune_modes.should_prune_account_history(block.number, tip) &&
                !self.prune_modes.should_prune_storage_history(block.number, tip)
        });
        self.db().merge_transitions(with_reverts);
        self.stats.merge_transitions_duration += time.elapsed();

        if self.first_block == 0 {
            self.first_block = block.number;
        }
        Ok(())
    }

    fn execute_and_verify_receipt(
        &mut self,
        block: &Block,
        total_difficulty: U256,
        senders: Option<Vec<Address>>,
    ) -> Result<(), BlockExecutionError> {
        // execute block
        self.execute(block, total_difficulty, senders)?;

        // TODO Before Byzantium, receipts contained state root that would mean that expensive
        // operation as hashing that is needed for state root got calculated in every
        // transaction This was replaced with is_success flag.
        // See more about EIP here: https://eips.ethereum.org/EIPS/eip-658
        if self.chain_spec.fork(Hardfork::Byzantium).active_at_block(block.header.number) {
            let time = Instant::now();
            if let Err(e) = verify_receipt(
                block.header.receipts_root,
                block.header.logs_bloom,
                self.receipts.last().unwrap().iter(),
            ) {
                debug!(
                    target = "sync",
                    "receipts verification failed {:?} receipts:{:?}",
                    e,
                    self.receipts.last().unwrap()
                );
                return Err(e)
            };
            self.stats.receipt_root_duration += time.elapsed();
        }

        Ok(())
    }

    fn set_prune_modes(&mut self, prune_modes: PruneModes) {
        self.prune_modes = prune_modes;
    }

    fn set_tip(&mut self, tip: BlockNumber) {
        self.tip = Some(tip);
    }

    fn take_output_state(&mut self) -> BundleState {
        let receipts = std::mem::take(&mut self.receipts);
        BundleState::new(self.evm.db().unwrap().take_bundle(), receipts, self.first_block)
    }

    fn stats(&self) -> BlockExecutorStats {
        self.stats.clone()
    }
}

/// Verify receipts
pub fn verify_receipt<'a>(
    expected_receipts_root: H256,
    expected_logs_bloom: Bloom,
    receipts: impl Iterator<Item = &'a Receipt> + Clone,
) -> Result<(), BlockExecutionError> {
    // Check receipts root.
    let receipts_with_bloom = receipts.map(|r| r.clone().into()).collect::<Vec<ReceiptWithBloom>>();
    let receipts_root = reth_primitives::proofs::calculate_receipt_root(&receipts_with_bloom);
    if receipts_root != expected_receipts_root {
        return Err(BlockValidationError::ReceiptRootDiff {
            got: receipts_root,
            expected: expected_receipts_root,
        }
        .into())
    }

    // Create header log bloom.
    let logs_bloom = receipts_with_bloom.iter().fold(Bloom::zero(), |bloom, r| bloom | r.bloom);
    if logs_bloom != expected_logs_bloom {
        return Err(BlockValidationError::BloomLogDiff {
            expected: Box::new(expected_logs_bloom),
            got: Box::new(logs_bloom),
        }
        .into())
    }

    Ok(())
}
