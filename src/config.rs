use directories::ProjectDirs;
use serde::Deserialize;
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) enum ConfigError {
    FileRead(PathBuf, io::Error),
    JsonParse(PathBuf, serde_json::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::FileRead(path, err) => {
                write!(
                    f,
                    "Failed to read settings file at {}: {}",
                    path.display(),
                    err
                )
            }
            ConfigError::JsonParse(path, err) => {
                write!(
                    f,
                    "Failed to parse JSON in settings file at {}: {}",
                    path.display(),
                    err
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Deserialize)]
struct SettingsFile {
    settings: Settings,
}

#[derive(Deserialize)]
struct Settings {
    #[serde(rename = "isoPath")]
    iso_path: String,
}

pub(crate) struct Config {
    project_dirs: ProjectDirs,
}

pub(crate) fn get_application_config() -> Config {
    let proj_dirs = ProjectDirs::from("", "", "swb").unwrap();

    Config {
        project_dirs: proj_dirs,
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
            _ => todo!(),
        }
    }

    /// Fetch the path to the ISO stored in Slippi Launcher's settings.
    pub(crate) fn iso_path(&self) -> Result<String, ConfigError> {
        let settings_path = self.config_path().join("Settings");
        let settings_content = fs::read_to_string(&settings_path)
            .map_err(|e| ConfigError::FileRead(settings_path.clone(), e))?;

        let settings: SettingsFile = serde_json::from_str(&settings_content)
            .map_err(|e| ConfigError::JsonParse(settings_path, e))?;

        Ok(settings.settings.iso_path)
    }


    pub(crate) fn spectate_replay_directory_path(&self) -> String {
        todo!()
    }
}
