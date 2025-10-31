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
use tokio::sync::oneshot;
use url::Url;

#[derive(Error, Debug)]
pub enum SpectatorModeClientError {
    #[error("Send error: {0}")]
    SendError(#[from] SendError<Call>),

    #[error("Close error: {0}")]
    CloseError(#[from] SendError<ezsockets::InMessage>),

    #[error("Unable to connect: {0}")]
    ConnectError(&'static str),

    #[error("Connection task panicked")]
    ConnectionTaskPanickedError,

    #[error("Connection task result already consumed")]
    ConnectionTaskResultAlreadyConsumedError,

    #[error("Connection finished with error: {0}")]
    ConnectionTaskResultError(#[from] ezsockets::Error)
}

pub struct MyClient {
    handle: ezsockets::Client<Self>,
    connected_sender: Option<oneshot::Sender<BridgeInfo>>,
    initially_connected: bool,
}

pub struct SpectatorModeClient {
    ws_client: ezsockets::Client<MyClient>,
}

pub struct ConnectionMonitor {
    connection_task: Option<tokio::task::JoinHandle<Result<(), ezsockets::Error>>>,
}

impl ConnectionMonitor {
    pub async fn wait_for_close(&mut self) -> Result<(), SpectatorModeClientError> {
        if let Some(task) = self.connection_task.take() {
            match task.await {
                Ok(connection_task_result) => {
                    connection_task_result.map_err(|ezsockets_err| SpectatorModeClientError::ConnectionTaskResultError(ezsockets_err))
                }
                Err(_) => Err(SpectatorModeClientError::ConnectionTaskPanickedError)
            }
        } else {
            Err(SpectatorModeClientError::ConnectionTaskResultAlreadyConsumedError)
        }
    }
}

pub enum Call {
    GameData(Bytes),
}

#[derive(Deserialize, Debug, Clone)]
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
            if let Some(sender) = self.connected_sender.take() {
                let _ = sender.send(bridge_info); // Ignore send errors (receiver might be dropped)
            }
            self.initially_connected = true;
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
    type Error = SpectatorModeClientError;

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
) -> Result<(SpectatorModeClient, ConnectionMonitor, BridgeInfo), SpectatorModeClientError> {
    tracing::info!("Connecting to SpectatorMode...");
    let url = Url::parse(address).unwrap();
    let mut socket_config = SocketConfig::default();
    socket_config.timeout = Duration::from_secs(15);

    let config = ClientConfig::new(url)
        .socket_config(socket_config)
        .max_initial_connect_attempts(3)
        .max_reconnect_attempts(3)
        .query_parameter("stream_count", stream_count.to_string().as_str());

    let (connected_sender, connected_receiver) = oneshot::channel::<BridgeInfo>();
    let (handle_tx, handle_rx) = oneshot::channel();

    // Initiate the connection and await its completion within a background task.
    // This allows us to wait for connected_receiver to receive the bridge info
    // from the server, and drop to an error case if the connection completes,
    // which would mean the receiver would never receive a value.
    let connection_task = tokio::spawn(async move {
        let (sm_handle, sm_future) = ezsockets::connect(
            |handle| MyClient {
                handle,
                connected_sender: Some(connected_sender),
                initially_connected: false,
            },
            config,
        )
        .await;

        let _ = handle_tx.send(sm_handle);
        sm_future.await
    });

    // Wait for initial connection handle
    // An unwrap failure would indicate the sender was dropped, which is
    // impossible since there are no error cases before the send in connection_task
    let sm_handle = handle_rx.await.unwrap();

    // Wait for bridge info on successful connection
    // TODO: When this fails, it's because connected_sender is dropped, which
    // means connection_task finished before bridge info was received, meaning
    // the connection never finished. Therefore, would like to return error
    // from sm_future, but it isn't accessible here.
    let bridge_info = connected_receiver
        .await
        .map_err(|_| SpectatorModeClientError::ConnectError("Failed to receive bridge info"))?;

    tracing::info!(
        "Connected to SpectatorMode with bridge ID {} and stream IDs {:?}",
        bridge_info.bridge_id,
        bridge_info.stream_ids
    );

    let monitor = ConnectionMonitor {
        connection_task: Some(connection_task),
    };

    Ok((
        SpectatorModeClient {
            ws_client: sm_handle,
        },
        monitor,
        bridge_info,
    ))
}
