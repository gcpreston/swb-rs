use std::{
    net::SocketAddr,
    str::FromStr,
    time::Duration,
};
use futures_util::{SinkExt, stream::StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Bytes, Message}
};
use clap::Parser;

mod dolphin_connection;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "ws://localhost:4000/bridge_socket/websocket")]
    dest: String,

    #[arg(short, long, default_value = "127.0.0.1:51441")]
    source: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut conn = dolphin_connection::DolphinConnection::new();
    let address = SocketAddr::from_str(&args.source).unwrap();
    conn.connect(address);

    let (ws_stream, _) = connect_async(&args.dest).await.expect("Failed to connect");
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
