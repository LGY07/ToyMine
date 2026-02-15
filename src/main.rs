#![feature(async_fn_traits)]
#![feature(unboxed_closures)]

mod command;
mod core;
mod plugin;
mod runtime;
mod versions;

use crate::core::config::project::McServerConfig;
use crate::core::mc_server::runner::{Runner, sync_channel_stdio};
use crate::core::task::TaskManager;
use crate::versions::vanilla::Vanilla;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::error;

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
    // 初始化日志输出
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();
    // 参数解析
    let cli = Cli::parse();
    // 创建任务管理器
    let task_manager = Arc::new(TaskManager::new());
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
        let mut command_loader = command::CommandLoader::new();

        let server = Arc::new(Runner::spawn_server(&server, &task_manager).await?);
        command_loader.register(&server, vec![Box::new(command::raw::ExamplePlugin)])?;
        let server_clone = Arc::clone(&server);
        let task_manager_clone = Arc::clone(&task_manager);
        task_manager
            .spawn_with_cancel(async move |t| {
                sync_channel_stdio(
                    server_clone.input.clone(),
                    command_loader
                        .load(server_clone.clone().as_ref(), task_manager_clone.as_ref())
                        .await?,
                    t,
                )
                .await?;
                Ok(())
            })
            .await?;

        server.wait().await?;
        task_manager.shutdown().await;
    }

    std::process::exit(0)
}
