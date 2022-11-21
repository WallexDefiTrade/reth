#![allow(clippy::result_large_err)]
#![warn(missing_docs, unreachable_pub)]
#![deny(unused_must_use, rust_2018_idioms)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! RLPx ECIES framed transport protocol.

pub mod algorithm;
pub mod mac;
pub mod stream;
pub mod util;

mod error;
pub use error::ECIESError;

mod codec;

use reth_primitives::H512 as PeerId;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Raw egress values for an ECIES protocol
pub enum EgressECIESValue {
    /// The AUTH message being sent out
    Auth,
    /// The ACK message being sent out
    Ack,
    /// The message being sent out (wrapped bytes)
    Message(bytes::Bytes),
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Raw ingress values for an ECIES protocol
pub enum IngressECIESValue {
    /// Receiving a message from a [`PeerId`]
    AuthReceive(PeerId),
    /// Receiving an ACK message
    Ack,
    /// Receiving a message
    Message(bytes::BytesMut),
}
