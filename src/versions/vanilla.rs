use std::path::Path;

use crate::core::mc_server::base::{McServer, McVersion};
use crate::core::mc_server::runtime::McServerRuntime;
use crate::core::mc_server::update::McServerUpdate;
use crate::core::mc_server::{McChannel, McType};
use anyhow::Result;
use async_trait::async_trait;
use erased_serde::Deserializer;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

#[derive(Serialize, Deserialize)]
pub struct Vanilla {
    version: McChannel,
}

impl Default for Vanilla {
    fn default() -> Self {
        tracing::warn!("Test");
        Self {
            version: McChannel::Release(1, 21, 11),
        }
    }
}

#[async_trait]
impl McServer for Vanilla {
    fn check(path: &Path) -> bool {
        unreachable!("This version should be quickly analyzed.")
    }

    fn version(&self) -> Result<McVersion> {
        Ok(McVersion {
            server_type: McType::Java("vanilla".to_string()),
            channel: self.version.clone(),
        })
    }

    fn script(&self) -> Result<String> {
        unreachable!("It should be implemented in McRuntime.")
    }

    fn to_config(&self) -> Result<Box<dyn erased_serde::Serialize + '_>> {
        Ok(Box::new(self))
    }

    fn load_config(&mut self, de: &mut dyn Deserializer) -> Result<()> {
        trace!("try loading config");
        *self = erased_serde::deserialize::<Self>(de)?;

        Ok(())
    }

    fn start(&self) -> Result<tokio::process::Command> {
        let mut command = tokio::process::Command::new("java");
        command.arg("-jar").arg("server.jar").arg("-nogui");
        Ok(command)
    }
}

#[async_trait]
impl McServerUpdate for Vanilla {
    async fn check_update(&self) -> Result<McVersion> {
        todo!()
    }
    async fn apply_update(&self, target: McVersion) -> Result<()> {
        todo!()
    }
}

#[async_trait]
impl McServerRuntime for Vanilla {
    async fn check_runtime(&self, path: &Path) -> Result<()> {
        todo!()
    }

    async fn setup_runtime(&self, path: &Path) -> Result<()> {
        todo!()
    }

    fn ext_script(&self, arch: &str, os: &str) -> Result<String> {
        todo!()
    }
}
