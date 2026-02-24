#![allow(incomplete_features)]
#![feature(async_fn_traits)]
#![feature(unboxed_closures)]
#![feature(specialization)]

#[cfg(feature = "telemetry")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

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
use crate::versions::VersionManager;
use anyhow::anyhow;
use clap::{Parser, Subcommand};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::select;
use tokio::signal::ctrl_c;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};

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

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    let fmt_layer = tracing_subscriber::fmt::Layer::default()
        .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG);

    #[cfg(not(feature = "telemetry"))]
    let subscriber_builder = Registry::default().with(fmt_layer);

    #[cfg(feature = "telemetry")]
    let (subscriber_builder, _g1, _g2) = {
        // console layer
        let console_layer = console_subscriber::ConsoleLayer::builder()
            .with_default_env()
            .spawn();

        // chrome layer
        let (chrome_layer, chrome_guard) = tracing_chrome::ChromeLayerBuilder::new().build();

        // flame layer
        let (flame_layer, flame_guard) =
            tracing_flame::FlameLayer::with_file("flamegraph.folded").unwrap();

        (
            Registry::default()
                .with(fmt_layer)
                .with(console_layer)
                .with(chrome_layer)
                .with(flame_layer),
            chrome_guard,
            flame_guard,
        )
    };

    subscriber_builder.init();

    #[cfg(feature = "telemetry")]
    dhat::Profiler::new_heap();

    // 参数解析
    let cli = Cli::parse();

    if let Commands::Start {
        generate,
        detach,
        attach,
    } = &cli.command
    {
        if *generate {
            return Ok(());
        }

        // 尝试从当前目录获取配置文件
        let cfg = McServerConfig::current().await;
        // 尝试从当前目录发现服务端
        let server = match cfg {
            None => {
                info!(
                    "The configuration file was not found. Attempting to locate the server file."
                );
                VersionManager::detect()?
            }
            Some(c) => VersionManager::from_version(&c.project.version),
        };
        let server = match server {
            None => return Err(anyhow!("MC Server Not Found")),
            Some(v) => v,
        };
        let server = Arc::new(Runner::spawn_server(server.as_ref()).await?);

        let mut command_loader = CommandLoader::new();
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
                info!("Exit: {}",e?)
            }
            _ = ctrl_c() => {
                server.kill_with_timeout(Duration::from_secs(10)).await?;
                info!("Stop: {}",server.wait().await?)
            }
        }

        TASK_MANAGER.shutdown().await;
    }
    Ok(())
}
