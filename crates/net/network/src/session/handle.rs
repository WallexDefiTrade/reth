//! Session handles
use crate::{
    message::{Capabilities, CapabilityMessage},
    session::{Direction, SessionId},
    NodeId,
};
use reth_ecies::{stream::ECIESStream, ECIESError};
use std::{io, net::SocketAddr, sync::Arc, time::Instant};
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot},
};

/// A handler attached to a peer session that's not authenticated yet, pending Handshake and hello
/// message which exchanges the `capabilities` of the peer.
///
/// This session needs to wait until it is authenticated.
#[derive(Debug)]
pub(crate) struct PendingSessionHandle {
    /// Can be used to tell the session to disconnect the connection/abort the handshake process.
    pub(crate) disconnect_tx: oneshot::Sender<()>,
}

/// An established session with a remote peer.
///
/// Within an active session that supports the `Ethereum Wire Protocol `, three high-level tasks can
/// be performed: chain synchronization, block propagation and transaction exchange.
#[derive(Debug)]
pub(crate) struct ActiveSessionHandle {
    /// The assigned id for this session
    pub(crate) session_id: SessionId,
    /// The identifier of the remote peer
    pub(crate) remote_id: NodeId,
    /// The timestamp when the session has been established.
    pub(crate) established: Instant,
    /// Announced capabilities of the peer.
    pub(crate) capabilities: Arc<Capabilities>,
    /// Sender half of the command channel used send commands _to_ the spawned session
    pub(crate) commands: mpsc::Sender<SessionCommand>,
}

// === impl ActiveSessionHandle ===

impl ActiveSessionHandle {
    /// Sends a disconnect command to the session.
    pub(crate) fn disconnect(&self) {
        // Note: we clone the sender which ensures the channel has capacity to send the message
        let _ = self.commands.clone().try_send(SessionCommand::Disconnect);
    }
}

/// Events a pending session can produce.
///
/// This represents the state changes a session can undergo until it is ready to send capability messages <https://github.com/ethereum/devp2p/blob/6b0abc3d956a626c28dce1307ee9f546db17b6bd/rlpx.md>.
///
/// A session starts with a `Handshake`, followed by a `Hello` message which
#[derive(Debug)]
pub(crate) enum PendingSessionEvent {
    /// Initial handshake step was successful <https://github.com/ethereum/devp2p/blob/6b0abc3d956a626c28dce1307ee9f546db17b6bd/rlpx.md#initial-handshake>
    SuccessfulHandshake { remote_addr: SocketAddr, session_id: SessionId },
    /// Represents a successful `Hello` exchange: <https://github.com/ethereum/devp2p/blob/6b0abc3d956a626c28dce1307ee9f546db17b6bd/rlpx.md#hello-0x00>
    Hello {
        session_id: SessionId,
        node_id: NodeId,
        capabilities: Arc<Capabilities>,
        stream: ECIESStream<TcpStream>,
    },
    /// Handshake unsuccessful, session was disconnected.
    Disconnected {
        remote_addr: SocketAddr,
        session_id: SessionId,
        direction: Direction,
        error: Option<ECIESError>,
    },
    /// Thrown when unable to establish a [`TcpStream`].
    OutgoingConnectionError {
        remote_addr: SocketAddr,
        session_id: SessionId,
        node_id: NodeId,
        error: io::Error,
    },
    /// Thrown when authentication via Ecies failed.
    EciesAuthError { remote_addr: SocketAddr, session_id: SessionId, error: ECIESError },
}

/// Commands that can be sent to the spawned session.
#[derive(Debug)]
pub(crate) enum SessionCommand {
    /// Disconnect the connection
    Disconnect,
    Message(CapabilityMessage),
}

/// Message variants an active session can produce and send back to the
/// [`SessionManager`](crate::session::SessionManager)
#[derive(Debug)]
pub(crate) enum ActiveSessionMessage {
    /// Session disconnected.
    Closed { node_id: NodeId, remote_addr: SocketAddr },
    /// A session received a valid message via RLPx.
    ValidMessage {
        /// Identifier of the remote peer.
        node_id: NodeId,
        /// Message received from the peer.
        message: CapabilityMessage,
    },
    /// Received a message that does not match the announced capabilities of the peer.
    InvalidMessage {
        /// Identifier of the remote peer.
        node_id: NodeId,
        /// Announced capabilities of the remote peer.
        capabilities: Arc<Capabilities>,
        /// Message received from the peer.
        message: CapabilityMessage,
    },
}

/// A Cloneable connection for sending messages directly to the session of a peer.
#[derive(Debug, Clone)]
pub struct PeerMessageSender {
    /// id of the remote node.
    pub(crate) peer: NodeId,
    /// The Sender half connected to a session.
    pub(crate) to_session_tx: mpsc::Sender<CapabilityMessage>,
}
