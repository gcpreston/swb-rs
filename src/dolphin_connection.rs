use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    str
};

use rusty_enet as enet;
use base64::{prelude::BASE64_STANDARD, Engine};
use serde::de::Error;
use serde_json::{Result as SerdeResult, Value};
use futures::task::{Context, Poll};

#[derive(Debug)]
pub enum ConnectionEvent {
    Connect,
    Disconnect,
    Message {
        payload: Vec<u8>
    },
    StartGame,
    EndGame
}

pub struct DolphinConnection {
    host: enet::Host<UdpSocket>
}

const MAX_PEERS: usize = 32;

// TODO: Graceful shutdown on stop
impl DolphinConnection {
  pub fn new() -> DolphinConnection {
    let socket =
        UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))).unwrap();
    let host = enet::Host::<UdpSocket>::new(
        socket,
        enet::HostSettings {
            peer_limit: MAX_PEERS,
            channel_limit: 3,
            ..Default::default()
        },
    )
    .unwrap();

    Self { host }
  }

  pub fn connect(&mut self, addr: SocketAddr) {
    let peer = self.host.connect(addr, 3, 1337).unwrap();
    peer.set_ping_interval(100);
  }

  pub fn next_event(&mut self, cx: &mut Context<'_>) -> Poll<Result<ConnectionEvent, &'static str>> {
    match self.host.service() {
        Err(_) => Poll::Ready(Err("host service error")),
        Ok(None) => {
            cx.waker().clone().wake();
            Poll::Pending
        },
        Ok(Some(event)) => {
            match event {
                enet::Event::Connect { peer, .. } => {
                    let packet = enet::Packet::reliable(r#"{"type":"connect_request","cursor":0}"#.as_bytes());
                    _ = peer.send(0, &packet);
                    cx.waker().clone().wake();
                    Poll::Pending
                }
                enet::Event::Disconnect { .. } => {
                    Poll::Ready(Ok(ConnectionEvent::Disconnect))
                }
                enet::Event::Receive { packet, .. } => {
                    if let Ok(message) = str::from_utf8(packet.data()) {
                        // println!("Received packet: {:?}", message);

                        // https://stackoverflow.com/questions/65575385/deserialization-of-json-with-serde-by-a-numerical-value-as-type-identifier/65576570#65576570

                        // let connect_reply: ConnectReply = serde_json::from_str(message).expect("Error deserializing JSON");
                        // Parse the string of data into serde_json::Value.
                        let v: Value = serde_json::from_str(message).unwrap();
                        let packet_type_result: SerdeResult<String> = match &v["type"] {
                            Value::String(s) => Ok(s.to_string()),
                            _ => Err(Error::custom("something"))
                        };
                        let packet_type = packet_type_result.unwrap();

                        match packet_type.as_str() {
                            "connect_reply" => Poll::Ready(Ok(ConnectionEvent::Connect)),
                            "game_event" => {
                                if let Value::String(encoded_payload) = &v["payload"] {
                                    let payload = BASE64_STANDARD.decode(encoded_payload).unwrap();
                                    Poll::Ready(Ok(ConnectionEvent::Message { payload }))
                                } else {
                                    Poll::Ready(Err("payload access error"))
                                }
                            },
                            "start_game" => Poll::Ready(Ok(ConnectionEvent::StartGame)),
                            "end_game" => Poll::Ready(Ok(ConnectionEvent::EndGame)),
                            _ => Poll::Ready(Err("unexpected packet type"))
                        }
                    } else {
                        Poll::Ready(Err("failed to decode packet"))
                    }
                }
            }
        }
    }
  }

  // TODO: Could coerce message payloads into a futures_util stream so that it
  //   can just be mapped and forwarded automatically
  // https://github.com/snapview/tokio-tungstenite/blob/a8d9f1983f1f17d7cac9ef946bbac8c1574483e0/examples/client.rs#L32
  pub fn service(&mut self) -> Result<Option<ConnectionEvent>, &'static str> {
    match self.host.service() {
        Err(_) => Err("host service error"),

        Ok(None) => Ok(None),

        Ok(Some(event)) => {
            match event {
                enet::Event::Connect { peer, .. } => {
                    let packet = enet::Packet::reliable(r#"{"type":"connect_request","cursor":0}"#.as_bytes());
                    _ = peer.send(0, &packet);
                    Ok(None)
                }
                enet::Event::Disconnect { .. } => {
                    Ok(Some(ConnectionEvent::Disconnect))
                }
                enet::Event::Receive { packet, .. } => {
                    if let Ok(message) = str::from_utf8(packet.data()) {
                        // println!("Received packet: {:?}", message);

                        // https://stackoverflow.com/questions/65575385/deserialization-of-json-with-serde-by-a-numerical-value-as-type-identifier/65576570#65576570

                        // let connect_reply: ConnectReply = serde_json::from_str(message).expect("Error deserializing JSON");
                        // Parse the string of data into serde_json::Value.
                        let v: Value = serde_json::from_str(message).unwrap();
                        let packet_type_result: SerdeResult<String> = match &v["type"] {
                            Value::String(s) => Ok(s.to_string()),
                            _ => Err(Error::custom("something"))
                        };
                        let packet_type = packet_type_result.unwrap();

                        match packet_type.as_str() {
                            "connect_reply" => Ok(Some(ConnectionEvent::Connect)),
                            "game_event" => {
                                if let Value::String(encoded_payload) = &v["payload"] {
                                    let payload = BASE64_STANDARD.decode(encoded_payload).unwrap();
                                    Ok(Some(ConnectionEvent::Message { payload }))
                                } else {
                                    Err("payload access error")
                                }
                            },
                            "start_game" => Ok(Some(ConnectionEvent::StartGame)),
                            "end_game" => Ok(Some(ConnectionEvent::EndGame)),
                            _ => Err("unexpected packet type")
                        }
                    } else {
                        Err("failed to decode packet")
                    }
                }
            }
        }
    }
  }
}
