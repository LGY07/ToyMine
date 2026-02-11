use crate::core::mc_server::base::{McServer, McVersion};
use async_trait::async_trait;
use std::any::{Any, TypeId};

/// 更新器
/// 实现更新器时应考虑未安装的情况，apply_update也应该有安装作用
#[async_trait]
pub trait McServerUpdate: McServer {
    async fn check_update(&self) -> anyhow::Result<McVersion>;
    async fn apply_update(&self, target: McVersion) -> anyhow::Result<()>;
}

pub struct Updater;

/// 更新管理器
impl Updater {
    /// 尝试管理更新
    pub fn impl_update(server: &dyn McServer) -> Option<&dyn McServerUpdate> {
        if server.type_id() == TypeId::of::<dyn McServerUpdate>() {
            Some(unsafe { *(server as *const dyn Any as *const &dyn McServerUpdate) })
        } else {
            None
        }
    }
}
