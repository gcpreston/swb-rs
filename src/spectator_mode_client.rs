use async_trait::async_trait;
use ezsockets::{Bytes, ClientConfig};
use ezsockets::{Error, SendError};
use futures::{
    Sink,
    task::{Context, Poll},
};
use serde::Deserialize;
use std::pin::Pin;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum ConnectError {
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
}

impl Sink<Bytes> for SpectatorModeClient {
    type Error = ConnectError;

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
        self.ws_client.close(None)?;
        Poll::Ready(Ok(()))
    }
}

pub async fn initiate_connection(address: &str) -> SpectatorModeClient {
    let url = Url::parse(address).unwrap();
    let config = ClientConfig::new(url);
    let (sm_handle, _future) = ezsockets::connect(|handle| MyClient { handle }, config).await;

    SpectatorModeClient {
        ws_client: sm_handle,
    }
}
