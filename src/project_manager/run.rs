use crate::project_manager::Config;
use crate::project_manager::config::{JavaMode, JavaType};
use crate::project_manager::tools::{
    ServerType, VersionInfo, analyze_jar, check_java, get_mime_type, install_bds, install_je,
    prepare_java,
};
use anyhow::Error;
use chrono::Utc;
use futures::future::join_all;
use log::{debug, error, info, warn};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, fs};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::{signal, spawn};

pub fn start_server(config: Config) -> Result<(), Error> {
    pre_run(&config)?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    let rt = Runtime::new()?;

    rt.block_on(async move {
        let mut handles: Vec<JoinHandle<Result<(), Error>>> = vec![];

        // backup 线程
        if config.backup.enable {
            let stop = stop_flag.clone();
            handles.push(spawn(async move {
                info!("Backup task enabled");
                while !stop.load(Ordering::SeqCst) {
                    debug!("Backup task running...");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                info!("Backup task stopping...");
                Ok(())
            }));
        }

        // 服务器线程
        let stop = stop_flag.clone();
        handles.push(spawn(async move {
            // 创建 channel
            let (tx, mut rx) = mpsc::channel::<String>(100);

            let mut child = if let ServerType::BDS = config.project.server_type {
                // BDS
                info!("Server starting...");
                Command::new(&config.project.execute)
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
            let stop_clone = stop_flag_clone.clone();
            let stdin_handle = spawn(async move {
                let mut stdin_reader = BufReader::new(tokio::io::stdin());
                let mut buffer = String::new();
                while !stop_clone.load(Ordering::SeqCst) {
                    if stdin_reader.read_line(&mut buffer).await? > 0 {
                        let mut stdin = child_stdin_clone.lock().await;
                        stdin.write_all(buffer.as_bytes()).await?;
                        buffer.clear();
                    }
                }
                Ok::<(), Error>(())
            });

            // 打印线程
            let print_handle = spawn(async move {
                while let Some(line) = rx.recv().await {
                    print!("{}", line);
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
                Ok(Ok(_)) => info!("Server exited gracefully."),
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
        }));

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
