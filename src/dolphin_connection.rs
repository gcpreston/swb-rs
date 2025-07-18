use std::{
    cell::RefCell,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    str,
    time::Duration,
};

use base64::{Engine, prelude::BASE64_STANDARD};
use futures::{
    channel::mpsc::Receiver,
    future,
    stream::{self, Stream},
    task::{Context, Poll},
};
use rusty_enet::{self as enet};
use serde::de::Error;
use serde_json::{Result as SerdeResult, Value};
use tokio::time::interval;

#[derive(Debug)]
pub enum ConnectionEvent {
    Connect,
    Disconnect,
    Message { payload: Vec<u8> },
    StartGame,
    EndGame,
}

pub struct DolphinConnection {
    host_cell: RefCell<enet::Host<UdpSocket>>,
    interrupt_cell: RefCell<Receiver<enet::PeerID>>,
}

const MAX_PEERS: usize = 32;

impl DolphinConnection {
    pub fn new(interrupt_receiver: Receiver<enet::PeerID>) -> DolphinConnection {
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

        let host_cell = RefCell::new(host);
        let interrupt_cell = RefCell::new(interrupt_receiver);
        Self {
            host_cell,
            interrupt_cell,
        }
    }

    pub fn initiate_connection(&self, addr: SocketAddr) -> enet::PeerID {
        let mut host = self.host_cell.borrow_mut();
        let peer = host.connect(addr, 3, 1337).unwrap();
        peer.set_ping_interval(100);
        peer.id()
    }

    pub async fn wait_for_connected(&self) {
        // 120Hz
        let mut i = interval(Duration::from_micros(8333));

        future::poll_fn(|cx: &mut Context<'_>| {
            let p = i.poll_tick(cx);
            match p {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => match self.service() {
                    Ok(Some(ConnectionEvent::Connect)) => Poll::Ready(()),
                    _ => {
                        cx.waker().clone().wake();
                        Poll::Pending
                    }
                },
            }
        })
        .await;
    }

    pub fn initiate_disconnect(&self, peer_id: enet::PeerID) {
        tracing::info!("Disconnecting from Slippi...");
        let mut host = self.host_cell.borrow_mut();
        let peer = host.peer_mut(peer_id);
        peer.disconnect(1337);
    }

    pub fn event_stream(&self) -> impl Stream<Item = Vec<ConnectionEvent>> {
        // Poll Dolphin connection at 120Hz
        let mut i = interval(Duration::from_micros(8333));
        let mut dcd = false;

        stream::poll_fn(move |cx: &mut Context<'_>| {
            if dcd {
                Poll::Ready(None)
            } else {
                let p = i.poll_tick(cx);
                match p {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(_) => {
                        match self.full_service() {
                            Result::Err(_e) => Poll::Ready(None),
                            Result::Ok(events) => {
                                if let Some(ConnectionEvent::Disconnect) = events.last() {
                                    dcd = true;
                                }

                                cx.waker().clone().wake();
                                Poll::Ready(Some(events))
                            }
                        }
                    }
                }
            }
        })
    }

    // Run service until there is no ready value.
    fn full_service(&self) -> Result<Vec<ConnectionEvent>, &'static str> {
        let mut events: Vec<ConnectionEvent> = Vec::new();

        loop {
            match self.service()? {
                None => break,
                Some(event) => events.push(event)
            }
        }

        Ok(events)
    }

    // https://github.com/snapview/tokio-tungstenite/blob/a8d9f1983f1f17d7cac9ef946bbac8c1574483e0/examples/client.rs#L32
    fn service(&self) -> Result<Option<ConnectionEvent>, &'static str> {
        let mut interrupt_receiver = self.interrupt_cell.borrow_mut();

        if let Ok(Some(peer_id)) = interrupt_receiver.try_next() {
            self.initiate_disconnect(peer_id);
            return Ok(None);
        }

        let mut host = self.host_cell.borrow_mut();

        match host.service() {
            Err(_) => Err("host service error"),

            Ok(None) => Ok(None),

            Ok(Some(event)) => {
                match event {
                    enet::Event::Connect { peer, .. } => {
                        let packet = enet::Packet::reliable(
                            r#"{"type":"connect_request","cursor":0}"#.as_bytes(),
                        );
                        _ = peer.send(0, &packet);
                        Ok(None)
                    }
                    enet::Event::Disconnect { .. } => Ok(Some(ConnectionEvent::Disconnect)),
                    enet::Event::Receive { packet, .. } => {
                        if let Ok(message) = str::from_utf8(packet.data()) {
                            // println!("Received packet: {:?}", message);

                            // https://stackoverflow.com/questions/65575385/deserialization-of-json-with-serde-by-a-numerical-value-as-type-identifier/65576570#65576570

                            // let connect_reply: ConnectReply = serde_json::from_str(message).expect("Error deserializing JSON");
                            // Parse the string of data into serde_json::Value.
                            let v: Value = serde_json::from_str(message).unwrap();
                            let packet_type_result: SerdeResult<String> = match &v["type"] {
                                Value::String(s) => Ok(s.to_string()),
                                _ => Err(Error::custom("something")),
                            };
                            let packet_type = packet_type_result.unwrap();

                            match packet_type.as_str() {
                                "connect_reply" => Ok(Some(ConnectionEvent::Connect)),
                                "game_event" => {
                                    if let Value::String(encoded_payload) = &v["payload"] {
                                        let payload =
                                            BASE64_STANDARD.decode(encoded_payload).unwrap();
                                        Ok(Some(ConnectionEvent::Message { payload }))
                                    } else {
                                        Err("payload access error")
                                    }
                                }
                                "start_game" => Ok(Some(ConnectionEvent::StartGame)),
                                "end_game" => Ok(Some(ConnectionEvent::EndGame)),
                                _ => Err("unexpected packet type"),
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
