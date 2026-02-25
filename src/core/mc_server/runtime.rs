use crate::core::mc_server::base::McServer;
use anyhow::{Result, anyhow};
use async_trait::async_trait;

/// Runtime 管理器
/// 管理 Java 等运行环境
/// 自定义的运行时请安装到 .toymine/runtime 目录
#[async_trait]
pub trait McServerRuntime: McServer {
    /// 检查当前运行时
    async fn ready_runtime(&self) -> Result<bool>;
    /// 安装运行时
    async fn setup_runtime(&self) -> Result<()>;
    /// 扩展的打印脚本
    /// 此方法比 McServer 的 script 有更高优先级
    fn ext_script(&self, arch: &str, os: &str) -> Result<String>;
}

impl dyn McServer {
    pub fn gen_script(&self) -> Result<String> {
        match self.impl_runtime() {
            None => self.script(),
            Some(t) => t.ext_script(std::env::consts::ARCH, std::env::consts::OS),
        }
    }

    pub fn specific_script(&self, arch: &str, os: &str) -> Result<String> {
        match self.impl_runtime() {
            None => Err(anyhow!(
                "The runtime manager has not been implemented for this server."
            )),
            Some(t) => t.ext_script(arch, os),
        }
    }

    pub async fn prepare(&self) -> Result<()> {
        match self.impl_runtime() {
            None => Err(anyhow!(
                "The runtime manager has not been implemented for this server."
            )),
            Some(t) => {
                if !t.ready_runtime().await? {
                    t.setup_runtime().await
                } else {
                    Ok(())
                }
            }
        }
    }
}
