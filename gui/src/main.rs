use iced::{stream, Element, Fill, Subscription};
use iced::widget::{button, column, text, container};
use iced::Alignment::Center;
use iced::futures::Stream;
use iced::futures::channel::mpsc;

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

#[derive(Debug, Clone)]
enum SwbLibEvent {
    BroadcastStarted(mpsc::Sender<SwbLibEvent>),
    BroadcastStopped,
    StopRequest // TODO: Figure out how this can be separate
}

#[derive(Debug, PartialEq)]
enum State {
    Standby,
    Broadcasting
}

struct SwbGui {
    state: State,
    broadcast_interrupt: Option<mpsc::Sender<SwbLibEvent>>
}

impl SwbGui {
    fn new() -> Self {
        Self {
            state: State::Standby,
            broadcast_interrupt: None
        }
    }

    fn update(&mut self, message: Message) -> () {
        println!("Processing update message {:?}", message);

        match message {
            Message::Broadcast => {
                self.state = State::Broadcasting;
            },

            Message::Stop => {
                if let Some(interrupt) = &mut self.broadcast_interrupt {
                    interrupt.try_send(SwbLibEvent::StopRequest).unwrap();
                }
            },

            Message::SwbLibMessage(event) => {
                println!("Received swb event: {:?}", event);

                match event {
                    SwbLibEvent::BroadcastStarted(interrupt_sender) => {
                        self.broadcast_interrupt = Some(interrupt_sender);
                    },

                    SwbLibEvent::BroadcastStopped => {
                        self.state = State::Standby;
                    },

                    SwbLibEvent::StopRequest => {
                        panic!("Unexpectedly received SwbLibEvent::StopRequest message in GUI.");
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
            State::Broadcasting => column![
                text(format!("Broadcasting"),).size(20),
                button("Stop broadcast").on_press(Message::Stop)
            ],
        };

        container(buttons.align_x(Center).spacing(10))
            .padding(10)
            .center_x(Fill)
            .center_y(Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.state == State::Broadcasting {
            Subscription::run(stream_channel_test).map(Message::SwbLibMessage)
        } else {
            Subscription::none()
        }
    }
}

impl Default for SwbGui {
    fn default() -> Self {
        SwbGui::new()
    }
}

fn stream_channel_test() -> impl Stream<Item = SwbLibEvent> {
    stream::channel(100, |mut output| async move {
        use iced::futures::{StreamExt, SinkExt};

        let (sender, mut receiver) = mpsc::channel(100);

        output.send(SwbLibEvent::BroadcastStarted(sender)).await.unwrap();
        println!("Sent to output");

        loop {
            if let Some(event) = receiver.next().await {
                println!("Received in channel test {:?}", event);
                output.send(SwbLibEvent::BroadcastStopped).await.unwrap();
            }
        }
    })
}
