//! Fetch data from the network.

use crate::{message::BlockRequest, peers::ReputationChange};
use futures::StreamExt;
use reth_eth_wire::{BlockBody, GetBlockBodies};
use reth_interfaces::p2p::{
    error::{RequestError, RequestResult},
    headers::client::HeadersRequest,
};
use reth_primitives::{Header, PeerId, H256};
use std::{
    collections::{HashMap, VecDeque},
    task::{Context, Poll},
    time::Instant,
};
use tokio::sync::{mpsc, mpsc::UnboundedSender, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Manages data fetching operations.
///
/// This type is hooked into the staged sync pipeline and delegates download request to available
/// peers and sends the response once ready.
pub struct StateFetcher {
    /// Currently active [`GetBlockHeaders`] requests
    inflight_headers_requests: HashMap<PeerId, Request<HeadersRequest, RequestResult<Vec<Header>>>>,
    /// Currently active [`GetBlockBodies`] requests
    inflight_bodies_requests: HashMap<PeerId, Request<Vec<H256>, RequestResult<Vec<BlockBody>>>>,
    /// The list of available peers for requests.
    peers: HashMap<PeerId, Peer>,
    /// Requests queued for processing
    queued_requests: VecDeque<DownloadRequest>,
    /// Receiver for new incoming download requests
    download_requests_rx: UnboundedReceiverStream<DownloadRequest>,
    /// Sender for download requests, used to detach a [`HeadersDownloader`]
    download_requests_tx: UnboundedSender<DownloadRequest>,
}

// === impl StateSyncer ===

impl StateFetcher {
    /// Invoked when connected to a new peer.
    pub(crate) fn new_connected_peer(
        &mut self,
        peer_id: PeerId,
        best_hash: H256,
        best_number: Option<u64>,
    ) {
        self.peers.insert(peer_id, Peer { state: PeerState::Idle, best_hash, best_number });
    }

    /// Invoked when an active session was closed.
    ///
    /// This cancels als inflight request and sends an error to the receiver.
    pub(crate) fn on_session_closed(&mut self, peer: &PeerId) {
        self.peers.remove(peer);
        if let Some(req) = self.inflight_headers_requests.remove(peer) {
            let _ = req.response.send(Err(RequestError::ConnectionDropped));
        }
        if let Some(req) = self.inflight_bodies_requests.remove(peer) {
            let _ = req.response.send(Err(RequestError::ConnectionDropped));
        }
    }

    /// Invoked when an active session is about to be disconnected.
    pub(crate) fn on_pending_disconnect(&mut self, peer_id: &PeerId) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.state = PeerState::Closing;
        }
    }

    /// Returns the next idle peer that's ready to accept a request
    fn next_peer(&mut self) -> Option<(&PeerId, &mut Peer)> {
        self.peers.iter_mut().find(|(_, peer)| peer.state.is_idle())
    }

    /// Returns the next action to return
    fn poll_action(&mut self) -> Option<FetchAction> {
        if self.queued_requests.is_empty() {
            return None
        }

        let peer_id = *self.next_peer()?.0;

        let request = self.queued_requests.pop_front().expect("not empty; qed");
        let request = self.prepare_block_request(peer_id, request);

        Some(FetchAction::BlockRequest { peer_id, request })
    }

    /// Received a request via a downloader
    fn on_download_request(&mut self, request: DownloadRequest) -> Option<FetchAction> {
        match request {
            DownloadRequest::GetBlockHeaders { request: _, response: _ } => {}
            DownloadRequest::GetBlockBodies { .. } => {}
        }
        None
    }

    /// Advance the state the syncer
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<FetchAction> {
        // drain buffered actions first
        if let Some(action) = self.poll_action() {
            return Poll::Ready(action)
        }

        loop {
            // poll incoming requests
            match self.download_requests_rx.poll_next_unpin(cx) {
                Poll::Ready(Some(request)) => {
                    if let Some(action) = self.on_download_request(request) {
                        return Poll::Ready(action)
                    }
                }
                Poll::Ready(None) => {
                    unreachable!("channel can't close")
                }
                Poll::Pending => break,
            }
        }

        if self.queued_requests.is_empty() {
            return Poll::Pending
        }

        Poll::Pending
    }

    /// Handles a new request to a peer.
    ///
    /// Caution: this assumes the peer exists and is idle
    fn prepare_block_request(&mut self, peer_id: PeerId, req: DownloadRequest) -> BlockRequest {
        // update the peer's state
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            peer.state = req.peer_state();
        }

        let started = Instant::now();
        match req {
            DownloadRequest::GetBlockHeaders { request, response } => {
                let inflight = Request { request, response, started };
                self.inflight_headers_requests.insert(peer_id, inflight);

                unimplemented!("unify start types");

                // BlockRequest::GetBlockHeaders(GetBlockHeaders {
                //     // TODO: this should be converted
                //     start_block: BlockHashOrNumber::Number(0),
                //     limit: request.limit,
                //     skip: 0,
                //     reverse: request.reverse,
                // })
            }
            DownloadRequest::GetBlockBodies { request, response } => {
                let inflight = Request { request: request.clone(), response, started };
                self.inflight_bodies_requests.insert(peer_id, inflight);
                BlockRequest::GetBlockBodies(GetBlockBodies(request))
            }
        }
    }

    /// Returns a new followup request for the peer.
    ///
    /// Caution: this expects that the peer is _not_ closed
    fn followup_request(&mut self, peer_id: PeerId) -> Option<BlockResponseOutcome> {
        let req = self.queued_requests.pop_front()?;
        let req = self.prepare_block_request(peer_id, req);
        Some(BlockResponseOutcome::Request(peer_id, req))
    }

    /// Called on a `GetBlockHeaders` response from a peer
    pub(crate) fn on_block_headers_response(
        &mut self,
        peer_id: PeerId,
        res: RequestResult<Vec<Header>>,
    ) -> Option<BlockResponseOutcome> {
        if let Some(resp) = self.inflight_headers_requests.remove(&peer_id) {
            let _ = resp.response.send(res);
        }
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            if peer.state.on_request_finished() {
                return self.followup_request(peer_id)
            }
        }
        None
    }

    /// Called on a `GetBlockBodies` response from a peer
    pub(crate) fn on_block_bodies_response(
        &mut self,
        peer_id: PeerId,
        res: RequestResult<Vec<BlockBody>>,
    ) -> Option<BlockResponseOutcome> {
        if let Some(resp) = self.inflight_bodies_requests.remove(&peer_id) {
            let _ = resp.response.send(res);
        }
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            if peer.state.on_request_finished() {
                return self.followup_request(peer_id)
            }
        }
        None
    }

    /// Returns a new [`HeadersDownloader`] that can send requests to this type
    pub(crate) fn headers_downloader(&self) -> HeadersDownloader {
        HeadersDownloader { request_tx: self.download_requests_tx.clone() }
    }
}

impl Default for StateFetcher {
    fn default() -> Self {
        let (download_requests_tx, download_requests_rx) = mpsc::unbounded_channel();
        Self {
            inflight_headers_requests: Default::default(),
            inflight_bodies_requests: Default::default(),
            peers: Default::default(),
            queued_requests: Default::default(),
            download_requests_rx: UnboundedReceiverStream::new(download_requests_rx),
            download_requests_tx,
        }
    }
}

/// Front-end API for downloading headers.
#[derive(Debug)]
pub struct HeadersDownloader {
    /// Sender half of the request channel.
    request_tx: UnboundedSender<DownloadRequest>,
}

// === impl HeadersDownloader ===

impl HeadersDownloader {
    /// Sends a `GetBlockHeaders` request to an available peer.
    pub async fn get_block_headers(&self, request: HeadersRequest) -> RequestResult<Vec<Header>> {
        let (response, rx) = oneshot::channel();
        self.request_tx.send(DownloadRequest::GetBlockHeaders { request, response })?;
        rx.await?
    }
}

/// Represents a connected peer
struct Peer {
    /// The state this peer currently resides in.
    state: PeerState,
    /// Best known hash that the peer has
    best_hash: H256,
    /// Tracks the best number of the peer.
    best_number: Option<u64>,
}

/// Tracks the state of an individual peer
enum PeerState {
    /// Peer is currently not handling requests and is available.
    Idle,
    /// Peer is handling a `GetBlockHeaders` request.
    GetBlockHeaders,
    /// Peer is handling a `GetBlockBodies` request.
    GetBlockBodies,
    /// Peer session is about to close
    Closing,
}

// === impl PeerState ===

impl PeerState {
    /// Returns true if the peer is currently idle.
    fn is_idle(&self) -> bool {
        matches!(self, PeerState::Idle)
    }

    /// Resets the state on a received response.
    ///
    /// If the state was already marked as `Closing` do nothing.
    ///
    /// Returns `true` if the peer is ready for another request.
    fn on_request_finished(&mut self) -> bool {
        if !matches!(self, PeerState::Closing) {
            *self = PeerState::Idle;
            return true
        }
        false
    }
}

/// A request that waits for a response from the network so it can send it back through the response
/// channel.
struct Request<Req, Resp> {
    request: Req,
    response: oneshot::Sender<Resp>,
    started: Instant,
}

/// Requests that can be sent to the Syncer from a [`HeadersDownloader`]
enum DownloadRequest {
    /// Download the requested headers and send response through channel
    GetBlockHeaders {
        request: HeadersRequest,
        response: oneshot::Sender<RequestResult<Vec<Header>>>,
    },
    /// Download the requested headers and send response through channel
    GetBlockBodies { request: Vec<H256>, response: oneshot::Sender<RequestResult<Vec<BlockBody>>> },
}

// === impl DownloadRequest ===

impl DownloadRequest {
    /// Returns the corresponding state for a peer that handles the request.
    fn peer_state(&self) -> PeerState {
        match self {
            DownloadRequest::GetBlockHeaders { .. } => PeerState::GetBlockHeaders,
            DownloadRequest::GetBlockBodies { .. } => PeerState::GetBlockBodies,
        }
    }
}

/// An action the syncer can emit.
pub(crate) enum FetchAction {
    /// Dispatch an eth request to the given peer.
    BlockRequest {
        /// The targeted recipient for the request
        peer_id: PeerId,
        /// The request to send
        request: BlockRequest,
    },
}

/// Outcome of a processed response.
///
/// Returned after processing a response.
#[derive(Debug)]
pub(crate) enum BlockResponseOutcome {
    /// Continue with another request to the peer.
    Request(PeerId, BlockRequest),
    /// How to handle a bad response and the reputation change to apply.
    BadResponse(PeerId, ReputationChange),
}
