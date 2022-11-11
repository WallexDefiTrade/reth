use super::ProviderImpl;
use crate::{
    db::{tables, Database, DatabaseGAT, DbCursorRO, DbDupCursorRO, DbTx},
    provider::{Error, StateProvider, StateProviderFactory},
    Result,
};
use reth_primitives::{
    Account, Address, BlockHash, BlockNumber, Bytes, StorageKey, StorageValue, TxNumber, H256, U256,
};
use std::marker::PhantomData;

impl<DB: Database> StateProviderFactory for ProviderImpl<DB> {
    type HistorySP<'a> = StateProviderImplHistory<'a,<DB as DatabaseGAT<'a>>::TX> where Self: 'a;
    type LatestSP<'a> = StateProviderImplLatest<'a,<DB as DatabaseGAT<'a>>::TX> where Self: 'a;
    /// Storage provider for latest block
    fn latest(&self) -> Result<Self::LatestSP<'_>> {
        Ok(StateProviderImplLatest::new(self.db.tx()?))
    }

    fn history_by_block_number(&self, block_number: BlockNumber) -> Result<Self::HistorySP<'_>> {
        let tx = self.db.tx()?;
        // get block hash
        let block_hash = tx
            .get::<tables::CanonicalHeaders>(block_number)?
            .ok_or(Error::BlockNumberNotExists { block_number })?;

        // get transaction number
        let block_num_hash = (block_number, block_hash);
        let transaction_number = tx
            .get::<tables::CumulativeTxCount>(block_num_hash.into())?
            .ok_or(Error::BlockTxNumberNotExists { block_hash })?;

        Ok(StateProviderImplHistory::new(tx, transaction_number))
    }

    fn history_by_block_hash(&self, block_hash: BlockHash) -> Result<Self::HistorySP<'_>> {
        let tx = self.db.tx()?;
        // get block number
        let block_number = tx
            .get::<tables::HeaderNumbers>(block_hash)?
            .ok_or(Error::BlockHashNotExist { block_hash })?;

        // get transaction number
        let block_num_hash = (block_number, block_hash);
        let transaction_number = tx
            .get::<tables::CumulativeTxCount>(block_num_hash.into())?
            .ok_or(Error::BlockTxNumberNotExists { block_hash })?;

        Ok(StateProviderImplHistory::new(tx, transaction_number))
    }
}

/// State provider with given hash
pub struct StateProviderImplHistory<'a, TX: DbTx<'a>> {
    /// Transaction
    db: TX,
    /// Transaction number is main indexer of account and storage changes
    transaction_number: TxNumber,
    _phantom: PhantomData<&'a TX>,
}

impl<'a, TX: DbTx<'a>> StateProviderImplHistory<'a, TX> {
    /// Create new StateProvider from history transaction number
    pub fn new(db: TX, transaction_number: TxNumber) -> Self {
        Self { db, transaction_number, _phantom: PhantomData {} }
    }
}

impl<'a, TX: DbTx<'a>> StateProvider for StateProviderImplHistory<'a, TX> {
    /// Get storage.
    fn storage(&self, account: Address, storage_key: StorageKey) -> Result<Option<StorageValue>> {
        // TODO when StorageHistory is defined
        let transaction_number =
            self.db.get::<tables::StorageHistory>(Vec::new())?.map(|_integer_list|
            // TODO select integer that is one less from transaction_number
            self.transaction_number);

        if transaction_number.is_none() {
            return Ok(None)
        }
        let num = transaction_number.unwrap();
        let mut cursor = self.db.cursor_dup::<tables::StorageChangeSet>()?;

        if let Some((_, entry)) = cursor.seek_exact((num, account).into())? {
            if entry.key == storage_key {
                return Ok(Some(entry.value))
            }

            if let Some((_, entry)) = cursor.seek(storage_key)? {
                if entry.key == storage_key {
                    return Ok(Some(entry.value))
                }
            }
        }
        Ok(None)
    }

    /// Get basic account information.
    fn basic_account(&self, _address: Address) -> Result<Option<Account>> {
        // TODO add when AccountHistory is defined
        Ok(None)
    }

    /// Get account code by its hash
    fn bytecode_by_hash(&self, code_hash: H256) -> Result<Option<Bytes>> {
        self.db.get::<tables::Bytecodes>(code_hash).map_err(Into::into).map(|r| r.map(Bytes::from))
    }

    /// Get block hash by number.
    fn block_hash(&self, number: U256) -> Result<Option<H256>> {
        self.db.get::<tables::CanonicalHeaders>(number.as_u64()).map_err(Into::into)
    }
}

/// State Provider over latest state
pub struct StateProviderImplLatest<'a, TX: DbTx<'a>> {
    /// database transaction
    db: TX,
    /// Phantom data over lifetime
    phantom: PhantomData<&'a TX>,
}

impl<'a, TX: DbTx<'a>> StateProviderImplLatest<'a, TX> {
    /// Create new state provider
    pub fn new(db: TX) -> Self {
        Self { db, phantom: PhantomData {} }
    }
}

impl<'a, TX: DbTx<'a>> StateProvider for StateProviderImplLatest<'a, TX> {
    /// Get storage.
    fn storage(&self, account: Address, storage_key: StorageKey) -> Result<Option<StorageValue>> {
        let mut cursor = self.db.cursor_dup::<tables::PlainStorageState>()?;
        if let Some((_, entry)) = cursor.seek_exact(account)? {
            if entry.key == storage_key {
                return Ok(Some(entry.value))
            }

            if let Some((_, entry)) = cursor.seek(storage_key)? {
                if entry.key == storage_key {
                    return Ok(Some(entry.value))
                }
            }
        }
        Ok(None)
    }

    /// Get basic account information.
    fn basic_account(&self, address: Address) -> Result<Option<Account>> {
        self.db.get::<tables::PlainAccountState>(address).map_err(Into::into)
    }

    /// Get account code by its hash
    fn bytecode_by_hash(&self, code_hash: H256) -> Result<Option<Bytes>> {
        self.db.get::<tables::Bytecodes>(code_hash).map_err(Into::into).map(|r| r.map(Bytes::from))
    }

    /// Get block hash by number.
    fn block_hash(&self, number: U256) -> Result<Option<H256>> {
        self.db.get::<tables::CanonicalHeaders>(number.as_u64()).map_err(Into::into)
    }
}
