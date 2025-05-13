use std::{
    net::SocketAddr,
    str::{self, FromStr},
    time::Duration,
};
use futures_util::{SinkExt, stream::StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Bytes, Message}
};

mod dolphin_connection;

const SLIPPI_ADDRESS: &str = "127.0.0.1";
const SLIPPI_PORT: i32 = 51441;

#[tokio::main]
async fn main() {
    let mut conn = dolphin_connection::DolphinConnection::new();
        let address = SocketAddr::from_str(format!("{SLIPPI_ADDRESS}:{SLIPPI_PORT}").as_str()).unwrap();
    conn.connect(address);

    let (ws_stream, _) = connect_async("ws://localhost:4000/bridge_socket/websocket").await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");
    let (mut sink, _stream) = ws_stream.split();

    'outer: loop {
        while let Some(event) = conn.service().unwrap() {
            match event {
                dolphin_connection::ConnectionEvent::Connect => {
                    println!("Connected");
                }
                dolphin_connection::ConnectionEvent::Disconnect => {
                    println!("Disconnected");
                    break 'outer;
                }
                dolphin_connection::ConnectionEvent::Message { payload } => {
                    println!("Got a message, payload size: {:?}", payload.len());
                    let message = Message::Binary(Bytes::from(payload));
                    sink.send(message).await.unwrap();
                }
                dolphin_connection::ConnectionEvent::StartGame => {
                    println!("Game started");
                }
                dolphin_connection::ConnectionEvent::EndGame => {
                    println!("Game ended");
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
