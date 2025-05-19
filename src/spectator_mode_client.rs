use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use futures::{stream::SplitSink, StreamExt};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct BridgeInfo {
    pub bridge_id: String,
    // reconnect_token: String
}

pub async fn connect(address: String) -> (BridgeInfo, SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) {
    let (ws_stream, _) = connect_async(address).await.expect("Failed to connect");
    let (sink, mut stream) = ws_stream.split();

    // TODO: Result return type
    if let Message::Text(first_message) = stream.next().await.unwrap().unwrap() {
        let bridge_info = serde_json::from_str(first_message.as_str()).unwrap();
        (bridge_info, sink)
    } else {
        panic!("AHHHHHHH");
    }
}
