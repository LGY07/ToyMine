use crate::GLOBAL_RUNTIME;
use crate::util::downloader::Downloader;
use anyhow::{Error, Result};
use std::path::PathBuf;
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
            list: Default::default(),
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
    info!("Download complete. Start decompression.");
    let file = tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&file)?)?;
        let path = GLOBAL_RUNTIME.join(format!(
            "graalvm-jdk-{}-{}-{}",
            version,
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let outpath = path.join(file.name());
            if file.is_dir() {
                std::fs::create_dir_all(&outpath)?;
                continue;
            }
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            let mut buffer = Vec::new();
            let _ = file.read_to_end(&mut buffer);
            outfile.write_all(&buffer)?;
        }
        Ok::<PathBuf, Error>(file)
    })
    .await??;
    info!("Clearing download cache");
    tokio::fs::remove_file(file).await?;
    Ok(())
}
