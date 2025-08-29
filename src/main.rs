use std::{error::Error, net::{Ipv4Addr, SocketAddr}, path::Path, pin::Pin, str::FromStr, sync::{Arc, Mutex}};

use clap::Parser;
use futures::{channel::mpsc::channel, future, SinkExt, StreamExt};
use spectator_mode_client::WSError;
use tracing::Level;
use self_update::cargo_crate_version;
use url::{Host, Url};

use crate::{common::SlippiDataStream, slp_file_writer::SlpStreamEvent};
use crate::slp_file_writer::SlpFileWriter;

mod broadcast;
mod spectate;
mod spectator_mode_client;
mod slp_file_writer;
mod common;

#[derive(Parser, Debug)]
#[command(version, about, long_about)]
struct Args {
    #[arg(long)]
    skip_update: bool,

    /// The SpectatorMode WebSocket endpoint to connect and forward data to.
    #[arg(short, long, default_value = "wss://spectatormode.tv/bridge_socket/websocket")]
    dest: String,

    /// Slippi sources to forward data from, in the format schema://host:port.
    /// schema may be "console" or "dolphin". Multiple may be specified.
    #[arg(short, long, default_value = "dolphin://127.0.0.1:51441")]
    source: Vec<String>,

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
            connect_and_forward_packets_until_completion(&args.source, args.dest.as_str()).await;
        });

    println!("\nGoodbye!");

    Ok(())
}

async fn mirror_to_dolphin(stream_url: &str) {
    let (mut interrupt_sender, interrupt_receiver) = channel::<bool>(100);
    let stream_conn = spectate::websocket_connection::data_stream(stream_url, interrupt_receiver).await;
    let file_writer = SlpFileWriter::new();
    spectate::playback_dolphin::launch_playback_dolphin();
    
    let mut already_interrupted = false;
    ctrlc::set_handler(move || {
        if already_interrupted {
            std::process::exit(2);
        } else {
            already_interrupted = true;
            interrupt_sender.send(true);
        }
    })
    .unwrap();

    let _: Vec<()> =
        stream_conn.map(|data| {
            // This is assuming that no events are split between stream items
            let events = file_writer.write(data);

            for event in events {
                match event {
                    SlpStreamEvent::StartGame(fp) => {
                        spectate::playback_dolphin::mirror_file(fp);
                    }
                    SlpStreamEvent::EndGame => {
                        // don't actually think i need to do anything here
                    }
                }
            }
        }).collect().await;

    spectate::playback_dolphin::close_playback_dolphin();
}

async fn connect_and_forward_packets_until_completion(sources: &Vec<String>, dest: &str) {
    // Initiate connections.
    let mut slippi_conns = vec![];
    let mut slippi_interrupts = vec![];
    let sources_owned = sources.clone();
    let mut already_interrupted = false;

    for source_string in sources_owned {
        let string_to_parse =
            if !source_string.contains("://") {
                format!("{}{}", "console://", source_string)
            } else {
                source_string
            };
        let parsed_url = Url::parse(string_to_parse.as_str()).expect("Invalid URL scheme");

        let scheme = parsed_url.scheme();
        let host = parsed_url.host().unwrap_or(Host::Ipv4(Ipv4Addr::from_str("127.0.0.1").unwrap()));
        let port = parsed_url.port().unwrap_or(51441);

        tracing::debug!("using url scheme: {:?}, host: {:?}, port: {:?}", scheme, host, port);

        let socket_addr_string = format!("{}:{}", host, port);
        let source_addr = SocketAddr::from_str(socket_addr_string.as_str()).expect("Invalid socket address");
        let is_console =
            match scheme {
                "console" => true,
                "dolphin" => false,
                _ => {
                    tracing::error!("Invalid Slippi platform scheme");
                    return;
                }
            };

        let (slippi_conn, slippi_interrupt) = connect_to_slippi(source_addr, is_console).await;
        slippi_conns.push(slippi_conn);
        slippi_interrupts.push(slippi_interrupt);
    }

    let slippi_interrupts = Arc::new(Mutex::new(slippi_interrupts));
    let slippi_interrupts_clone = Arc::clone(&slippi_interrupts);

    ctrlc::set_handler(move || {
        if already_interrupted {
            std::process::exit(2);
        } else {
            already_interrupted = true;
            for interrupt in slippi_interrupts.lock().unwrap().iter_mut() {
                interrupt();
            }
        }
    })
    .unwrap();

    let (sm_client, sm_client_future, bridge_info) = spectator_mode_client::initiate_connection(dest, sources.len()).await;

    // Set up the futures to await.
    // Each individual future will attempt to gracefully disconnect the other.
    let merged_stream = broadcast::connection_manager::merge_slippi_streams(slippi_conns, bridge_info.stream_ids).unwrap();
    let dolphin_to_sm = broadcast::connection_manager::forward_slippi_data(merged_stream, sm_client);
    let extended_sm_client_future = async {
        let result = sm_client_future.await;
        slippi_interrupts_clone.lock().unwrap().iter_mut().for_each(|interrupt| interrupt());
        result
    };

    // Run until both futures complete.
    let (slippi_to_sm_result, sm_client_result) = future::join(dolphin_to_sm, extended_sm_client_future).await;

    log_forward_result(slippi_to_sm_result);
    log_sm_client_result(sm_client_result);
}

async fn connect_to_slippi(source_addr: SocketAddr, is_console: bool) -> (Pin<Box<SlippiDataStream>>, impl FnMut()) {
    let (sender, receiver) = channel::<bool>(100);
    let mut other_sender = sender.clone();

    tracing::info!("Connecting to Slippi {} at {}...", if is_console { "console" } else { "Dolphin" }, source_addr);
    let conn  =
        if is_console {
            broadcast::console_connection::data_stream(source_addr, receiver).await
        } else {
            broadcast::dolphin_connection::data_stream(source_addr, receiver).await
        };

    let interruptor_to_return = move || {
        match other_sender.try_send(true) {
            Ok(_) => tracing::debug!("interrupt sent"),
            Err(_) => tracing::debug!("sender already disconnected")
        }
    };

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
