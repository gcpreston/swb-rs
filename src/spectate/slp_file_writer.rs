use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Error, ErrorKind, Read, Write},
    path::PathBuf,
};

use byteorder::{BE, ReadBytesExt};
use chrono::{DateTime, Local};

use crate::{config::{self, ConfigError}, spectate::playback_dolphin};

type PayloadSizes = HashMap<u8, u16>;

// TODO: New name since this is really a full dolphin mirror manager
pub(crate) struct SlpFileWriter {
    mirror_in_dolphin: bool,
    spectate_directory_path: PathBuf,
    current_file: Option<File>,
    payload_sizes: Option<PayloadSizes>,
}

impl SlpFileWriter {
    pub(crate) fn new(mirror_in_dolphin: bool) -> Result<SlpFileWriter, ConfigError> {
        if mirror_in_dolphin {
            playback_dolphin::launch_playback_dolphin()?;
        }

        let config = config::get_application_config();

        Ok(SlpFileWriter {
            mirror_in_dolphin: mirror_in_dolphin,
            spectate_directory_path: config.get_spectate_replay_directory_path()?,
            current_file: None,
            payload_sizes: None,
        })
    }

    fn read_next_event<R: Read>(&mut self, mut data: R) -> std::io::Result<usize> {
        // so payload sizes might be send
        match &self.payload_sizes {
            None => {
                let (bytes_read, payload_sizes) = parse_payloads(data)?;
                self.payload_sizes = Some(payload_sizes);
                Ok(bytes_read)
            }
            Some(payload_sizes) => {
                let command = data.read_u8()?;
                let command_size = payload_sizes.get(&command).unwrap().to_owned();
                let mut read_buf = vec![0 as u8; command_size as usize];
                data.read_exact(&mut read_buf).unwrap();
                Ok((command_size + 1) as usize) // include command byte in read size
            }
        }
    }

    fn write_payload(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if let Some(ref mut file) = self.current_file {
            file.write_all(data)?;
            Ok(data.len())
        } else {
            Ok(0)
        }
    }
}

impl Write for SlpFileWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let mut r = BufReader::new(data);
        let mut bytes_written: usize = 0;

        while let Ok(bytes_read) = self.read_next_event(&mut r) {
            let event_data = &data[bytes_written..(bytes_written + bytes_read)];

            // Set current file and mirror as needed according to event type
            match event_data[0] {
                0x35 => {
                    // event payloads; new game started
                    let current_local: DateTime<Local> = Local::now();
                    let dt = current_local.format("%Y%m%d%H%M%S");
                    let filename = format!("Game_{}.slp", dt);
                    let fp = self.spectate_directory_path.join(filename.clone());
                    self.current_file = Some(File::create(&fp).unwrap());

                    if self.mirror_in_dolphin {
                        playback_dolphin::mirror_file(fp);
                    }

                    bytes_written += self.write_payload(event_data)?;
                }
                0x39 => {
                    // game end
                    bytes_written += self.write_payload(event_data)?;
                    self.current_file = None;
                    self.payload_sizes = None;
                }
                _ => {
                    bytes_written += self.write_payload(event_data)?;
                }
            }
        }

        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[repr(u8)]
#[expect(unused)]
pub enum Event {
    MessageSplitter = 0x10,
    Payloads = 0x35,
    GameStart = 0x36,
    FramePre = 0x37,
    FramePost = 0x38,
    GameEnd = 0x39,
    FrameStart = 0x3A,
    Item = 0x3B,
    FrameEnd = 0x3C,
    GeckoCodes = 0x3D,
    FodPlatform = 0x3F,
    DreamlandWhispy = 0x40,
    StadiumTransformation = 0x41,
}

// https://github.com/hohav/peppi/blob/aae5bd380fb6660d846797b19dd66dd232b5b04c/src/io/slippi/de.rs#L531

/// Parses an Event Payloads event from `r`, which must come first in the raw
/// stream and tells us the sizes for all other events to follow.
///
/// Returns the number of bytes read, and a map of event codes to payload sizes.
/// This map uses raw event codes as keys (as opposed to `Event` enum values)
/// for forwards compatibility, to allow skipping unknown events.
fn parse_payloads<R: Read>(mut r: R) -> std::io::Result<(usize, PayloadSizes)> {
    let code = r.read_u8()?;
    if code != Event::Payloads as u8 {
        return Err(Error::new(ErrorKind::Other, format!("expected event payloads, but got: {:#02x}", code)));
    }

    // Size in bytes of the subsequent list of payload-size kv pairs.
    // Each pair is 3 bytes, so this size should be divisible by 3.
    // However the value includes this size byte itself, so it's off-by-one.
    let size = r.read_u8()?;
    if size % 3 != 1 {
        return Err(Error::new(ErrorKind::Other, format!("invalid payload size: {}", size)));
    }

    let mut buf = vec![0; (size - 1) as usize];
    r.read_exact(&mut buf)?;
    let buf = &mut &buf[..];

    let mut sizes: PayloadSizes = HashMap::new();
    for _ in (0..size - 1).step_by(3) {
        let code = buf.read_u8()?;
        let size = buf.read_u16::<BE>()?;
        sizes.insert(code, size);
    }

    sizes
        .get(&(Event::GameStart as u8))
        .ok_or_else(|| Error::new(ErrorKind::Other, "missing Game Start in payload sizes"))?;
    sizes
        .get(&(Event::GameEnd as u8))
        .ok_or_else(|| Error::new(ErrorKind::Other, "missing Game End in payload sizes"))?;

    Ok((1 + size as usize, sizes)) // +1 byte for the event code
}
