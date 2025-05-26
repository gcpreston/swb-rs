use std::{error::Error, net::SocketAddr, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use ezsockets::Bytes;
use futures::{SinkExt, StreamExt, channel::mpsc::channel};
use futures_util::{future, pin_mut};
use rusty_enet as enet;
use spectator_mode_client::WSError;
use tracing::Level;
use self_update::cargo_crate_version;

mod dolphin_connection;
mod spectator_mode_client;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "wss://ssbm.tv/bridge_socket/websocket")]
    dest: String,

    #[arg(short, long, default_value = "127.0.0.1:51441")]
    source: String,

    #[arg(short, long, action)]
    verbose: bool
}

fn update_if_needed() -> Result<self_update::Status, Box<dyn std::error::Error>> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("gcpreston")
        .repo_name("swb-rs")
        .bin_name("swb")
        .bin_path_in_archive("{{ bin }}-v{{ version }}-{{ target }}/{{ bin }}")
        .show_download_progress(true)
        .current_version(cargo_crate_version!())
        .build()?
        .update()?;

    println!("Update status: `{}`!", status.version());
    Ok(status)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let update_status = update_if_needed()?;

    if update_status.updated() {
        println!("\nUpdate complete, please relaunch swb.")
    } else {
        // equivalent to #[tokio::main]
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                tokio_main().await;
            })
    }

    Ok(())
}

async fn tokio_main() {
    let args = Args::parse();

    if args.verbose {
        tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
    } else {
        tracing_subscriber::fmt().with_env_filter("swb=info").init();
    };

    println!("[CTRL + C to quit]\n");

    let (mut sender, receiver) = channel::<enet::PeerID>(100);
    let mut already_interrupted = false;

    let conn = dolphin_connection::DolphinConnection::new(receiver);
    let address = SocketAddr::from_str(&args.source).expect("Invalid socket address");
    tracing::info!("Connecting to Slippi...");
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

    tracing::info!("Connecting to SpectatorMode...");
    let (sm_client, sm_client_future) = spectator_mode_client::initiate_connection(&args.dest).await;

    pin_mut!(sm_client);

    let dolphin_to_sm = conn
        .event_stream()
        .filter_map(async |es| {
            let mut data: Vec<Vec<u8>> = Vec::new();

            let _: Vec<()> =
                es.into_iter().map(|e| {
                    // Side-effects
                    // ConnectionEvent::Connected will not reach the stream because
                    // it is awaited before initiating the SpectatorMode connection.
                    match e {
                        ConnectionEvent::StartGame => tracing::info!("Received game start event."),
                        ConnectionEvent::EndGame => tracing::info!("Received game end event."),
                        ConnectionEvent::Disconnect => tracing::info!("Disconnected from Slippi."),
                        ConnectionEvent::Message { payload } => {
                            data.push(payload);
                        },
                        _ => ()
                    };
                }).collect();

            // Return
            if data.len() > 0 {
                let b = Bytes::from(data.into_iter().flatten().collect::<Vec<u8>>());
                Some(Ok(b))
            } else {
                None
            }
        })
        .forward(&mut sm_client);

    pin_mut!(dolphin_to_sm, sm_client_future);

    match future::select(dolphin_to_sm, sm_client_future).await {
        future::Either::Left((forward_result, _p)) => {
            log_forward_result(forward_result);
            sm_client.close().await.unwrap();
        },
        future::Either::Right((sm_client_result, _p)) => {
            log_sm_client_result(sm_client_result);
        }
    }

    tracing::info!("Disconnected from SpectatorMode.");
}

fn log_forward_result(result: Result<(), WSError>) {
    match result {
        Ok(_) => tracing::debug!("Slippi stream finished successfully"),
        Err(e) => tracing::debug!("Slippi stream finished with error: {e:?}")
    }
}

fn log_sm_client_result(result: Result<(), Box<dyn Error + Send + Sync>>) {
    match result {
        Ok(_) => tracing::debug!("SpectatorMode connection "),
        Err(e) => tracing::debug!("Slippi stream finished with error: {e:?}")
    }
}
