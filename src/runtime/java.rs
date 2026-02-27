use crate::util::downloader::Downloader;
use crate::GLOBAL_RUNTIME;
use anyhow::{Error, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;
use tracing::info;

pub static GLOBAL_JAVA: LazyLock<GeneralJavaRuntimeManager> =
    LazyLock::new(|| GeneralJavaRuntimeManager::new());

pub struct GeneralJavaRuntimeManager {
    list: Mutex<Vec<JavaRuntime>>,
}

struct JavaRuntime {
    java_home: PathBuf,
    distribution: JavaType,
    version: usize,
    installing: Arc<Mutex<()>>,
}

#[derive(Clone, PartialEq)]
pub enum JavaType {
    OracleJDK,
    OpenJDK,
    GraalVM,
}

impl GeneralJavaRuntimeManager {
    fn new() -> Self {
        Self {
            list: Mutex::new(vec![JavaRuntime {
                java_home: Default::default(),
                distribution: JavaType::GraalVM,
                version: 21,
                installing: Arc::new(Default::default()),
            }]),
        }
    }
    pub async fn check(&self, version: usize) -> Vec<(PathBuf, JavaType)> {
        self.list
            .lock()
            .await
            .iter()
            .filter(|&x| x.version == version)
            .filter(|&x| !x.installing.try_lock().is_ok())
            .map(|x| (x.java_home.clone(), x.distribution.clone()))
            .collect()
    }
    pub async fn install(&self, version: usize) -> Result<PathBuf> {
        // GraalVM Test
        let install_lock = Arc::new(Mutex::new(()));
        let path = GLOBAL_RUNTIME.join(format!(
            "graalvm-jdk-{}-{}-{}",
            version,
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        let mut guard = self.list.lock().await;
        match guard
            .iter()
            .find(|&x| x.distribution == JavaType::GraalVM && x.version == version)
        {
            None => {
                guard.push(JavaRuntime {
                    java_home: path.clone(),
                    distribution: JavaType::GraalVM,
                    version,
                    installing: install_lock.clone(),
                });
            }
            Some(v) => {
                let _ = v.installing.lock().await;
                return Ok(v.java_home.clone());
            }
        };
        drop(guard);
        get_graal(version).await?;
        Ok(path)
    }
}

/// 拉平一层目录
async fn flatten_single_child(dir: &Path) -> std::io::Result<()> {
    let mut rd = tokio::fs::read_dir(dir).await?;
    let mut entries = Vec::new();
    while let Some(entry) = rd.next_entry().await? {
        entries.push(entry);
    }
    // 如果不是只有一个目录，直接返回
    if entries.len() != 1 {
        return Ok(());
    }
    let child_path = entries[0].path();
    if !child_path.is_dir() {
        return Ok(());
    }
    // 遍历子目录文件
    let mut rd = tokio::fs::read_dir(&child_path).await?;
    while let Some(entry) = rd.next_entry().await? {
        let path = entry.path();
        if let Some(name) = path.file_name() {
            tokio::fs::rename(&path, dir.join(name)).await?;
        }
    }
    // 删除空目录（如果里面还有东西会失败）
    tokio::fs::remove_dir(child_path).await?;
    Ok(())
}

/// 获取 GraalVM
async fn get_graal(version: usize) -> Result<()> {
    info!("Start downloading GraalVM JDK {version}");
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let url = format!(
        "https://download.oracle.com/graalvm/{version}/archive/graalvm-jdk-{version}_{}-{}_bin.{extension}",
        std::env::consts::OS,
        std::env::consts::ARCH.replace("86_", ""),
        version = version,
        extension = extension
    );
    let hash = Downloader::new()
        .await
        .get(format!("{url}.sha256"))
        .await?
        .text()
        .await?;
    let file = Downloader::new()
        .await
        .download_with_sha256(url, hash)
        .await?;
    info!("Download complete. Start unzipping.");
    let (file, path) = tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&file)?)?;
        let path = GLOBAL_RUNTIME.join(format!(
            "graalvm-jdk-{}-{}-{}",
            version,
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        let pb = ProgressBar::new(zip.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{bar:40}] {pos}/{len} ({eta}) {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            pb.set_message(format!("Unzipping {}", file.name()));
            let out_path = path.join(file.name());
            if file.is_dir() {
                std::fs::create_dir_all(&out_path)?;
                continue;
            }
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            let mut buffer = Vec::new();
            let _ = file.read_to_end(&mut buffer);
            out_file.write_all(&buffer)?;
            pb.inc(1)
        }
        pb.finish_with_message("done");
        Ok::<(PathBuf, PathBuf), Error>((file, path))
    })
    .await??;
    flatten_single_child(&path).await?;
    info!("Clearing download cache");
    tokio::fs::remove_file(file).await?;
    Ok(())
}
