use std::{net::SocketAddr, str::FromStr};

use clap::Parser;
use dolphin_connection::ConnectionEvent;
use futures::{SinkExt, StreamExt, stream_select};
use futures_util::pin_mut;
use signal_hook;
use signal_hook_tokio::Signals;
use tokio_tungstenite::tungstenite::Message;

mod dolphin_connection;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "ws://localhost:4000/bridge_socket/websocket"
    )]
    dest: String,

    #[arg(short, long, default_value = "127.0.0.1:51441")]
    source: String,
}

enum EventOrSignal {
    Event(ConnectionEvent),
    Signal(i32),
}

use async_trait::async_trait;
use ezsockets::ClientConfig;
use ezsockets::CloseCode;
use ezsockets::CloseFrame;
use ezsockets::Error;
use std::io::BufRead;
use url::Url;

enum Call {
    NewLine(String),
}

struct Client {
    handle: ezsockets::Client<Self>,
}

#[async_trait]
impl ezsockets::ClientExt for Client {
    type Call = Call;

    async fn on_text(&mut self, text: ezsockets::Utf8Bytes) -> Result<(), Error> {
        tracing::info!("received message: {text}");
        Ok(())
    }

    async fn on_binary(&mut self, bytes: ezsockets::Bytes) -> Result<(), Error> {
        tracing::info!("received bytes: {bytes:?}");
        Ok(())
    }

    async fn on_call(&mut self, call: Self::Call) -> Result<(), Error> {
        match call {
            Call::NewLine(line) => {
                if line == "exit" {
                    tracing::info!("exiting...");
                    self.handle
                        .close(Some(CloseFrame {
                            code: CloseCode::Normal,
                            reason: "adios!".into(),
                        }))
                        .unwrap();
                    return Ok(());
                }
                tracing::info!("sending {line}");
                self.handle.text(line).unwrap();
            }
        };
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    println!("[CTRL + C to quit]\n");

    let conn = dolphin_connection::DolphinConnection::new();
    let address = SocketAddr::from_str(&args.source).expect("Invalid socket address");
    let pid = conn.initiate_connection(address);
    conn.wait_for_connected().await;
    println!("Connected to Slippi.");

    let url = Url::parse(&args.dest).unwrap();
    let config = ClientConfig::new(url);
    let (sm_handle, _future) = ezsockets::connect(|handle| Client { handle }, config).await;
    // TODO: Actually wait for connect
    println!("Connected to SpectatorMode.");

    let signals = Signals::new(&[signal_hook::consts::SIGINT]).unwrap();
    let handle = signals.handle();

    let mapped_signals = signals.map(|s| EventOrSignal::Signal(s));
    let mapped_events = conn.event_stream().map(|e| EventOrSignal::Event(e));
    let combined = stream_select!(mapped_signals, mapped_events);

    let mut interrupted = false;
    // pin_mut!(sink);

    let dolphin_to_sm = combined
        .map(|e| {
            // ConnectionEvent::Disconnected will not reach the stream because
            // it is sent as Poll::Ready(None), i.e. the stream end.
            match e {
                EventOrSignal::Signal(n) => {
                    if !interrupted {
                        println!("Disconnecting...");
                        conn.initiate_disconnect(pid);
                        interrupted = true;
                    } else {
                        std::process::exit(n);
                    }
                }
                EventOrSignal::Event(ConnectionEvent::Connect) => println!("Connected to Slippi."),
                EventOrSignal::Event(ConnectionEvent::StartGame) => println!("Game start"),
                EventOrSignal::Event(ConnectionEvent::EndGame) => println!("Game end"),
                EventOrSignal::Event(ConnectionEvent::Disconnect) => handle.close(),
                _ => (),
            };
            e
        })
        .filter_map(async |e| match e {
            EventOrSignal::Event(ConnectionEvent::Message { payload }) => {
                // Some(Ok(Message::Binary(payload.into())))
                Some(payload)
            }
            _ => None,
        })
        .map(|message| {
            sm_handle.binary(message).unwrap();
            Ok(())
        })
        .forward(futures::sink::drain());
        // .forward(&mut sink);

    dolphin_to_sm.await.unwrap();
    // sink.close().await.unwrap();
    // TODO: Re-disconnect sink
    println!("Disconnected.");
}
