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
    io::{Read, Result, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration
};
use futures::{
    channel::mpsc::Receiver,
    stream::{self, Stream},
    task::{Context, Poll}
};
use tokio::time::interval;
// use thiserror::Error;

// use crate::dolphin_connection::ConnectionEvent;

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
                        let mut buffer = [0; 1024];
                        self.stream.read(&mut buffer).unwrap();

                        cx.waker().clone().wake();
                        Poll::Ready(Some(buffer.to_vec()))
                    }
                }
            }
        })
    }
}
