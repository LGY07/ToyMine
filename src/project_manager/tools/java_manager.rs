use crate::project_manager::config::JavaType;
use crate::project_manager::tools::download_files;
use crate::project_manager::tools::downloader::{DownloadError, FileDownloadResult};
use rustic_core::repofile::NodeType::File;
use std::fmt::format;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const CACHE_DIR: &str = ".nmsl/cache/download";
const DEFAULT_DOWNLOAD_THREAD: usize = 5;

/// 自动管理 Java 的情况下，自动下载 Java
pub fn prepare_java(edition: JavaType, version: usize) -> Result<(), String> {
    // 确定安装位置
    let runtime_path = PathBuf::from_str(
        format!(
            "java-{}-graalvm-{}-{}",
            version,
            std::env::consts::OS,
            std::env::consts::ARCH
        )
        .as_str(),
    )
    .unwrap();

    // 下载 OpenJDK 并安装至 .nmsl/runtime/
    if let JavaType::OracleJDK = edition
        && !check_java(&runtime_path)
    {
        todo!()
    }
    // 下载 GraalVM 并安装至 .nmsl/runtime/
    if let JavaType::GraalVM = edition
        && !check_java(&runtime_path)
    {
        prepare_graalvm(version, &runtime_path)?
    }
    // 下载 Oracle JDK 并安装至 .nmsl/runtime/
    if let JavaType::OpenJDK = edition
        && !check_java(&runtime_path)
    {
        todo!()
    }

    // 自定义模式不应该调用次函数
    if let JavaType::Custom = edition {
        unreachable!()
    }
    todo!()
}

/// 检查一个 JAVA_HOME 是否可用
fn check_java(java_home: &Path) -> bool {
    todo!()
}

/// 安装 GraalVM
fn prepare_graalvm(version: usize, runtime_path: &Path) -> Result<(), String> {
    // 确定文件扩展名
    let extension = if std::env::consts::OS == "windows" {
        "zip"
    } else {
        "tar.gz"
    };
    // 获得下载链接
    let links = [format!(
        "https://download.oracle.com/graalvm/{}/archive/graalvm-jdk-{}_{}-{}_bin.{}",
        version,
        version,
        std::env::consts::OS,
        std::env::consts::ARCH.replace("86_", ""),
        extension
    )];
    // 下载文件
    let files = match download_files(links.to_vec(), CACHE_DIR, DEFAULT_DOWNLOAD_THREAD)[0] {
        Ok(v) => v,
        Err(e) => return Err(format!("{e:?}")),
    };
    // 校验文件
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("{}.sha256", &links[0]))
        .send()
        .map_err(|e| format!("{e:?}"))?;
    if !response.status().is_success() {
        return Err(format!("Request failed: {}", response.status()));
    }
    if files.sha256 != response.text().unwrap_or(String::new()) {
        return Err("Verification failed. sha256 information does not match.".to_string());
    }
    // 安装到 runtime
    // 创建目录
    let _ = fs::create_dir(runtime_path);
    let archive = std::fs::File::open(files.path).unwrap();
    if extension == "zip" {
        // 解压 zip
        todo!()
    } else if extension == "tar.gz" {
        // 解压 tar.gz
        todo!()
    } else {
        return Err("Unsupported formats".to_string());
    }
}
