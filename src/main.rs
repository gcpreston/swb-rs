use std::{
    net::SocketAddr, str::FromStr, task::Context, time::Duration
};

use futures::{future, stream};
use futures::task::Poll;
use futures_util::{SinkExt, stream::StreamExt};
use tokio::time::interval;
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

    // Poll Dolphin connection at 120Hz
    let mut i = interval(Duration::from_micros(8333));

    let mut dolphin_event_stream = stream::poll_fn(move |cx: &mut Context<'_>| {
        let p = i.poll_tick(cx);
        match p {
            Poll::Pending => {
                Poll::Pending
            },
            Poll::Ready(_) => {
                match conn.service() {
                    Result::Err(e) => panic!("dolphin service error: {e}"),
                    Result::Ok(None) => {
                        cx.waker().clone().wake();
                        Poll::Pending
                    },
                    Result::Ok(Some(event)) => {
                        Poll::Ready(Some(event))
                    }
                }
            }
        }
    });

    loop {
        println!("Got from dolphin: {:?}", dolphin_event_stream.next().await.unwrap());
    }
}
