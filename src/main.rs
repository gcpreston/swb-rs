use std::{net::SocketAddr, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use futures::{SinkExt, StreamExt, stream_select};
use futures_util::pin_mut;
use signal_hook;
use signal_hook_tokio::Signals;
use ezsockets::Bytes;

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
    Signal(i32),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    println!("[CTRL + C to quit]\n");

    let conn = dolphin_connection::DolphinConnection::new();
    let address = SocketAddr::from_str(&args.source).expect("Invalid socket address");
    let pid = conn.initiate_connection(address);
    conn.wait_for_connected().await;
    tracing::info!("Connected to Slippi.");

    let sm_client = spectator_mode_client::connect(&args.dest).await;

    let signals = Signals::new(&[signal_hook::consts::SIGINT]).unwrap();
    let handle = signals.handle();

    let mapped_signals = signals.map(|s| EventOrSignal::Signal(s));
    let mapped_events = conn.event_stream().map(|e| EventOrSignal::Event(e));
    let combined = stream_select!(mapped_signals, mapped_events);

    let mut interrupted = false;
    pin_mut!(sm_client);

    let dolphin_to_sm = combined
        .map(|e| {
            // ConnectionEvent::Disconnected will not reach the stream because
            // it is sent as Poll::Ready(None), i.e. the stream end.
            match e {
                EventOrSignal::Signal(n) => {
                    if !interrupted {
                        tracing::info!("Disconnecting...");
                        conn.initiate_disconnect(pid);
                        interrupted = true;
                    } else {
                        std::process::exit(n);
                    }
                }
                // EventOrSignal::Event(ConnectionEvent::Connect) => println!("Connected to Slippi."),
                EventOrSignal::Event(ConnectionEvent::StartGame) => tracing::info!("Game start"),
                EventOrSignal::Event(ConnectionEvent::EndGame) => tracing::info!("Game end"),
                EventOrSignal::Event(ConnectionEvent::Disconnect) => handle.close(),
                _ => (),
            };
            e
        })
        .filter_map(async |e| match e {
            EventOrSignal::Event(ConnectionEvent::Message { payload }) => {
                Some(Ok(Bytes::from(payload)))
            }
            _ => None,
        })
        .forward(&mut sm_client);

    dolphin_to_sm.await.unwrap();
    sm_client.close().await.unwrap();
    tracing::info!("Disconnected.");
}
