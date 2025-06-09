use async_trait::async_trait;
use ezsockets::client::ClientCloseMode;
use ezsockets::{Bytes, ClientConfig, CloseFrame, CloseCode, SocketConfig};
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

    async fn on_disconnect(&mut self) -> Result<ClientCloseMode, Error> {
        tracing::info!("Reconnecting to SpectatorMode...");
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
        tracing::debug!("in poll_close");
        // self.ws_client.text("quit")?;
        self.ws_client.close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "adios!".into(),
        }))?;
        Poll::Ready(Ok(()))
    }
}

pub async fn initiate_connection(address: &str) -> (SpectatorModeClient, impl std::future::Future<Output = Result<(), Error>>) {
    let url = Url::parse(address).unwrap();
    let mut socket_config = SocketConfig::default();
    socket_config.timeout = Duration::from_secs(15);
    let config = ClientConfig::new(url).socket_config(socket_config).max_reconnect_attempts(3);
    let (sm_handle, future) = ezsockets::connect(|handle| MyClient { handle }, config).await;

    (SpectatorModeClient { ws_client: sm_handle }, future)
}
