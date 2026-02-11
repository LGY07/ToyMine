use crate::core::mc_server::base::McServer;
use crate::versions::quick_analyze::{analyze_je_game, get_mime_type};

use anyhow::Result;
use std::path::PathBuf;

mod quick_analyze;
pub mod vanilla;

pub struct VersionManager;

impl VersionManager {
    async fn open_current() -> Result<Box<dyn McServer>> {
        let mc_version = if get_mime_type(&PathBuf::from("server.jar")) == "application/zip" {
            analyze_je_game(&PathBuf::from("server.jar"))?
        } else if get_mime_type(&PathBuf::from("bedrock_server")) == "application/x-executable" {
            todo!()
        } else if get_mime_type(&PathBuf::from("bedrock_server.exe"))
            == "application/vnd.microsoft.portable-executable"
        {
            todo!()
        } else {
            todo!()
        };

        todo!()
    }
}
