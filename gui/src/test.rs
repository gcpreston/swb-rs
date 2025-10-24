use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    str::FromStr,
    time::Duration,
};

use futures::StreamExt;
use rusty_enet as enet;

const MAX_PEERS: usize = 32;

#[tokio::main]
async fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut handles = Vec::new();
    handles.push(
        runtime.spawn(async {
            // do_enet().await;
            let s = futures::stream::once(async { 42 });
            let collected = s.collect::<Vec<i32>>().await;
            println!("Stream result: {:?}", collected);
        })
    );

    println!("spanwed and pushed");
    for handle in handles {
        println!("doing task {:?}", handle);
        handle.await.expect("Panic in task");
        println!("done");
    }
}

async fn do_enet() {
    let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))).unwrap();

    let mut host = enet::Host::<UdpSocket>::new(
        socket,
        enet::HostSettings {
            peer_limit: MAX_PEERS,
            channel_limit: 3,
            ..Default::default()
        },
    )
    .unwrap();
    let address = SocketAddr::from_str("127.0.0.1:51441").unwrap();
    let peer = host.connect(address, 3, 1337).unwrap();
    peer.set_ping_interval(100);
    loop {
        while let Some(event) = host.service().unwrap() {
            match event {
                enet::Event::Connect { peer, .. } => {
                    println!("Connected");
                    let packet = enet::Packet::reliable(
                        r#"{"type":"connect_request","cursor":0}"#.as_bytes(),
                    );
                    _ = peer.send(0, &packet);
                }
                enet::Event::Disconnect { .. } => {
                    println!("Disconnected");
                }
                enet::Event::Receive { packet, .. } => {
                    println!("Received data: {:?}", packet.data());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/*
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    str::{self, FromStr},
    time::Duration,
};

use rusty_enet as enet;

const MAX_PEERS: usize = 32;

fn main() {
    let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))).unwrap();
    let mut host = enet::Host::<UdpSocket>::new(
        socket,
        enet::HostSettings {
            peer_limit: MAX_PEERS,
            channel_limit: 3,
            ..Default::default()
        },
    )
    .unwrap();
    let address = SocketAddr::from_str("127.0.0.1:51441").unwrap();
    let peer = host.connect(address, 3, 1337).unwrap();
    peer.set_ping_interval(100);
    loop {
        while let Some(event) = host.service().unwrap() {
            match event {
                enet::Event::Connect { peer, .. } => {
                    println!("Connected");
                    let packet = enet::Packet::reliable(
                        r#"{"type":"connect_request","cursor":0}"#.as_bytes()
                    );
                    _ = peer.send(0, &packet);
                }
                enet::Event::Disconnect { .. } => {
                    println!("Disconnected");
                }
                enet::Event::Receive { packet, .. } => {
                    if let Ok(message) = str::from_utf8(packet.data()) {
                        println!("Received packet: {:?}", message);
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
 */
