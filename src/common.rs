use std::pin::Pin;
use futures::stream::Stream;

pub trait SlippiDataStream {
   fn data_stream(&mut self) -> Pin<Box<dyn Stream<Item = Vec<u8>>>>;
}
