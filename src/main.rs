use std::{
    net::SocketAddr,
    str::{self, FromStr},
    time::Duration,
};

mod dolphin_connection;

const SLIPPI_ADDRESS: &str = "127.0.0.1";
const SLIPPI_PORT: i32 = 51441;

fn main() {
    let mut conn = dolphin_connection::DolphinConnection::new();
        let address = SocketAddr::from_str(format!("{SLIPPI_ADDRESS}:{SLIPPI_PORT}").as_str()).unwrap();
    conn.connect(address);

    'outer: loop {
        while let Some(event) = conn.service().unwrap() {
            match event {
                dolphin_connection::ConnectionEvent::Connect => {
                    println!("Connected");
                }
                dolphin_connection::ConnectionEvent::Disconnect { .. } => {
                    println!("Disconnected");
                    break 'outer;
                }
                dolphin_connection::ConnectionEvent::Message => {
                    println!("Got a message :)");
                }
                dolphin_connection::ConnectionEvent::StartGame => {
                    println!("Game started");
                }
                dolphin_connection::ConnectionEvent::EndGame => {
                    println!("Game ended");
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
