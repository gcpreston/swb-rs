use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use iced::{stream, Element, Fill, Subscription};
use iced::widget::{button, column, text, container};
use iced::Alignment::Center;
use iced::futures::{future, Stream};
use iced::futures::channel::mpsc;

use swb::spectator_mode_client::BridgeInfo;

pub fn main() -> iced::Result {
    iced::application("SpectatorMode client", SwbGui::update, SwbGui::view)
        .window_size(iced::Size::new(300.0, 400.0))
        .subscription(SwbGui::subscription)
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Broadcast,
    Stop,
    SwbLibMessage(SwbLibEvent)
}

/// Events sent from the swb lib, received by the client application.
#[derive(Debug, Clone)]
enum SwbLibEvent {
    SlippiConnected,
    BroadcastStarted(BridgeInfo, mpsc::Sender<SwbLibSignal>),
    BroadcastStopped
}


/// Inputs to the swb lib from the client application.
#[derive(Debug)]
enum SwbLibSignal {
    StopRequest
}

#[derive(Debug)]
enum State {
    Standby,
    SlippiConnecting,
    SpectatorModeConnecting,
    Broadcasting(BridgeInfo, mpsc::Sender<SwbLibSignal>)
}

struct SwbGui {
    state: State
}

impl SwbGui {
    fn new() -> Self {
        Self {
            state: State::Standby
        }
    }

    fn update(&mut self, message: Message) -> () {
        println!("Processing update message {:?}", message);

        match message {
            Message::Broadcast => {
                self.state = State::SlippiConnecting;
            },

            Message::Stop => {
                if let State::Broadcasting(_, interrupt) = &mut self.state {
                    interrupt.try_send(SwbLibSignal::StopRequest).unwrap();
                }
            },

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
                        self.state = State::Standby;
                    }
                }
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let buttons = match &self.state {
            State::Standby => column![
                button("Broadcast").on_press(Message::Broadcast),
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
            State::Standby => Subscription::none(),
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
        let (sm_client, mut sm_connection_monitor, bridge_info) = swb::spectator_mode_client::initiate_connection(dest, 1).await.unwrap();

        // This is the sender/receiver for the main thread to tell things to this sub-thread
        // Specifically, to initiate a disconnect request
        let (sender, mut receiver) = mpsc::channel(100);
        println!("Got slippi conn; sending sender {:?}", sender);

        output
            .send(SwbLibEvent::BroadcastStarted(bridge_info.clone(), sender))
            .await
            .unwrap();
        println!("Sent sender");

        // Set up the futures to await.
        // Each individual future will attempt to gracefully disconnect the other.
        let merged_stream = swb::broadcast::connection_manager::merge_slippi_streams(vec![slippi_conn], bridge_info.stream_ids).unwrap();
        let dolphin_to_sm = swb::broadcast::connection_manager::forward_slippi_data(merged_stream, sm_client);

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
