use tokio_stream::StreamMap;
use futures::{stream::StreamExt, Stream};
use ezsockets::Bytes;

use crate::{dolphin_connection::{ConnectionEvent, DolphinConnection}, spectator_mode_client::{SpectatorModeClient, WSError}};

pub fn merge_slippi_streams(slippi_conns: Vec<&DolphinConnection>) -> impl Stream<Item = (u32, Vec<ConnectionEvent>)> {
    let mut map = StreamMap::new();
    let mut k = 0;

    for slippi_conn in slippi_conns {
        let slippi_stream = slippi_conn.event_stream();
        // futures::pin_mut!(slippi_stream);
        map.insert(k, slippi_stream);
        k += 1;
    }

    map
}

/* Packet spec
 * +------------------------------+
 * | stream ID (32 bits, 4 bytes) |
 * +------------------------------+
 * | data size (4 bytes)          |
 * +------------------------------+
 * | Data...
 * +------------------------------+
 *
 * Header size: 8 bytes
 * Size addition for different average data sizes:
 * - 8 bytes: +100%
 * - 80 bytes: +10%
 * - 800 bytes: +1%
 */

pub fn forward_slippi_data(stream: impl Stream<Item = (u32, Vec<ConnectionEvent>)>, sm_client: SpectatorModeClient) -> impl Future<Output = Result<(), WSError>> {
    stream.filter_map(async |(k, v)| {
        let mut data: Vec<Vec<u8>> = Vec::new();

        let _: Vec<()> =
            v.into_iter().map(|e| {
                // Side-effects
                // ConnectionEvent::Connected will not reach the stream because
                // it is awaited before initiating the SpectatorMode connection.
                match e {
                    ConnectionEvent::StartGame => tracing::info!("Received game start event."),
                    ConnectionEvent::EndGame => tracing::info!("Received game end event."),
                    ConnectionEvent::Disconnect => tracing::info!("Disconnected from Slippi."),
                    ConnectionEvent::Message { payload } => {
                        data.push(payload);
                    },
                    _ => ()
                };
            }).collect();

        // Return
        if data.len() > 0 {
            Some(Ok(create_packet(k, data)))
        } else {
            None
        }
  }).forward(sm_client)
}

// https://stackoverflow.com/a/72631195
fn convert(data: &[u32; 2]) -> [u8; 8] {
    let mut res = [0; 8];
    for i in 0..2 {
        res[2*i..][..2].copy_from_slice(&data[i].to_le_bytes());
    }
    res
}

fn create_packet(stream_id: u32, data: Vec<Vec<u8>>) -> Bytes {
    let header = [stream_id, data.len() as u32];
    let mut packet: Vec<u8> = convert(&header).into();
    let mut flat_data: Vec<u8> = data.into_iter().flatten().collect();
    packet.append(&mut flat_data);
    Bytes::from(packet)
}
