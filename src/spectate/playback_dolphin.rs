use std::path::PathBuf;

pub(crate) fn launch_playback_dolphin() {
    println!("launch_playback_dolphin");
    // write blank comm spec file in temp location
    // locate playback dolphin
    // launch with blank spec file
}

pub(crate) fn mirror_file(fp: PathBuf) {
    println!("mirror_file with {:?}", fp);
    // overwrite comm spec with
    // commandId: random string (launcher uses 16 bytes)
    // replay: fp
    // mode: mirror
}

pub(crate) fn close_playback_dolphin() {
    println!("close_playback_dolphin");
    // clean up temp comm spec
    // gracefully shutdown the process
}