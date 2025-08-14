use std::{error::Error, net::SocketAddr, str::FromStr};

use clap::Parser;
use futures::{channel::mpsc::channel, future};
use rusty_enet as enet;
use spectator_mode_client::WSError;
use tracing::Level;
use self_update::cargo_crate_version;

use crate::{dolphin_connection::DolphinConnection};

mod dolphin_connection;
mod spectator_mode_client;
mod connection_manager;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    skip_update: bool,

    #[arg(short, long, default_value = "wss://spectatormode.tv/bridge_socket/websocket")]
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
    let args = Args::parse();

    if !args.skip_update {
        let update_status = update_if_needed()?;

        if update_status.updated() {
            println!("\nUpdate complete, please relaunch swb.");
            return Ok(());
        }
    }

    if args.verbose {
        tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
    } else {
        tracing_subscriber::fmt().with_env_filter("swb=info").init();
    };

    println!("[CTRL + C to quit]\n");

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            connect_and_forward_packets_until_completion(args.source.as_str(), args.dest.as_str()).await;
        });

    println!("\nGoodbye!");

    Ok(())
}

async fn connect_and_forward_packets_until_completion(source: &str, dest: &str) {
    // Initiate connections.
    let (slippi_conn, mut slippi_interrupt) = connect_to_slippi(source).await;
    let (sm_client, sm_client_future, bridge_info) = spectator_mode_client::initiate_connection(dest, 1).await;
    let mut slippi_interrupts = vec![&mut slippi_interrupt];

    // Set up the futures to await.
    // Each individual future will attempt to gracefully disconnect the other.
    let merged_stream = connection_manager::merge_slippi_streams(vec![&slippi_conn], bridge_info.stream_ids).unwrap();
    let dolphin_to_sm = connection_manager::forward_slippi_data(merged_stream, sm_client);
    let extended_sm_client_future = async {
        let result = sm_client_future.await;
        slippi_interrupts.iter_mut().for_each(|interrupt| interrupt());
        result
    };

    // Run until both futures complete.
    let (slippi_to_sm_result, sm_client_result) = future::join(dolphin_to_sm, extended_sm_client_future).await;

    log_forward_result(slippi_to_sm_result);
    log_sm_client_result(sm_client_result);
}

async fn connect_to_slippi(source_addr: &str) -> (DolphinConnection, impl FnMut() -> ()) {
    let (mut sender, receiver) = channel::<enet::PeerID>(100);
    let mut other_sender = sender.clone();

    let mut already_interrupted = false;

    let conn = dolphin_connection::DolphinConnection::new(receiver);
    let address = SocketAddr::from_str(source_addr).expect("Invalid socket address");
    tracing::info!("Connecting to Slippi...");
    let peer_id = conn.initiate_connection(address);
    conn.wait_for_connected().await;
    tracing::info!("Connected to Slippi.");

    let interruptor_to_return = move || {
        other_sender.try_send(peer_id).unwrap();
    };

    ctrlc::set_handler(move || {
        if already_interrupted {
            std::process::exit(2);
        } else {
            already_interrupted = true;
            sender.try_send(peer_id).unwrap();
        }
    })
    .unwrap();

    (conn, interruptor_to_return)
}

fn log_forward_result(result: Result<(), WSError>) {
    match result {
        Ok(_) => tracing::debug!("Slippi stream finished successfully"),
        Err(e) => tracing::debug!("Slippi stream finished with error: {e:?}")
    }
}

fn log_sm_client_result(result: Result<(), Box<dyn Error + Send + Sync>>) {
    match result {
        Ok(_) => tracing::debug!("SpectatorMode connection finished successfully"),
        Err(e) => tracing::debug!("SpectatorMode connection finished with error: {e:?}")
    };
    tracing::info!("Disconnected from SpectatorMode.");
}
