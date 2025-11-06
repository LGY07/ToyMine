mod daemon;
mod project_manager;

use crate::project_manager::{
    CACHE_DIR, create_project, get_info, pre_run, print_info, start_server,
};
use clap::{Parser, Subcommand};
use colored::Colorize;
use home::home_dir;
use log::{LevelFilter, error, info};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server with the current configuration
    Run {
        /// Generate startup scripts
        #[arg(short, long)]
        generate: bool,
        /// Run by the daemon process
        #[arg(short, long)]
        detach: bool,
        /// Connect to the game running in the daemon
        #[arg(short, long)]
        attach: bool,
    },
    /// Print the project information of the current location
    Info,
    /// Create a project in a new directory
    New {
        /// The path of the new directory
        path: PathBuf,
    },
    /// Create a project at the current location
    Init,
    /// Install the necessary files to make the project run properly
    Install,
    /// Update the plugins
    Update {
        /// Automatically confirm for update
        #[arg(short, long)]
        yes: bool,
    },
    /// Upgrade the server core
    Upgrade,
    /// Run the daemon process
    Daemon {
        /// Specify the location of the configuration file
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Automatically configure as a systemd service
        #[arg(long)]
        install_systemd: bool,
        /// Automatically configure as a OpenRC service
        #[arg(long)]
        install_openrc: bool,
    },
}

fn main() {
    // 启用日志输出
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    // 解析参数
    let cli = Cli::parse();

    // info 子命令，查看当前项目的信息
    if let Commands::Info = &cli.command {
        print_info()
    }

    // 按当前配置启动游戏
    if let Commands::Run {
        generate,
        detach,
        attach,
    } = &cli.command
    {
        // 生成启动脚本
        if *generate {
            todo!();
        }
        // 推送到守护进程，仅支持 Unix 平台
        if *detach {
            #[cfg(target_family = "unix")]
            {
                todo!();
                return;
            }
            error!("This feature is not supported on this platform");
        }
        // 连接到守护进程，仅支持 Unix 平台
        if *attach {
            #[cfg(target_family = "unix")]
            {
                todo!();
                return;
            }
            error!("This feature is not supported on this platform");
        }
        // 正常启动游戏
        match get_info() {
            Ok(v) => start_server(v).expect("The program exited with errors!"),
            Err(e) => error!("The configuration cannot be opened: {:?}", e),
        };
    }

    // new 子命令，根据传入的地址创建目录并初始化项目
    if let Commands::New { path } = &cli.command {
        // 创建目录
        fs::create_dir(path).unwrap_or_else(|e| {
            error!("{}", e);
            panic!("{}", "Failed to create the directory!".red());
        });
        // 进入目录
        std::env::set_current_dir(path)
            .unwrap_or_else(|_| panic!("{}", "The directory cannot be opened!".red()));
        // 初始化项目
        create_project()
    }

    // init 子命令，初始化当前目录
    if let Commands::Init = &cli.command {
        // 初始化项目
        create_project()
    }

    // install 子命令，执行运行前准备工作
    if let Commands::Install = &cli.command {
        // 读取配置并运行
        match get_info() {
            Ok(v) => pre_run(&v).expect("The program exited with errors!"),
            Err(e) => error!("The configuration cannot be opened: {:?}", e),
        };
    }

    // daemon 子命令
    if let Commands::Daemon {
        config,
        install_systemd,
        install_openrc,
    } = &cli.command
    {
        if *install_openrc {
            todo!();
            return;
        }
        if *install_openrc {
            todo!();
            return;
        }
        if let Some(config) = config {
            match daemon::server(
                daemon::Config::from_file(config)
                    .expect("The configuration file could not be opened"),
            ) {
                Ok(v) => info!("Server successfully exited"),
                Err(e) => error!("The program exited with errors: {:?}", e),
            }
        } else {
            match daemon::server(
                daemon::Config::from_file(format!(
                    "{}/.pacmine/config.toml",
                    home_dir()
                        .expect("The configuration file could not be opened")
                        .display()
                ))
                .expect("The configuration file could not be opened"),
            ) {
                Ok(v) => info!("Server successfully exited:"),
                Err(e) => error!("The program exited with errors: {:?}", e),
            }
        }
    }

    // 清理缓存
    fs::remove_dir_all(CACHE_DIR).expect("Cache cleanup failed!");
    fs::create_dir(CACHE_DIR).expect("Cache create failed!");
}
