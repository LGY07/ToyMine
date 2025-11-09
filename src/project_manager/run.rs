use crate::project_manager::config::{JavaMode, JavaType};
use crate::project_manager::tools::backup::{backup_check_repo, backup_init_repo, backup_new_snap};
use crate::project_manager::tools::{
    ServerType, VersionInfo, analyze_jar, check_java, get_mime_type, install_bds, install_je,
    prepare_java,
};
use crate::project_manager::{BACKUP_DIR, Config, LOG_DIR, WORK_DIR};
use anyhow::Error;
use chrono::{Local, Utc};
use cron_tab::AsyncCron;
use futures::future::join_all;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::{env, fs};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, stdin, stdout};
use tokio::process::Command;
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio::task::JoinHandle;
use tokio::{signal, spawn};
use tracing::{debug, error, info, warn};

/// 启动服务器
pub fn start_server(config: Config) -> Result<(), Error> {
    match pre_run(&config) {
        Ok(_) => (),
        Err(e) => {
            error!("{}", e);
            return Err(e);
        }
    }

    let config = Arc::new(config);
    let stop_flag = Arc::new(Notify::new());

    // 以下开始异步🤯
    let rt = Runtime::new()?;
    rt.block_on(async move {
        let mut handles: Vec<JoinHandle<Result<(), Error>>> = vec![];

        // 启动 backup 线程
        if config.backup.enable {
            handles.push(spawn(backup_thread(Arc::clone(&config), stop_flag.clone())));
        }

        // 启动服务器线程
        handles.push(spawn(server_thread_with_terminal(
            Arc::clone(&config),
            stop_flag.clone(),
        )));

        // 停止信号
        tokio::select! {
            _ = stop_flag.notified() => {},
            _ = signal::ctrl_c()=> {
                info!("Ctrl+C received, setting stop flag...");
                stop_flag.notify_waiters();
            },
        }

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

    // 停机
    rt.shutdown_background();
    info!("Server exited successfully.");

    Ok(())
}

/// 服务器线程(同步到终端)
async fn server_thread_with_terminal(config: Arc<Config>, stop: Arc<Notify>) -> Result<(), Error> {
    // channel：外层发送给 server_thread 的 stdin
    let (tx_in, rx_in) = mpsc::channel::<String>(100);
    // channel：server_thread 输出 stdout/stderr
    let (tx_out, mut rx_out) = mpsc::channel::<String>(100);

    let stop_clone = stop.clone();
    let config_clone = config.clone();
    // spawn server_thread
    let server_handle = spawn(async move {
        server_thread(rx_in, tx_out, stop_clone, config_clone)
            .await
            .unwrap_or_else(|e| error!("Server thread error: {}", e));
    });

    // spawn 打印线程（输出到终端）
    let stop_clone = stop.clone();
    let print_handle = spawn(async move {
        let mut out = stdout();
        while let Some(line) = rx_out.recv().await {
            tokio::select! {
                _ = stop_clone.notified() => break,
                _ = async {
                    let _ = out.write_all(line.as_bytes()).await;
                    let _ = out.flush().await;
                } => {}
            }
        }
    });

    // spawn 读取终端输入线程（写入 server_thread stdin）
    let tx_in_clone = tx_in.clone();
    let stop_clone = stop.clone();
    let input_handle = spawn(async move {
        let mut buf = [0u8; 1024];
        let mut stdin = stdin();
        loop {
            tokio::select! {
                _ = stop_clone.notified() => break,
                result = stdin.read(&mut buf) => {
                    match result {
                        Ok(n) if n > 0 => {
                            let line = String::from_utf8_lossy(&buf[..n]).to_string();
                            let _ = tx_in_clone.send(line).await;
                        }
                        _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
                    }
                }
            }
        }
    });

    // 等待 server_task 完成
    let _ = server_handle.await;
    let _ = print_handle.await;
    let _ = input_handle.await;

    Ok(())
}

/// 服务端线程，仅同步到 mpsc 通道
pub async fn server_thread(
    mut rx: mpsc::Receiver<String>, // 接收外部消息 -> 写入子进程 stdin
    tx: mpsc::Sender<String>,       // 发送子进程 stdout/stderr 给外部
    stop: Arc<Notify>,
    config: Arc<Config>,
) -> Result<(), Error> {
    // 启动子进程
    let mut child = if let ServerType::BDS = config.as_ref().project.server_type {
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

    let child_stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));

    // 日志文件
    let stdout_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!(
                "{}/stdout-{}.log",
                LOG_DIR,
                Utc::now().format("%Y-%m-%d_%H-%M-%S")
            ))
            .await?,
    ));

    let stderr_file = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!(
                "{}/stderr-{}.log",
                LOG_DIR,
                Utc::now().format("%Y-%m-%d_%H-%M-%S")
            ))
            .await?,
    ));

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // stdout -> tx + log
    let tx_stdout = tx.clone();
    let stdout_file_clone = stdout_file.clone();
    let stop_clone = stop.clone();
    let stdout_handle = spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            tokio::select! {
                _ = stop_clone.notified() => break,
                _ = async {
                    let _ = tx_stdout.send(line.clone()).await;
                    let mut f = stdout_file_clone.lock().await;
                    let _ = f.write_all(line.as_bytes()).await;
                    line.clear();
                } => {}
            }
        }
        Ok::<(), Error>(())
    });

    // stderr -> tx + log
    let tx_stderr = tx.clone();
    let stderr_file_clone = stderr_file.clone();
    let stop_clone = stop.clone();
    let stderr_handle = spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            tokio::select! {
                _ = stop_clone.notified() => break,
                _ = async {
                    let _ = tx_stderr.send(line.clone()).await;
                    let mut f = stderr_file_clone.lock().await;
                    let _ = f.write_all(line.as_bytes()).await;
                    line.clear();
                } => {}
            }
        }
        Ok::<(), Error>(())
    });

    // rx -> stdin
    let child_stdin_clone = child_stdin.clone();
    let stop_clone = stop.clone();
    let stdin_handle = spawn(async move {
        while let Some(msg) = rx.recv().await {
            tokio::select! {
                _ = stop_clone.notified() => break,
                _ = async {
                    let mut stdin = child_stdin_clone.lock().await;
                    let _ = stdin.write_all(msg.as_bytes()).await;
                    let _ = stdin.write_all(b"\n").await;
                } => {}
            }
        }
        Ok::<(), Error>(())
    });

    // 等待子进程结束或停止信号
    tokio::select! {
        _ = stop.notified() => {
            let mut stdin = child_stdin.lock().await;
            let _ = stdin.write_all(b"stop\n").await;
            let _ = stdin.flush().await;
            info!("Stopping server...");
            match tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await {
                Ok(Ok(_)) => info!("Server exited gracefully."),
                Ok(Err(e)) => error!("Error waiting for server exit: {}", e),
                Err(_) => {
                    warn!("Server did not exit in 10s, killing...");
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
            }
        }
        status = child.wait() => {
            let status = status?;
            stop.notify_waiters();
            info!("Server exited: {:?}", status.code());
        }
    }

    // 等待所有线程完成
    stdout_handle.abort();
    stderr_handle.abort();
    stdin_handle.abort();
    drop(tx);

    Ok(())
}

/// 备份线程
pub async fn backup_thread(config: Arc<Config>, stop: Arc<Notify>) -> Result<(), Error> {
    let mut backup_handles = vec![];
    info!("Backup task enabled");
    // 初始化仓库
    let init_handle_world: JoinHandle<Result<_, Error>> = spawn(async {
        if backup_check_repo(format!("{}/world", BACKUP_DIR).as_str()).is_err() {
            backup_init_repo(format!("{}/world", BACKUP_DIR).as_str())?;
        }
        Ok(())
    });
    let init_handle_other: JoinHandle<Result<_, Error>> = spawn(async {
        if backup_check_repo(format!("{}/other", BACKUP_DIR).as_str()).is_err() {
            backup_init_repo(format!("{}/other", BACKUP_DIR).as_str())?;
        }
        Ok(())
    });
    init_handle_world.await??;
    init_handle_other.await??;
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
            let stop = stop.clone();
            let config = Arc::clone(&config);
            backup_handles.push(spawn(async move {
                // 配置 Cron 备份
                let mut cron = AsyncCron::new(Local);
                cron.add_fn(config.backup.time.as_ref().unwrap().cron.trim(), {
                    let config = Arc::clone(&config); // clone 一份给闭包
                    move || {
                        let config = Arc::clone(&config); // async move 闭包内部再 clone
                        async move {
                            let _ =
                                run_backup("Corn", config.backup.world, config.backup.other).await;
                        }
                    }
                })
                .await?;
                // 开始 Cron 备份
                cron.start().await;
                debug!("[Cron Backup] Wait for stop signal");
                // 等待 Stop
                stop.notified().await;
                // 停止 Cron 备份
                cron.stop().await;
                Ok(())
            }));
        }
        if config.backup.time.as_ref().unwrap().interval != 0 {
            info!("Interval backup enabled");
            // 开始间隔备份
            let stop = stop.clone();
            let config = Arc::clone(&config);
            backup_handles.push(spawn(async move {
                loop {
                    tokio::select! {
                        _ = stop.notified() => {
                            // 等待 Stop
                            info!("Stop signal received. Exiting interval backup loop.");
                            break Ok(());
                        }
                        result = run_backup("Interval", config.backup.world, config.backup.other) => {
                            if let Err(e) = result {
                                error!("Backup failed: {:?}", e);
                            }
                            tokio::select! {
                                _ = stop.notified() => {
                                    // 等待 Stop
                                    info!("Stop signal received. Exiting interval backup loop.");
                                    break Ok(());
                                    }
                                _ = tokio::time::sleep(std::time::Duration::from_secs(config.backup.time.as_ref().unwrap().interval as u64)) => {}
                            }
                        }
                    }
                }
            }));
        }
    }
    debug!("[Backup] Wait for stop signal");
    // 等待 Stop
    stop.notified().await;
    // 停止时备份
    if config.backup.event.is_some() && config.backup.event.as_ref().unwrap().stop {
        info!("Backup is enabled at stop");
        run_backup("Stop", config.backup.world, config.backup.other).await?;
    }
    info!("Backup task stopping...");
    for i in backup_handles {
        i.await??
    }
    debug!("Backup task stopped.");
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
            backup_new_snap(
                format!("{}/world", BACKUP_DIR).as_str(),
                tag.as_ref(),
                vec!["world".parse()?],
            )?;
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
                    if file_name != WORK_DIR && file_name != "world" {
                        path_list.push(path);
                    }
                }
            }
            // 运行备份
            backup_new_snap(
                format!("{}/other", BACKUP_DIR).as_str(),
                tag.as_ref(),
                path_list,
            )?;
            Ok::<(), Error>(())
        }))
    }
    join_all(handles).await;
    Ok(())
}

/// 运行前准备工作
pub fn pre_run(config: &Config) -> Result<(), Error> {
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
                Path::new(&format!("{:?}.bak", config.project.execute)),
            )?
        }
        // 安装服务端
        debug!("Install the Bedrock Edition server");
        install_bds()?;
        return Ok(());
    }
    // 准备 Java 版
    debug!("Prepare the Java Edition server");
    let jar_version = analyze_jar(Path::new(&config.project.execute)); //仅判断服务端是否可用，不主动更改版本
    if jar_version.is_err() {
        // 备份有问题的文件/目录
        if Path::new(&config.project.execute).exists() {
            debug!("The file exists but has problems. Make a backup.");
            fs::rename(
                Path::new(&config.project.execute),
                Path::new(&format!("{:?}.bak", config.project.execute)),
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
