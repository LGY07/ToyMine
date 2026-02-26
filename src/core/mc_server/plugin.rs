use crate::core::mc_server::NotImplemented;
use crate::core::mc_server::base::McServer;
use anyhow::Result;
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
    /// 移除插件
    async fn remove(&self, plugin: ServerPlugin) -> Result<()>;
    /// 查询最新版本
    async fn latest(&self, plugin: ServerPlugin) -> Result<ServerPlugin>;
}

impl dyn McServer {
    pub async fn plugin_list(&self) -> Result<Vec<(&'static str, ServerPlugin)>> {
        match self.impl_plugin() {
            None => Err(NotImplemented::Plugin.into()),
            Some(t) => {
                let mut list = Vec::new();
                for repo in t.get_repo() {
                    list.extend(
                        repo.list()
                            .await
                            .into_iter()
                            .map(|plugin| (repo.name(), plugin)),
                    )
                }
                Ok(list)
            }
        }
    }

    pub async fn plugin_search(&self, keyword: &str) -> Result<Vec<(&'static str, ServerPlugin)>> {
        match self.impl_plugin() {
            None => Err(NotImplemented::Plugin.into()),
            Some(t) => {
                let mut list = Vec::new();
                for repo in t.get_repo() {
                    list.extend(
                        repo.search(keyword)
                            .await
                            .into_iter()
                            .map(|plugin| (repo.name(), plugin)),
                    )
                }
                Ok(list)
            }
        }
    }

    pub async fn plugin_install(&self, repo_name: &str, plugin: ServerPlugin) -> Result<()> {
        match self.impl_plugin() {
            None => Err(NotImplemented::Plugin.into()),
            Some(t) => {
                if let Some(repo) = t.get_repo().iter().find(|&&repo| repo.name() == repo_name) {
                    repo.install(plugin).await
                } else {
                    // 静态插件仓库不应该找不到
                    unreachable!("No such plugin repository: {}", repo_name)
                }
            }
        }
    }
    pub async fn plugin_remove(&self, repo_name: &str, plugin: ServerPlugin) -> Result<()> {
        match self.impl_plugin() {
            None => Err(NotImplemented::Plugin.into()),
            Some(t) => {
                if let Some(repo) = t.get_repo().iter().find(|&&repo| repo.name() == repo_name) {
                    repo.remove(plugin).await
                } else {
                    // 静态插件仓库不应该找不到
                    unreachable!("No such plugin repository: {}", repo_name)
                }
            }
        }
    }

    pub async fn plugin_latest(
        &self,
        repo_name: &str,
        plugin: ServerPlugin,
    ) -> Result<ServerPlugin> {
        match self.impl_plugin() {
            None => Err(NotImplemented::Plugin.into()),
            Some(t) => {
                if let Some(repo) = t.get_repo().iter().find(|&&repo| repo.name() == repo_name) {
                    repo.latest(plugin).await
                } else {
                    // 静态插件仓库不应该找不到
                    unreachable!("No such plugin repository: {}", repo_name)
                }
            }
        }
    }
}
