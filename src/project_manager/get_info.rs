use crate::project_manager::Config;
use std::path::Path;

#[derive(PartialEq)]
pub enum ConfigErr {
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

fn read_config() -> Result<Config, ConfigErr> {
    let config_path = Path::new("NMSL.toml");

    if !config_path.is_file() {
        return Err(ConfigErr::ConfigBroken);
    } else if !Path::new(".nmsl").is_dir() {
        return Err(ConfigErr::ConfigBroken);
    }

    match Config::from_file(config_path) {
        Ok(v) => Ok(v),
        Err(_) => Err(ConfigErr::ConfigBroken),
    }
}

pub fn get_info() -> Result<Config, ConfigErr> {
    if !test_exists() {
        return Err(ConfigErr::NotConfigured);
    }

    read_config()
}
