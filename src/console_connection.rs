/* Flow notes
 * - Goal: make swb work with consoles in the same way as it works with Dolphin
 * - swb --source 192.168.2.2
 *   - port skipped, fall back to default
 *   - taking from non-default (localhost) source means it's most likely console,
 *     so don't require extra flag (require flag for foreign replay connection (?))
 *
 * - In main.rs, want a Stream of raw Slippi bytes, source-agnostic
 *   - Trait comes to mind
 * - Needs a setup (connection) step, but this may be able to be built-in to the
 *   one public method that matters
 *
 */

use std::{
    io::{Cursor, Read, Write}, net::SocketAddr, pin::Pin, time::Duration
};
use futures::{
    pin_mut, stream::{self, Stream}, task::{Context, Poll}
};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt}, 
    net::{TcpStream, ToSocketAddrs}, 
    time::interval
};
use async_stream::{stream, try_stream};
// use thiserror::Error;

use crate::common::SlippiDataStream;

// #[derive(Error, Debug)]
// pub enum ConsoleError {
//     #[error("Connect error: {0}")]
//     ConnectError(#[from] SendError<Call>),

//     #[error("Close error: {0}")]
//     CloseError(#[from] SendError<ezsockets::InMessage>),
// }

//        export enum CommunicationType {
//         HANDSHAKE = 1,
//         REPLAY = 2,
//         KEEP_ALIVE = 3,
//        }

//        export type CommunicationMessage = {
//         type: CommunicationType;
//         payload: {
//             cursor: Uint8Array;
//             clientToken: Uint8Array;
//             pos: Uint8Array;
//             nextPos: Uint8Array;
//             data: Uint8Array;
//             nick: string | null;
//             forcePos: boolean;
//             nintendontVersion: string | null;
//         };
//        };



#[derive(Debug, Deserialize)]
struct CommunicationMessage {
    r#type: u8,
    payload: Option<CommunicationMessagePayload>
}

#[derive(Debug, Deserialize)]
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

pub struct ConsoleConnection {
    // interrupt_receiver: Receiver<()>,
    pub stream: TcpStream
}

impl ConsoleConnection {
    pub async fn connect(addr: impl ToSocketAddrs) -> std::io::Result<ConsoleConnection> {
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

        Ok(ConsoleConnection { stream })
    }
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
    Ok(msg_buf)
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

// impl SlippiDataStream for ConsoleConnection {
//     async fn data_stream_test(&mut self) -> Pin<Box<dyn Stream<Item = Vec<u8>>>> {
//         Box::pin(
//             while let Some(v) = self.read_next_message() {
//                 yield v
//             }
//         )
//     }
// }

// impl SlippiDataStream for ConsoleConnection {
//     fn data_stream(&mut self) -> Pin<Box<dyn Stream<Item = Vec<u8>>>> {
//         // Move the stream out of self
//         let mut stream = std::mem::replace(&mut self.stream, TcpStream::connect("127.0.0.1:0").unwrap());
        
//         // Poll console connection at 120Hz
//         let mut i = interval(Duration::from_micros(8333));
//         let dcd = false; // TODO: Handle DC

//         Box::pin(stream::poll_fn(move |cx: &mut Context<'_>| {
//             if dcd {
//                 Poll::Ready(None)
//             } else {
//                 let p = i.poll_tick(cx);
//                 match p {
//                     Poll::Pending => Poll::Pending,
//                     Poll::Ready(_) => {
//                         let mut total_buf: Vec<u8> = Vec::new();
//                         let mut leftover = Cursor::new(Vec::new());

//                         // Receive and parse logic
//                         let mut msg_size_buf = [0 as u8; 4];

//                         let read_result =
//                             match leftover.read_exact(&mut msg_size_buf) {
//                                 Ok(amount) => {
//                                     println!("got from leftover");
//                                     Ok(amount)
//                                 },
//                                 Err(_e) => {
//                                     println!("got from conn");
//                                     stream.read_exact(&mut msg_size_buf)
//                                 }
//                             };

//                         match read_result {
//                             Ok(_) => {
//                                 let msg_size = u32::from_be_bytes(msg_size_buf);
//                                 let mut bytes_remaining: usize = msg_size as usize;
//                                 println!("Receiving message of size {:?} ({:?}), remaining: {:?}", msg_size, msg_size_buf, bytes_remaining);

//                                 while bytes_remaining > 0 {
//                                     let mut local_buf = [0 as u8; 1024];
//                                     let leftover_bytes_read = leftover.read(&mut local_buf).unwrap();
//                                     let bytes_read =
//                                         if leftover_bytes_read > 0 {
//                                             leftover_bytes_read
//                                         } else {
//                                             stream.read(&mut local_buf).unwrap()
//                                         };

//                                     println!("Read bytes {:?}, left {:?}", bytes_read, bytes_remaining);

//                                     if bytes_read > bytes_remaining {
//                                         total_buf.append(&mut local_buf[..bytes_remaining].to_vec());
//                                         let leftover_range = bytes_remaining..bytes_read;
//                                         let leftover_slice = &local_buf[leftover_range];
//                                         println!("leftover_range length {:?}", bytes_read - bytes_remaining - 1);
//                                         println!("Set leftover to {:?} (length {:?})", leftover_slice, leftover_slice.len());
//                                         // leftover.write(leftover_slice).unwrap();
//                                         // leftover.set_position(0);
//                                         leftover = Cursor::new(leftover_slice.to_vec());
//                                         bytes_remaining = 0;
//                                     } else {
//                                         total_buf.append(&mut local_buf[0..bytes_read].to_vec());
//                                         bytes_remaining -= bytes_read;
//                                     }
//                                 }

//                                 // Message received; deserialize
//                                 println!("Received message: {:?}, length {:?}", total_buf, total_buf.len());
//                                 let result: CommunicationMessage = ubjson_rs::from_slice(total_buf.as_slice()).unwrap();
//                                 println!("deserialized: {:?}", result);

//                                 total_buf.clear();
//                                 cx.waker().clone().wake();

//                                 match result.payload {
//                                     None => {
//                                         Poll::Pending
//                                     }
//                                     Some(payload) => {
//                                         Poll::Ready(Some(payload.data))
//                                     }
//                                 }
//                             }

//                             Err(e) => {
//                                 println!("got error on read_exact: {:?}", e);
//                                 Poll::Ready(None)
//                             }
//                         }
//                     }
//                 }
//             }
//         }))
//     }
// }
