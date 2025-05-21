use std::{net::SocketAddr, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use ezsockets::Bytes;
use futures::{SinkExt, StreamExt, channel::mpsc::channel};
use futures_util::pin_mut;
use rusty_enet as enet;

mod dolphin_connection;
mod spectator_mode_client;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wss://ssbm.tv/bridge_socket/websocket")]
    dest: String,

    #[arg(short, long, default_value = "127.0.0.1:51441")]
    source: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    println!("[CTRL + C to quit]\n");

    let (mut sender, receiver) = channel::<enet::PeerID>(100);
    let mut already_interrupted = false;

    let conn = dolphin_connection::DolphinConnection::new(receiver);
    let address = SocketAddr::from_str(&args.source).expect("Invalid socket address");
    let peer_id = conn.initiate_connection(address);
    conn.wait_for_connected().await;
    tracing::info!("Connected to Slippi.");

    ctrlc::set_handler(move || {
        if already_interrupted {
            std::process::exit(2);
        } else {
            already_interrupted = true;
            tracing::info!("Disconnecting...");
            sender.try_send(peer_id).unwrap();
        }
    })
    .unwrap();

    let sm_client = spectator_mode_client::initiate_connection(&args.dest).await;

    pin_mut!(sm_client);

    let dolphin_to_sm = conn
        .event_stream()
        .filter_map(async |e| {
            // Side-effects (logs)
            // ConnectionEvent::Connected will not reach the stream because
            // it is awaited before initiating the SpectatorMode connection.
            match e {
                ConnectionEvent::StartGame => tracing::info!("Received game start event."),
                ConnectionEvent::EndGame => tracing::info!("Received game end event."),
                _ => (),
            };

            // Return
            match e {
                ConnectionEvent::Message { payload } => Some(Ok(Bytes::from(payload))),
                _ => None,
            }
        })
        .forward(&mut sm_client);

    dolphin_to_sm.await.unwrap();
    sm_client.close().await.unwrap();
    tracing::info!("Disconnected.");
}
