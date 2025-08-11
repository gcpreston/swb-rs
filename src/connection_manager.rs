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
 *
 * TODO: I'm not sure if the data size is actually needed. This isn't a
 * raw socket we are reading a fixed number of bytes from, it is a WebSocket
 * connection which already abstracts byte size to create messages.
 * However, if we want to send data for multiple streams in one message,
 * byte size would be required.
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
fn create_header(data: &[u32; 2]) -> [u8; 8] {
    let mut res = [0; 8];
    for i in 0..2 {
        res[4*i..][..4].copy_from_slice(&data[i].to_le_bytes());
    }
    res
}

fn create_packet(stream_id: u32, data: Vec<Vec<u8>>) -> Bytes {
    // TODO: Could this be more efficient by using BytesMut?
    //   Think this would have to depend on the bytes package, and the packet sizes
    //   would have to be known beforehand.
    let mut flat_data: Vec<u8> = data.into_iter().flatten().collect();
    let header = [stream_id, flat_data.len() as u32];
    let mut packet: Vec<u8> = create_header(&header).into();
    packet.append(&mut flat_data);
    Bytes::from(packet)
}

fn decode_packet(bytes: Bytes) -> (u32, u32, Vec<u8>) {
    let stream_id_bytes = bytes.slice(0..4);
    let stream_id_vec = stream_id_bytes.to_vec();
    let stream_id = u32::from_le_bytes(stream_id_vec.try_into().unwrap());

    let size_bytes = bytes.slice(4..8);
    let size_vec = size_bytes.to_vec();
    let size = u32::from_le_bytes(size_vec.try_into().unwrap());

    let data = bytes.slice(8..).to_vec();

    (stream_id, size, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_header_generates_valid_header() {
        let result = create_header(&[257, 10_000_000]);
        assert_eq!(result, [1, 1, 0, 0, 128, 150, 152, 0]);
    }

    #[test]
    fn create_packet_generates_valid_packet() {
        let data_1: Vec<u8> = vec![255, 60, 75, 0, 1, 127];
        let data_2: Vec<u8> = vec![205, 15, 99, 191];
        let data: Vec<Vec<u8>> = vec![data_1, data_2];
        let result = create_packet(12345, data);

        let stream_id_vec = result.slice(0..4).to_vec();
        let size_vec = result.slice(4..8).to_vec();
        let data_vec = result.slice(8..).to_vec();

        assert_eq!(stream_id_vec, vec![57, 48, 0, 0]);
        assert_eq!(size_vec, vec![10, 0, 0, 0]);
        assert_eq!(data_vec, vec![255, 60, 75, 0, 1, 127, 205, 15, 99, 191]);
    }

    #[test]
    fn decode_packet_reads_header_and_data() {
        let data_1: Vec<u8> = vec![255, 60, 75, 0, 1, 127];
        let data_2: Vec<u8> = vec![205, 15, 99, 191];
        let data: Vec<Vec<u8>> = vec![data_1, data_2];
        let packet = create_packet(12345, data);
        let result = decode_packet(packet);

        assert_eq!(result.0, 12345);
        assert_eq!(result.1, 10);
        assert_eq!(result.2, vec![255, 60, 75, 0, 1, 127, 205, 15, 99, 191]);
    }
}
