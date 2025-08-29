use futures::channel::mpsc::Receiver;
use futures::StreamExt;
// use tungstenite::http::{Method, Request};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::connect_async;

use crate::common::SlippiDataStream;

// TODO: disconnect between socketaddr and &str between here and broadcast
// Does it matter? These functions shouldn't really be interchangable anyways,
// maybe just the return type matters. In that case, change name too.
pub async fn data_stream(address: &str, interrupt_receiver: Receiver<bool>) { // -> Pin<Box<SlippiDataStream>> {
  tracing::info!("Connecting to SpectatorMode...");
  let request = address.into_client_request().unwrap();
  let (stream, response) = connect_async(request).await.unwrap();
  println!("connect response {:?}", response);
  tracing::info!("Connected to SpectatorMode.");

  stream.map(|v| {
    println!("got message v {:?}", v);
  }).collect::<()>().await;
  
  // Box::pin(stream! {
  //   yield Vec::new();
  // })
}