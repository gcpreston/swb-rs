use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use futures::{SinkExt, StreamExt};
use iced::{stream, Element, Fill, Subscription};
use iced::widget::{button, column, text, container, text_input};
use iced::Alignment::Center;
use iced::futures::{future, Stream};
use iced::futures::channel::mpsc;

use swb::spectator_mode_client::BridgeInfo;

pub fn main() -> iced::Result {
    iced::application("SpectatorMode Client", SwbGui::update, SwbGui::view)
        .window_size(iced::Size::new(400.0, 300.0))
        .subscription(SwbGui::subscription)
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Broadcast,
    Spectate(u32),
    Stop,
    SwbLibMessage(SwbLibEvent),
    SpectateMessage(SpectateEvent),
    SpectateTextFieldChanged(String)
}

/// Events sent from the swb lib, received by the client application.
#[derive(Debug, Clone)]
enum SwbLibEvent {
    SlippiConnected,
    BroadcastStarted(BridgeInfo, mpsc::Sender<SwbLibSignal>),
    BroadcastStopped
}

#[derive(Debug, Clone)]
enum SpectateEvent {
    Started(u32),
    Stopped
}

/// Inputs to the swb lib from the client application.
#[derive(Debug)]
enum SwbLibSignal {
    StopRequest
}

#[derive(Debug)]
enum State {
    Standby(String), // Entered stream ID
    SlippiConnecting,
    SpectatorModeConnecting,
    Broadcasting(BridgeInfo, mpsc::Sender<SwbLibSignal>),
    Spectating(u32)
}

struct SwbGui {
    state: State
}

impl SwbGui {
    fn new() -> Self {
        Self {
            state: State::Standby(String::new()),
        }
    }

    fn update(&mut self, message: Message) -> () {
        println!("Processing update message {:?}", message);

        match message {
            Message::Broadcast => {
                self.state = State::SlippiConnecting;
            },

            Message::Spectate(stream_id) => {
                self.state = State::Spectating(stream_id);
            }

            Message::Stop => {
                if let State::Broadcasting(_, interrupt) = &mut self.state {
                    interrupt.try_send(SwbLibSignal::StopRequest).unwrap();
                } else {
                    // If not fully broadcasting, stop request won't do anything.
                    // Instead, we can simply drop the subscription and the thread will be cleaned up.
                    self.state = State::Standby(String::new());
                }
            }

            Message::SwbLibMessage(event) => {
                println!("Received swb event: {:?}", event);

                match event {
                    SwbLibEvent::SlippiConnected => {
                        self.state = State::SpectatorModeConnecting;
                    }

                    SwbLibEvent::BroadcastStarted(bridge_info, interrupt_sender) => {
                        self.state = State::Broadcasting(bridge_info, interrupt_sender);
                    }

                    SwbLibEvent::BroadcastStopped => {
                        self.state = State::Standby(String::new());
                    }
                }
            }

            Message::SpectateMessage(event) => {
                println!("Received spectate event: {:?}", event);

                match event {
                    SpectateEvent::Started(stream_id) => {
                        println!("Spectate started, stream id {stream_id}");
                    }

                    SpectateEvent::Stopped => {
                        self.state = State::Standby(String::new());
                    }
                }
            }

            Message::SpectateTextFieldChanged(new_text) => {
                self.state = State::Standby(new_text);
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let buttons = match &self.state {
            State::Standby(entered_stream_id) => column![
                button("Broadcast").on_press(Message::Broadcast),
                text_input("Stream ID to spectate...", entered_stream_id).on_input(Message::SpectateTextFieldChanged),
                button("Spectate").on_press_maybe(
                    if let Ok(stream_id) = entered_stream_id.parse() {
                        Some(Message::Spectate(stream_id))
                    } else {
                        None
                    }
                )
            ],
            State::SlippiConnecting => column![
                text(format!("Connecting to Slippi...")).size(20),
                button("Stop broadcast").on_press(Message::Stop)
            ],
            State::SpectatorModeConnecting => column![
                text(format!("Slippi connected; connecting to SpectatorMode...")).size(20),
                button("Stop broadcast").on_press(Message::Stop)
            ],
            State::Broadcasting(bridge_info, _interrupt) => column![
                text(format!("Broadcasting with stream ID {}", bridge_info.stream_ids[0])).size(20),
                button("Stop broadcast").on_press(Message::Stop)
            ],
            State::Spectating(stream_id) => column![
                text(format!("Spectating stream ID {stream_id}")).size(20),
                button("Stop spectating").on_press(Message::Stop)
            ]
        };

        container(buttons.align_x(Center).spacing(10))
            .padding(10)
            .center_x(Fill)
            .center_y(Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        match self.state {
            State::Standby(_) => Subscription::none(),
            State::Spectating(stream_id) => Subscription::run_with_id(123, spectate(stream_id)).map(Message::SpectateMessage),
            _ => Subscription::run(broadcast).map(Message::SwbLibMessage)
        }
    }
}

impl Default for SwbGui {
    fn default() -> Self {
        SwbGui::new()
    }
}

fn broadcast() -> impl Stream<Item = SwbLibEvent> {
    stream::channel(100, |mut output| async move {
        use iced::futures::SinkExt;

        let source = "127.0.0.1:51441";
        let dest = "ws://localhost:4000/bridge_socket/websocket";
        let source_addr = SocketAddr::from_str(source).unwrap();

        let (slippi_conn, slippi_interrupt) = swb::connect_to_slippi(source_addr, false).await;
        output.send(SwbLibEvent::SlippiConnected).await.unwrap();
        let (sm_client, mut sm_connection_monitor, bridge_info) = swb::initiate_spectatormode_connection(dest, 1).await.unwrap();

        // This is the sender/receiver for the main thread to tell things to this sub-thread
        // Specifically, to initiate a disconnect request
        let (sender, mut receiver) = mpsc::channel(100);
        output.send(SwbLibEvent::BroadcastStarted(bridge_info.clone(), sender)).await.unwrap();

        // Set up the futures to await.
        // Each individual future will attempt to gracefully disconnect the other.
        let dolphin_to_sm = swb::forward_streams(vec![slippi_conn], bridge_info.stream_ids, sm_client);

        let slippi_interrupt = Arc::new(Mutex::new(slippi_interrupt));
        let slippi_interrupt_clone = Arc::clone(&slippi_interrupt);

        let sm_connection_future = async {
            let sm_client_result = sm_connection_monitor.wait_for_close().await;
            tracing::debug!("SpectatorMode connection has finished, cleaning up...");
            slippi_interrupt.lock().unwrap()();
            sm_client_result
        };

        // Handle receiver messages in parallel.
        tokio::spawn(async move {
            if let Some(event) = receiver.next().await {
                println!("Received event in broadcast channel: {:?}", event);

                match event {
                    SwbLibSignal::StopRequest => {
                        slippi_interrupt_clone.lock().unwrap()();
                    }
                }
            }
        });

        // Run until both futures complete.
        let (slippi_to_sm_result, sm_client_result) = future::join(dolphin_to_sm, sm_connection_future).await;

        println!("slippi to sm result: {:?}", slippi_to_sm_result);
        println!("sm client result: {:?}", sm_client_result);

        output.send(SwbLibEvent::BroadcastStopped).await.unwrap();
    })
}

fn spectate(stream_id: u32) -> impl Stream<Item = SpectateEvent> {
    stream::channel(100, move |mut output| async move {
        output.send(SpectateEvent::Started(stream_id)).await.unwrap();
        swb::mirror_to_dolphin(format!("ws://localhost:4000/viewer_socket/websocket?stream_id={}&full_replay=true", stream_id).as_str()).await.unwrap();
        output.send(SpectateEvent::Stopped).await.unwrap();
    })
}
