use std::{
    net::SocketAddr, pin::Pin, time::Duration
};
use serde::{Deserialize, Serialize};
use futures::channel::mpsc::Receiver;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt}, 
    net::TcpStream, 
    time::timeout
};
use async_stream::stream;
use thiserror::Error;

use crate::common::SlippiDataStream;

#[derive(Debug, Deserialize, Serialize)]
struct CommunicationMessage {
    r#type: u8,
    payload: Option<CommunicationMessagePayload>
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[allow(non_snake_case)]
struct CommunicationMessagePayload {
    cursor: Option<Vec<u8>>,
    clientToken: Option<Vec<u8>>,
    pos: Vec<u8>,
    nextPos: Option<Vec<u8>>,
    data: Option<Vec<u8>>,
    nick: Option<String>,
    force_pos: Option<bool>,
    nintendontVersion: Option<String>
}

#[derive(Error, Debug)]
pub enum ConsoleCommunicationError {
    #[error("Connection error: {0}")]
    SocketConnectionError(std::io::Error),

    #[error("Read error: {0}")]
    SocketReadError(#[from] std::io::Error),

    #[error("Decode error: {0}")]
    DecodeError(#[from] ubjson_rs::UbjsonError),
}

async fn establish_console_connection(addr: SocketAddr) -> Result<TcpStream, ConsoleCommunicationError> {
    let result: Result<TcpStream, std::io::Error> = async {
        let mut tcp_stream = TcpStream::connect(addr).await?;
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
        tcp_stream.write_all(&dummy_handshake_out).await?;
        Ok(tcp_stream)
    }.await;

    result.map_err(|io_error| ConsoleCommunicationError::SocketConnectionError(io_error))
}

async fn read_next_message(tcp_stream: &mut TcpStream) -> Result<CommunicationMessage, ConsoleCommunicationError> {
    // TODO: Map Err value `Os { code: 61, kind: ConnectionRefused, message: "Connection refused" }` to ConsoleConnectionError
    let msg_size = tcp_stream.read_u32().await?;
    let mut msg_buf: Vec<u8> = vec![0; msg_size as usize];
    tcp_stream.read_exact(&mut msg_buf).await?;

    let result: CommunicationMessage = ubjson_rs::from_slice(&msg_buf)?;
    Ok(result)
}

pub async fn data_stream(addr:  SocketAddr, mut interrupt_receiver: Receiver<bool>) -> Pin<Box<SlippiDataStream>> {
    let mut tcp_stream = establish_console_connection(addr).await.unwrap();
    tracing::info!("Connected to Slippi.");

    Box::pin(stream! {
        loop {
            match interrupt_receiver.try_next() {
                Ok(Some(_)) => {
                    // interrupt was sent
                    break
                }
                Ok(None) => {
                    // interrupt channel is closed; something is wrong
                    // TODO: Try to reconnect if it makes sense
                    tracing::error!("Interrupt channel closed unexpectedly");
                    break
                }
                _ => ()
            }

            match timeout(Duration::from_secs(5), read_next_message(&mut tcp_stream)).await {
                Ok(read_result) => {
                    match read_result {
                        Ok(message) => {
                            match message.payload {
                                None => continue,
                                Some(payload) => yield payload.data.unwrap_or(Vec::new())
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error reading console message: {:?}", e);
                            break
                        }
                    }
                }
                Err(_) => {
                    // TODO: Reconnect instead
                    tracing::error!("Timeout receiving next message");
                    break
                }
            }
        }
    })
}
