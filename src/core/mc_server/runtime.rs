use crate::core::mc_server::base::McServer;
use async_trait::async_trait;
use std::any::{Any, TypeId};

/// Runtime 管理器
/// 管理 Java 等运行环境
#[async_trait]
pub trait McServerRuntime: McServer {
    async fn check_runtime(&self, path: &std::path::Path) -> anyhow::Result<()>;
    async fn setup_runtime(&self, path: &std::path::Path) -> anyhow::Result<()>;
    /// 更高级的打印脚本
    /// 此方法比 McServer 的 script 有更高优先级
    fn ext_script(&self, arch: &str, os: &str) -> anyhow::Result<String>;
}

pub struct RuntimeManager;

impl RuntimeManager {
    /// 尝试管理 runtime
    pub fn impl_runtime(server: &dyn McServer) -> Option<&dyn McServerRuntime> {
        if server.type_id() == TypeId::of::<dyn McServerRuntime>() {
            Some(unsafe { *(server as *const dyn Any as *const &dyn McServerRuntime) })
        } else {
            None
        }
    }
}
