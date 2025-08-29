use std::pin::Pin;
use futures::channel::mpsc::Receiver;

use crate::common::SlippiDataStream;

// TODO: disconnect between socketaddr and &str between here and broadcast
// Does it matter? These functions shouldn't really be interchangable anyways,
// maybe just the return type matters. In that case, change name too.
pub async fn data_stream(addr: &str, interrupt_receiver: Receiver<bool>) -> Pin<Box<SlippiDataStream>> {}