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
