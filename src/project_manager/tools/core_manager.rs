use crate::project_manager::config::ServerType;
use crate::project_manager::tools::version_parser::PaperProject;
use crate::project_manager::tools::{VersionInfo, VersionManifest, download_files};
use crate::project_manager::{
    CACHE_DIR, DEFAULT_DOWNLOAD_THREAD, FOLIA_PROJECT_API, LEAVES_PROJECT_API, PAPER_PROJECT_API,
    PURPUR_PROJECT_API,
};
use anyhow::Error;
use log::{error, info, warn};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// 安装 Bedrock Edition 服务端
pub fn install_bds() -> Result<(), Error> {
    todo!()
}

/// 安装 Java Edition 服务端
pub fn install_je(version_info: VersionInfo) -> Result<(), Error> {
    match version_info.server_type {
        ServerType::Vanilla => vanilla(version_info.name)?,
        ServerType::Paper => paper_like(PAPER_PROJECT_API, version_info.name)?,
        ServerType::Folia => paper_like(FOLIA_PROJECT_API, version_info.name)?,
        ServerType::Purpur => paper_like(PURPUR_PROJECT_API, version_info.name)?,
        ServerType::Leaves => paper_like(LEAVES_PROJECT_API, version_info.name)?,
        ServerType::Other => info!("No server is installed."),
        _ => unreachable!("不存在的服务端类型"),
    };
    Ok(())
}

/// 安装 Vanilla
fn vanilla(version: String) -> Result<(), Error> {
    // 下载版本清单
    let manifest = VersionManifest::fetch()?;
    // 获得下载链接
    let (url, sha1) = manifest.search(version)?.to_download()?;
    // 下载文件
    let files = download_files(
        vec![url],
        format!("{}/download", CACHE_DIR).as_str(),
        DEFAULT_DOWNLOAD_THREAD,
    );
    // 校验文件
    let file = files
        .first()
        .ok_or(Error::msg("No files downloaded"))?
        .as_ref()
        .map_err(|e| Error::msg(format!("{:?}", e)))?;
    if file.sha1 != sha1 {
        return Err(Error::msg("SHA1 verification failed"));
    }
    // 清理存在的文件
    if Path::new("server.jar").exists() {
        fs::rename("server.jar", "server.jar.bak")?
    }
    // 安装文件
    fs::rename(file.path.clone(), "server.jar")?;
    Ok(())
}

/// 用于解析 Paper API Version 的 JSON
#[derive(Debug, Deserialize)]
struct PaperVersion {
    builds: Vec<usize>,
}

#[derive(Debug, Deserialize)]
struct ApplicationDownload {
    name: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct Downloads {
    application: ApplicationDownload,
}

/// 用于解析 Paper API Builds 的 JSON
#[derive(Debug, Deserialize)]
struct PaperBuild {
    downloads: Downloads,
}

/// 安装 Paper 类服务端
fn paper_like(project_api: &str, version: String) -> Result<(), Error> {
    let version_list = PaperProject::fetch(project_api)?;
    // 查找版本
    if version_list
        .versions
        .iter()
        .any(|available_version| &version == available_version)
    {
        // 创建 Client
        let client = reqwest::blocking::Client::new();
        // Versions 列表
        let response = client
            .get(format!("{}/versions/{}", project_api, version))
            .send()?;
        if !response.status().is_success() {
            return Err(Error::msg("Request failed"));
        }
        let builds = response.json::<PaperVersion>()?;
        // Builds 列表
        let response = client
            .get(format!(
                "{}/versions/{}/builds/{}",
                project_api,
                version,
                builds.builds.last().expect("No build is available")
            ))
            .send()?;
        if !response.status().is_success() {
            return Err(Error::msg("Request failed"));
        }
        let download_info = response.json::<PaperBuild>()?.downloads.application;
        // 下载文件
        let files = download_files(
            vec![format!(
                "{}/versions/{}/builds/{}/downloads/{}",
                project_api,
                version,
                builds.builds.last().expect("No build is available"),
                download_info.name
            )],
            format!("{}/download", CACHE_DIR).as_str(),
            DEFAULT_DOWNLOAD_THREAD,
        );
        // 校验文件
        let file = files
            .first()
            .ok_or(Error::msg("No files downloaded"))?
            .as_ref()
            .map_err(|e| Error::msg(format!("{:?}", e)))?;
        if file.sha256 != download_info.sha256 {
            return Err(Error::msg("SHA256 verification failed"));
        }
        // 清理存在的文件
        if Path::new("server.jar").exists() {
            fs::rename("server.jar", "server.jar.bak")?
        }
        // 安装文件
        fs::rename(file.path.clone(), "server.jar")?;
        Ok(())
    } else {
        // 不存在版本时输出支持的版本
        error!("Your server type does not support this version");
        println!("Supported versions:");
        println!("===================");
        for i in version_list.versions {
            println!("{}", i)
        }
        println!("===================");
        warn!("Please change the version to a supported version and try again");
        Err(Error::msg("Paper version does not exist."))
    }
}
