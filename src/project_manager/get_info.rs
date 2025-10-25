use crate::project_manager::Config;
use std::path::Path;

pub enum NotValid {
    NotConfigured,
    ConfigBroken,
}

fn test_exists() -> bool {
    if Path::new("NMSL.toml").exists() {
        return true;
    } else if Path::new(".nmsl").exists() {
        return true;
    }
    false
}

fn read_config() -> Result<Config, NotValid> {
    let config_path = Path::new("NMSL.toml");

    if !config_path.is_file() {
        return Err(NotValid::ConfigBroken);
    } else if !Path::new(".nmsl").is_dir() {
        return Err(NotValid::ConfigBroken);
    }

    match Config::from_file(config_path) {
        Ok(v) => Ok(v),
        Err(_) => Err(NotValid::ConfigBroken),
    }
}

pub fn get_info() -> Result<Config, NotValid> {
    if !test_exists() {
        return Err(NotValid::NotConfigured);
    }

    read_config()
}
