use std::io::Read;

use chrono::{DateTime, Local};
use peppi::io::slippi::de::{self, ParseState};

pub(crate) struct SlpFileWriter {
    command_number: u32,
    current_file: Option<String>,
    parse_state: Option<ParseState>
}

pub(crate) enum SlpStreamEvent {
    StartGame(String), // file path 
    EndGame,
    Other
}

impl SlpFileWriter {
    pub(crate) fn new() -> SlpFileWriter {
        SlpFileWriter { 
            command_number: 0,
            current_file: None,
            parse_state: None
         }
    }

    pub(crate) fn write<R: Read>(&self, data: R) -> SlpStreamEvent {
        // TODO: Actually write
        match self.parse_state {
            None => {
                self.parse_state = Some(de::parse_start(data, None).unwrap());
                let current_local: DateTime<Local> = Local::now();
                let dt = current_local.format("%Y%m%d%H%M%S");
                let filename = format!("Game_{}.slp", dt);
                SlpStreamEvent::StartGame(filename)
            }
            Some(state) => {
                let command =  de::parse_event(data, state, None).unwrap();
                if command == de::Event::GameEnd as u8 {
                    SlpStreamEvent::EndGame
                } else {
                    SlpStreamEvent::Other
                }
            }
        }
    } 
}