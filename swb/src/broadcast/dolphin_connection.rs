use std::{
    cell::RefCell, net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket}, pin::Pin, str, thread, time::Duration
};

use base64::{Engine, prelude::BASE64_STANDARD};
use futures::{
    channel::mpsc::Receiver,
    stream,
    task::{Context, Poll},
};
use rusty_enet::{self as enet};
use serde::de::Error;
use serde_json::{Result as SerdeResult, Value};
use tokio::time::interval;


use crate::common::SlippiDataStream;

fn full_service(
    host: enet::Host<UdpSocket>,
    interrupt_cell: &RefCell<Receiver<bool>>,
    peer_id: enet::PeerID
) -> Result<(enet::Host<UdpSocket>, Vec<ConnectionEvent>), &'static str> {
    let mut events: Vec<ConnectionEvent> = Vec::new();

    let mut host_cycle = host;

    loop {
        let (new_host, result) = service(host_cycle, interrupt_cell, peer_id);
        host_cycle = new_host;
        match result? {
            None => break,
            Some(event) => events.push(event),
        }
    }

    Ok((host_cycle, events))
}

// TODO: Probably makes more sense to return Result as outer layer
fn service(
    mut host: enet::Host<UdpSocket>,
    interrupt_cell: &RefCell<Receiver<bool>>,
    peer_id: enet::PeerID
) -> (enet::Host<UdpSocket>, Result<Option<ConnectionEvent>, &'static str>) {
    let mut interrupt_receiver = interrupt_cell.borrow_mut();

    if let Ok(Some(_)) = interrupt_receiver.try_next() {
        let new_host = initiate_disconnect(host, peer_id);
        return (new_host, Ok(None));
    }

    let result =
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
                    tracing::debug!("enet received packet of size, {:?}", packet.data().len());
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
                                    let payload = BASE64_STANDARD.decode(encoded_payload).unwrap();
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
    };

    (host, result)
}

fn initiate_disconnect(mut host: enet::Host<UdpSocket>, peer_id: enet::PeerID) -> enet::Host<UdpSocket> {
    tracing::info!("Disconnecting from Slippi...");
    let peer = host.peer_mut(peer_id);
    peer.disconnect(1337);
    host
}

#[derive(Debug)]
pub enum ConnectionEvent {
    Connect,
    Disconnect,
    Message { payload: Vec<u8> },
    StartGame,
    EndGame,
}

const MAX_PEERS: usize = 32;

fn wait_for_connected(
    host: enet::Host<UdpSocket>,
    interrupt_cell: &RefCell<Receiver<bool>>,
    peer_id: enet::PeerID
) -> enet::Host<UdpSocket> {
    let mut host_cycle = host;
    loop {
        let (new_host, result) = service(host_cycle, interrupt_cell, peer_id);
        host_cycle = new_host;
        match result {
            Ok(Some(ConnectionEvent::Connect)) => return host_cycle,
            _ => ()
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub async fn data_stream(addr: SocketAddr, interrupt_receiver: Receiver<bool>) -> Pin<Box<SlippiDataStream>> {
    let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))).unwrap();

    let mut host =
        enet::Host::<UdpSocket>::new(
            socket,
            enet::HostSettings {
                peer_limit: MAX_PEERS,
                channel_limit: 3,
                ..Default::default()
            },
        )
        .unwrap();
    let interrupt_cell = RefCell::new(interrupt_receiver);

    // Initiate connection
    let peer = host.connect(addr, 3, 1337).unwrap();
    peer.set_ping_interval(100);
    let peer_id = peer.id();

    host = wait_for_connected(host, &interrupt_cell, peer_id);

    // Poll Dolphin connection at 120Hz
    let mut i = interval(Duration::from_micros(8333));
    let mut dcd = false;

    let (sender, receiver) = std::sync::mpsc::channel::<enet::Host<UdpSocket>>();
    sender.send(host).unwrap();

    Box::pin(stream::poll_fn(move |cx: &mut Context<'_>| {
        if dcd {
            Poll::Ready(None)
        } else {
            let p = i.poll_tick(cx);

            match p {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    let channel_host = receiver.try_recv().unwrap();
                    match full_service(channel_host, &interrupt_cell, peer_id) {
                        Result::Err(_e) => Poll::Ready(None),
                        Result::Ok((new_host, events)) => {
                            sender.send(new_host).unwrap();
                            if let Some(ConnectionEvent::Disconnect) = events.last() {
                                dcd = true;
                            }

                            cx.waker().clone().wake();
                            let game_data: Vec<u8> = events
                                .into_iter()
                                .map(|event| match event {
                                    ConnectionEvent::Message { payload } => payload,
                                    _ => vec![],
                                })
                                .flatten()
                                .collect();
                            Poll::Ready(Some(game_data))
                        }
                    }
                }
            }
        }
    }))
}
