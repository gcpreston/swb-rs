use std::{net::SocketAddr, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use futures::{stream_select, SinkExt, StreamExt};
use futures_util::pin_mut;
use signal_hook;
use signal_hook_tokio::Signals;
use tokio_tungstenite::tungstenite::Message;

mod dolphin_connection;
mod spectator_mode_client;

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
    println!("[CTRL + C to quit]\n");

    let conn = dolphin_connection::DolphinConnection::new();
    let address = SocketAddr::from_str(&args.source).expect("Invalid socket address");
    let pid = conn.initiate_connection(address);
    conn.wait_for_connected().await;
    println!("Connected to Slippi.");

    let (bridge_info, sink) = spectator_mode_client::connect(args.dest).await.expect("Error connecting to Spectator Mode");
    println!("Connected to SpectatorMode with stream ID: {}", bridge_info.bridge_id);

    let signals = Signals::new(&[signal_hook::consts::SIGINT]).unwrap();
    let handle = signals.handle();

    let mapped_signals = signals.map(|s| EventOrSignal::Signal(s));
    let mapped_events = conn.event_stream().map(|e| EventOrSignal::Event(e));
    let combined = stream_select!(mapped_signals, mapped_events);

    let mut interrupted = false;
    pin_mut!(sink);

    let dolphin_to_sm = combined
        .map(|e| {
            // ConnectionEvent::Disconnected will not reach the stream because
            // it is sent as Poll::Ready(None), i.e. the stream end.
            match e {
                EventOrSignal::Signal(n) => {
                    if !interrupted {
                        println!("Disconnecting...");
                        conn.initiate_disconnect(pid);
                        interrupted = true;
                    } else {
                        std::process::exit(n);
                    }
                },
                EventOrSignal::Event(ConnectionEvent::Connect) => println!("Connected to Slippi."),
                EventOrSignal::Event(ConnectionEvent::StartGame) => println!("Game start"),
                EventOrSignal::Event(ConnectionEvent::EndGame) => println!("Game end"),
                EventOrSignal::Event(ConnectionEvent::Disconnect) => handle.close(),
                _ => (),
            };
            e
        })
        .filter_map(async |e| match e {
            EventOrSignal::Event(ConnectionEvent::Message { payload }) => {
                Some(Ok(Message::Binary(payload.into())))
            }
            _ => None,
        })
        .forward(&mut sink);

    dolphin_to_sm.await.unwrap();
    sink.close().await.unwrap();
    println!("Disconnected.");
}
