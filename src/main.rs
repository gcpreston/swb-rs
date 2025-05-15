use std::{
    net::SocketAddr, str::FromStr, task::Context, time::Duration
};

use futures::future::{poll_fn, poll_immediate};
use futures_util::{SinkExt, stream::StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Bytes, Message}
};
use serde_json::Value;
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

fn parse_connect_reply(reply: Message) -> Result<(String, String), &'static str> {
    match reply {
        Message::Text(bytes) => {
            let v: Value = serde_json::from_str(bytes.as_str()).unwrap();
            if let Value::String(bridge_id) = &v["bridge_id"] {
                if let Value::String(reconnect_token) = &v["reconnect_token"] {
                    Ok((bridge_id.to_string(), reconnect_token.to_string()))
                } else {
                    Err("where's reconnect token?")
                }
            } else {
                Err("where's bridge id?")
            }
        }
        _ => Err("didn't expect that!")
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut conn = dolphin_connection::DolphinConnection::new();
    let address = SocketAddr::from_str(&args.source).unwrap();
    conn.connect(address);

    // let (ws_stream, _) = connect_async(&args.dest).await.expect("Failed to connect");
    // println!("WebSocket handshake has been successfully completed");
    // let (mut sink, mut stream) = ws_stream.split();

    let dolphin_event_future = poll_fn(move |cx: &mut Context<'_>| {
        conn.next_event(cx)
    });

    println!("Got from dolphin: {:?}", dolphin_event_future.await.unwrap());
}
