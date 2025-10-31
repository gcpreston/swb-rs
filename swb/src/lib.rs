use std::io::Write;
use std::net::{AddrParseError, SocketAddr};
use std::pin::Pin;

use futures::StreamExt;
use futures::{channel::mpsc::channel};
use thiserror::Error;

use crate::common::SlippiDataStream;

pub mod broadcast;
pub mod spectate;
pub mod spectator_mode_client;
pub mod common;
pub mod config;

#[derive(Error, Debug)]
pub enum SwbError {
    #[error("Config error: {0}")]
    ConfigError(#[from] config::ConfigError),

    #[error("Error parsing socket address: {0}")]
    SocketAddrParseError(#[from] AddrParseError),

    #[error("URL parse error: {0}")]
    URLParseError(#[from] url::ParseError),

    #[error("Unknown source scheme: {0}")]
    UnknownSourceScheme(String),

    #[error("SpectatorMode connection error: {0}")]
    SpectatorModeClientError(#[from] spectator_mode_client::SpectatorModeClientError)
}

pub async fn connect_to_slippi(source_addr: SocketAddr, is_console: bool) -> (Pin<Box<SlippiDataStream>>, impl FnMut()) {
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

// TODO: Exit automatically when stream being watched is finished
pub async fn mirror_to_dolphin(stream_url: &str) -> Result<(), SwbError> {
    // let (interrupt_sender, mut interrupt_receiver) = mpsc::channel::<bool>(100);
    let stream_conn = spectate::websocket_connection::data_stream(stream_url).await;
    let mut playback_writer = spectate::slp_file_writer::SlpFileWriter::new(true)?;

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

pub use spectator_mode_client::initiate_spectatormode_connection;
pub use broadcast::connection_manager::forward_streams;
