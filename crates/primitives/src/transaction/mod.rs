mod access_list;
mod signature;
mod tx_type;

use crate::{Address, Bytes, TxHash, U256};
pub use access_list::{AccessList, AccessListItem};
use bytes::Buf;
use ethers_core::utils::keccak256;
use reth_rlp::{length_of_length, Decodable, DecodeError, Encodable, Header, EMPTY_STRING_CODE};
pub use signature::Signature;
use std::ops::Deref;
pub use tx_type::TxType;

/// Raw Transaction.
/// Transaction type is introduced in EIP-2718: https://eips.ethereum.org/EIPS/eip-2718
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Transaction {
    /// Legacy transaciton.
    Legacy {
        /// Added as EIP-155: Simple replay attack protection
        chain_id: Option<u64>,
        /// A scalar value equal to the number of transactions sent by the sender; formally Tn.
        nonce: u64,
        /// A scalar value equal to the number of
        /// Wei to be paid per unit of gas for all computation
        /// costs incurred as a result of the execution of this transaction; formally Tp.
        gas_price: u64,
        /// A scalar value equal to the maximum
        /// amount of gas that should be used in executing
        /// this transaction. This is paid up-front, before any
        /// computation is done and may not be increased
        /// later; formally Tg.
        gas_limit: u64,
        /// The 160-bit address of the message call’s recipient or, for a contract creation
        /// transaction, ∅, used here to denote the only member of B0 ; formally Tt.
        to: TransactionKind,
        /// A scalar value equal to the number of Wei to
        /// be transferred to the message call’s recipient or,
        /// in the case of contract creation, as an endowment
        /// to the newly created account; formally Tv.
        value: U256,
        /// Input has two uses depending if transaction is Create or Call (if `to` field is None or
        /// Some). init: An unlimited size byte array specifying the
        /// EVM-code for the account initialisation procedure CREATE,
        /// data: An unlimited size byte array specifying the
        /// input data of the message call, formally Td.
        input: Bytes,
    },
    /// Transaction with AccessList. https://eips.ethereum.org/EIPS/eip-2930
    Eip2930 {
        /// Added as EIP-155: Simple replay attack protection
        chain_id: u64,
        /// A scalar value equal to the number of transactions sent by the sender; formally Tn.
        nonce: u64,
        /// A scalar value equal to the number of
        /// Wei to be paid per unit of gas for all computation
        /// costs incurred as a result of the execution of this transaction; formally Tp.
        gas_price: u64,
        /// A scalar value equal to the maximum
        /// amount of gas that should be used in executing
        /// this transaction. This is paid up-front, before any
        /// computation is done and may not be increased
        /// later; formally Tg.
        gas_limit: u64,
        /// The 160-bit address of the message call’s recipient or, for a contract creation
        /// transaction, ∅, used here to denote the only member of B0 ; formally Tt.
        to: TransactionKind,
        /// A scalar value equal to the number of Wei to
        /// be transferred to the message call’s recipient or,
        /// in the case of contract creation, as an endowment
        /// to the newly created account; formally Tv.
        value: U256,
        /// Input has two uses depending if transaction is Create or Call (if `to` field is None or
        /// Some). init: An unlimited size byte array specifying the
        /// EVM-code for the account initialisation procedure CREATE,
        /// data: An unlimited size byte array specifying the
        /// input data of the message call, formally Td.
        input: Bytes,
        /// The accessList specifies a list of addresses and storage keys;
        /// these addresses and storage keys are added into the `accessed_addresses`
        /// and `accessed_storage_keys` global sets (introduced in EIP-2929).
        /// A gas cost is charged, though at a discount relative to the cost of
        /// accessing outside the list.
        access_list: AccessList,
    },
    /// Transaction with priority fee. https://eips.ethereum.org/EIPS/eip-1559
    Eip1559 {
        /// Added as EIP-155: Simple replay attack protection
        chain_id: u64,
        /// A scalar value equal to the number of transactions sent by the sender; formally Tn.
        nonce: u64,
        /// A scalar value equal to the maximum
        /// amount of gas that should be used in executing
        /// this transaction. This is paid up-front, before any
        /// computation is done and may not be increased
        /// later; formally Tg.
        gas_limit: u64,
        /// A scalar value equal to the maximum
        /// amount of gas that should be used in executing
        /// this transaction. This is paid up-front, before any
        /// computation is done and may not be increased
        /// later; formally Tg.
        max_fee_per_gas: u64,
        /// Max Priority fee that transaction is paying
        max_priority_fee_per_gas: u64,
        /// The 160-bit address of the message call’s recipient or, for a contract creation
        /// transaction, ∅, used here to denote the only member of B0 ; formally Tt.
        to: TransactionKind,
        /// A scalar value equal to the number of Wei to
        /// be transferred to the message call’s recipient or,
        /// in the case of contract creation, as an endowment
        /// to the newly created account; formally Tv.
        value: U256,
        /// Input has two uses depending if transaction is Create or Call (if `to` field is None or
        /// Some). init: An unlimited size byte array specifying the
        /// EVM-code for the account initialisation procedure CREATE,
        /// data: An unlimited size byte array specifying the
        /// input data of the message call, formally Td.
        input: Bytes,
        /// The accessList specifies a list of addresses and storage keys;
        /// these addresses and storage keys are added into the `accessed_addresses`
        /// and `accessed_storage_keys` global sets (introduced in EIP-2929).
        /// A gas cost is charged, though at a discount relative to the cost of
        /// accessing outside the list.
        access_list: AccessList,
    },
}

impl Transaction {
    /// Heavy operation that return hash over rlp encoded transaction.
    /// It is only used for signature signing.
    pub fn signature_hash(&self) -> TxHash {
        let mut encoded = Vec::with_capacity(self.length());
        self.encode(&mut encoded);
        keccak256(encoded).into()
    }

    /// Sets the transaction's chain id to the provided value.
    pub fn set_chain_id(&mut self, chain_id: u64) {
        match self {
            Transaction::Legacy { chain_id: ref mut c, .. } => *c = Some(chain_id),
            Transaction::Eip2930 { chain_id: ref mut c, .. } => *c = chain_id,
            Transaction::Eip1559 { chain_id: ref mut c, .. } => *c = chain_id,
        }
    }

    /// Gets the transaction's [`TransactionKind`], which is the address of the recipient or
    /// [`TransactionKind::Create`] if the transaction is a contract creation.
    pub fn kind(&self) -> &TransactionKind {
        match self {
            Transaction::Legacy { to, .. } => to,
            Transaction::Eip2930 { to, .. } => to,
            Transaction::Eip1559 { to, .. } => to,
        }
    }

    /// Gets the transaction's value field.
    pub fn value(&self) -> &U256 {
        match self {
            Transaction::Legacy { value, .. } => value,
            Transaction::Eip2930 { value, .. } => value,
            Transaction::Eip1559 { value, .. } => value,
        }
    }

    /// Get the transaction's nonce.
    pub fn nonce(&self) -> u64 {
        match self {
            Transaction::Legacy { nonce, .. } => *nonce,
            Transaction::Eip2930 { nonce, .. } => *nonce,
            Transaction::Eip1559 { nonce, .. } => *nonce,
        }
    }

    /// Get the transaction's input field.
    pub fn input(&self) -> &Bytes {
        match self {
            Transaction::Legacy { input, .. } => input,
            Transaction::Eip2930 { input, .. } => input,
            Transaction::Eip1559 { input, .. } => input,
        }
    }

    /// Encodes individual transaction fields into the desired buffer, without a RLP header.
    pub(crate) fn encode_inner(&self, out: &mut dyn bytes::BufMut) {
        match self {
            Transaction::Legacy { .. } => self.encode_fields(out),
            Transaction::Eip2930 { .. } => {
                out.put_u8(1);
                let list_header = Header { list: true, payload_length: self.fields_len() };
                list_header.encode(out);
                self.encode_fields(out);
            }
            Transaction::Eip1559 { .. } => {
                out.put_u8(2);
                let list_header = Header { list: true, payload_length: self.fields_len() };
                list_header.encode(out);
                self.encode_fields(out);
            }
        }
    }

    /// Encodes EIP-155 arguments into the desired buffer. Only encodes values for legacy
    /// transactions.
    pub(crate) fn encode_eip155_fields(&self, out: &mut dyn bytes::BufMut) {
        // if this is a legacy transaction without a chain ID, it must be pre-EIP-155
        // and does not need to encode the chain ID for the signature hash encoding
        if let Transaction::Legacy { chain_id: Some(id), .. } = self {
            // EIP-155 encodes the chain ID and two zeroes
            id.encode(out);
            0x00u8.encode(out);
            0x00u8.encode(out);
        }
    }

    /// Outputs the length of EIP-155 fields. Only outputs a non-zero value for EIP-155 legacy
    /// transactions.
    pub(crate) fn eip155_fields_len(&self) -> usize {
        if let Transaction::Legacy { chain_id: Some(id), .. } = self {
            // EIP-155 encodes the chain ID and two zeroes, so we add 2 to the length of the chain
            // ID to get the length of all 3 fields
            // len(chain_id) + (0x00) + (0x00)
            id.length() + 2
        } else {
            // this is either a pre-EIP-155 legacy transaction or a typed transaction
            0
        }
    }

    /// Outputs the length of the transaction payload without the length of the RLP header or
    /// eip155 fields.
    pub(crate) fn payload_len(&self) -> usize {
        match self {
            Transaction::Legacy { .. } => self.fields_len(),
            _ => {
                let mut len = self.fields_len();
                // add list header length
                len += length_of_length(len);
                // add transaction type byte length
                len + 1
            }
        }
    }

    /// Outputs the length of the transaction's fields, without a RLP header or length of the
    /// eip155 fields.
    pub(crate) fn fields_len(&self) -> usize {
        match self {
            Transaction::Legacy { chain_id: _, nonce, gas_price, gas_limit, to, value, input } => {
                let mut len = 0;
                len += nonce.length();
                len += gas_price.length();
                len += gas_limit.length();
                len += to.length();
                len += value.length();
                len += input.0.length();
                len
            }
            Transaction::Eip2930 {
                chain_id,
                nonce,
                gas_price,
                gas_limit,
                to,
                value,
                input,
                access_list,
            } => {
                let mut len = 0;
                len += chain_id.length();
                len += nonce.length();
                len += gas_price.length();
                len += gas_limit.length();
                len += to.length();
                len += value.length();
                len += input.0.length();
                len += access_list.length();
                len
            }
            Transaction::Eip1559 {
                chain_id,
                nonce,
                gas_limit,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                to,
                value,
                input,
                access_list,
            } => {
                let mut len = 0;
                len += chain_id.length();
                len += nonce.length();
                len += max_priority_fee_per_gas.length();
                len += max_fee_per_gas.length();
                len += gas_limit.length();
                len += to.length();
                len += value.length();
                len += input.0.length();
                len += access_list.length();
                len
            }
        }
    }

    /// Encodes only the transaction's fields into the desired buffer, without a RLP header.
    pub(crate) fn encode_fields(&self, out: &mut dyn bytes::BufMut) {
        match self {
            Transaction::Legacy { chain_id: _, nonce, gas_price, gas_limit, to, value, input } => {
                nonce.encode(out);
                gas_price.encode(out);
                gas_limit.encode(out);
                to.encode(out);
                value.encode(out);
                input.0.encode(out);
            }
            Transaction::Eip2930 {
                chain_id,
                nonce,
                gas_price,
                gas_limit,
                to,
                value,
                input,
                access_list,
            } => {
                chain_id.encode(out);
                nonce.encode(out);
                gas_price.encode(out);
                gas_limit.encode(out);
                to.encode(out);
                value.encode(out);
                input.0.encode(out);
                access_list.encode(out);
            }
            Transaction::Eip1559 {
                chain_id,
                nonce,
                gas_limit,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                to,
                value,
                input,
                access_list,
            } => {
                chain_id.encode(out);
                nonce.encode(out);
                max_priority_fee_per_gas.encode(out);
                max_fee_per_gas.encode(out);
                gas_limit.encode(out);
                to.encode(out);
                value.encode(out);
                input.0.encode(out);
                access_list.encode(out);
            }
        }
    }
}

/// This encodes the transaction _without_ the signature, and is only suitable for creating a hash
/// intended for signing.
impl Encodable for Transaction {
    fn length(&self) -> usize {
        // TODO: fix
        let len = self.payload_len();
        len + length_of_length(len)
    }
    fn encode(&self, out: &mut dyn bytes::BufMut) {
        match self {
            Transaction::Legacy { .. } => {
                let header = Header {
                    list: true,
                    payload_length: self.payload_len() + self.eip155_fields_len(),
                };
                header.encode(out);
                self.encode_inner(out);
                self.encode_eip155_fields(out);
            }
            Transaction::Eip2930 { .. } => {
                self.encode_inner(out);
            }
            Transaction::Eip1559 { .. } => {
                self.encode_inner(out);
            }
        }
    }
}

/// Whether or not the transaction is a contract creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransactionKind {
    /// A transaction that creates a contract.
    Create,
    /// A transaction that calls a contract or transfer.
    Call(Address),
}

impl Encodable for TransactionKind {
    fn length(&self) -> usize {
        match self {
            TransactionKind::Call(to) => to.length(),
            TransactionKind::Create => 1, // EMPTY_STRING_CODE is a single byte
        }
    }
    fn encode(&self, out: &mut dyn reth_rlp::BufMut) {
        match self {
            TransactionKind::Call(to) => to.encode(out),
            TransactionKind::Create => out.put_u8(EMPTY_STRING_CODE),
        }
    }
}

impl Decodable for TransactionKind {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if let Some(&first) = buf.first() {
            if first == EMPTY_STRING_CODE {
                buf.advance(1);
                Ok(TransactionKind::Create)
            } else {
                let addr = <Address as Decodable>::decode(buf)?;
                Ok(TransactionKind::Call(addr))
            }
        } else {
            Err(DecodeError::InputTooShort)
        }
    }
}

/// Signed transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionSigned {
    transaction: Transaction,
    hash: TxHash,
    signature: Signature,
}

impl AsRef<Transaction> for TransactionSigned {
    fn as_ref(&self) -> &Transaction {
        &self.transaction
    }
}

impl Deref for TransactionSigned {
    type Target = Transaction;

    fn deref(&self) -> &Self::Target {
        &self.transaction
    }
}

impl Encodable for TransactionSigned {
    fn length(&self) -> usize {
        let len = self.payload_len();

        // add the length of the RLP header
        len + length_of_length(len)
    }
    fn encode(&self, out: &mut dyn bytes::BufMut) {
        if let Transaction::Legacy { chain_id, .. } = self.transaction {
            let header = Header { list: true, payload_length: self.payload_len() };
            header.encode(out);
            self.transaction.encode_fields(out);

            if let Some(id) = chain_id {
                self.signature.encode_eip155_inner(out, id);
            } else {
                // if the transaction has no chain id then it is a pre-EIP-155 transaction
                self.signature.encode_inner_legacy(out);
            }
        } else {
            let header = Header { list: false, payload_length: self.payload_len() };
            header.encode(out);
            match self.transaction {
                Transaction::Eip2930 { .. } => {
                    out.put_u8(1);
                    let list_header = Header { list: true, payload_length: self.inner_tx_len() };
                    list_header.encode(out);
                }
                Transaction::Eip1559 { .. } => {
                    out.put_u8(2);
                    let list_header = Header { list: true, payload_length: self.inner_tx_len() };
                    list_header.encode(out);
                }
                Transaction::Legacy { .. } => {
                    unreachable!("Legacy transaction should be handled above")
                }
            }

            self.transaction.encode_fields(out);
            self.signature.odd_y_parity.encode(out);
            self.signature.r.encode(out);
            self.signature.s.encode(out);
        }
    }
}

/// This `Decodable` implementation only supports decoding the transaction format sent over p2p.
impl Decodable for TransactionSigned {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        // keep this around so we can use it to calculate the hash
        let original_encoding = *buf;

        let first_header = Header::decode(buf)?;
        // if the transaction is encoded as a string then it is a typed transaction
        if !first_header.list {
            let tx_type = *buf
                .first()
                .ok_or(DecodeError::Custom("typed tx cannot be decoded from an empty slice"))?;
            buf.advance(1);
            // decode the list header for the rest of the transaction
            let header = Header::decode(buf)?;
            if !header.list {
                return Err(DecodeError::Custom("typed tx fields must be encoded as a list"))
            }

            // decode common fields
            let transaction = match tx_type {
                1 => Transaction::Eip2930 {
                    chain_id: Decodable::decode(buf)?,
                    nonce: Decodable::decode(buf)?,
                    gas_price: Decodable::decode(buf)?,
                    gas_limit: Decodable::decode(buf)?,
                    to: Decodable::decode(buf)?,
                    value: Decodable::decode(buf)?,
                    input: Bytes(Decodable::decode(buf)?),
                    access_list: Decodable::decode(buf)?,
                },
                2 => Transaction::Eip1559 {
                    chain_id: Decodable::decode(buf)?,
                    nonce: Decodable::decode(buf)?,
                    max_priority_fee_per_gas: Decodable::decode(buf)?,
                    max_fee_per_gas: Decodable::decode(buf)?,
                    gas_limit: Decodable::decode(buf)?,
                    to: Decodable::decode(buf)?,
                    value: Decodable::decode(buf)?,
                    input: Bytes(Decodable::decode(buf)?),
                    access_list: Decodable::decode(buf)?,
                },
                _ => return Err(DecodeError::Custom("unsupported typed transaction type")),
            };

            let signature = Signature {
                odd_y_parity: Decodable::decode(buf)?,
                r: Decodable::decode(buf)?,
                s: Decodable::decode(buf)?,
            };

            let mut signed = TransactionSigned { transaction, hash: Default::default(), signature };
            let tx_length = first_header.payload_length + first_header.length();
            signed.hash = keccak256(&original_encoding[..tx_length]).into();
            Ok(signed)
        } else {
            let mut transaction = Transaction::Legacy {
                nonce: Decodable::decode(buf)?,
                gas_price: Decodable::decode(buf)?,
                gas_limit: Decodable::decode(buf)?,
                to: Decodable::decode(buf)?,
                value: Decodable::decode(buf)?,
                input: Bytes(Decodable::decode(buf)?),
                chain_id: None,
            };
            let (signature, extracted_id) = Signature::decode_eip155_inner(buf)?;
            if let Some(id) = extracted_id {
                transaction.set_chain_id(id);
            }

            let mut signed = TransactionSigned { transaction, hash: Default::default(), signature };
            let tx_length = first_header.payload_length + first_header.length();
            signed.hash = keccak256(&original_encoding[..tx_length]).into();
            Ok(signed)
        }
    }
}

impl TransactionSigned {
    /// Transaction signature.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Transaction hash. Used to identify transaction.
    pub fn hash(&self) -> TxHash {
        self.hash
    }

    /// Create a new signed transaction from a transaction and its signature.
    /// This will also calculate the transaction hash using its encoding.
    pub fn from_transaction_and_signature(transaction: Transaction, signature: Signature) -> Self {
        let mut initial_tx = Self { transaction, hash: Default::default(), signature };
        let mut buf = Vec::new();
        initial_tx.encode(&mut buf);
        initial_tx.hash = keccak256(&buf).into();
        initial_tx
    }

    /// Output the length of the inner transaction and signature fields.
    pub(crate) fn inner_tx_len(&self) -> usize {
        let mut len = self.transaction.fields_len();
        if let Transaction::Legacy { chain_id, .. } = self.transaction {
            if let Some(id) = chain_id {
                len += self.signature.eip155_payload_len(id);
            } else {
                // if the transaction has no chain id then it is a pre-EIP-155 transaction
                len += self.signature.payload_len_legacy();
            }
        } else {
            len += self.signature.odd_y_parity.length();
            len += self.signature.r.length();
            len += self.signature.s.length();
        }
        len
    }

    /// Output the length of the signed transaction's rlp payload without a rlp header.
    pub(crate) fn payload_len(&self) -> usize {
        let mut len = self.inner_tx_len();
        if let Transaction::Legacy { .. } = self.transaction {
            len
        } else {
            // length of the list header
            len += Header { list: true, payload_length: len }.length();
            // add type byte
            len + 1
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::{
        transaction::{signature::Signature, TransactionKind},
        Address, Transaction, TransactionSigned, H256, U256,
    };
    use bytes::BytesMut;
    use ethers_core::{types::Bytes, utils::hex};
    use reth_rlp::{Decodable, Encodable};

    #[test]
    fn test_decode_create() {
        // panic!("not implemented");
        // tests that a contract creation tx encodes and decodes properly
        let request = Transaction::Eip2930 {
            chain_id: 1u64,
            nonce: 0,
            gas_price: 1,
            gas_limit: 2,
            to: TransactionKind::Create,
            value: U256::from(3),
            input: Bytes::from(vec![1, 2]),
            access_list: Default::default(),
        };
        let signature = Signature { odd_y_parity: true, r: U256::default(), s: U256::default() };
        let tx = TransactionSigned::from_transaction_and_signature(request, signature);

        let mut encoded = BytesMut::new();
        tx.encode(&mut encoded);

        let decoded = TransactionSigned::decode(&mut &*encoded).unwrap();
        assert_eq!(decoded, tx);
    }

    #[test]
    fn test_decode_create_goerli() {
        // test that an example create tx from goerli decodes properly
        let tx_bytes =
              hex::decode("b901f202f901ee05228459682f008459682f11830209bf8080b90195608060405234801561001057600080fd5b50610175806100206000396000f3fe608060405234801561001057600080fd5b506004361061002b5760003560e01c80630c49c36c14610030575b600080fd5b61003861004e565b604051610045919061011d565b60405180910390f35b60606020600052600f6020527f68656c6c6f2073746174656d696e64000000000000000000000000000000000060405260406000f35b600081519050919050565b600082825260208201905092915050565b60005b838110156100be5780820151818401526020810190506100a3565b838111156100cd576000848401525b50505050565b6000601f19601f8301169050919050565b60006100ef82610084565b6100f9818561008f565b93506101098185602086016100a0565b610112816100d3565b840191505092915050565b6000602082019050818103600083015261013781846100e4565b90509291505056fea264697066735822122051449585839a4ea5ac23cae4552ef8a96b64ff59d0668f76bfac3796b2bdbb3664736f6c63430008090033c080a0136ebffaa8fc8b9fda9124de9ccb0b1f64e90fbd44251b4c4ac2501e60b104f9a07eb2999eec6d185ef57e91ed099afb0a926c5b536f0155dd67e537c7476e1471")
                  .unwrap();
        let _decoded = TransactionSigned::decode(&mut &tx_bytes[..]).unwrap();
    }

    #[test]
    fn test_decode_call() {
        let request = Transaction::Eip2930 {
            chain_id: 1u64,
            nonce: 0,
            gas_price: 1,
            gas_limit: 2,
            to: TransactionKind::Call(Address::default()),
            value: U256::from(3),
            input: Bytes::from(vec![1, 2]),
            access_list: Default::default(),
        };

        let signature = Signature { odd_y_parity: true, r: U256::default(), s: U256::default() };

        let tx = TransactionSigned::from_transaction_and_signature(request, signature);

        let mut encoded = BytesMut::new();
        tx.encode(&mut encoded);

        let decoded = TransactionSigned::decode(&mut &*encoded).unwrap();
        assert_eq!(decoded, tx);
    }

    #[test]
    fn decode_transaction_consumes_buffer() {
        let bytes = &mut &hex::decode("b87502f872041a8459682f008459682f0d8252089461815774383099e24810ab832a5b2a5425c154d58829a2241af62c000080c001a059e6b67f48fb32e7e570dfb11e042b5ad2e55e3ce3ce9cd989c7e06e07feeafda0016b83f4f980694ed2eee4d10667242b1f40dc406901b34125b008d334d47469").unwrap()[..];
        let _transaction_res = TransactionSigned::decode(bytes).unwrap();
        assert_eq!(
            bytes.len(),
            0,
            "did not consume all bytes in the buffer, {:?} remaining",
            bytes.len()
        );
    }

    #[test]
    fn decode_multiple_network_txs() {
        let bytes_first = &mut &hex::decode("f86b02843b9aca00830186a094d3e8763675e4c425df46cc3b5c0f6cbdac39604687038d7ea4c68000802ba00eb96ca19e8a77102767a41fc85a36afd5c61ccb09911cec5d3e86e193d9c5aea03a456401896b1b6055311536bf00a718568c744d8c1f9df59879e8350220ca18").unwrap()[..];
        let expected_request = Transaction::Legacy {
            chain_id: Some(4u64),
            nonce: 2,
            gas_price: 1000000000,
            gas_limit: 100000,
            to: TransactionKind::Call(
                Address::from_str("d3e8763675e4c425df46cc3b5c0f6cbdac396046").unwrap(),
            ),
            value: U256::from(1000000000000000u64),
            input: Bytes::default(),
        };
        let expected_signature = Signature {
            odd_y_parity: false,
            r: U256::from_str("eb96ca19e8a77102767a41fc85a36afd5c61ccb09911cec5d3e86e193d9c5ae")
                .unwrap(),
            s: U256::from_str("3a456401896b1b6055311536bf00a718568c744d8c1f9df59879e8350220ca18")
                .unwrap(),
        };
        let expected =
            TransactionSigned::from_transaction_and_signature(expected_request, expected_signature);
        assert_eq!(expected, TransactionSigned::decode(bytes_first).unwrap());
        assert_eq!(
            expected.hash,
            H256::from_str("0xa517b206d2223278f860ea017d3626cacad4f52ff51030dc9a96b432f17f8d34")
                .unwrap()
        );

        let bytes_second = &mut &hex::decode("f86b01843b9aca00830186a094d3e8763675e4c425df46cc3b5c0f6cbdac3960468702769bb01b2a00802ba0e24d8bd32ad906d6f8b8d7741e08d1959df021698b19ee232feba15361587d0aa05406ad177223213df262cb66ccbb2f46bfdccfdfbbb5ffdda9e2c02d977631da").unwrap()[..];
        let expected_request = Transaction::Legacy {
            chain_id: Some(4),
            nonce: 1u64,
            gas_price: 1000000000u64,
            gas_limit: 100000u64,
            to: TransactionKind::Call(Address::from_slice(
                &hex::decode("d3e8763675e4c425df46cc3b5c0f6cbdac396046").unwrap()[..],
            )),
            value: 693361000000000u64.into(),
            input: Default::default(),
        };
        let expected_signature = Signature {
            odd_y_parity: false,
            r: U256::from_str("e24d8bd32ad906d6f8b8d7741e08d1959df021698b19ee232feba15361587d0a")
                .unwrap(),
            s: U256::from_str("5406ad177223213df262cb66ccbb2f46bfdccfdfbbb5ffdda9e2c02d977631da")
                .unwrap(),
        };

        let expected =
            TransactionSigned::from_transaction_and_signature(expected_request, expected_signature);
        assert_eq!(expected, TransactionSigned::decode(bytes_second).unwrap());

        let bytes_third = &mut &hex::decode("f86b0384773594008398968094d3e8763675e4c425df46cc3b5c0f6cbdac39604687038d7ea4c68000802ba0ce6834447c0a4193c40382e6c57ae33b241379c5418caac9cdc18d786fd12071a03ca3ae86580e94550d7c071e3a02eadb5a77830947c9225165cf9100901bee88").unwrap()[..];
        let expected_request = Transaction::Legacy {
            chain_id: Some(4),
            nonce: 3,
            gas_price: 2000000000,
            gas_limit: 10000000,
            to: TransactionKind::Call(Address::from_slice(
                &hex::decode("d3e8763675e4c425df46cc3b5c0f6cbdac396046").unwrap()[..],
            )),
            value: 1000000000000000u64.into(),
            input: Bytes::default(),
        };

        let expected_signature = Signature {
            odd_y_parity: false,
            r: U256::from_str("ce6834447c0a4193c40382e6c57ae33b241379c5418caac9cdc18d786fd12071")
                .unwrap(),
            s: U256::from_str("3ca3ae86580e94550d7c071e3a02eadb5a77830947c9225165cf9100901bee88")
                .unwrap(),
        };

        let expected =
            TransactionSigned::from_transaction_and_signature(expected_request, expected_signature);
        assert_eq!(expected, TransactionSigned::decode(bytes_third).unwrap());

        let bytes_fourth = &mut &hex::decode("b87502f872041a8459682f008459682f0d8252089461815774383099e24810ab832a5b2a5425c154d58829a2241af62c000080c001a059e6b67f48fb32e7e570dfb11e042b5ad2e55e3ce3ce9cd989c7e06e07feeafda0016b83f4f980694ed2eee4d10667242b1f40dc406901b34125b008d334d47469").unwrap()[..];
        let expected = Transaction::Eip1559 {
            chain_id: 4,
            nonce: 26,
            max_priority_fee_per_gas: 1500000000,
            max_fee_per_gas: 1500000013,
            gas_limit: 21000,
            to: TransactionKind::Call(Address::from_slice(
                &hex::decode("61815774383099e24810ab832a5b2a5425c154d5").unwrap()[..],
            )),
            value: 3000000000000000000u64.into(),
            input: Default::default(),
            access_list: Default::default(),
        };

        let expected_signature = Signature {
            odd_y_parity: true,
            r: U256::from_str("59e6b67f48fb32e7e570dfb11e042b5ad2e55e3ce3ce9cd989c7e06e07feeafd")
                .unwrap(),
            s: U256::from_str("016b83f4f980694ed2eee4d10667242b1f40dc406901b34125b008d334d47469")
                .unwrap(),
        };

        let expected =
            TransactionSigned::from_transaction_and_signature(expected, expected_signature);
        assert_eq!(expected, TransactionSigned::decode(bytes_fourth).unwrap());

        let bytes_fifth = &mut &hex::decode("f8650f84832156008287fb94cf7f9e66af820a19257a2108375b180b0ec491678204d2802ca035b7bfeb9ad9ece2cbafaaf8e202e706b4cfaeb233f46198f00b44d4a566a981a0612638fb29427ca33b9a3be2a0a561beecfe0269655be160d35e72d366a6a860").unwrap()[..];
        let expected = Transaction::Legacy {
            chain_id: Some(4),
            nonce: 15,
            gas_price: 2200000000,
            gas_limit: 34811,
            to: TransactionKind::Call(Address::from_slice(
                &hex::decode("cf7f9e66af820a19257a2108375b180b0ec49167").unwrap()[..],
            )),
            value: 1234u64.into(),
            input: Bytes::default(),
        };
        let signature = Signature {
            odd_y_parity: true,
            r: U256::from_str("35b7bfeb9ad9ece2cbafaaf8e202e706b4cfaeb233f46198f00b44d4a566a981")
                .unwrap(),
            s: U256::from_str("612638fb29427ca33b9a3be2a0a561beecfe0269655be160d35e72d366a6a860")
                .unwrap(),
        };

        let expected = TransactionSigned::from_transaction_and_signature(expected, signature);
        assert_eq!(expected, TransactionSigned::decode(bytes_fifth).unwrap());
    }
}
