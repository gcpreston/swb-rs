use std::pin::Pin;

use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::connect_async;
use async_stream::stream;
use tokio_tungstenite::tungstenite::Message;

use crate::common::SlippiDataStream;

// TODO: disconnect between socketaddr and &str between here and broadcast
// Does it matter? These functions shouldn't really be interchangable anyways,
// maybe just the return type matters. In that case, change name too.
pub async fn data_stream(address: &str) -> Pin<Box<SlippiDataStream>> {
    tracing::info!("Connecting to SpectatorMode at {}...", address);
    let request = address.into_client_request().unwrap();
    let (stream, _response) = connect_async(request).await.unwrap();
    tracing::info!("Connected to SpectatorMode.");

    Box::pin(stream! {
        for await result in stream {
            if let Ok(Message::Binary(bytes)) = result {
                yield bytes.to_vec();
            } else {
                break;
            }
        }
    })
}
