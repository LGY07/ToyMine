use crate::project_manager::config::JavaType;
use crate::project_manager::tools::{DEFAULT_DOWNLOAD_THREAD, DOWNLOAD_CACHE_DIR, download_files};
use flate2::read::GzDecoder;
use log::debug;
use std::error::Error;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::Archive;
use zip::ZipArchive;

/// 自动管理 Java 的情况下，自动下载 Java
pub fn prepare_java(edition: JavaType, version: usize) -> Result<(), Box<dyn Error>> {
    debug!("Prepare Java");
    let runtime_path = PathBuf::from(format!(
        ".nmsl/runtime/java-{}-{}-{}-{}",
        version,
        edition,
        std::env::consts::OS,
        std::env::consts::ARCH
    ));

    if check_java(&runtime_path) {
        return Ok(()); // 已安装可用
    }

    match edition {
        JavaType::GraalVM => prepare_graalvm(version, &runtime_path)?,
        JavaType::OpenJDK => prepare_openjdk(version, &runtime_path)?,
        JavaType::Custom => unreachable!("Custom Java should not call prepare_java"),
    }

    Ok(())
}

/// 检查 JAVA_HOME 是否可用，通过尝试运行 `java -version`
pub fn check_java(java_home: &Path) -> bool {
    debug!("Check Java");
    let java_bin = if cfg!(windows) {
        java_home.join("bin").join("java.exe")
    } else {
        java_home.join("bin").join("java")
    };

    if !java_bin.exists() {
        return false;
    }

    // 尝试运行 `java -version`，忽略输出，只关心是否能执行成功
    Command::new(java_bin)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// 下载并安装 GraalVM
fn prepare_graalvm(version: usize, runtime_path: &Path) -> Result<(), Box<dyn Error>> {
    debug!("Prepare GraalVM");
    // 拼接 URL
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let url = format!(
        "https://download.oracle.com/graalvm/{version}/archive/graalvm-jdk-{version}_{}-{}_bin.{extension}",
        std::env::consts::OS,
        std::env::consts::ARCH.replace("86_", ""),
        version = version,
        extension = extension
    );
    // 下载文件
    let files_vec = download_files(
        vec![url.clone()],
        DOWNLOAD_CACHE_DIR,
        DEFAULT_DOWNLOAD_THREAD,
    );
    let files = files_vec
        .first()
        .ok_or("No files downloaded")?
        .as_ref()
        .map_err(|e| format!("{:?}", e))?;

    // 校验文件
    debug!("Verify the SHA256 value");
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("{}.sha256", url))
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Request failed: {}", resp.status()).into());
    }
    let remote_sha = resp.text().unwrap_or_default().trim().to_string();
    if files.sha256 != remote_sha {
        return Err(Box::from("SHA256 verification failed"));
    }
    // 解压文件
    fs::create_dir_all(runtime_path).map_err(|e| e.to_string())?;
    if extension == "zip" {
        unzip_file(&files.path, runtime_path)?;
    } else {
        untar_gz_file(&files.path, runtime_path)?;
    }
    flatten_runtime_dir(runtime_path)?;
    // 检查安装是否成功
    if check_java(runtime_path) {
        Ok(())
    } else {
        Err(Box::from("Failed to install Runtime"))
    }
}

/// 下载并安装 OpenJDK
fn prepare_openjdk(version: usize, runtime_path: &Path) -> Result<(), Box<dyn Error>> {
    debug!("Prepare OpenJDK");
    // 拼接 URL，此处使用 Microsoft 构建的 OpenJDK
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let url = format!(
        "https://aka.ms/download-jdk/microsoft-jdk-{}-{}-{}.{}",
        version,
        std::env::consts::OS,
        std::env::consts::ARCH.replace("86_", ""),
        extension
    );
    // 下载文件
    let files_vec = download_files(
        vec![url.clone()],
        DOWNLOAD_CACHE_DIR,
        DEFAULT_DOWNLOAD_THREAD,
    );
    let files = files_vec
        .first()
        .ok_or("No files downloaded")?
        .as_ref()
        .map_err(|e| format!("{:?}", e))?;
    // 校验文件
    debug!("Verify the SHA256 value");
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("{}.sha256sum.txt", url))
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Request failed: {}", resp.status()).into());
    }
    let remote_sha = resp.text().unwrap_or_default().trim().to_string();
    if files.sha256 != remote_sha.split(' ').collect::<Vec<&str>>()[0].trim() {
        return Err(Box::from("SHA256 verification failed"));
    }
    // 解压文件
    fs::create_dir_all(runtime_path).map_err(|e| e.to_string())?;
    if extension == "zip" {
        unzip_file(&files.path, runtime_path)?;
    } else {
        untar_gz_file(&files.path, runtime_path)?;
    }
    flatten_runtime_dir(runtime_path)?;
    // 检查安装是否成功
    if check_java(runtime_path) {
        Ok(())
    } else {
        Err(Box::from("Failed to install Runtime"))
    }
}

/// 解决解压文件夹内还有文件夹的问题
fn flatten_runtime_dir(dest_dir: &Path) -> Result<(), Box<dyn Error>> {
    let entries: Vec<_> = fs::read_dir(dest_dir)?.collect::<Result<_, _>>()?;
    if entries.len() == 1 {
        let inner = entries[0].path();
        if inner.is_dir() {
            for entry in fs::read_dir(&inner)? {
                let entry = entry?;
                fs::rename(entry.path(), dest_dir.join(entry.file_name()))?;
            }
            fs::remove_dir(inner)?;
        }
    }
    Ok(())
}

/// 解压 zip 文件
fn unzip_file(zip_path: &Path, dest_dir: &Path) -> Result<(), Box<dyn Error>> {
    debug!("Unzip the ZIP file");
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = dest_dir.join(file.mangled_name());

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 解压 tar.gz 文件
fn untar_gz_file(tar_gz_path: &Path, dest_dir: &Path) -> Result<(), Box<dyn Error>> {
    debug!("Unzip the tar.gz file");
    let tar_gz = fs::File::open(tar_gz_path).map_err(|e| e.to_string())?;
    let tar = GzDecoder::new(BufReader::new(tar_gz));
    let mut archive = Archive::new(tar);
    archive.unpack(dest_dir).map_err(Box::from)
}
