pub(crate) use crate::core::mc_server::McVersion;
use anyhow::Result;
use async_trait::async_trait;
use erased_serde::{Deserializer, Serialize};
use std::any::Any;

/// MC 服务端最少实现的 trait
#[async_trait]
pub trait McServer: Any {
    /// 严格检查是否符合当前类型的服务端
    fn check(path: &std::path::Path) -> bool
    where
        Self: Sized;
    /// 获取版本信息
    fn version(&self) -> Result<McVersion>;
    /// 打印当前平台的运行脚本
    fn script(&self) -> Result<String>;
    /// 需要持久化的内部配置文件
    fn to_config(&self) -> Result<Box<dyn Serialize + '_>>;
    /// 加载配置文件
    /// ```
    /// let cfg = erased_serde::deserialize::<YourConfig>(de)?;
    /// ```
    fn load_config(&mut self, de: &mut dyn Deserializer) -> Result<()>;
    /// 启动实例
    fn start(&self) -> Result<tokio::process::Command>;
}
