use crate::project_manager::Config;
use crate::project_manager::config::{JavaMode, JavaType};
use crate::project_manager::tools::backup::{backup_check_repo, backup_init_repo, backup_new_snap};
use crate::project_manager::tools::{
    ServerType, VersionInfo, analyze_jar, check_java, get_mime_type, install_bds, install_je,
    prepare_java,
};
use anyhow::Error;
use chrono::{FixedOffset, Local, TimeZone, Utc};
use cron_tab::AsyncCron;
use futures::future::join_all;
use log::{debug, error, info, warn};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::{signal, spawn};

const BACKUP_WORLD_DIR: &str = ".nmsl/backup/world/";
const BACKUP_OTHER_DIR: &str = ".nmsl/backup/other/";

/// 启动服务器
pub fn start_server(config: Config) -> Result<(), Error> {
    pre_run(&config)?;

    let config = Arc::new(config);
    let stop_flag = Arc::new(AtomicBool::new(false));

    // 以下开始异步🤯
    let rt = Runtime::new()?;
    rt.block_on(async move {
        let mut handles: Vec<JoinHandle<Result<(), Error>>> = vec![];

        // 启动 backup 线程
        if config.backup.enable {
            handles.push(spawn(backup_thread(Arc::clone(&config), stop_flag.clone())));
        }

        // 启动服务器线程
        handles.push(spawn(server_thread(Arc::clone(&config), stop_flag.clone())));

        // Ctrl+C 信号
        signal::ctrl_c().await?;
        info!("Ctrl+C received, setting stop flag...");
        stop_flag.store(true, Ordering::SeqCst);

        // 等待所有任务完成
        let results = join_all(handles).await;
        for r in results {
            match r {
                Ok(Ok(())) => (),
                Ok(Err(e)) => error!("Task error: {}", e),
                Err(e) => error!("Join error: {}", e),
            }
        }

        Ok::<(), Error>(())
    })?;

    Ok(())
}

/// 服务器线程
async fn server_thread(config: Arc<Config>, stop: Arc<AtomicBool>) -> Result<(), Error> {
    let config = Arc::clone(&config);
    // 创建 channel
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let mut child = if let ServerType::BDS = config.as_ref().project.server_type {
        // BDS
        info!("Server starting...");
        Command::new(&config.as_ref().project.execute)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    } else {
        // Java
        let mut mem_options = Vec::new();
        if config.runtime.java.xms != 0 {
            mem_options.push(format!("-Xms{}M", config.runtime.java.xms));
        }
        if config.runtime.java.xmx != 0 {
            mem_options.push(format!("-Xmx{}M", config.runtime.java.xmx));
        }
        info!("Server starting...");
        Command::new(config.runtime.java.to_binary()?)
            .args(&config.runtime.java.arguments)
            .args(mem_options)
            .arg("-jar")
            .arg(&config.project.execute)
            .arg("-nogui")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    };

    // stdin/log 文件包装
    let child_stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));

    let stdout_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!(
                ".nmsl/log/stdout-{}.log",
                Utc::now().format("%Y-%m-%d_%H-%M-%S")
            ))
            .await?,
    ));

    let stderr_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!(
                ".nmsl/log/stderr-{}.log",
                Utc::now().format("%Y-%m-%d_%H-%M-%S")
            ))
            .await?,
    ));

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // stdout -> tx + stdout.log
    let tx_stdout = tx.clone();
    let stdout_file_clone = stdout_file.clone();
    let stdout_handle = spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            let _ = tx_stdout.send(line.clone()).await;
            let mut f = stdout_file_clone.lock().await;
            f.write_all(line.as_bytes()).await?;
            line.clear();
        }
        Ok::<(), Error>(())
    });

    // stderr -> tx + stderr.log
    let tx_stderr = tx.clone();
    let stderr_file_clone = stderr_file.clone();
    let stderr_handle = spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            let _ = tx_stderr.send(line.clone()).await;
            let mut f = stderr_file_clone.lock().await;
            f.write_all(line.as_bytes()).await?;
            line.clear();
        }
        Ok::<(), Error>(())
    });

    // stdin -> 子进程 stdin
    let child_stdin_clone = child_stdin.clone();
    let stop_clone = stop.clone();
    let stdin_handle = spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 1024];

        while !stop_clone.load(Ordering::SeqCst) {
            match stdin.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let mut child_stdin = child_stdin_clone.lock().await;
                    let _ = child_stdin.write_all(&buf[..n]).await;
                }
                _ => {
                    // 没有输入就稍微休眠，避免忙循环
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
        Ok::<(), Error>(())
    });

    // 打印线程
    let print_handle = spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            let _ = out.write_all(line.as_bytes()).await;
            let _ = out.flush().await;
        }
    });

    // 等待 stop
    while !stop.load(Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    info!("Stopping server...");

    // 先发送 stop
    {
        let mut stdin = child_stdin.lock().await;
        let _ = stdin.write_all(b"stop\n").await;
        let _ = stdin.flush().await;
    }

    // 等待服务器退出或超时 kill
    match tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await {
        Ok(Ok(_)) => info!("Server exited gracefully. Press Enter to exit"),
        Ok(Err(e)) => error!("Error waiting for server exit: {}", e),
        Err(_) => {
            warn!("Server did not exit in 10s, killing...");
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    // 等待所有线程完成
    let _ = stdout_handle.await?;
    let _ = stderr_handle.await?;
    let _ = stdin_handle.await?;
    drop(tx);
    let _ = print_handle.await;

    Ok(())
}

/// 备份线程
async fn backup_thread(config: Arc<Config>, stop: Arc<AtomicBool>) -> Result<(), Error> {
    let mut backup_handles = vec![];
    info!("Backup task enabled");
    // 初始化仓库
    if backup_check_repo(BACKUP_WORLD_DIR).is_err() {
        backup_init_repo(BACKUP_WORLD_DIR)?
    }
    if backup_check_repo(BACKUP_OTHER_DIR).is_err() {
        backup_init_repo(BACKUP_OTHER_DIR)?
    }
    // 启动时备份
    if config.backup.event.is_some() && config.backup.event.as_ref().unwrap().start {
        info!("Backup is enabled at start");
        backup_handles.push(spawn(run_backup(
            "Start",
            config.backup.world,
            config.backup.other,
        )))
    }
    // 时间备份
    if config.backup.time.is_some() {
        if !config.backup.time.as_ref().unwrap().cron.is_empty() {
            info!("Cron backup enabled");
            // 配置 Cron 备份
            let local_tz = Local::from_offset(&FixedOffset::east_opt(7).unwrap());
            let mut cron = AsyncCron::new(local_tz);
            let config = Arc::clone(&config); // 原始 Arc 不动
            cron.add_fn(config.backup.time.as_ref().unwrap().cron.trim(), {
                let config = Arc::clone(&config); // clone 一份给闭包
                move || {
                    let config = Arc::clone(&config); // async move 闭包内部再 clone
                    async move {
                        let _ = run_backup("Corn", config.backup.world, config.backup.other).await;
                    }
                }
            })
            .await?;
            // 开始 Cron 备份
            cron.start().await;
            while !stop.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            // 停止 Cron 备份
            cron.stop().await
        }
        if config.backup.time.as_ref().unwrap().interval != 0 {
            info!("Interval backup enabled");
            // 开始间隔备份
            let stop = stop.clone();
            let config = Arc::clone(&config);
            let time_backup_handle: JoinHandle<Result<(), Error>> = spawn(async move {
                loop {
                    run_backup("Interval", config.backup.world, config.backup.other).await?;
                    tokio::time::sleep(std::time::Duration::from_secs(
                        config.backup.time.as_ref().unwrap().interval as u64,
                    ))
                    .await
                }
            });
            while !stop.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            // 停止时间备份
            time_backup_handle.abort()
        }
    }
    while !stop.load(Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    // 停止时备份
    if config.backup.event.is_some() && config.backup.event.as_ref().unwrap().stop {
        info!("Backup is enabled at stop");
        run_backup("Stop", config.backup.world, config.backup.other).await?;
    }
    info!("Backup task stopping...");
    join_all(backup_handles).await;
    Ok(())
}

/// 运行备份
async fn run_backup(tag: &str, world: bool, other: bool) -> Result<(), Error> {
    debug!("{} backup job executed at: {}", tag, Local::now());
    let mut handles = vec![];
    let tag_arc = Arc::new(tag.to_string());
    if world {
        let tag = Arc::clone(&tag_arc);
        handles.push(spawn(async move {
            // 运行备份
            backup_new_snap(BACKUP_WORLD_DIR, tag.as_ref(), vec!["world".parse()?])?;
            Ok::<(), Error>(())
        }))
    }
    if other {
        let tag = Arc::clone(&tag_arc);
        handles.push(spawn(async move {
            // 构建路径列表
            let mut dir_list = tokio::fs::read_dir(env::current_dir()?).await?;
            let mut path_list = Vec::new();
            while let Some(entry) = dir_list.next_entry().await? {
                let path = entry.path();
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    // 排除目录
                    if file_name != ".nmsl" && file_name != "world" {
                        path_list.push(path);
                    }
                }
            }
            // 运行备份
            backup_new_snap(BACKUP_OTHER_DIR, tag.as_ref(), path_list)?;
            Ok::<(), Error>(())
        }))
    }
    join_all(handles).await;
    Ok(())
}

/// 运行前准备工作
fn pre_run(config: &Config) -> Result<(), Error> {
    // 准备基岩版
    if let ServerType::BDS = config.project.server_type {
        debug!("Prepare the Bedrock Edition server");
        // 检查文件是否存在
        let mime_type = get_mime_type(Path::new(&config.project.execute));
        if mime_type == "application/x-executable" && env::consts::OS == "linux" {
            return Ok(());
        }
        if mime_type == "application/vnd.microsoft.portable-executable"
            && env::consts::OS == "windows"
        {
            return Ok(());
        }
        // 备份有问题的文件/目录
        if Path::new(&config.project.execute).exists() {
            debug!("The file exists but has problems. Make a backup.");
            fs::rename(
                Path::new(&config.project.execute),
                Path::new(&format!("{}.bak", config.project.execute)),
            )?
        }
        // 安装服务端
        debug!("Install the Bedrock Edition server");
        install_bds()?;
        return Ok(());
    }
    // 准备 Java 版
    debug!("Prepare the Java Edition server");
    let jar_version = analyze_jar(Path::new(&config.project.execute));
    if jar_version.is_err() {
        // 备份有问题的文件/目录
        if Path::new(&config.project.execute).exists() {
            debug!("The file exists but has problems. Make a backup.");
            fs::rename(
                Path::new(&config.project.execute),
                Path::new(&format!("{}.bak", config.project.execute)),
            )?
        }
        // 安装 Java 版服务端
        debug!("Install the Java Edition server");
        install_je(VersionInfo::get_version_info(
            &config.project.version,
            config.project.server_type.clone(),
        )?)?;
    }
    // 准备 Java 运行环境
    debug!("Prepare the Java Runtime");
    // 自动模式
    if let JavaMode::Auto = config.runtime.java.mode {
        // 分析 Jar 文件需要的 Java 版本
        let jar_version = analyze_jar(Path::new(&config.project.execute))?;
        // 准备 Java
        prepare_java(JavaType::OpenJDK, jar_version.java_version as usize)?;
    }
    // 手动模式
    if let JavaMode::Manual = config.runtime.java.mode {
        if let JavaType::Custom = config.runtime.java.edition {
            // 自定义模式
            return if check_java(Path::new(&config.runtime.java.custom)) {
                Ok(())
            } else {
                Err(Error::msg("The custom Java cannot be used!"))
            };
        } else {
            // 准备 Java
            prepare_java(
                config.runtime.java.edition.clone(),
                config.runtime.java.version,
            )?;
        }
    }
    // 准备完成
    debug!("All the work before operation is ready");
    Ok(())
}
