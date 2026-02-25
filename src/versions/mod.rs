use crate::core::config::project::McServerConfig;
use crate::core::mc_server::McType::{Bedrock, Java};
use crate::core::mc_server::base::McServer;
use crate::versions::bds::BDS;
use crate::versions::paper_like::{PAPER_MAP, PaperLike};
use crate::versions::pumpkin::Pumpkin;
use crate::versions::quick_analyze::analyze_je_game;
use crate::versions::vanilla::Vanilla;
use anyhow::{Result, anyhow};
use std::path::Path;

mod bds;
mod paper_like;
mod pumpkin;
mod quick_analyze;
pub mod vanilla;

pub struct VersionManager;

impl VersionManager {
    pub fn detect_server() -> Result<Option<Box<dyn McServer>>> {
        let jar = Path::new("server.jar");
        let bds = Path::new(match std::env::consts::OS {
            "windows" => "bedrock_server.exe",
            _ => "bedrock_server",
        });
        let pum = Path::new(match std::env::consts::OS {
            "windows" => "pumpkin.exe",
            _ => "pumpkin",
        });

        let jar_mime = "application/zip";
        let bin_mime = match std::env::consts::OS {
            "windows" => "application/vnd.microsoft.portable-executable",
            _ => "application/x-executable",
        };

        let is_jar =
            jar.is_file() && infer::get_from_path(jar)?.is_some_and(|t| t.mime_type() == jar_mime);
        let is_bds =
            bds.is_file() && infer::get_from_path(bds)?.is_some_and(|t| t.mime_type() == bin_mime);
        let is_pum =
            pum.is_file() && infer::get_from_path(pum)?.is_some_and(|t| t.mime_type() == bin_mime);

        let find_count = [is_jar, is_bds, is_pum].into_iter().filter(|&x| x).count();
        if find_count == 0 {
            return Ok(None);
        } else if find_count > 1 {
            return Err(anyhow!("Find multiple servers"));
        }

        if is_jar {
            return Ok(match analyze_je_game(jar)?.server_type {
                Java(s) => {
                    if s.as_str() == "vanilla" {
                        Some(Vanilla::new(jar))
                    } else if PAPER_MAP.iter().filter(|&x| x.name == s.as_str()).count() != 0 {
                        Some(PaperLike::new(jar))
                    } else {
                        None
                    }
                }
                Bedrock(_) => unreachable!(),
            });
        }
        if is_bds {
            return Ok(Some(BDS::new(bds)));
        }
        if is_pum {
            return Ok(Some(Pumpkin::new(pum)));
        }

        unreachable!()
    }
    pub fn detect_config() -> Result<McServerConfig> {
        todo!()
    }
    pub fn from_cfg(cfg: &McServerConfig) -> Option<Box<dyn McServer>> {
        match &cfg.project.version.server_type {
            Java(s) => {
                if s == "vanilla" {
                    Some(Vanilla::new(&cfg.project.server_file))
                } else if PAPER_MAP.iter().filter(|&x| x.name == s.as_str()).count() != 0 {
                    Some(PaperLike::new(&cfg.project.server_file))
                } else if s == "pumpkin" {
                    Some(Pumpkin::new(&cfg.project.server_file))
                } else {
                    None
                }
            }
            Bedrock(_) => Some(BDS::new(&cfg.project.server_file)),
        }
    }
}
