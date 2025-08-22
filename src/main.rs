use std::{cell::RefCell, error::Error, io::{Read, Write}, net::SocketAddr, str::FromStr, sync::{Arc, Mutex}, thread::sleep};
use std::io::Cursor;

use clap::Parser;
use futures::{channel::mpsc::channel, future, StreamExt};
use rusty_enet as enet;
use spectator_mode_client::WSError;
use tracing::Level;
use self_update::cargo_crate_version;
use ubjson_rs::deserializer::UbjsonDeserializer;

use crate::{console_connection::SlippiStream, dolphin_connection::DolphinConnection};

mod dolphin_connection;
mod console_connection;
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
            // connect_and_forward_packets_until_completion(&args.source, args.dest.as_str()).await;
            test_console_connection().await;
        });

    println!("\nGoodbye!");

    Ok(())
}

async fn test_console_connection() {
    let mut conn = console_connection::ConsoleConnection::connect("192.168.2.2:51441").unwrap();
    let mut total_buf: Vec<u8> = Vec::new();
    let mut leftover = Cursor::new(Vec::new());

    // Receive and parse loop
    loop {
        let mut msg_size_buf = [0 as u8; 4];

        let read_result =
            match leftover.read_exact(&mut msg_size_buf) {
                Ok(amount) => {
                    println!("got from leftover");
                    Ok(amount)
                },
                Err(e) => {
                    println!("got from conn");
                    conn.stream.read_exact(&mut msg_size_buf)
                }
            };

        match read_result {
            Ok(_) => {
                let msg_size = u32::from_be_bytes(msg_size_buf);
                let mut bytes_remaining: usize = msg_size as usize;
                println!("Receiving message of size {:?} ({:?}), remaining: {:?}", msg_size, msg_size_buf, bytes_remaining);

                while bytes_remaining > 0 {
                    let mut local_buf = [0 as u8; 1024];
                    let leftover_bytes_read = leftover.read(&mut local_buf).unwrap();
                    let bytes_read =
                        if leftover_bytes_read > 0 {
                            leftover_bytes_read
                        } else {
                            conn.stream.read(&mut local_buf).unwrap()
                        };

                    println!("Read bytes {:?}, left {:?}", bytes_read, bytes_remaining);

                    if bytes_read > bytes_remaining {
                        total_buf.append(&mut local_buf[..bytes_remaining].to_vec());
                        let leftover_range = bytes_remaining..bytes_read;
                        let leftover_slice = &local_buf[leftover_range];
                        println!("leftover_range length {:?}", bytes_read - bytes_remaining - 1);
                        println!("Set leftover to {:?} (length {:?})", leftover_slice, leftover_slice.len());
                        // leftover.write(leftover_slice).unwrap();
                        // leftover.set_position(0);
                        leftover = Cursor::new(leftover_slice.to_vec());
                        bytes_remaining = 0;
                    } else {
                        total_buf.append(&mut local_buf[0..bytes_read].to_vec());
                        bytes_remaining -= bytes_read;
                    }
                }

                // Message received; deserialize
                println!("Received message: {:?}, length {:?}", total_buf, total_buf.len());
                let mut  deserializer = UbjsonDeserializer::new(Cursor::new(total_buf.clone()));
                let result = deserializer.deserialize_value();
                println!("deserialized: {:?}", result);

                total_buf.clear();
                sleep(std::time::Duration::from_millis(10));
            }

            Err(e) => {
                println!("got error on read_exact: {:?}", e);
                sleep(std::time::Duration::from_millis(100));
            }
        }

    }

    // conn.event_stream().map(|buf| {
    //     let mut i = buf.iter();
    //     i.next();
    //     i.next();
    //     i.next();
    //     i.next();

    //     println!("got buf: {:?}", i);
    //     // let chars: Vec<char> = buf.iter().map(|n| {
    //     //     match char::from_u32((*n).into()) {
    //     //         None => ' ',
    //     //         Some('\0') => ' ',
    //     //         Some(c) => c
    //     //     }
    //     // }).collect();
    //     // let s: String = chars.into_iter().collect();
    //     // println!("translated buf: {:?}", s);

    //     let mut  deserializer = UbjsonDeserializer::new(Cursor::new(i));
    //     let result = deserializer.deserialize_value();
    //     println!("deserialized: {:?}", result)

    //     /* PROCESS
    //      * 1. Read uint32 big endian for byte size of message
    //      * 2. Read that many bytes into buffer
    //      * 3. Parse that many bytes as UBJSON; handle
    //      *
    //      * UBJSON is made so that it can be streamed, but since the JS library
    //      * doesn't offer this capability, they had to put another layer on top
    //      * to ensure full-message parsing in JS. Could make my job easier,
    //      * since no need to implement UBJSON streaming, just full ser/de.
    //      *
    //      * They seem to use msgSize + 4 in slippi-js though, TODO figure out why
    //      *
    //      * Each message is parsed as CommunicationMessage type:
    //      *
    //        export enum CommunicationType {
    //         HANDSHAKE = 1,
    //         REPLAY = 2,
    //         KEEP_ALIVE = 3,
    //        }

    //        export type CommunicationMessage = {
    //         type: CommunicationType;
    //         payload: {
    //             cursor: Uint8Array;
    //             clientToken: Uint8Array;
    //             pos: Uint8Array;
    //             nextPos: Uint8Array;
    //             data: Uint8Array;
    //             nick: string | null;
    //             forcePos: boolean;
    //             nintendontVersion: string | null;
    //         };
    //        };

    //      * The type is not 100% accurate, for example the keep-alive message
    //      * is just { type: 3 }.
    //      */
    // }).collect::<()>().await;
}

async fn connect_and_forward_packets_until_completion(sources: &Vec<String>, dest: &str) {
    // Initiate connections.
    let mut slippi_conns: Vec<DolphinConnection> = vec![];
    let mut slippi_interrupts = vec![];
    let sources_owned = sources.clone();
    let mut already_interrupted = false;

    for source in sources_owned {
        let (slippi_conn, slippi_interrupt) = connect_to_slippi(source).await;
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
    let merged_stream = connection_manager::merge_slippi_streams(&slippi_conns, bridge_info.stream_ids).unwrap();
    let dolphin_to_sm = connection_manager::forward_slippi_data(merged_stream, sm_client);
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

async fn connect_to_slippi(source_addr: String) -> (DolphinConnection, impl FnMut()) {
    let (sender, receiver) = channel::<enet::PeerID>(100);
    let mut other_sender = sender.clone();

    let conn = dolphin_connection::DolphinConnection::new(receiver);
    let address = SocketAddr::from_str(source_addr.as_str()).expect("Invalid socket address");
    tracing::info!("Connecting to Slippi...");
    let peer_id = conn.initiate_connection(address);
    conn.wait_for_connected().await;
    tracing::info!("Connected to Slippi.");

    let interruptor_to_return = move || {
        other_sender.try_send(peer_id).unwrap();
    };

    (conn, interruptor_to_return.clone())
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
