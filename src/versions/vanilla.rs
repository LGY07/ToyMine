use crate::core::mc_server::base::{McServer, McVersion};
use crate::core::mc_server::runtime::McServerRuntime;
use crate::core::mc_server::update::McServerUpdate;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

#[derive(Serialize, Deserialize)]
pub struct Vanilla {
    server_path: PathBuf,
    runtime_path: PathBuf,
}

#[async_trait]
impl McServer for Vanilla {
    fn new(path: &Path) -> Box<dyn McServer>
    where
        Self: Sized,
    {
        debug!("Vanilla");
        Box::new(Vanilla {
            server_path: path.to_path_buf(),
            runtime_path: "java".parse().unwrap(),
        })
    }

    fn script(&self) -> Result<String> {
        unreachable!("It should be implemented in McRuntime.")
    }

    fn start(&self) -> Result<tokio::process::Command> {
        let mut command = tokio::process::Command::new(&self.runtime_path);
        command.arg("-jar").arg(&self.server_path).arg("-nogui");
        Ok(command)
    }

    fn impl_update<'a>(&'a self) -> Option<&'a dyn McServerUpdate> {
        Some(self)
    }
}

#[async_trait]
impl McServerUpdate for Vanilla {
    async fn latest_version(&self) -> Result<McVersion> {
        todo!()
    }

    async fn install_version(&self, target: McVersion) -> Result<()> {
        todo!()
    }
}

#[async_trait]
impl McServerRuntime for Vanilla {
    async fn ready_runtime(&self) -> Result<bool> {
        todo!()
    }
    async fn setup_runtime(&self) -> Result<()> {
        todo!()
    }
    fn ext_script(&self, _: &str, os: &str) -> Result<String> {
        let mut s = String::new();
        if os == "windows" {
            s.push_str("@echo off\n");
        } else {
            s.push_str("#!/bin/env bash")
        }
        s.push_str(
            format!(
                "{} -jar {} -nogui",
                self.runtime_path.to_string_lossy(),
                self.server_path.to_string_lossy()
            )
            .as_str(),
        );
        Ok(s)
    }
}
