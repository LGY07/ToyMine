use reqwest::Client;
use sha2::{Sha256, Digest};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use tokio::task;
use futures::future::join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle, HumanDuration};

#[derive(Debug)]
pub enum DownloadError {
    Request(reqwest::Error),
    Io(std::io::Error),
    InvalidResponse,
    Other(String),
}

impl From<reqwest::Error> for DownloadError {
    fn from(err: reqwest::Error) -> Self { DownloadError::Request(err) }
}
impl From<std::io::Error> for DownloadError {
    fn from(err: std::io::Error) -> Self { DownloadError::Io(err) }
}

#[derive(Debug)]
pub struct FileDownloadResult {
    pub url: String,
    pub path: PathBuf,
    pub sha256: String,
}

pub fn download_files(urls: Vec<String>, dir: &str, threads: usize) -> Vec<Result<FileDownloadResult, DownloadError>> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async { download_files_async(urls, dir, threads).await })
}

async fn download_files_async(
    urls: Vec<String>,
    dir: &str,
    threads: usize,
) -> Vec<Result<FileDownloadResult, DownloadError>> {
    fs::create_dir_all(dir).ok();
    let mp = Arc::new(MultiProgress::new());

    let mut total_bytes = 0u64;
    let client = Client::builder()
        .use_rustls_tls()
        .build()
        .unwrap();

    for url in &urls {
        if let Ok(resp) = client.head(url).send().await {
            if let Some(len) = resp.headers().get(reqwest::header::CONTENT_LENGTH) {
                if let Ok(len_str) = len.to_str() {
                    if let Ok(size) = len_str.parse::<u64>() {
                        total_bytes += size;
                    }
                }
            }
        }
    }

    let total_progress = mp.add(ProgressBar::new(total_bytes));
    total_progress.set_style(
        ProgressStyle::with_template(
            "{msg} [{bar:40.cyan/blue}] {human_pos}/{human_len} ({percent}%) {bytes_per_sec} ETA: {eta_precise}"
        )
            .unwrap(),
    );
    total_progress.set_message("Total Progress");
    total_progress.enable_steady_tick(Duration::from_millis(200));

    let start_time = Instant::now();
    let total_count = urls.len();
    let completed_files = Arc::new(Mutex::new(0usize));

    let mut handles = vec![];
    for url in urls.clone() {
        let dir = dir.to_string();
        let mp = mp.clone();
        let total_progress = total_progress.clone();
        let completed_files = completed_files.clone();

        let handle = tokio::spawn(async move {
            let res = download_single(&url, &dir, threads, mp, total_progress.clone()).await;
            let mut completed = completed_files.lock().unwrap();
            *completed += 1;
            total_progress.set_message(format!("Total Progress ({}/{})", *completed, total_count));
            res
        });
        handles.push(handle);
    }

    let results = join_all(handles).await;
    total_progress.finish_with_message(format!(
        "All downloads completed (elapsed: {})",
        HumanDuration(start_time.elapsed())
    ));

    results
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| Err(DownloadError::Other(e.to_string()))))
        .collect()
}

async fn download_single(
    url: &str,
    dir: &str,
    threads: usize,
    mp: Arc<MultiProgress>,
    total_progress: ProgressBar,
) -> Result<FileDownloadResult, DownloadError> {
    let client = Client::builder()
        .use_rustls_tls()
        .build()
        .map_err(DownloadError::Request)?;

    let resp = client.head(url).send().await?;
    let total_size = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .ok_or(DownloadError::InvalidResponse)?
        .to_str()
        .map_err(|_| DownloadError::InvalidResponse)?
        .parse::<u64>()
        .map_err(|_| DownloadError::InvalidResponse)?;

    let filename = url.split('/').last().unwrap_or("file");
    let filepath = Path::new(dir).join(filename);

    if !filepath.exists() {
        File::create(&filepath)?;
    }

    let chunk_size = (total_size + threads as u64 - 1) / threads as u64;
    let file_path = Arc::new(filepath.clone());

    let pb = mp.add(ProgressBar::new(total_size));
    pb.set_style(
        ProgressStyle::with_template(
            "{msg} [{bar:40.cyan/blue}] {human_pos}/{human_len} ({percent}%) {bytes_per_sec} ETA: {eta_precise}"
        )
            .unwrap(),
    );
    pb.set_message(filename.to_string());
    pb.enable_steady_tick(Duration::from_millis(200));

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
        let total_progress = total_progress.clone();

        let handle = task::spawn(async move {
            let range_header = format!("bytes={}-{}", start, end);
            let mut resp = client
                .get(&url)
                .header(reqwest::header::RANGE, range_header)
                .send()
                .await
                .map_err(DownloadError::Request)?;

            let mut f = OpenOptions::new()
                .write(true)
                .open(&*file_path)
                .map_err(DownloadError::Io)?;
            f.seek(SeekFrom::Start(start)).map_err(DownloadError::Io)?;

            while let Some(chunk) = resp.chunk().await.map_err(DownloadError::Request)? {
                f.write_all(&chunk).map_err(DownloadError::Io)?;
                let len = chunk.len() as u64;
                pb.inc(len);
                total_progress.inc(len);
            }
            Ok::<(), DownloadError>(())
        });
        handles.push(handle);
    }

    for r in join_all(handles).await {
        match r {
            Ok(inner) => inner?,
            Err(e) => return Err(DownloadError::Other(e.to_string())),
        }
    }

    pb.finish_with_message(format!("{} done", filename));

    let mut file = File::open(&filepath)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(FileDownloadResult {
        url: url.to_string(),
        path: filepath,
        sha256: hex::encode(hasher.finalize()),
    })
}
