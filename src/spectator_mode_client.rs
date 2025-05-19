use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::{self, Message}, MaybeTlsStream, WebSocketStream};
use futures::{stream::SplitSink, StreamExt};
use serde::Deserialize;
use thiserror::Error;

#[derive(Deserialize)]
pub struct BridgeInfo {
    pub bridge_id: String,
    // reconnect_token: String
}

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("Connection closed normally")]
    Exhausted,
    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] tungstenite::Error),
    #[error("JSON decode error: {0}")]
    UnexpectedMessageSchema(#[from] serde_json::Error),
    #[error("Expected a text message")]
    UnexpectedMessageType
}

pub async fn connect(address: String) -> Result<(BridgeInfo, SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>), ConnectError> {
    let (ws_stream, _) = connect_async(address).await?;
    let (sink, mut stream) = ws_stream.split();

    match stream.next().await {
        None => Result::Err(ConnectError::Exhausted),
        Some(Result::Err(e)) => Result::Err(ConnectError::WebSocketError(e)),
        Some(Ok(Message::Text(content))) => {
            let bridge_info = serde_json::from_str::<BridgeInfo>(content.as_str())?;
            Result::Ok((bridge_info, sink))
        }
        Some(Ok(_)) => Result::Err(ConnectError::UnexpectedMessageType)
    }
}
