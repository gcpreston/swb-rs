use async_trait::async_trait;
use ezsockets::client::ClientCloseMode;
use ezsockets::{Bytes, ClientConfig, SocketConfig};
use ezsockets::{Error, SendError};
use futures::{
    Sink,
    task::{Context, Poll},
};
use serde::Deserialize;
use std::pin::Pin;
use std::time::Duration;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum WSError {
    #[error("Send error: {0}")]
    SendError(#[from] SendError<Call>),

    #[error("Close error: {0}")]
    CloseError(#[from] SendError<ezsockets::InMessage>),
}

pub struct MyClient {
    handle: ezsockets::Client<Self>,
}

pub struct SpectatorModeClient {
    ws_client: ezsockets::Client<MyClient>,
}

pub enum Call {
    GameData(Bytes),
}

#[derive(Deserialize)]
pub struct BridgeInfo {
    pub bridge_id: String,
}

#[async_trait]
impl ezsockets::ClientExt for MyClient {
    type Call = Call;

    async fn on_text(&mut self, text: ezsockets::Utf8Bytes) -> Result<(), Error> {
        let bridge_info = serde_json::from_str::<BridgeInfo>(text.as_str())?;
        tracing::info!(
            "Connected to SpectatorMode with stream ID {}",
            bridge_info.bridge_id
        );
        Ok(())
    }

    async fn on_binary(&mut self, _bytes: ezsockets::Bytes) -> Result<(), Error> {
        Ok(())
    }

    async fn on_call(&mut self, call: Self::Call) -> Result<(), Error> {
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

    async fn on_disconnect(&mut self) -> Result<ClientCloseMode, Error> {
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

pub async fn initiate_connection(address: &str) -> (SpectatorModeClient, impl std::future::Future<Output = Result<(), Error>>) {
    tracing::info!("Connecting to SpectatorMode...");
    let url = Url::parse(address).unwrap();
    let mut socket_config = SocketConfig::default();
    socket_config.timeout = Duration::from_secs(15);
    let config = ClientConfig::new(url).socket_config(socket_config).max_reconnect_attempts(3);
    let (sm_handle, future) = ezsockets::connect(|handle| MyClient { handle }, config).await;

    (SpectatorModeClient { ws_client: sm_handle }, future)
}
