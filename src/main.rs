use std::{net::SocketAddr, os::unix::process, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use futures::{stream_select, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Bytes, Message},
};
use signal_hook;
use signal_hook_tokio::Signals;

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

enum EventOrSignal {
    Event(ConnectionEvent),
    Signal(i32)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let conn = dolphin_connection::DolphinConnection::new();
    let address = SocketAddr::from_str(&args.source).unwrap();
    let pid = conn.initiate_connection(address);
    conn.wait_for_connected().await;

    let (ws_stream, _) = connect_async(&args.dest).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");
    let (sink, mut _stream) = ws_stream.split();

    let dolphin_event_stream = conn.catch_up_stream().chain(conn.event_stream());

    let signals = Signals::new(&[signal_hook::consts::SIGINT]).unwrap();
    let handle = signals.handle();

    let mapped_signals = signals.map(|s| EventOrSignal::Signal(s));
    let mapped_events = dolphin_event_stream.map(|e| EventOrSignal::Event(e));

    let combined = stream_select!(mapped_signals, mapped_events);

    let mut interrupted = false;

    let dolphin_to_sm = combined
        .map(|e| {
            // ConnectionEvent::Disconnected will not reach the stream because
            // it is sent as Poll::Ready(None), i.e. the stream end.
            match e {
                EventOrSignal::Signal(n) => {
                    if !interrupted {
                        println!("Got a primary signal: {:?}", n);
                        // handle.close();
                        conn.initiate_disconnect(pid);
                        interrupted = true;
                    } else {
                        println!("Got a signal again: {:?}", n);
                        std::process::exit(n);
                    }
                },
                EventOrSignal::Event(ConnectionEvent::Connect) => println!("Connected to Slippi."),
                EventOrSignal::Event(ConnectionEvent::StartGame) => println!("Game start"),
                EventOrSignal::Event(ConnectionEvent::EndGame) => println!("Game end"),
                _ => ()
            };
            e
        })
        .filter_map(async |e| match e {
            EventOrSignal::Event(ConnectionEvent::Message { payload }) => Some(payload),
            _ => None,
        })
        .map(|payload| Ok(Message::Binary(Bytes::from(payload))))
        .forward(sink);

    dolphin_to_sm.await.unwrap();

    handle.close();
    println!("Disconnected from Slippi.");
}
