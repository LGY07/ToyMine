use crate::GLOBAL_CACHE;
use crate::util::hash::Sha256Digest;
use anyhow::Result;
use anyhow::anyhow;
use futures::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use nyquest::r#async::Response;
use nyquest::{AsyncClient, Request};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, OnceCell};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, error};

pub struct Downloader {
    client: AsyncClient,
}

/// 多线程下载分片大小
const BLOCK_SIZE: u64 = 1024 * 1024;
/// 单个文件最大连接数
const CONCURRENCY: usize = 8;
/// 最大重试次数
const MAX_RETRY: usize = 3;

static GLOBAL_DOWNLOADER: OnceCell<Downloader> = OnceCell::const_new();

impl Downloader {
    /// 获取下载器
    pub async fn new() -> &'static Downloader {
        GLOBAL_DOWNLOADER
            .get_or_init(|| async {
                nyquest_preset::register();

                let client = nyquest::ClientBuilder::default()
                    .user_agent("curl/7.68.0 nyquest/0")
                    .build_async()
                    .await
                    .expect("Failed to build client");

                Self { client }
            })
            .await
    }
    /// GET 请求
    pub async fn get(&self, uri: impl Into<Cow<'static, str>>) -> nyquest::Result<Response> {
        self.client.request(Request::get(uri)).await
    }
    /// 下载文件，自动启用多线程
    pub async fn download(&self, uri: impl Into<Cow<'static, str>>) -> Result<PathBuf> {
        let inner = Box::pin(async move {
            let uri = Cow::clone(&uri.into());
            // 获取文件信息
            let head = self.client.request(Request::head(uri.clone())).await?;
            let file_name = GLOBAL_CACHE.join(uuid::Uuid::new_v4().to_string());
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&file_name)
                .await?;
            let support_range = matches!(
                head.get_header("accept-ranges")?
                    .first()
                    .map(|t| t.as_str()),
                Some("bytes")
            );
            // 判断是否多线程下载
            if let Some(total_size) = head.content_length()
                && support_range
            {
                debug!("Multithreaded downloading");
                // 设置进度条
                let pb = ProgressBar::new(total_size);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg} [{bar:40}] {binary_bytes}/{binary_total_bytes} {binary_bytes_per_sec} ({eta})")
                        .unwrap()
                        .progress_chars("=>-"),
                );
                pb.set_message("Downloading...");
                // 计算分片
                let split_ranges = (0..total_size).step_by(BLOCK_SIZE as usize).map(|start| {
                    let end = (start + BLOCK_SIZE - 1).min(total_size - 1);
                    (start, end)
                });
                file.set_len(total_size).await?;
                let file = Arc::new(Mutex::new(file));
                let pb = Arc::new(pb);
                // 并发下载
                stream::iter(split_ranges)
                    .for_each_concurrent(CONCURRENCY, |(start, end)| {
                        let file = file.clone();
                        let uri = uri.clone();
                        let pb = pb.clone();
                        async move {
                            for _ in 0..MAX_RETRY {
                                if let Err(e) =
                                    download_chunk(&self.client, uri.clone(), &file, start, end)
                                        .await
                                {
                                    error!("chunk error: {e:?}");
                                } else {
                                    break;
                                }
                            }
                            pb.inc(end - start)
                        }
                    })
                    .await;
                file.lock().await.flush().await?;
                pb.finish_with_message("done");
            } else {
                debug!("Single-threaded downloading");
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner} {msg}")
                        .unwrap(),
                );
                pb.set_message("Downloading...");
                pb.enable_steady_tick(Duration::from_millis(100));
                for _ in 0..MAX_RETRY {
                    let mut stream = self
                        .client
                        .request(Request::get(uri.clone()))
                        .await?
                        .into_async_read()
                        .compat();
                    if let Err(e) = tokio::io::copy(&mut stream, &mut file).await {
                        error!("downloading error: {e:?}")
                    } else {
                        break;
                    }
                }
                pb.finish_with_message("done");
                file.flush().await?;
            }

            Ok(file_name)
        });
        inner.await
    }
    /// 下载并校验 sha256
    pub async fn download_with_sha256(
        &self,
        uri: impl Into<Cow<'static, str>>,
        sha256sum: impl Into<Sha256Digest>,
    ) -> Result<PathBuf> {
        use tokio::io::AsyncReadExt;
        let path = self.download(uri).await?;

        let file = tokio::fs::File::open(&path).await?;
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = Box::new([0u8; 8192]);
        loop {
            let n = reader.read(&mut buffer[..]).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        let digest = hasher.finalize();
        if digest[..] == sha256sum.into().0 {
            Ok(path)
        } else {
            Err(anyhow!("File verification error"))
        }
    }
}

/// 下载分片
async fn download_chunk(
    client: &AsyncClient,
    uri: Cow<'static, str>,
    file: &Arc<Mutex<tokio::fs::File>>,
    start: u64,
    end: u64,
) -> Result<()> {
    let range_header = format!("bytes={}-{}", start, end);
    let resp = client
        .request(Request::get(uri).with_header("Range", range_header))
        .await?;
    let bytes = resp.bytes().await?;
    let mut file = file.lock().await;
    file.seek(tokio::io::SeekFrom::Start(start)).await?;
    file.write_all(&bytes).await?;
    Ok(())
}
