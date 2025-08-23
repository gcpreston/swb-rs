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
    io::{Read, Result, Write, Cursor},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
    thread::sleep
};
use futures::{
    channel::mpsc::Receiver,
    stream::{self, Stream},
    task::{Context, Poll}
};
use serde::Deserialize;
use tokio::time::interval;
// use thiserror::Error;


pub trait SlippiStream {
    // TODO: Have this be ConnectionEvent? Or have DolphinConnection just give a stream of u8?
    fn event_stream(&mut self) -> impl Stream<Item = Vec<u8>>;
}

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
    pub fn connect(addr: impl ToSocketAddrs) -> Result<ConsoleConnection> {
        let mut stream = TcpStream::connect(addr)?;
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
        stream.write_all(&dummy_handshake_out)?;

        Ok(ConsoleConnection { stream })
    }
}

impl SlippiStream for ConsoleConnection {
    fn event_stream(&mut self) -> impl Stream<Item = Vec<u8>> {
        // Poll console connection at 120Hz
        let mut i = interval(Duration::from_micros(8333));
        let mut dcd = false; // TODO: Handle DC

        stream::poll_fn(move |cx: &mut Context<'_>| {
            if dcd {
                Poll::Ready(None)
            } else {
                let p = i.poll_tick(cx);
                match p {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(_) => {
                        let mut total_buf: Vec<u8> = Vec::new();
                        let mut leftover = Cursor::new(Vec::new());

                        // Receive and parse logic
                        let mut msg_size_buf = [0 as u8; 4];

                        let read_result =
                            match leftover.read_exact(&mut msg_size_buf) {
                                Ok(amount) => {
                                    println!("got from leftover");
                                    Ok(amount)
                                },
                                Err(e) => {
                                    println!("got from conn");
                                    self.stream.read_exact(&mut msg_size_buf)
                                }
                            };

                        match read_result {
                            Ok(_) => {
                                let msg_size = u32::from_be_bytes(msg_size_buf);
                                let mut bytes_remaining: usize = msg_size as usize;
                                println!("Receiving message of size {:?} ({:?}), remaining: {:?}", msg_size, msg_size_buf, bytes_remaining);

                                while bytes_remaining > 0 {
                                    let mut local_buf = [0 as u8; 1024];
                                    let leftover_bytes_read = leftover.read(&mut local_buf).unwrap();
                                    let bytes_read =
                                        if leftover_bytes_read > 0 {
                                            leftover_bytes_read
                                        } else {
                                            self.stream.read(&mut local_buf).unwrap()
                                        };

                                    println!("Read bytes {:?}, left {:?}", bytes_read, bytes_remaining);

                                    if bytes_read > bytes_remaining {
                                        total_buf.append(&mut local_buf[..bytes_remaining].to_vec());
                                        let leftover_range = bytes_remaining..bytes_read;
                                        let leftover_slice = &local_buf[leftover_range];
                                        println!("leftover_range length {:?}", bytes_read - bytes_remaining - 1);
                                        println!("Set leftover to {:?} (length {:?})", leftover_slice, leftover_slice.len());
                                        // leftover.write(leftover_slice).unwrap();
                                        // leftover.set_position(0);
                                        leftover = Cursor::new(leftover_slice.to_vec());
                                        bytes_remaining = 0;
                                    } else {
                                        total_buf.append(&mut local_buf[0..bytes_read].to_vec());
                                        bytes_remaining -= bytes_read;
                                    }
                                }

                                // Message received; deserialize
                                println!("Received message: {:?}, length {:?}", total_buf, total_buf.len());
                                let result: CommunicationMessage = ubjson_rs::from_slice(total_buf.as_slice()).unwrap();
                                println!("deserialized: {:?}", result);

                                total_buf.clear();

                                match result.payload {
                                    None => {
                                        sleep(Duration::from_millis(10));
                                        Poll::Pending
                                    }
                                    Some(payload) => {
                                        Poll::Ready(Some(payload.data))
                                    }
                                }
                            }

                            Err(e) => {
                                println!("got error on read_exact: {:?}", e);
                                Poll::Ready(None)
                            }
                        }
                    }
                }
            }
        })
    }
}
