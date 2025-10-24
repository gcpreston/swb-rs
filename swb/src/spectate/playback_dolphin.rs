use std::{fs::File, path::PathBuf, process::Command};

use serde::{Deserialize, Serialize};
use rand::{distr::Alphanumeric, Rng};

use crate::config::{self, ConfigError};

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct CommSpec {
    mode: String,
    commandId: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay: Option<String>
}

pub(crate) fn launch_playback_dolphin() -> Result<(), ConfigError> {
    let spec = CommSpec { mode: "mirror".to_string(), commandId: "0".to_string(), replay: None };
    write_comm_spec(spec);

    let config = config::get_application_config();

    Command::new(config.playback_dolphin_path()?)
        .args(["-e", &config.iso_path().unwrap(), "-i", config.comm_spec_path().to_str().unwrap()])
        .spawn()
        .expect("failed to execute command");

    Ok(())
}

pub(crate) fn mirror_file(fp: PathBuf) {
    let command_id: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();

    let spec = CommSpec { mode: "mirror".to_string(), commandId: command_id.to_string(), replay: Some(fp.to_str().unwrap().to_string()) };
    write_comm_spec(spec);
}

fn write_comm_spec(spec: CommSpec) {
    let config = config::get_application_config();
    let file = File::create(config.comm_spec_path()).unwrap();
    serde_json::to_writer(file, &spec).unwrap();
}

pub fn close_playback_dolphin() {
    println!("close_playback_dolphin");
    // clean up temp comm spec
    // gracefully shutdown the process
}
