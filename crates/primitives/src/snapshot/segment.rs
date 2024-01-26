use crate::{
    snapshot::{Compression, Filters, InclusionFilter},
    BlockNumber, TxNumber,
};
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::{ops::RangeInclusive, str::FromStr};
use strum::{AsRefStr, EnumIter, EnumString};

#[derive(
    Debug,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Ord,
    PartialOrd,
    Deserialize,
    Serialize,
    EnumString,
    EnumIter,
    AsRefStr,
    Display,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
/// Segment of the data that can be snapshotted.
pub enum SnapshotSegment {
    #[strum(serialize = "headers")]
    /// Snapshot segment responsible for the `CanonicalHeaders`, `Headers`, `HeaderTD` tables.
    Headers,
    #[strum(serialize = "transactions")]
    /// Snapshot segment responsible for the `Transactions` table.
    Transactions,
    #[strum(serialize = "receipts")]
    /// Snapshot segment responsible for the `Receipts` table.
    Receipts,
}

impl SnapshotSegment {
    /// Returns the default configuration of the segment.
    pub const fn config(&self) -> SegmentConfig {
        let default_config = SegmentConfig {
            filters: Filters::WithFilters(
                InclusionFilter::Cuckoo,
                super::PerfectHashingFunction::Fmph,
            ),
            compression: Compression::Lz4,
        };

        match self {
            SnapshotSegment::Headers => default_config,
            SnapshotSegment::Transactions => default_config,
            SnapshotSegment::Receipts => default_config,
        }
    }

    /// Returns the number of columns for the segment
    pub const fn columns(&self) -> usize {
        match self {
            SnapshotSegment::Headers => 3,
            SnapshotSegment::Transactions => 1,
            SnapshotSegment::Receipts => 1,
        }
    }

    /// Returns the default file name for the provided [`SegmentHeader`]
    pub fn filename_from_header(&self, header: SegmentHeader) -> String {
        self.filename(&header.block_range)
    }

    /// Returns the default file name for the provided segment and range.
    pub fn filename(&self, block_range: &RangeInclusive<BlockNumber>) -> String {
        // ATTENTION: if changing the name format, be sure to reflect those changes in
        // [`Self::parse_filename`].
        format!("snapshot_{}_{}_{}", self.as_ref(), block_range.start(), block_range.end())
    }

    /// Returns file name for the provided segment and range, alongisde filters, compression.
    pub fn filename_with_configuration(
        &self,
        filters: Filters,
        compression: Compression,
        block_range: &RangeInclusive<BlockNumber>,
    ) -> String {
        let prefix = self.filename(block_range);

        let filters_name = match filters {
            Filters::WithFilters(inclusion_filter, phf) => {
                format!("{}-{}", inclusion_filter.as_ref(), phf.as_ref())
            }
            Filters::WithoutFilters => "none".to_string(),
        };

        // ATTENTION: if changing the name format, be sure to reflect those changes in
        // [`Self::parse_filename`.]
        format!("{prefix}_{}_{}", filters_name, compression.as_ref())
    }

    /// Parses a filename into a `SnapshotSegment` and its expected block range.
    ///
    /// The filename is expected to follow the format:
    /// "snapshot_{segment}_{block_start}_{block_end}". This function checks
    /// for the correct prefix ("snapshot"), and then parses the segment and the inclusive
    /// ranges for blocks. It ensures that the start of each range is less than or equal to the
    /// end.
    ///
    /// # Returns
    /// - `Some((segment, block_range))` if parsing is successful and all conditions are met.
    /// - `None` if any condition fails, such as an incorrect prefix, parsing error, or invalid
    ///   range.
    ///
    /// # Note
    /// This function is tightly coupled with the naming convention defined in [`Self::filename`].
    /// Any changes in the filename format in `filename` should be reflected here.
    pub fn parse_filename(name: &str) -> Option<(Self, RangeInclusive<BlockNumber>)> {
        let mut parts = name.split('_');
        if parts.next() != Some("snapshot") {
            return None
        }

        let segment = Self::from_str(parts.next()?).ok()?;
        let (block_start, block_end) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);

        if block_start > block_end {
            return None
        }

        Some((segment, block_start..=block_end))
    }
}

/// A segment header that contains information common to all segments. Used for storage.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct SegmentHeader {
    /// Block range of the snapshot segment
    block_range: RangeInclusive<BlockNumber>,
    /// Transaction range of the snapshot segment
    tx_range: Option<RangeInclusive<TxNumber>>,
    /// Segment type
    segment: SnapshotSegment,
}

impl SegmentHeader {
    /// Returns [`SegmentHeader`].
    pub fn new(
        block_range: RangeInclusive<BlockNumber>,
        tx_range: Option<RangeInclusive<TxNumber>>,
        segment: SnapshotSegment,
    ) -> Self {
        Self { block_range, tx_range, segment }
    }

    /// Returns the snapshot segment kind.
    pub fn segment(&self) -> SnapshotSegment {
        self.segment
    }

    /// Returns the block range.
    pub fn block_range(&self) -> RangeInclusive<BlockNumber> {
        self.block_range.clone()
    }

    /// Returns the transaction range.
    pub fn tx_range(&self) -> Option<RangeInclusive<TxNumber>> {
        self.tx_range.clone()
    }

    /// Returns the first block number of the segment.
    pub fn block_start(&self) -> BlockNumber {
        *self.block_range.start()
    }

    /// Returns the last block number of the segment.
    pub fn block_end(&self) -> BlockNumber {
        *self.block_range.end()
    }

    /// Returns the first transaction number of the segment.  
    ///  
    /// ### Panics
    ///
    /// This method panics if `self.tx_range` is `None`.
    pub fn tx_start(&self) -> TxNumber {
        *self.tx_range.as_ref().expect("should exist").start()
    }

    /// Returns the last transaction number of the segment.   
    ///
    /// ### Panics
    ///
    /// This method panics if `self.tx_range` is `None`.
    #[track_caller]
    pub fn tx_end(&self) -> TxNumber {
        *self.tx_range.as_ref().expect("should exist").end()
    }

    /// Number of transactions.  
    ///
    /// ### Panics
    ///
    /// This method panics if `self.tx_range` is `None`.
    #[track_caller]
    pub fn tx_len(&self) -> u64 {
        self.tx_range.as_ref().expect("should exist").end() + 1 -
            self.tx_range.as_ref().expect("should exist").start()
    }

    /// Number of blocks.
    pub fn block_len(&self) -> u64 {
        self.block_range.end() + 1 - self.block_range.start()
    }

    /// Increments block end range depending on segment
    pub fn increment_block(&mut self) {
        self.block_range = *self.block_range.start()..=*self.block_range.end() + 1;
    }

    /// Increments tx end range depending on segment
    pub fn increment_tx(&mut self) {
        match self.segment {
            SnapshotSegment::Headers => (),
            SnapshotSegment::Transactions | SnapshotSegment::Receipts => {
                if let Some(tx_range) = &mut self.tx_range {
                    *tx_range = *tx_range.start()..=*tx_range.end() + 1;
                } else {
                    self.tx_range = Some(0..=0);
                }
            }
        }
    }

    /// Removes `num` elements from end of tx or block range.
    pub fn prune(&mut self, num: u64) {
        match self.segment {
            SnapshotSegment::Headers => {
                self.block_range =
                    *self.block_range.start()..=self.block_range.end().saturating_sub(num)
            }
            SnapshotSegment::Transactions | SnapshotSegment::Receipts => {
                self.tx_range = self.tx_range.as_ref().and_then(|tx_range| {
                    if num > *tx_range.end() {
                        return None
                    }
                    Some(*tx_range.start()..=tx_range.end().saturating_sub(num))
                });
            }
        };
    }

    /// Sets a new block_range.
    pub fn set_block_range(&mut self, block_range: RangeInclusive<BlockNumber>) {
        self.block_range = block_range;
    }

    /// Sets a new tx_range.
    pub fn set_tx_range(&mut self, tx_range: RangeInclusive<TxNumber>) {
        self.tx_range = Some(tx_range);
    }

    /// Returns the row offset which depends on whether the segment is block or transaction based.
    pub fn start(&self) -> u64 {
        match self.segment {
            SnapshotSegment::Headers => self.block_start(),
            SnapshotSegment::Transactions | SnapshotSegment::Receipts => self.tx_start(),
        }
    }
}

/// Configuration used on the segment.
#[derive(Debug, Clone, Copy)]
pub struct SegmentConfig {
    /// Inclusion filters used on the segment
    pub filters: Filters,
    /// Compression used on the segment
    pub compression: Compression,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filename() {
        let test_vectors = [
            (SnapshotSegment::Headers, 2..=30, "snapshot_headers_2_30", None),
            (SnapshotSegment::Receipts, 30..=300, "snapshot_receipts_30_300", None),
            (
                SnapshotSegment::Transactions,
                1_123_233..=11_223_233,
                "snapshot_transactions_1123233_11223233",
                None,
            ),
            (
                SnapshotSegment::Headers,
                2..=30,
                "snapshot_headers_2_30_cuckoo-fmph_lz4",
                Some((
                    Compression::Lz4,
                    Filters::WithFilters(
                        InclusionFilter::Cuckoo,
                        crate::snapshot::PerfectHashingFunction::Fmph,
                    ),
                )),
            ),
            (
                SnapshotSegment::Headers,
                2..=30,
                "snapshot_headers_2_30_cuckoo-fmph_zstd",
                Some((
                    Compression::Zstd,
                    Filters::WithFilters(
                        InclusionFilter::Cuckoo,
                        crate::snapshot::PerfectHashingFunction::Fmph,
                    ),
                )),
            ),
            (
                SnapshotSegment::Headers,
                2..=30,
                "snapshot_headers_2_30_cuckoo-fmph_zstd-dict",
                Some((
                    Compression::ZstdWithDictionary,
                    Filters::WithFilters(
                        InclusionFilter::Cuckoo,
                        crate::snapshot::PerfectHashingFunction::Fmph,
                    ),
                )),
            ),
        ];

        for (segment, block_range, filename, configuration) in test_vectors {
            if let Some((compression, filters)) = configuration {
                assert_eq!(
                    segment.filename_with_configuration(filters, compression, &block_range,),
                    filename
                );
            } else {
                assert_eq!(segment.filename(&block_range), filename);
            }

            assert_eq!(SnapshotSegment::parse_filename(filename), Some((segment, block_range)));
        }

        assert_eq!(SnapshotSegment::parse_filename("snapshot_headers_2"), None);
        assert_eq!(SnapshotSegment::parse_filename("snapshot_headers_"), None);
    }
}
