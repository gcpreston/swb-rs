use std::{error::Error, net::SocketAddr, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use ezsockets::Bytes;
use futures::{channel::mpsc::channel, future, StreamExt};
use rusty_enet as enet;
use spectator_mode_client::WSError;
use tokio::task::LocalSet;
use tracing::Level;
use self_update::cargo_crate_version;

use crate::{dolphin_connection::DolphinConnection, spectator_mode_client::SpectatorModeClient};

mod dolphin_connection;
mod spectator_mode_client;

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

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            // It seems that both of these approaches work and have similar performance as far
            // as I can tell from most basic testing.
            // The problem is that performance is highly degraded based on the current real version.
            // This is most likely due to .lock() calls.
            // The bright side is it's evident that the viewer accurately skips to live when needed, lol.

            // TODO: Do this with a list of sources
            // tokio::spawn(async move {
                connect_and_forward_packets_until_completion(args.source.as_str(), args.dest.as_str()).await;
            // });

            // let s = LocalSet::new();
            // s.run_until(connect_and_forward_packets_until_completion(args.source.as_str(), args.dest.as_str())).await;
        });

    println!("\nGoodbye!");

    Ok(())
}

// IDEA: be able to forward multiple streams to SpectatorMode from one bridge
//
// Draft 1, changes required:
// - swb should be able to act as a server and take arbitrary connections to forward
//   * sounds like a router to be honest
// - SpectatorMode should be able to accept multiple streams from a single bridge connection
//
// Draft 1, feedback:
// - This just makes me think of internet routing. What if each client just maintains a TCP
//   connection to the server, through a series of routers, and sends packets at-will?
//   * problem: slippi nintendont isn't prepped for TCP in the same way arbitrary machines are
// - This approach relies on a single process' ability to push lots of data through a single channel.
//   Chances are this program will be run on a minimally powerful computer, as well (such as an RPi).
//   * Is this inherently faster or slower than having more TCP connections? I'd guess it relies
//     on how the OS schedules work
// - Multiplexing is complicated, maybe an abstraction already exists though
//
// Draft 2:
// - Single swb process, opens and closes TCP connections (websocket) to SpectatorMode at-will.
// - Each stream has its own connection
// - Most likely clarifies the state machine of figuring out active streams
//
// Draft 2, feedback:
// - How does the bottleneck work? Does it make a difference if many connections are established
//   on one machine from the same process vs. from different processes?
// - This most likely requires making the logic of this program thread-safe, so many copies can
//   be spawned at-will.


// IDEA for mutli-threading
// - might be able to implement more authoritative interrupts like this

async fn connect_and_forward_packets_until_completion(source: &str, dest: &str) {
    // Initiate connections.
    let (slippi_conn, mut slippi_interrupt) = connect_to_slippi(source).await;
    let (sm_client, sm_client_future) = spectator_mode_client::initiate_connection(dest).await;

    // Set up the futures to await.
    // Each individual future will attempt to gracefully disconnect the other.
    let dolphin_to_sm = forward_slippi_data(&slippi_conn, sm_client);
    let extended_sm_client_future = async {
        let result = sm_client_future.await;
        slippi_interrupt();
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


fn forward_slippi_data(slippi_conn: &DolphinConnection, sm_client: SpectatorModeClient) -> impl Future<Output = Result<(), WSError>> {
    slippi_conn
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
        .forward(sm_client)
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
