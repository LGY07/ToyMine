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

#[async_trait]
pub trait TryMcServerRuntime: McServer {
    fn impl_runtime(&self) -> bool;
    fn gen_script(&self) -> Result<String>;
    fn specific_script(&self, arch: &str, os: &str) -> Result<String>;
    async fn prepare(&self) -> Result<()>;
}

#[async_trait]
impl<T> TryMcServerRuntime for T
where
    T: McServer + Sync,
{
    default fn impl_runtime(&self) -> bool {
        false
    }

    default fn gen_script(&self) -> Result<String> {
        self.script()
    }

    default fn specific_script(&self, _: &str, _: &str) -> Result<String> {
        Err(anyhow!(
            "The runtime manager has not been implemented for this server."
        ))
    }

    default async fn prepare(&self) -> Result<()> {
        Err(anyhow!(
            "The runtime manager has not been implemented for this server."
        ))
    }
}

#[async_trait]
impl<T> TryMcServerRuntime for T
where
    T: McServerRuntime + Sync,
{
    default fn impl_runtime(&self) -> bool {
        true
    }
    default fn gen_script(&self) -> Result<String> {
        self.ext_script(std::env::consts::ARCH, std::env::consts::OS)
    }
    default fn specific_script(&self, arch: &str, os: &str) -> Result<String> {
        Ok(self.ext_script(arch, os)?)
    }
    default async fn prepare(&self) -> Result<()> {
        if !self.ready_runtime().await? {
            self.setup_runtime().await
        } else {
            Ok(())
        }
    }
}
