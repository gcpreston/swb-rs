use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    str
};

use rusty_enet as enet;

use serde::de::Error;
use serde_json::{Result as SerdeResult, Value};

pub enum ConnectionEvent {
    Connect,
    Disconnect,
    Message,
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
                            "game_event" => Ok(Some(ConnectionEvent::Message)),
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
