use crate::core::mc_server::base::McServer;
use anyhow::{Result, anyhow};
use async_trait::async_trait;

/// 插件管理器
#[async_trait]
pub trait McServerPlugin: McServer {
    fn get_repo(&self) -> &'static [&'static dyn ServerPluginRepo];
}

pub struct ServerPlugin {
    id: usize,
    name: String,
    version: String,
    description: String,
}

/// 插件仓库
#[async_trait]
pub trait ServerPluginRepo: Sync {
    /// 仓库名称，此字符串同时用于标识仓库
    fn name(&self) -> &'static str;
    /// 列出本地插件
    async fn list(&self) -> Vec<ServerPlugin>;
    /// 查询插件
    async fn search(&self, keyword: &str) -> Vec<ServerPlugin>;
    /// 安装插件
    async fn install(&self, plugin: ServerPlugin) -> Result<()>;
    /// 查询最新版本
    async fn latest(&self, plugin: ServerPlugin) -> Result<ServerPlugin>;
}

#[async_trait]
pub trait TryMcServerPlugin: McServer {
    fn impl_plugin(&self) -> bool;
    /// 列出本地插件
    async fn list(&self) -> Result<Vec<(&'static str, ServerPlugin)>>;
    /// 查询插件
    async fn search(&self, keyword: &str) -> Result<Vec<(&'static str, ServerPlugin)>>;
    /// 安装插件
    async fn install(&self, repo: &str, plugin: ServerPlugin) -> Result<()>;
    /// 查询最新版本
    async fn latest(&self, repo: &str, plugin: ServerPlugin) -> Result<ServerPlugin>;
}

#[async_trait]
impl<T> TryMcServerPlugin for T
where
    T: McServer + Sync,
{
    default fn impl_plugin(&self) -> bool {
        false
    }

    default async fn list(&self) -> Result<Vec<(&'static str, ServerPlugin)>> {
        Err(anyhow!(
            "The plugin manager has not been implemented for this server."
        ))
    }

    default async fn search(&self, _: &str) -> Result<Vec<(&'static str, ServerPlugin)>> {
        Err(anyhow!(
            "The plugin manager has not been implemented for this server."
        ))
    }

    default async fn install(&self, _: &str, _: ServerPlugin) -> Result<()> {
        Err(anyhow!(
            "The plugin manager has not been implemented for this server."
        ))
    }

    default async fn latest(&self, _: &str, _: ServerPlugin) -> Result<ServerPlugin> {
        Err(anyhow!(
            "The plugin manager has not been implemented for this server."
        ))
    }
}

#[async_trait]
impl<T> TryMcServerPlugin for T
where
    T: McServerPlugin + Sync,
{
    default fn impl_plugin(&self) -> bool {
        false
    }

    default async fn list(&self) -> Result<Vec<(&'static str, ServerPlugin)>> {
        let mut list = Vec::new();
        for repo in self.get_repo() {
            list.extend(
                repo.list()
                    .await
                    .into_iter()
                    .map(|plugin| (repo.name(), plugin)),
            )
        }
        Ok(list)
    }

    default async fn search(&self, keyword: &str) -> Result<Vec<(&'static str, ServerPlugin)>> {
        let mut list = Vec::new();
        for repo in self.get_repo() {
            list.extend(
                repo.search(keyword)
                    .await
                    .into_iter()
                    .map(|plugin| (repo.name(), plugin)),
            )
        }
        Ok(list)
    }

    default async fn install(&self, repo_name: &str, plugin: ServerPlugin) -> Result<()> {
        if let Some(repo) = self
            .get_repo()
            .iter()
            .find(|&&repo| repo.name() == repo_name)
        {
            repo.install(plugin).await
        } else {
            // 静态插件仓库不应该找不到
            unreachable!("No such plugin repository: {}", repo_name)
        }
    }

    default async fn latest(&self, repo_name: &str, plugin: ServerPlugin) -> Result<ServerPlugin> {
        if let Some(repo) = self
            .get_repo()
            .iter()
            .find(|&&repo| repo.name() == repo_name)
        {
            repo.latest(plugin).await
        } else {
            // 静态插件仓库不应该找不到
            unreachable!("No such plugin repository: {}", repo_name)
        }
    }
}
