use crate::core::mc_server::base::{McServer, McVersion};
use anyhow::{Result, anyhow};
use async_trait::async_trait;

/// 更新器
#[async_trait]
pub trait McServerUpdate: McServer {
    async fn latest_version(&self) -> Result<McVersion>;
    async fn install_version(&self, target: McVersion) -> Result<()>;
}

#[async_trait]
pub trait TryMcServerUpdate: McServer {
    fn impl_updater(&self) -> bool;
    async fn latest(&self) -> Result<McVersion>;
    async fn is_latest(&self, current: McVersion) -> Result<bool>;
    async fn install(&self, target: McVersion) -> Result<()>;
    async fn update(&self, current: McVersion) -> Result<()>;
}

#[async_trait]
impl<T> TryMcServerUpdate for T
where
    T: McServer + Sync,
{
    default fn impl_updater(&self) -> bool {
        false
    }

    default async fn latest(&self) -> Result<McVersion> {
        Err(anyhow!(
            "The updater has not been implemented for this server."
        ))
    }

    default async fn is_latest(&self, _: McVersion) -> Result<bool> {
        Err(anyhow!(
            "The updater has not been implemented for this server."
        ))
    }

    default async fn install(&self, _: McVersion) -> Result<()> {
        Err(anyhow!(
            "The updater has not been implemented for this server."
        ))
    }

    default async fn update(&self, _: McVersion) -> Result<()> {
        Err(anyhow!(
            "The updater has not been implemented for this server."
        ))
    }
}

#[async_trait]
impl<T> TryMcServerUpdate for T
where
    T: McServerUpdate + Sync,
{
    default fn impl_updater(&self) -> bool {
        true
    }

    default async fn latest(&self) -> Result<McVersion> {
        self.latest_version().await
    }

    default async fn is_latest(&self, current: McVersion) -> Result<bool> {
        Ok(current >= self.latest().await?)
    }

    default async fn install(&self, target: McVersion) -> Result<()> {
        self.install_version(target).await
    }

    default async fn update(&self, current: McVersion) -> Result<()> {
        let latest = self.latest().await?;
        if current < latest {
            self.install(latest).await?
        }
        Ok(())
    }
}
