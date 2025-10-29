use crate::Commands;
use crate::project_manager::Config;
use crate::project_manager::config::{JavaMode, JavaType};
use crate::project_manager::tools::{
    ServerType, VersionInfo, analyze_jar, check_java, get_mime_type, install_bds, install_je,
    prepare_java,
};
use anyhow::Error;
use log::debug;
use std::path::Path;
use std::process::Command;
use std::thread::Thread;
use std::{fs, thread};
use toml::Value::String;

pub fn start_server(config: Config) -> Result<(), Error> {
    pre_run(&config)?;
    // 启用备份线程
    let backup_handle = thread::spawn(|| {});
    // 启动服务器
    if let ServerType::BDS = config.project.server_type {
        // 构建启动参数
        let command = Command::new(config.project.execute);
    } else {
        // 构建启动参数
        let command = Command::new(config.runtime.java.to_binary()?);
    }
    todo!()
}

/// 运行前准备工作
fn pre_run(config: &Config) -> Result<(), Error> {
    // 准备基岩版
    if let ServerType::BDS = config.project.server_type {
        debug!("Prepare the Bedrock Edition server");
        // 检查文件是否存在
        let mime_type = get_mime_type(Path::new(&config.project.execute));
        if mime_type == "application/x-executable" && std::env::consts::OS == "linux" {
            return Ok(());
        }
        if mime_type == "application/vnd.microsoft.portable-executable"
            && std::env::consts::OS == "windows"
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
                Err(anyhow::Error::msg("The custom Java cannot be used!"))
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
