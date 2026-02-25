pub use crate::core::mc_server::McVersion;
use crate::core::mc_server::plugin::McServerPlugin;
use crate::core::mc_server::runtime::McServerRuntime;
use crate::core::mc_server::update::McServerUpdate;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::path::Path;
use tokio::process::Command;

/// MC 服务端最少实现的 trait
pub trait McServer: Any + Sync {
    /// 从版本号创建空实例
    fn new(path: &Path) -> Box<dyn McServer>
    where
        Self: Sized;
    /// 打印当前平台的运行脚本
    /// 若实现了 McServerRuntime Trait ，此函数无效
    fn script(&self) -> Result<String>;
    /// 启动实例
    fn start(&self) -> Result<Command>;
    /// 实现更新器
    fn impl_update<'a>(&'a self) -> Option<&'a dyn McServerUpdate> {
        None
    }
    /// 实现 Runtime 管理器
    fn impl_runtime<'a>(&'a self) -> Option<&'a dyn McServerRuntime> {
        None
    }
    /// 实现插件管理器
    fn impl_plugin<'a>(&'a self) -> Option<&'a dyn McServerPlugin> {
        None
    }
    /// 需要持久化的内部配置信息
    fn to_config(&self) -> Result<Box<dyn erased_serde::Serialize + '_>> {
        #[derive(Serialize)]
        struct NoConfig;
        Ok(Box::new(NoConfig))
    }
    /// 加载配置信息
    fn load_config(&mut self, de: &mut dyn erased_serde::Deserializer) -> Result<()> {
        #[derive(Deserialize)]
        struct NoConfig;
        let _cfg = erased_serde::deserialize::<NoConfig>(de)?;
        Ok(())
    }
}
