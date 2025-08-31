use std::{fs::{self, File}, path::{Path, PathBuf}};
use directories::ProjectDirs;

pub(crate) struct Config {
    project_dirs: ProjectDirs
}

pub(crate) fn get_application_config() -> Config {
    let proj_dirs = ProjectDirs::from("", "", "swb").unwrap();

    Config {
        project_dirs: proj_dirs
    }
}

impl Config {
    fn config_path(&self) -> &Path {
        let config_path = self.project_dirs.config_dir(); // TODO: On windows this probably is not quite right
        if !config_path.exists() {
            fs::create_dir_all(config_path).unwrap();
        }
        config_path
    }

    pub(crate) fn comm_spec_path(&self) -> PathBuf {
        let temp_path = self.config_path().join("temp");
        if !temp_path.exists() {
            fs::create_dir(&temp_path).unwrap();
        }
        let comm_spec_path = temp_path.join("launch.json");
        if !comm_spec_path.exists() {
            File::create(&comm_spec_path).unwrap();
        }
        comm_spec_path
    }

    pub(crate) fn playback_dolphin_path(&self) -> PathBuf {
        let playback_path = self.config_path().join("playback");

        // https://github.com/jmlee337/auto-slp-player/blob/bb8fe89370ae7d5d7e954a07e7437f3e2a1da1e5/src/main/ipc.ts#L207
        match std::env::consts::OS {
            "linux" => playback_path.join("Slippi_Playback-x86_64.AppImage"),
            "macos" => playback_path.join("Slippi Dolphin.app/Contents/MacOS/Slippi Dolphin"),
            "windows" => playback_path.join("Slippi Dolphin.exe"),
            _ => todo!()
        }
    }

    pub(crate) fn iso_path(&self) -> String {
        todo!()
    }

    pub(crate) fn spectate_replay_directory_path(&self) -> String {
        todo!()
    }
}