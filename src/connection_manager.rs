use tokio_stream::StreamMap;
use futures::stream::StreamExt;
use ezsockets::Bytes;

use crate::{dolphin_connection::{ConnectionEvent, DolphinConnection}, spectator_mode_client::{SpectatorModeClient, WSError}};

pub fn merge_slippi_streams(slippi_conns: Vec<&DolphinConnection>, sm_client: SpectatorModeClient) -> impl Future<Output = Result<(), WSError>> {
    let mut map = StreamMap::new();
    let mut k = 0;

    for slippi_conn in slippi_conns {
        let slippi_stream = slippi_conn.event_stream();
        // futures::pin_mut!(slippi_stream);
        map.insert(k, slippi_stream);
        k += 1;
    }

    map.filter_map(async |(_k, v)| {
        let mut data: Vec<Vec<u8>> = Vec::new();

        let _: Vec<()> =
            v.into_iter().map(|e| {
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

        if data.len() > 0 {
            let b = Bytes::from(data.into_iter().flatten().collect::<Vec<u8>>());
            Some(Ok(b))
        } else {
            None
        }
  }).forward(sm_client)
}
