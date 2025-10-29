use crate::project_manager::tools::{
    DEFAULT_DOWNLOAD_THREAD, DOWNLOAD_CACHE_DIR, ServerType, VersionInfo, VersionManifest,
    download_files,
};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 安装 Bedrock Edition 服务端
pub fn install_bds() -> Result<(), Error> {
    todo!()
}

/// 安装 Java Edition 服务端
pub fn install_je(version: VersionInfo) -> Result<(), Error> {
    // 下载版本清单
    let manifest = VersionManifest::fetch()?;
    // 获得下载链接
    let (url, sha1) = manifest.search(version.name)?.to_download()?;
    // 下载文件
    let files = download_files(vec![url], DOWNLOAD_CACHE_DIR, DEFAULT_DOWNLOAD_THREAD);
    // 校验文件
    let file = files
        .first()
        .ok_or(anyhow::Error::msg("No files downloaded"))?
        .as_ref()
        .map_err(|e| anyhow::Error::msg(format!("{:?}", e)))?;
    if file.sha1 != sha1 {
        return Err(anyhow::Error::msg("SHA1 verification failed"));
    }
    // 清理存在的文件
    if Path::new("server.jar").exists() {
        fs::rename("server.jar", "server.jar.bak")?
    }
    // 安装文件
    fs::rename(file.path.clone(), "server.jar")?;
    Ok(())
}
