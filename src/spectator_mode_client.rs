use async_channel::{self, RecvError};
use async_trait::async_trait;
use ezsockets::client::ClientCloseMode;
use ezsockets::{Bytes, ClientConfig, SendError, SocketConfig};
use futures::{
    Sink,
    task::{Context, Poll},
};
use serde::Deserialize;
use std::pin::Pin;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{oneshot, watch};
use url::Url;

#[derive(Error, Debug)]
pub enum WSError {
    #[error("Send error: {0}")]
    SendError(#[from] SendError<Call>),

    #[error("Close error: {0}")]
    CloseError(#[from] SendError<ezsockets::InMessage>),

    #[error("Connect error: {0}")]
    ConnectError(&'static str),
}

pub struct MyClient {
    handle: ezsockets::Client<Self>,
    connected_sender: async_channel::Sender<BridgeInfo>,
    initially_connected: bool,
}

pub struct SpectatorModeClient {
    ws_client: ezsockets::Client<MyClient>,
}

pub struct ConnectionMonitor {
    // Signals when the connection is lost/closed
    connection_closed_rx: watch::Receiver<bool>,
    // The actual connection future (runs in background)
    _connection_task: tokio::task::JoinHandle<Result<(), ezsockets::Error>>,
}

impl ConnectionMonitor {
    pub async fn wait_for_close(&mut self) {
        let _ = self.connection_closed_rx.changed().await;
    }

    pub fn is_closed(&self) -> bool {
        *self.connection_closed_rx.borrow()
    }
}

pub enum Call {
    GameData(Bytes),
}

#[derive(Deserialize, Debug)]
pub struct BridgeInfo {
    pub bridge_id: String,
    pub stream_ids: Vec<u32>,
}

#[async_trait]
impl ezsockets::ClientExt for MyClient {
    type Call = Call;

    async fn on_text(&mut self, text: ezsockets::Utf8Bytes) -> Result<(), ezsockets::Error> {
        let bridge_info = serde_json::from_str::<BridgeInfo>(text.as_str())?;
        if !self.initially_connected {
            self.connected_sender.send(bridge_info).await?;
            self.initially_connected = true;
            self.connected_sender.close();
        }
        Ok(())
    }

    async fn on_binary(&mut self, _bytes: ezsockets::Bytes) -> Result<(), ezsockets::Error> {
        Ok(())
    }

    async fn on_call(&mut self, call: Self::Call) -> Result<(), ezsockets::Error> {
        match call {
            Call::GameData(payload) => {
                self.handle.binary(payload).unwrap();
            }
        };
        Ok(())
    }

    // async fn on_close(&mut self, _close_frame: std::option::Option<ezsockets::CloseFrame>) -> Result<ClientCloseMode, Error> {
    //     Ok(ClientCloseMode::Close)
    // }

    async fn on_disconnect(&mut self) -> Result<ClientCloseMode, ezsockets::Error> {
        tracing::info!("Reconnecting to SpectatorMode...");
        // Ok(ClientCloseMode::Close)
        Ok(ClientCloseMode::Reconnect)
    }
}

impl Sink<Bytes> for SpectatorModeClient {
    type Error = WSError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        self.ws_client.call(Call::GameData(item))?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        tracing::info!("Disconnecting from SpectatorMode...");

        if let Err(SendError(..)) = self.ws_client.close(None) {
            tracing::debug!("SpectatorMode connection is already closed.");
        }

        Poll::Ready(Ok(()))
    }
}

pub async fn initiate_connection(
    address: &str,
    stream_count: usize,
) -> Result<(SpectatorModeClient, ConnectionMonitor, BridgeInfo), WSError> {
    tracing::info!("Connecting to SpectatorMode...");
    let url = Url::parse(address).unwrap();
    let mut socket_config = SocketConfig::default();
    socket_config.timeout = Duration::from_secs(15);

    let config = ClientConfig::new(url)
        .socket_config(socket_config)
        .max_initial_connect_attempts(3)
        .max_reconnect_attempts(3)
        .query_parameter("stream_count", stream_count.to_string().as_str());

    let (connected_sender, connected_receiver) = async_channel::unbounded();
    let (handle_tx, handle_rx) = oneshot::channel();
    let (connection_closed_tx, connection_closed_rx) = watch::channel(false);

    // Spawn the connection task
    let connection_task = tokio::spawn(async move {
        let (sm_handle, sm_future) = ezsockets::connect(
            |handle| MyClient {
                handle,
                connected_sender,
                initially_connected: false,
            },
            config,
        )
        .await;

        // Send the handle back immediately
        let _ = handle_tx.send(sm_handle);

        // Monitor the connection future
        let result = sm_future.await;
        let _ = connection_closed_tx.send(true);
        result
    });

    // Wait for initial connection handle
    let sm_handle = handle_rx
        .await
        .map_err(|_| WSError::ConnectError("Connection task failed"))?;

    // Wait for bridge info with timeout
    let bridge_info = tokio::time::timeout(Duration::from_secs(10), connected_receiver.recv())
        .await
        .map_err(|_| WSError::ConnectError("Timeout waiting for bridge info"))?
        .map_err(|_| WSError::ConnectError("Failed to receive bridge info"))?;

    tracing::info!(
        "Connected to SpectatorMode with bridge ID {} and stream IDs {:?}",
        bridge_info.bridge_id,
        bridge_info.stream_ids
    );

    let monitor = ConnectionMonitor {
        connection_closed_rx,
        _connection_task: connection_task,
    };

    Ok((
        SpectatorModeClient {
            ws_client: sm_handle,
        },
        monitor,
        bridge_info,
    ))
}
