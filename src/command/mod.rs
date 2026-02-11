use crate::core::mc_server::runner::Runner;
use crate::core::task::TaskManager;
use anyhow::Result;
use tokio::select;
use tokio::sync::Mutex;
use tracing::warn;

mod raw;

/// 命令插件处理结果
/// 多个命令插件时：
/// 若有多个 Unchange，则会向原通道发送一次未处理信息
/// 若有一个 Clear 则取消所有 Unchange
/// 所有的 ToInput 和 ToOutput 会被分别发送到对应位置一次，不受 Clear 影响
pub enum CommandResult {
    /// 结果发送到输入
    ToInput(String),
    /// 结果发送到输出
    ToOutput(String),
    /// 保持结果不变
    Unchange,
    /// 吞掉结果
    Clear,
}

/// 命令插件
trait CommandPlugin {
    fn input_process(input: &str) -> CommandResult
    where
        Self: Sized;
    fn output_process(output: &str) -> CommandResult
    where
        Self: Sized;
}

struct CommandLoader {
    command_plugins: Mutex<Vec<Box<dyn CommandPlugin>>>,
}

impl CommandLoader {
    async fn new(runner: Runner, task_manager: TaskManager) -> Result<Self> {
        warn!("During construction.");
        let command_plugins = Mutex::new(vec![]);

        task_manager
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        _ = t.cancelled() => break
                    }
                }
                Ok(())
            })
            .await?;

        Ok(CommandLoader { command_plugins })
    }
}
