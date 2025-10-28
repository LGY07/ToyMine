use crate::project_manager::Config;
use futures::future::err;
use log::error;
use std::path::Path;

#[derive(PartialEq, Debug)]
pub enum ConfigErr {
    NotConfigured,
    ConfigBroken,
}

fn test_exists() -> bool {
    // NMSL.toml 或者 .nmsl 存在
    Path::new("NMSL.toml").exists() || Path::new(".nmsl").exists()
}

fn read_config() -> Result<Config, ConfigErr> {
    let config_path = Path::new("NMSL.toml");

    // 检查目录/文件是否正确
    if !config_path.is_file() || !Path::new(".nmsl").is_dir() {
        return Err(ConfigErr::ConfigBroken);
    }

    match Config::from_file(config_path) {
        Ok(v) => Ok(v),
        Err(e) => {
            error!("{:?}", e);
            error!(
                "Failed to read the configuration file. Please check if the configuration file is correct."
            );
            Err(ConfigErr::ConfigBroken)
        }
    }
}

pub fn get_info() -> Result<Config, ConfigErr> {
    if !test_exists() {
        return Err(ConfigErr::NotConfigured);
    }

    read_config()
}

pub fn print_info() {
    match get_info() {
        Ok(v) => {
            println!("{}", v)
        }
        Err(e) => {
            error!("{:?}", e)
        }
    }
}
