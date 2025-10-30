// For receiving external messages in multithreaded iced application:
// https://stackoverflow.com/questions/74737759/how-to-send-messages-to-iced-application
// Subscription::run: https://docs.rs/iced/latest/iced/struct.Subscription.html
// - Really want a stream of connection events to subscribe to: connected, error, disconnected, etc, not the data

use futures::SinkExt;
use iced::widget::{button, column, container, text, text_input};
use iced::futures::{channel::mpsc, Stream};
use iced::Alignment::Center;
use iced::{stream, Element, Fill, Subscription};

#[derive(Debug, Clone)]
enum Message {
    Broadcast,
    Spectate(u32),
    Stop,
    ContentChanged(String),
    ConnectionMessage(Event)
}

#[derive(Debug)]
enum State {
    Standby(String), // Spectate stream ID being inputted
    Broadcasting(String, u32, Option<mpsc::Sender<Event>>), // Bridge ID, Stream ID, disconnect interrupt
    Spectating(u32) // Stream ID
}

impl Default for State {
    fn default() -> Self {
        State::Standby("".to_string())
    }
}

fn main() -> iced::Result {
    iced::application("SpectatorMode Client", update, view)
        .window_size(iced::Size::new(300.0, 400.0))
        .subscription(subscription)
        .run()
}

fn update(state: &mut State, message: Message) {
    println!("Update handling message {:?}", message);

    match message {
        Message::Broadcast => {
            /*
            let _handle = tokio::spawn(async {
                let result = connect_and_forward_packets_until_completion(
                    &vec!["dolphin://127.0.0.1:51441".to_string()],
                    "ws://localhost:4000/bridge_socket/websocket"
                ).await;

                if let Err(err) = result {
                    tracing::error!("{}", err);
                }
            });
             */
            println!("Pretend broadcasting");
            *state = State::Broadcasting("".to_string(), 0, None);
        },
        Message::Spectate(stream_id) => {
            *state = State::Spectating(stream_id);
        },
        Message::Stop => {
            println!("What is state {:?}", state);
            if let State::Broadcasting(_bridge_id, _stream_id, maybe_sender) = state {
                if let Some(sender) = maybe_sender {
                    println!("Trying to send DC req");
                    sender.try_send(Event::DisconnectRequest).unwrap();
                }
            }
            // *state = State::Standby("".to_string());
        },
        Message::ContentChanged(new_content) => {
            *state = State::Standby(new_content);
        },
        Message::ConnectionMessage(event) => {
            println!("Got swb connection event {:?}", event);

            if let Event::Connected(sender) = event {
                *state = State::Broadcasting("".to_string(), 0, Some(sender));
            }
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
            State::Broadcasting(_bridge_id, stream_id, _maybe_interrupt) => column![
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

#[derive(Debug, Clone)]
pub enum Event {
    Connected(mpsc::Sender<Event>),
    Disconnected,
    DisconnectRequest,
    DCACK
}

fn subscription<P>(_state: &P) -> Subscription<Message> {
    Subscription::run(stream_channel_test).map(|x| {
        println!("got result from subscription {:?}", x);
        Message::ConnectionMessage(x)
    })
}

enum SwbConnState {
    Disconnected,
    Connected(Pin<Box<SlippiDataStream>>, mpsc::Receiver<Event>, Box<dyn FnMut() + Send>)
}

fn stream_channel_test() -> impl Stream<Item = Event> {
    stream::channel(100, |mut output| async move {
        let (sender, mut receiver) = mpsc::channel(100);

        output.send(Event::Connected(sender)).await.unwrap();
        println!("Sent to output");

        loop {
            if let Some(event) = receiver.next().await {
                println!("Received in channel test {:?}", event);
                output.send(Event::DCACK).await.unwrap();
            }
        }
    })
}

fn connect_and_forward_data() -> impl Stream<Item = Event> {
    stream::channel(100, |mut output: mpsc::Sender<Event>| async move {
        let mut state = SwbConnState::Disconnected;

        loop {
            match &mut state {
                SwbConnState::Disconnected => {
                    let source_addr = SocketAddr::from_str("127.0.0.1:51441").unwrap();
                    let (slippi_conn, slippi_interrupt) = connect_to_slippi(source_addr, false).await;

                    // This is the sender/receiver for the main thread to tell things to this sub-thread
                    // Specifically, to initiate a disconnect request
                    let (sender, receiver) = mpsc::channel(100);
                    println!("Got slippi conn; sending sender {:?}", sender);

                    let _ = output
                        .send(Event::Connected(sender))
                        .await
                        .unwrap();
                    println!("Sent sender");

                    state = SwbConnState::Connected(slippi_conn, receiver, Box::new(slippi_interrupt));
                },
                SwbConnState::Connected(slippi_conn, receiver, slippi_interrupt) => {
                    /* Choose between
                     * - Receiving connection event (disconnect successful)
                     * - Receive event from parent (disconnect request)
                     */

                    // TODO: Going to want another layer of abstraction because we don't care about slippi data
                    //   at all, and don't want to select it, just want to select connection events
                    let mut fused_slippi_conn = slippi_conn.by_ref().fuse();

                    futures::select! {
                        // TODO: Can probably use select_next_some for a clean solution but still a bit confused by it
                        received = fused_slippi_conn.next() => {
                            match received {
                                Some(data) => {
                                    // TODO: Forward to SM
                                    tracing::debug!("Got Slippi data of size {:?}", data.len());
                                },

                                None => {
                                    output.send(Event::Disconnected).await.unwrap();
                                    state = SwbConnState::Disconnected;
                                }
                            }
                        }

                        message = receiver.select_next_some() => {
                            match message {
                                // These 2 aren't valid
                                Event::Connected(_sender) => todo!(),

                                Event::Disconnected => todo!(),

                                Event::DCACK => todo!(),

                                // This one matters
                                Event::DisconnectRequest => {
                                    slippi_interrupt();
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

// ===================================
// Just copied from cli for the moment
// ===================================

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
    // let mut already_interrupted = false;

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
