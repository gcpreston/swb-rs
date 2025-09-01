use std::{error::Error, io::Write, net::{AddrParseError, Ipv4Addr, SocketAddr}, num::ParseIntError, pin::Pin, str::FromStr, sync::{Arc, Mutex}};

use clap::{Args, Parser, Subcommand};
use futures::{channel::mpsc::channel, future, StreamExt};
use spectator_mode_client::WSError;
use thiserror::Error;
use tracing::Level;
use self_update::cargo_crate_version;
// use tokio::sync::mpsc;
use url::{Host, Url};

use crate::{common::SlippiDataStream, config::ConfigError};
use crate::spectate::slp_file_writer::SlpFileWriter;

mod broadcast;
mod spectate;
mod spectator_mode_client;
mod common;
mod config;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Don't attempt to auto-update.
    #[arg(long, global = true)]
    skip_update: bool,

    /// Log debug messages.
    #[arg(short, long, action, global = true)]
    verbose: bool
}

#[derive(Subcommand, Debug)]
enum Commands {
    Broadcast(Broadcast),
    Spectate(Spectate)
}

/// Stream one or multiple Slippi instances to SpectatorMode. 
#[derive(Args, Debug)]
struct Broadcast {
    /// The SpectatorMode WebSocket endpoint to connect and forward data to.
    #[arg(short, long, default_value = "wss://spectatormode.tv/bridge_socket/websocket")]
    dest: String,

    /// Slippi sources to forward data from, in the format schema://host:port.
    /// schema may be "console" or "dolphin", and defaults to "console" if 
    /// unspecified. Multiple sources may be given.
    #[arg(short, long, default_value = "dolphin://127.0.0.1:51441")]
    source: Vec<String>,
}

/// Mirror a stream in Playback Dolphin. This can consume a stream either from
/// SpectatorMode, or from an arbitrary source.
/// 
/// If an arbitrary source is provided, swb expects it to be a WebSocket server 
/// which (1) sends the entire .slp replay up to the current point upon 
/// connection, and (2) sends Slippi events as they come in after connection.
/// 
/// Messages must be sent from the server in binary mode as unwrapped Slippi
/// events. One message may contain multiple Slippi events, but a Slippi event
/// must not be split between multiple messages.
#[derive(Args, Debug)]
struct Spectate {
    /// The stream identifier. This can either be the stream ID from
    /// SpectatorMode, or a full WebSocket URL to the source.
    #[arg(value_parser = infer_stream_url)]
    stream_url: String
}

fn infer_stream_url(stream_param: &str) -> Result<String, ParseIntError> {
    if let Ok(_url) = Url::parse(stream_param) {
        return Ok(stream_param.to_string());
    }
    
    let stream_id = u32::from_str(stream_param)? ;
    let sm_url = format!("wss://spectatormode.tv/viewer_socket/websocket?stream_id={}&full_replay=true", stream_id);
    Ok(sm_url)
}

// TODO: Coerce various specific errors into user-friendly high-level errors
#[derive(Error, Debug)]
pub(crate) enum SwbError {
    #[error("Config error: {0}")]
    ConfigError(#[from] ConfigError),

    #[error("Error parsing socket address: {0}")]
    SocketAddrParseError(#[from] AddrParseError),

    #[error("URL parse error: {0}")]
    URLParseError(#[from] url::ParseError),

    #[error("Unknown source scheme: {0}")]
    UnknownSourceScheme(String),
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
    let args = Cli::parse();
    println!("parsed args {:?}", args);

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
            let result = 
                match &args.command {
                    Commands::Broadcast(b) => {
                        connect_and_forward_packets_until_completion(&b.source, b.dest.as_str()).await
                    }
                    Commands::Spectate(s) => {
                        mirror_to_dolphin(s.stream_url.as_str()).await
                    }
                };

            if let Err(err) = result {
                tracing::error!("{}", err);
            }
        });

    println!("\nGoodbye!");

    Ok(())
}

async fn mirror_to_dolphin(stream_url: &str) -> Result<(), SwbError> {
    // let (interrupt_sender, mut interrupt_receiver) = mpsc::channel::<bool>(100);
    let stream_conn = spectate::websocket_connection::data_stream(stream_url).await;
    let mut playback_writer = SlpFileWriter::new(true)?;
    
    // let mut already_interrupted = false;
    // ctrlc::set_handler(move || {
    //     if already_interrupted {
    //         std::process::exit(2);
    //     } else {
    //         already_interrupted = true;
    //         interrupt_sender.try_send(true).unwrap();
    //     }
    // })
    // .unwrap();

    stream_conn.map(|data| {
        // This is assuming that no events are split between stream items
        playback_writer.write_all(&data).unwrap();
    }).collect::<()>().await;

    spectate::playback_dolphin::close_playback_dolphin();

    Ok(())
}

// TODO: Bubble existing errors
async fn connect_and_forward_packets_until_completion(sources: &Vec<String>, dest: &str) -> Result<(), SwbError>  {
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
        let parsed_url = Url::parse(string_to_parse.as_str())?;

        let scheme = parsed_url.scheme();
        let host = parsed_url.host().unwrap_or(Host::Ipv4(Ipv4Addr::from_str("127.0.0.1").unwrap()));
        let port = parsed_url.port().unwrap_or(51441);

        tracing::debug!("using url scheme: {:?}, host: {:?}, port: {:?}", scheme, host, port);

        let socket_addr_string = format!("{}:{}", host, port);
        let source_addr = SocketAddr::from_str(socket_addr_string.as_str())?;
        let is_console =
            match scheme {
                "console" => Ok(true),
                "dolphin" => Ok(false),
                other_scheme => Err(SwbError::UnknownSourceScheme(other_scheme.to_string()))
            }?;

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

    Ok(())
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
    tracing::info!("Connected to Slippi.");

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
