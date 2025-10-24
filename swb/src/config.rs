use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File}, io, path::{Path, PathBuf}
};
// use thiserror::Error;

// TODO: Change to thiserror definition as appropriate
#[derive(Debug)]
pub enum ConfigError {
    FileRead(PathBuf, io::Error),
    FileWrite(PathBuf, io::Error),
    JsonParse(PathBuf, serde_json::Error),
    JsonSerialize(PathBuf, serde_json::Error),
    PlatformError(String),

    // #[error("Error decoding path: {0}")]
    // PathDecodeError(#[from] OsString),
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
            ConfigError::FileWrite(path, err) => {
                write!(
                    f,
                    "Failed to write settings file at {}: {}",
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
            ConfigError::JsonSerialize(path, err) => {
                write!(
                    f,
                    "Failed to serialize JSON for settings file at {}: {}",
                    path.display(),
                    err
                )
            }
            ConfigError::PlatformError(platform) => {
                write!(
                    f,
                    "Unable to locate Playback Dolphin for unknown platform: {}, please submit a bug report to the repository indicating your operating system:\nhttps://github.com/gcpreston/swb-rs/issues/new",
                    platform
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Deserialize)]
struct SlippiLauncherSettingsFile {
    settings: SlippiLauncherSettings,
}

#[derive(Deserialize)]
struct SlippiLauncherSettings {
    #[serde(rename = "isoPath")]
    iso_path: String,
    #[serde(rename = "rootSlpPath")]
    root_slp_path: String,
}

#[derive(Deserialize, Serialize, Default)]
struct SpectateSettings {
    #[serde(rename = "spectate_directory", skip_serializing_if = "Option::is_none")]
    spectate_directory: Option<String>,
}

const SETTINGS_FILE_NAME: &str = "settings.json";

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
        let config_path = self.project_dirs.config_dir();

        if !config_path.exists() {
            fs::create_dir_all(config_path).unwrap();
        }
        config_path
    }

    fn slippi_launcher_config_path(&self) -> PathBuf {
        // On Windows, the actual config path as set by the directories library
        // has an extra \config directory appended, which Mac and Linux don't
        // have. This logic allows us to navigate correctly.
        let join_path =
            if std::env::consts::OS == "windows" {
               "../../Slippi Launcher"
            } else {
                "../Slippi Launcher"
            };
        self.config_path().join(join_path)
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

    pub(crate) fn playback_dolphin_path(&self) -> Result<PathBuf, ConfigError> {
        let playback_path = self.slippi_launcher_config_path().join("playback");

        // https://github.com/jmlee337/auto-slp-player/blob/bb8fe89370ae7d5d7e954a07e7437f3e2a1da1e5/src/main/ipc.ts#L207
        match std::env::consts::OS {
            "linux" => Ok(playback_path.join("Slippi_Playback-x86_64.AppImage")),
            "macos" => Ok(playback_path.join("Slippi Dolphin.app/Contents/MacOS/Slippi Dolphin")),
            "windows" => Ok(playback_path.join("Slippi Dolphin.exe")),
            other_platform => Err(ConfigError::PlatformError(other_platform.to_string())),
        }
    }

    fn slippi_launcher_settings(&self) -> Result<SlippiLauncherSettingsFile, ConfigError> {
        let launcher_settings_path = self.slippi_launcher_config_path().join("Settings");
        let launcher_settings_content = fs::read_to_string(&launcher_settings_path)
            .map_err(|e| ConfigError::FileRead(launcher_settings_path.clone(), e))?;

        let launcher_settings: SlippiLauncherSettingsFile = serde_json::from_str(&launcher_settings_content)
            .map_err(|e| ConfigError::JsonParse(launcher_settings_path, e))?;

        Ok(launcher_settings)
    }

    /// Get the root SLP path from Slippi Launcher settings.
    fn root_slp_path(&self) -> Result<String, ConfigError> {
        let launcher_settings = self.slippi_launcher_settings()?;
        Ok(launcher_settings.settings.root_slp_path)
    }

    /// Get the path to the ISO stored in Slippi Launcher's settings.
    pub(crate) fn iso_path(&self) -> Result<String, ConfigError> {
        let launcher_settings = self.slippi_launcher_settings()?;
        Ok(launcher_settings.settings.iso_path)
    }

    // TODO: All these are starting to feel more and more like functions rather
    // than methods. It feels like they should probably just be the same but
    // calling the equivalent of get_application_config() as their first line.
    // This can be changed once the logic is implemented and all the usages
    // throughout the application are obvious.

    /// Set the directory in which to download Slippi replays that are being spectated.
    fn set_spectate_replay_directory_path(
        &self,
        dir_path: String,
    ) -> Result<(), ConfigError> {
        let settings_path = self.config_path().join(SETTINGS_FILE_NAME);

        // Read existing settings or create empty object
        let mut settings = if settings_path.exists() {
            let content = fs::read_to_string(&settings_path)
                .map_err(|e| ConfigError::FileRead(settings_path.clone(), e))?;

            if content.trim().is_empty() {
                SpectateSettings::default()
            } else {
                serde_json::from_str(&content)
                    .map_err(|e| ConfigError::JsonParse(settings_path.clone(), e))?
            }
        } else {
            SpectateSettings::default()
        };

        // Update the spectate directory
        settings.spectate_directory = Some(dir_path);

        // Write back to file
        let json_content = serde_json::to_string_pretty(&settings)
            .map_err(|e| ConfigError::JsonSerialize(settings_path.clone(), e))?;

        fs::write(&settings_path, json_content)
            .map_err(|e| ConfigError::FileWrite(settings_path, e))?;

        Ok(())
    }

    /// Fetch the path to download replays to which are being spectated.
    /// If not explicitly set, defaults to rootSlpPath + "Spectate" from Slippi Launcher settings
    /// and saves this default to the settings file.
    pub(crate) fn get_spectate_replay_directory_path(&self) -> Result<PathBuf, ConfigError> {
        let settings_path = self.config_path().join(SETTINGS_FILE_NAME);

        // Try to read existing spectate settings
        let maybe_spectate_directory = if settings_path.exists() {
            let content = fs::read_to_string(&settings_path)
                .map_err(|e| ConfigError::FileRead(settings_path.clone(), e))?;

            if !content.trim().is_empty() {
                let settings: SpectateSettings = serde_json::from_str(&content)
                    .map_err(|e| ConfigError::JsonParse(settings_path, e))?;
                settings.spectate_directory
            } else {
                None
            }
        } else {
            None
        };

        let spectate_directory =
            if let Some(dir) = maybe_spectate_directory {
                PathBuf::from(dir)
            } else {
                // Use default: rootSlpPath + "Spectate"
                let root_slp_path = self.root_slp_path()?;
                let default_path = PathBuf::from(root_slp_path).join("Spectate");

                // Save the default path to settings
                self.set_spectate_replay_directory_path(default_path.clone().into_os_string().into_string().unwrap())?;

                default_path
            };

        // Create spectate directory if it doesn't exist
        fs::create_dir_all(spectate_directory.clone()).unwrap(); // TODO: Handle error via ConfigError

        Ok(spectate_directory)
    }
}
