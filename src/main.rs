#![feature(async_fn_traits)]
#![feature(unboxed_closures)]

mod command;
mod core;
mod plugin;
mod runtime;
mod versions;

use crate::command::CommandLoader;
use crate::core::backup::BackupManager;
use crate::core::config::project::McServerConfig;
use crate::core::mc_server::runner::{Runner, sync_channel_stdio};
use crate::core::task::TaskManager;
use crate::versions::vanilla::Vanilla;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::select;
use tokio::signal::ctrl_c;
use tracing::error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// 创建任务管理器
pub static TASK_MANAGER: LazyLock<TaskManager> = LazyLock::new(|| TaskManager::new());
// 创建备份管理器
pub static BACKUP_MANAGER: LazyLock<BackupManager> = LazyLock::new(|| BackupManager::new());

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server with the current configuration
    Start {
        /// Generate startup scripts
        #[arg(short, long)]
        generate: bool,
        /// Run by the daemon process, only the default configuration path is supported
        #[arg(short, long)]
        detach: bool,
        /// Connect to the game running in the daemon, only the default configuration path is supported
        #[arg(short, long)]
        attach: bool,
    },
    /// Print the project information of the current location
    Info,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // console layer
    let console_layer = console_subscriber::ConsoleLayer::builder().spawn();
    // fmt layer
    let fmt_layer = tracing_subscriber::fmt::Layer::default();
    // env filter
    let filter_layer = tracing_subscriber::EnvFilter::new("trace");

    // 初始化日志输出
    tracing_subscriber::registry()
        .with(console_layer)
        .with(fmt_layer)
        .with(filter_layer)
        .init();

    // 参数解析4
    let cli = Cli::parse();
    // 尝试从当前目录获取配置文件
    let cfg = if PathBuf::from("PacMine.toml").is_file() {
        match McServerConfig::open("PacMine.toml".as_ref()).await {
            Ok(v) => Some(v),
            Err(e) => {
                error!("Invalid config file: {}", e);
                None
            }
        }
    } else {
        None
    };

    if let Commands::Start {
        generate,
        detach,
        attach,
    } = &cli.command
    {
        if *generate {
            return Ok(());
        }

        let server = Vanilla::default();
        let mut command_loader = CommandLoader::new();

        let server = Arc::new(Runner::spawn_server(&server).await?);
        command_loader.register(server.id, vec![Box::new(command::raw::ExamplePlugin)])?;
        let server_clone = Arc::clone(&server);
        TASK_MANAGER
            .spawn_with_cancel(async move |t| {
                sync_channel_stdio(
                    server_clone.input.clone(),
                    command_loader.load(server_clone.clone().as_ref()).await?,
                    t,
                )
                .await?;
                Ok(())
            })
            .await?;

        select! {
            e = server.wait() => {
                println!("Exit: {}",e?)
            }
            _ = ctrl_c() => {
                server.kill_with_timeout(Duration::from_secs(10)).await?;
                println!("Stop: {}",server.wait().await?)
            }
        }

        TASK_MANAGER.shutdown().await;
    }
    Ok(())
}
