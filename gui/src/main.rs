use iced::widget::{button, column, container, text};
use iced::{Fill, Element};

#[derive(Debug, Clone)]
enum Message {
    Increment,
    Decrement
}

#[derive(Default)]
struct Counter {
    value: u64
}

pub fn main() -> iced::Result {
    iced::application("A cool counter", update, view)
        .window_size(iced::Size::new(300.0, 400.0))
        .run()
}

fn update(counter: &mut Counter, message: Message) {
    match message {
        Message::Increment => counter.value += 1,
        Message::Decrement => {
            if counter.value > 0 {
                counter.value -= 1
            }
        }
    }
}

fn view(counter: &Counter) -> Element<'_, Message> {
    container(
        column![
            text(counter.value).size(20),
            button("Increment").on_press(Message::Increment),
            button("Decrement").on_press(Message::Decrement)
        ]
        .spacing(10)
    )
    .padding(10)
    .center_x(Fill)
    .center_y(Fill)
    .into()
}
