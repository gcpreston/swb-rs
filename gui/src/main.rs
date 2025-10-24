use iced::widget::{button, column, container, text, text_input};
use iced::Alignment::Center;
use iced::{Fill, Element};

#[derive(Debug, Clone)]
enum Message {
    Broadcast,
    Spectate(u32),
    Stop,
    ContentChanged(String)
}

enum State {
    Standby(String), // Spectate stream ID being inputted
    Broadcasting(String, u32), // Bridge ID, Stream ID
    Spectating(u32) // Stream ID
}

impl Default for State {
    fn default() -> Self {
        State::Standby("".to_string())
    }
}

pub fn main() -> iced::Result {
    iced::application("SpectatorMode Client", update, view)
        .window_size(iced::Size::new(300.0, 400.0))
        .run()
}

fn update(state: &mut State, message: Message) {
    match message {
        Message::Broadcast => {
            *state = State::Broadcasting("".to_string(), 0);

            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();

            let _handle = rt.spawn(async {
                println!("In the inside thread");
                let result = connect_and_forward_packets_until_completion(
                    &vec!["dolphin://127.0.0.1:51441".to_string()],
                    "ws://localhost:4000/bridge_socket/websocket"
                ).await;

                if let Err(err) = result {
                    println!("Error {:?}", err);
                    tracing::error!("{}", err);
                }
            });
        },
        Message::Spectate(stream_id) => {
            *state = State::Spectating(stream_id);
        },
        Message::Stop => {
            *state = State::default();
        },
        Message::ContentChanged(new_content) => {
            *state = State::Standby(new_content);
        }
    }
}

fn view(state: &State) -> Element<'_, Message> {
    let buttons =
        match state {
            State::Standby(content) => column![
                button("Broadcast").on_press(Message::Broadcast),
                button("Spectate").on_press_maybe(
                    if let Ok(stream_id) = content.parse() {
                        Some(Message::Spectate(stream_id))
                    } else {
                        None
                    }
                ),
                text_input("Stream ID to spectate...", &content).on_input(Message::ContentChanged)
            ],
            State::Broadcasting(_bridge_id, stream_id) => column![
                text(format!("Broadcasting with stream ID: {stream_id}"),).size(20),
                button("Stop broadcast").on_press(Message::Stop)
            ],
            State::Spectating(stream_id) => column![
                text(format!("Spectating stream ID: {stream_id}")).size(20),
                button("Stop spectate").on_press(Message::Stop)
            ]
        };

    container(
        buttons.align_x(Center)
        .spacing(10)
    )
    .padding(10)
    .center_x(Fill)
    .center_y(Fill)
    .into()
}


// ===================================
// Just copied from cli for the moment
// ===================================

use std::time::Duration;
use std::{net::{AddrParseError, Ipv4Addr, SocketAddr}, pin::Pin, str::FromStr, sync::{Arc, Mutex}};
use url::{Host, Url};
use thiserror::Error;
use futures::{channel::mpsc::channel, future, StreamExt};

use swb::{broadcast, common::SlippiDataStream, config::ConfigError, spectator_mode_client};

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

    #[error("SpectatorMode connection error: {0}")]
    SpectatorModeClientError(#[from] spectator_mode_client::SpectatorModeClientError)
}

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

    // TODO: Move this to Messages::Stop
    // ctrlc::set_handler(move || {
    //     if already_interrupted {
    //         std::process::exit(2);
    //     } else {
    //         tracing::info!("Shutting down gracefully... press Ctrl + C again to force exit.");
    //         already_interrupted = true;
    //         for interrupt in slippi_interrupts.lock().unwrap().iter_mut() {
    //             interrupt();
    //         }
    //     }
    // })
    // .unwrap();

    let (sm_client, mut sm_connection_monitor, bridge_info) = spectator_mode_client::initiate_connection(dest, sources.len()).await?;

    // Set up the futures to await.
    // Each individual future will attempt to gracefully disconnect the other.
    let merged_stream = broadcast::connection_manager::merge_slippi_streams(slippi_conns, bridge_info.stream_ids).unwrap();
    let dolphin_to_sm = broadcast::connection_manager::forward_slippi_data(merged_stream, sm_client);

    let sm_connection_future = async {
        let sm_client_result = sm_connection_monitor.wait_for_close().await;
        tracing::debug!("SpectatorMode connection has finished, cleaning up...");
        slippi_interrupts_clone.lock().unwrap().iter_mut().for_each(|interrupt| interrupt());
        sm_client_result
    };

    // Run until both futures complete.
    let (slippi_to_sm_result, sm_client_result) = future::join(dolphin_to_sm, sm_connection_future).await;

    slippi_to_sm_result?;
    tracing::debug!("Slippi stream finished successfully");
    sm_client_result?;
    tracing::debug!("SpectatorMode connection finished successfully");

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
