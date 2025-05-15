use std::{net::SocketAddr, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use futures::future;
use futures_util::stream::StreamExt;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Bytes, Message},
};

mod dolphin_connection;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "ws://localhost:4000/bridge_socket/websocket"
    )]
    dest: String,

    #[arg(short, long, default_value = "127.0.0.1:51441")]
    source: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut conn = dolphin_connection::DolphinConnection::new();
    let address = SocketAddr::from_str(&args.source).unwrap();
    conn.initiate_connection(address);

    let (ws_stream, _) = connect_async(&args.dest).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");
    let (sink, mut _stream) = ws_stream.split();

    let dolphin_event_stream = conn.event_stream();

    let dolphin_to_sm = dolphin_event_stream
        .filter_map(|e| match e {
            ConnectionEvent::Message { payload } => future::ready(Some(payload)),
            _ => future::ready(None),
        })
        .map(|payload| Ok(Message::Binary(Bytes::from(payload))))
        .forward(sink);

    dolphin_to_sm.await.unwrap();
}
