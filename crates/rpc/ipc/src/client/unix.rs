//! [`jsonrpsee`] transport adapter implementation for Unix IPC by using Unix Sockets.

use std::path::Path;

use futures::StreamExt;
use jsonrpsee::core::client::{ReceivedMessage, TransportReceiverT, TransportSenderT};
use tokio::{
    io::AsyncWriteExt,
    net::{
        unix::{OwnedReadHalf, OwnedWriteHalf},
        UnixStream,
    },
};
use tokio_util::codec::FramedRead;

use crate::{client::IpcError, stream_codec::StreamCodec};

/// Sending end of IPC transport.
#[derive(Debug)]
pub(crate) struct Sender {
    inner: OwnedWriteHalf,
}

#[async_trait::async_trait]
impl TransportSenderT for Sender {
    type Error = IpcError;

    /// Sends out a request. Returns a Future that finishes when the request has been successfully
    /// sent.
    async fn send(&mut self, msg: String) -> Result<(), Self::Error> {
        Ok(self.inner.write_all(msg.as_bytes()).await?)
    }

    async fn send_ping(&mut self) -> Result<(), Self::Error> {
        tracing::trace!("send ping - not implemented");
        Err(IpcError::NotSupported)
    }

    /// Close the connection.
    async fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Receiving end of IPC transport.
#[derive(Debug)]
pub(crate) struct Receiver {
    #[cfg(unix)]
    pub(crate) inner: FramedRead<OwnedReadHalf, StreamCodec>,
    #[cfg(windows)]
    inner: FramedRead<Arc<NamedPipeClient>, StreamCodec>,
}

#[async_trait::async_trait]
impl TransportReceiverT for Receiver {
    type Error = IpcError;

    /// Returns a Future resolving when the server sent us something back.
    async fn receive(&mut self) -> Result<ReceivedMessage, Self::Error> {
        self.inner.next().await.map_or(Err(IpcError::Closed), |val| Ok(ReceivedMessage::Text(val?)))
    }
}

/// Builder for IPC transport [`Sender`] and [`Receiver`] pair.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub(crate) struct IpcTransportClientBuilder;

impl IpcTransportClientBuilder {
    /// Try to establish the connection.
    ///
    /// ```
    /// use jsonrpsee::{core::client::ClientT, rpc_params};
    /// use reth_ipc::client::IpcClientBuilder;
    /// # async fn run_client() -> Result<(), Box<dyn std::error::Error +  Send + Sync>> {
    /// let client = IpcClientBuilder::default().build("/tmp/my-uds").await?;
    /// let response: String = client.request("say_hello", rpc_params![]).await?;
    /// #   Ok(())
    /// # }
    /// ```
    pub(crate) async fn build(
        self,
        path: impl AsRef<Path>,
    ) -> Result<(Sender, Receiver), IpcError> {
        let path = path.as_ref();

        let stream = UnixStream::connect(path)
            .await
            .map_err(|err| IpcError::FailedToConnect { path: path.to_path_buf(), err })?;

        let (rhlf, whlf) = stream.into_split();

        Ok((
            Sender { inner: whlf },
            Receiver { inner: FramedRead::new(rhlf, StreamCodec::stream_incoming()) },
        ))
    }
}
