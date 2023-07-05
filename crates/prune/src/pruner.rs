//! Support for pruning.

use crate::PrunerError;
use futures_util::Stream;
use reth_primitives::BlockNumber;
use reth_provider::CanonStateNotification;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tracing::debug;

/// The future that returns the owned pipeline and the result of the pipeline run. See
/// [Pruner::run_as_fut].
pub type PrunerFut = Pin<Box<dyn Future<Output = PrunerWithResult> + Send>>;

/// The pipeline type itself with the result of [Pruner::run_as_fut]
pub type PrunerWithResult = (Pruner, Result<(), PrunerError>);

/// Pruning routine. Main pruning logic happens in [Pruner::run].
pub struct Pruner {
    /// Stream of canonical state notifications. Pruning is triggered by new incoming
    /// notifications.
    canon_state_stream: Box<dyn Stream<Item = CanonStateNotification> + Send + Unpin>,
    /// Minimum pruning interval measured in blocks. All prune parts are checked and, if needed,
    /// pruned, when the chain advances by the specified number of blocks.
    min_block_interval: u64,
    /// Maximum prune depth. Used to determine the pruning target for parts that are needed during
    /// the reorg, e.g. changesets.
    #[allow(dead_code)]
    max_prune_depth: u64,
    /// Last pruned block number. Used in conjunction with `min_block_interval` to determine
    /// when the pruning needs to be initiated.
    last_pruned_block_number: Option<BlockNumber>,
}

impl Pruner {
    /// Creates a new [Pruner].
    pub fn new(
        canon_state_stream: Box<dyn Stream<Item = CanonStateNotification> + Send + Unpin>,
        min_block_interval: u64,
        max_prune_depth: u64,
    ) -> Self {
        Self {
            canon_state_stream,
            min_block_interval,
            max_prune_depth,
            last_pruned_block_number: None,
        }
    }

    /// Consume the pruner and run it until it finishes.
    /// Return the pruner and its result as a future.
    #[track_caller]
    pub fn run_as_fut(mut self, tip_block_number: BlockNumber) -> PrunerFut {
        Box::pin(async move {
            let result = self.run(tip_block_number).await;
            (self, result)
        })
    }

    /// Run the pruner
    pub async fn run(&mut self, _tip_block_number: BlockNumber) -> Result<(), PrunerError> {
        // Pruning logic

        Ok(())
    }

    /// Drain canonical state stream to get the tip block number,
    /// and check against minimum pruning interval and last pruned block number.
    ///
    /// Returns `None` if either the stream is empty, or the minimum pruning interval check didn't
    /// pass.
    pub fn check_tip(&mut self, cx: &mut Context<'_>) -> Option<BlockNumber> {
        let mut latest_canon_state = None;
        while let Poll::Ready(Some(canon_state)) =
            Pin::new(&mut self.canon_state_stream).poll_next(cx)
        {
            latest_canon_state = Some(canon_state);
        }
        let latest_canon_state = latest_canon_state?;

        let tip = latest_canon_state.tip();
        let tip_block_number = tip.number;

        // Check minimum pruning interval according to the last pruned block and a new tip.
        // Saturating subtraction is needed for the case when `CanonStateNotification::Revert`
        // is received, meaning current block number might be less than the previously pruned
        // block number. If that's the case, no pruning is needed as outdated data is also
        // reverted.
        if self.last_pruned_block_number.map_or(true, |last_pruned_block_number| {
            tip_block_number.saturating_sub(last_pruned_block_number) >= self.min_block_interval
        }) {
            debug!(
                target: "pruner",
                last_pruned_block_number = ?self.last_pruned_block_number,
                %tip_block_number,
                "Minimum pruning interval reached"
            );
            self.last_pruned_block_number = Some(tip_block_number);
            Some(tip_block_number)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Pruner;
    use reth_primitives::SealedBlockWithSenders;
    use reth_provider::{test_utils::TestCanonStateSubscriptions, CanonStateSubscriptions, Chain};
    use std::{future::poll_fn, sync::Arc, task::Poll};

    #[tokio::test]
    async fn pruner_check_tip() {
        let mut canon_state_stream = TestCanonStateSubscriptions::default();
        let mut pruner = Pruner::new(Box::new(canon_state_stream.canonical_state_stream()), 5, 0);

        // Canonical state stream is empty
        poll_fn(|cx| {
            assert_eq!(pruner.check_tip(cx), None);
            Poll::Ready(())
        })
        .await;

        let mut chain = Chain::default();

        let first_block = SealedBlockWithSenders::default();
        let first_block_number = first_block.number;
        chain.blocks.insert(first_block_number, first_block);
        canon_state_stream.add_next_commit(Arc::new(chain.clone()));

        // No last pruned block number was set before
        poll_fn(|cx| {
            assert_eq!(pruner.check_tip(cx), Some(first_block_number));
            Poll::Ready(())
        })
        .await;

        canon_state_stream.add_next_commit(Arc::new(chain.clone()));
        let mut second_block = SealedBlockWithSenders::default();
        second_block.block.header.number = first_block_number + pruner.min_block_interval;
        let second_block_number = second_block.number;
        chain.blocks.insert(second_block_number, second_block);
        canon_state_stream.add_next_commit(Arc::new(chain.clone()));

        // Delta is larger than min block interval
        poll_fn(|cx| {
            assert_eq!(pruner.check_tip(cx), Some(second_block_number));
            Poll::Ready(())
        })
        .await;

        canon_state_stream.add_next_commit(Arc::new(chain.clone()));
        let mut third_block = SealedBlockWithSenders::default();
        third_block.block.header.number = second_block_number + 1;
        chain.blocks.insert(third_block.number, third_block);
        canon_state_stream.add_next_commit(Arc::new(chain.clone()));

        // Delta is smaller than min block interval
        poll_fn(|cx| {
            assert_eq!(pruner.check_tip(cx), None);
            Poll::Ready(())
        })
        .await;
    }
}
