use std::{
    net::SocketAddr, pin::Pin
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt}, 
    net::TcpStream, 
};
use async_stream::stream;
// use thiserror::Error;

use crate::common::SlippiDataStream;

#[derive(Debug, Deserialize, Serialize)]
struct CommunicationMessage {
    r#type: u8,
    payload: Option<CommunicationMessagePayload>
}

#[derive(Debug, Deserialize, Serialize)]
struct CommunicationMessagePayload {
    cursor: Vec<u8>,
    client_token: Vec<u8>,
    pos: Vec<u8>,
    next_pos: Vec<u8>,
    data: Vec<u8>,
    nick: Option<String>,
    force_pos: bool,
    nintendont_version: Option<String>
}

async fn establish_console_connection(addr: SocketAddr) -> std::io::Result<TcpStream> {
    let mut stream = TcpStream::connect(addr).await?;
    let dummy_handshake_out: Vec<u8> = vec![
        0x00, 0x00, 0x00, 0x4f, 0x7b, 0x69, 0x04, 0x74, 0x79, 0x70, 0x65,
        0x69, 0x01, 0x69, 0x07, 0x70, 0x61, 0x79, 0x6c, 0x6f, 0x61, 0x64,
        0x7b, 0x69, 0x06, 0x63, 0x75, 0x72, 0x73, 0x6f, 0x72, 0x5b, 0x24,
        0x55, 0x23, 0x69, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x69, 0x0b, 0x63, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x54, 0x6f,
        0x6b, 0x65, 0x6e, 0x5b, 0x24, 0x55, 0x23, 0x69, 0x04, 0x00, 0x00,
        0x00, 0x00, 0x69, 0x0a, 0x69, 0x73, 0x52, 0x65, 0x61, 0x6c, 0x74,
        0x69, 0x6d, 0x65, 0x46, 0x7d, 0x7d
    ];
    stream.write_all(&dummy_handshake_out).await?;
    Ok(stream)
}

async fn read_next_message(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let msg_size = stream.read_u32().await?;
    let mut msg_buf: Vec<u8> = vec![0; msg_size as usize];
    stream.read_exact(&mut msg_buf).await?;

    let result: CommunicationMessage = ubjson_rs::from_slice(&msg_buf).unwrap();
    match result.payload {
        None => Ok(Vec::new()),
        Some(payload) => Ok(payload.data)
    }
}

pub async fn data_stream(addr:  SocketAddr) -> Pin<Box<SlippiDataStream>> {
    Box::pin(stream! {
        let mut tcp_stream = establish_console_connection(addr).await.unwrap();

        loop {
            match read_next_message(&mut tcp_stream).await {
                Ok(message) => yield message,
                Err(_) => break,
            }
        }
    })
}
