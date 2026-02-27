#![feature(async_fn_traits)]
#![feature(unboxed_closures)]

#[cfg(feature = "telemetry")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

mod command;
mod core;
mod plugin;
mod runtime;
mod util;
mod versions;

use crate::core::arguments;
use crate::core::backup::BackupManager;
use crate::core::task::TaskManager;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::LazyLock;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};

// 创建任务管理器
pub static TASK_MANAGER: LazyLock<TaskManager> = LazyLock::new(|| TaskManager::new());
// 创建备份管理器
pub static BACKUP_MANAGER: LazyLock<BackupManager> = LazyLock::new(|| BackupManager::new());

// 全局缓存目录
pub static GLOBAL_CACHE: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = std::env::home_dir().unwrap().join(".toymine").join("cache");
    std::fs::create_dir_all(&path).expect("Failed to creat cache directory");
    path
});
// 共享运行时目录
pub static GLOBAL_RUNTIME: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = std::env::home_dir()
        .unwrap()
        .join(".toymine")
        .join("runtime");
    std::fs::create_dir_all(&path).expect("Failed to creat runtime directory");
    path
});

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
async fn main() -> Result<()> {
    // 初始化日志
    let fmt_layer = tracing_subscriber::fmt::Layer::default()
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

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

    match cli.command {
        Commands::Start {
            generate,
            detach,
            attach,
        } => arguments::start::start(generate, detach, attach).await?,
        Commands::Info => arguments::info::info().await?,
    }
    Ok(())
}
