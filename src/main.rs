mod daemon;
mod project_manager;

use crate::project_manager::run::detach_server;
use crate::project_manager::{
    CACHE_DIR, create_project, get_info, pre_run, print_info, start_server,
};
use clap::{Parser, Subcommand};
use colored::Colorize;
use home::home_dir;
use std::fs;
use std::path::PathBuf;
use tracing::{error, info};

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
        /// Run by the daemon process, only the default configuration path is supported
        #[arg(short, long)]
        detach: bool,
        /// Connect to the game running in the daemon, only the default configuration path is supported
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
        /// Print a default configuration file
        #[arg(short, long)]
        generate: bool,
        /// Automatically configure as a systemd service
        #[arg(short, long)]
        install_systemd: bool,
    },
}

fn main() {
    // 启用日志输出
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .compact()
        .pretty()
        .init();

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
                detach_server();
                return;
            }
            error!("This feature is not supported on this platform");
            return;
        }
        // 连接到守护进程，仅支持 Unix 平台
        if *attach {
            #[cfg(target_family = "unix")]
            {
                todo!();
                return;
            }
            error!("This feature is not supported on this platform");
            return;
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
        generate,
        install_systemd,
    } = &cli.command
    {
        if *generate {
            println!("{}", daemon::Config::default().to_pretty());
            return;
        }
        if *install_systemd {
            // systemd 仅在 Linux 可用
            #[cfg(not(target_os = "linux"))]
            {
                error!("Systemd is only available on Linux");
                return;
            }
            let config = daemon::Config::default();
            let work_dir = &config.storage.work_dir;
            let service = daemon::config::get_systemd_service();
            let service_dir = home_dir()
                .expect("Could not get home directory.")
                .join(".config")
                .join("systemd")
                .join("user");

            // 安装 PacMine
            if !work_dir.join("bin").is_dir() {
                fs::create_dir_all(&work_dir.join("bin"))
                    .expect("Could not create the work directory!");
            }
            fs::copy(
                std::env::current_exe().unwrap(),
                work_dir.join("bin").join("pacmine"),
            )
            .expect("Could not copy the binary to the work directory!");
            fs::write(work_dir.join("config.toml"), config.to_pretty())
                .expect("Could not write the config!");

            // 创建 systemd service unit
            if !service_dir.is_dir() {
                fs::create_dir_all(&service_dir).expect("Could not create the systemd directory!");
            }
            fs::write(service_dir.join("PacMine.service"), service)
                .expect("Could not write the systemd config!");

            // 启用 systemd 服务
            let output = std::process::Command::new("systemctl")
                .arg("--user")
                .arg("daemon-reload")
                .output()
                .expect("Failed to execute systemctl");
            if !output.status.success() {
                eprintln!("{:?}", String::from_utf8_lossy(&output.stderr));
                error!("Failed to reload the systemd service");
            }
            let output2 = std::process::Command::new("systemctl")
                .arg("--user")
                .arg("enable")
                .arg("--now")
                .arg("PacMine.service")
                .output()
                .expect("Failed to execute systemctl");
            if output2.status.success() {
                info!("The systemd service was successfully enabled");
                println!(
                    "The service starts only when the user logs in. If you need to start along with the system, please run \"{}\"",
                    "sudo loginctl enable-linger `whoami`".yellow()
                );
            } else {
                eprintln!("{:?}", String::from_utf8_lossy(&output2.stderr));
                error!("Failed to enable the systemd service");
            }

            return;
        }

        if let Some(config) = config {
            match daemon::server(
                daemon::Config::from_file(config)
                    .expect("The configuration file could not be opened"),
            ) {
                Ok(_) => info!("Server successfully exited"),
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
                Ok(_) => info!("Server successfully exited:"),
                Err(e) => error!("The program exited with errors: {:?}", e),
            }
        }
    }

    // 清理缓存
    let _ = fs::remove_dir_all(CACHE_DIR);
    let _ = fs::create_dir(CACHE_DIR);
}
