//! Handles loading, serialization, deserialization, and generation of the settings file
use anyhow::Result;
use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
        }
    }
}

impl Settings {
    pub const SETTINGS_FILE: &'static str = "settings.toml";

    pub fn load_current_settings() -> Result<Self> {
        let settings_path = Path::new(Self::SETTINGS_FILE);
        if !settings_path.exists() {
            Settings::generate_settings_file(settings_path)?;
        }
        let settings = Settings::from_path(settings_path)?;
        Ok(settings)
    }

    pub fn from_path<P: AsRef<Path> + Into<PathBuf>>(path: P) -> Result<Settings> {
        let path_str = path.as_ref().display().to_string();
        debug!("Loading settings file: {}", &path_str);
        let mut config = config::Config::default();
        let config_file = config::File::with_name(&path_str);
        config.merge(config_file)?;
        let settings: Settings = config.try_into()?;
        Ok(settings)
    }

    pub fn generate_settings_file<P: AsRef<Path> + Into<PathBuf> + Copy>(path: P) -> Result<()> {
        let settings = Self::default();
        let toml = toml::to_string(&settings)?;

        let mut file = File::create(&path)?;
        file.write_all(toml.as_bytes())?;

        debug!(
            "Generated settings file: {}",
            path_str = path.as_ref().display().to_string()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    #[test]
    fn generate_settings_file() -> Result<()> {
        let path = Path::new("generated_settings.toml");

        Settings::generate_settings_file(&path)?;

        let file_created = path.exists();
        if file_created {
            fs::remove_file(&path)?;
        }

        assert!(file_created);

        Ok(())
    }

    #[test]
    fn load_settings_file() -> Result<()> {
        let path = Path::new("test_settings.toml");

        Settings::generate_settings_file(&path)?;

        let result = Settings::from_path(&path);

        if path.exists() {
            fs::remove_file(&path)?;
        }

        let _ = result?;
        Ok(())
    }
}
