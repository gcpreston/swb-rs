pub(crate) struct SlpFileWriter {}

pub(crate) enum SlpStreamEvent {
    StartGame(String), // file path 
    EndGame
}

impl SlpFileWriter {
    pub fn new() -> SlpFileWriter {
        SlpFileWriter {  }
    }

    // TODO: really will probably be a vev
    pub fn write(&self, data: Vec<u8>) -> Vec<SlpStreamEvent> {
        Vec::new()
    } 
}