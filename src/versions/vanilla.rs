use crate::core::mc_server::base::{McServer, McVersion};
use crate::core::mc_server::runtime::McServerRuntime;
use crate::core::mc_server::update::McServerUpdate;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Serialize, Deserialize)]
pub struct Vanilla;

#[async_trait]
impl McServer for Vanilla {
    fn new() -> Box<dyn McServer>
    where
        Self: Sized,
    {
        debug!("Vanilla");
        Box::new(Vanilla)
    }

    fn script(&self) -> Result<String> {
        unreachable!("It should be implemented in McRuntime.")
    }

    fn start(&self) -> Result<tokio::process::Command> {
        let mut command = tokio::process::Command::new("java");
        command.arg("-jar").arg("server.jar").arg("-nogui");
        Ok(command)
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

    fn ext_script(&self, arch: &str, os: &str) -> Result<String> {
        todo!()
    }
}
