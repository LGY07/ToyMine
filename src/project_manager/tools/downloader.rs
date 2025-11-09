use crate::project_manager::MAX_RETRIES;
use anyhow::Error;
use futures::future::join_all;
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};
use reqwest::{Client, blocking};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::task;
use tracing::{debug, info};

#[derive(Debug)]
pub struct FileDownloadResult {
    pub url: String,
    pub path: PathBuf,
    pub sha256: String,
    pub sha1: String,
}

/// 多线程下载文件
pub fn download_files(
    urls: Vec<String>,
    dir: &str,
    threads: usize,
) -> Vec<Result<FileDownloadResult, Error>> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(download_files_async(urls, dir, threads))
}

/// 单线程下载文件
pub fn download_file_single_thread(url: &str, dir: &str) -> Result<FileDownloadResult, Error> {
    fs::create_dir_all(dir)?;
    let filename = url.split('/').last().unwrap_or("file");
    let filepath = Path::new(dir).join(filename);

    let client = blocking::Client::builder().build()?;
    let mut resp = client.get(url).send()?.error_for_status()?;

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&filepath)?;

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{msg} [{spinner}] {bytes}")?);
    pb.set_message(filename.to_string());

    let mut sha256 = Sha256::new();
    let mut sha1 = Sha1::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        sha256.update(&buf[..n]);
        sha1.update(&buf[..n]);
        pb.inc(n as u64);
    }

    pb.finish_with_message(format!("{} done", filename));

    Ok(FileDownloadResult {
        url: url.to_string(),
        path: filepath,
        sha256: hex::encode(sha256.finalize()),
        sha1: hex::encode(sha1.finalize()),
    })
}

async fn download_files_async(
    urls: Vec<String>,
    dir: &str,
    threads: usize,
) -> Vec<Result<FileDownloadResult, Error>> {
    debug!("Download the files");
    fs::create_dir_all(dir).ok();

    let start_time = Instant::now();
    let total_count = urls.len();
    let completed_files = Arc::new(Mutex::new(0usize));

    let mut handles = vec![];
    for url in urls.clone() {
        let dir = dir.to_string();
        let completed_files = completed_files.clone();

        let handle = tokio::spawn(async move {
            let res = download_single_with_retry(&url, &dir, threads).await;
            let mut completed = completed_files.lock().unwrap();
            *completed += 1;
            info!("({}/{}) {}", *completed, total_count, url);
            res
        });
        handles.push(handle);
    }

    let results = join_all(handles).await;

    info!(
        "All downloads completed (elapsed: {})",
        HumanDuration(start_time.elapsed())
    );

    results
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| Err(Error::msg(format!("Task failed: {}", e)))))
        .collect()
}

async fn download_single_with_retry(
    url: &str,
    dir: &str,
    threads: usize,
) -> Result<FileDownloadResult, Error> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match download_single(url, dir, threads).await {
            Ok(res) => return Ok(res),
            Err(e) => {
                if attempts >= MAX_RETRIES {
                    return Err(Error::msg(format!(
                        "Failed after {} retries: {}",
                        attempts, e
                    )));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

async fn download_single(
    url: &str,
    dir: &str,
    threads: usize,
) -> Result<FileDownloadResult, Error> {
    let client = Client::builder().use_rustls_tls().build()?;
    let resp = client.head(url).send().await?;
    let total_size = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .ok_or(Error::msg("Invalid Content-Length"))?
        .to_str()?
        .parse::<u64>()?;

    let filename = url.split('/').last().unwrap_or("file");
    let filepath = Path::new(dir).join(filename);
    if !filepath.exists() {
        File::create(&filepath)?;
    }

    let chunk_size = (total_size + threads as u64 - 1) / threads as u64;
    let file_path = Arc::new(filepath.clone());

    // 文件级进度条
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%)",
        )?
        .progress_chars("=> "),
    );
    pb.set_message(filename.to_string());

    let mut handles = vec![];
    for i in 0..threads {
        let start = i as u64 * chunk_size;
        if start >= total_size {
            break;
        }
        let end = (start + chunk_size - 1).min(total_size - 1);

        let client = client.clone();
        let url = url.to_string();
        let file_path = file_path.clone();
        let pb = pb.clone();

        let handle = task::spawn(async move {
            let mut attempt = 0;
            loop {
                attempt += 1;
                let range_header = format!("bytes={}-{}", start, end);
                let result: Result<(), Error> = async {
                    let mut resp = client
                        .get(&url)
                        .header(reqwest::header::RANGE, range_header)
                        .send()
                        .await?;

                    let mut f = OpenOptions::new().write(true).open(&*file_path)?;
                    f.seek(SeekFrom::Start(start))?;

                    while let Some(chunk) = resp.chunk().await? {
                        f.write_all(&chunk)?;
                        pb.inc(chunk.len() as u64);
                    }
                    Ok(())
                }
                .await;

                match result {
                    Ok(_) => break,
                    Err(e) if attempt < MAX_RETRIES => {
                        info!("{}", e);
                        tokio::time::sleep(Duration::from_millis(500)).await
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok::<(), Error>(())
        });
        handles.push(handle);
    }

    for r in join_all(handles).await {
        match r {
            Ok(inner) => inner?,
            Err(e) => return Err(Error::msg(format!("Join error: {}", e))),
        }
    }

    pb.finish_with_message(format!("{} done", filename));

    // 计算哈希
    let mut file = File::open(&filepath)?;
    let mut sha256 = Sha256::new();
    let mut sha1 = Sha1::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha256.update(&buf[..n]);
        sha1.update(&buf[..n]);
    }

    Ok(FileDownloadResult {
        url: url.to_string(),
        path: filepath,
        sha256: hex::encode(sha256.finalize()),
        sha1: hex::encode(sha1.finalize()),
    })
}
