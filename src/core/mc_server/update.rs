use crate::core::mc_server::base::{McServer, McVersion};
use anyhow::{Result, anyhow};
use async_trait::async_trait;

/// 更新器
#[async_trait]
pub trait McServerUpdate: McServer {
    async fn latest_version(&self) -> Result<McVersion>;
    async fn install_version(&self, target: McVersion) -> Result<()>;
}

impl dyn McServer {
    pub async fn latest(&self) -> Result<McVersion> {
        match self.impl_update() {
            None => Err(anyhow!(
                "The updater has not been implemented for this server."
            )),
            Some(t) => t.latest_version().await,
        }
    }

    pub async fn is_latest(&self, current: McVersion) -> Result<bool> {
        match self.impl_update() {
            None => Err(anyhow!(
                "The updater has not been implemented for this server."
            )),
            Some(t) => Ok(current >= t.latest_version().await?),
        }
    }

    pub async fn install(&self, target: McVersion) -> Result<()> {
        match self.impl_update() {
            None => Err(anyhow!(
                "The updater has not been implemented for this server."
            )),
            Some(t) => t.install_version(target).await,
        }
    }

    pub async fn update(&self, current: McVersion) -> Result<()> {
        match self.impl_update() {
            None => Err(anyhow!(
                "The updater has not been implemented for this server."
            )),
            Some(t) => {
                let latest = t.latest_version().await?;
                if current < latest {
                    t.install_version(latest).await?
                }
                Ok(())
            }
        }
    }
}
