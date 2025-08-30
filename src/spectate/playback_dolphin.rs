use std::{fs::File, path::PathBuf, process::Command};

use serde::{Deserialize, Serialize};
use rand::{distr::Alphanumeric, Rng};

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct CommSpec {
    mode: String,
    commandId: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay: Option<String>
}

pub(crate) fn launch_playback_dolphin() {
    println!("launch_playback_dolphin");
    // write blank comm spec file in temp location
    // locate playback dolphin
    // launch with blank spec file

    let playback_dolphin_path = "/Users/graham.preston/Library/Application Support/Slippi Launcher/playback/Slippi Dolphin.app/Contents/MacOS/Slippi Dolphin";
    let comm_spec_path = "./launch.json";
    let melee_path = "/Users/graham.preston/Games/Super Smash Bros. Melee (USA) (En,Ja) (v1.02).iso";

    let spec = CommSpec { mode: "mirror".to_string(), commandId: "0".to_string(), replay: None };
    let file = File::create(comm_spec_path).unwrap();
    serde_json::to_writer(file, &spec).unwrap();

    Command::new(playback_dolphin_path)
        .args(["-e", melee_path, "-i", comm_spec_path])
        .spawn()
        .expect("failed to execute command");
}

pub(crate) fn mirror_file(fp: PathBuf) {
    println!("mirror_file with {:?}", fp);
    // overwrite comm spec with
    // commandId: random string (launcher uses 16 bytes)
    // replay: fp
    // mode: mirror

    let command_id: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    println!("generated command id {}", command_id);

    let spec = CommSpec { mode: "mirror".to_string(), commandId: command_id.to_string(), replay: Some(fp.to_str().unwrap().to_string()) };
    let comm_spec_path = "./launch.json";
    let file = File::create(comm_spec_path).unwrap();
    serde_json::to_writer(file, &spec).unwrap();
}

pub(crate) fn close_playback_dolphin() {
    println!("close_playback_dolphin");
    // clean up temp comm spec
    // gracefully shutdown the process
}
