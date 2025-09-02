use async_trait::async_trait;
use ezsockets::client::ClientCloseMode;
use ezsockets::{Bytes, ClientConfig, SocketConfig, SendError};
use futures::{
    future,
    pin_mut,
    Sink,
    task::{Context, Poll},
};
use serde::Deserialize;
use std::pin::Pin;
use std::time::Duration;
use thiserror::Error;
use url::Url;
use async_channel::{self, RecvError};

#[derive(Error, Debug)]
pub enum WSError {
    #[error("Send error: {0}")]
    SendError(#[from] SendError<Call>),

    #[error("Close error: {0}")]
    CloseError(#[from] SendError<ezsockets::InMessage>),

    #[error("Connect error: {0}")]
    ConnectError(&'static str)
}

pub struct MyClient {
    handle: ezsockets::Client<Self>,
    connected_sender: async_channel::Sender<BridgeInfo>,
    initially_connected: bool
}

pub struct SpectatorModeClient {
    ws_client: ezsockets::Client<MyClient>,
}

pub enum Call {
    GameData(Bytes),
}

#[derive(Deserialize, Debug)]
pub struct BridgeInfo {
    pub bridge_id: String,
    pub stream_ids: Vec<u32>
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

pub async fn initiate_connection(address: &str, stream_count: usize) -> Result<(SpectatorModeClient, impl std::future::Future<Output = Result<(), ezsockets::Error>>, BridgeInfo), WSError> {
    tracing::info!("Connecting to SpectatorMode...");
    let url = Url::parse(address).unwrap();
    let mut socket_config = SocketConfig::default();
    socket_config.timeout = Duration::from_secs(5);

    let config = ClientConfig::new(url)
        .socket_config(socket_config)
        .max_initial_connect_attempts(3)
        .max_reconnect_attempts(3)
        .query_parameter("stream_count", stream_count.to_string().as_str());
    
    let (connected_sender, connected_receiver) = async_channel::unbounded();
    let (sm_handle, sm_future) = ezsockets::connect(|handle| MyClient { handle, connected_sender, initially_connected: false }, config).await;

    let recv_future = connected_receiver.recv();
    pin_mut!(recv_future);
    pin_mut!(sm_future);

    match future::select(recv_future, sm_future).await {
        future::Either::Left((bridge_info_result, _)) => {
            match bridge_info_result {
                Ok(bridge_info) => {
                    tracing::info!(
                        "Connected to SpectatorMode with bridge ID {} and stream IDs {:?}",
                        bridge_info.bridge_id,
                        bridge_info.stream_ids
                    );
                    Ok((SpectatorModeClient { ws_client: sm_handle }, sm_future, bridge_info))
                }
                Err(RecvError) => Err(WSError::ConnectError("Failed to receive SpectatorMode connection confirmation"))
            }
        }
        future::Either::Right((sm_future_result, _)) => {
            println!("what is future result {:?}", sm_future_result);
            Err(WSError::ConnectError("Connection closed before receiving connection confirmation"))
        }
    }
}
