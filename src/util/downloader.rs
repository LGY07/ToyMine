use crate::GLOBAL_CACHE;
use crate::util::hash::Sha256Digest;
use anyhow::Result;
use anyhow::anyhow;
use futures::AsyncReadExt;
use futures::{StreamExt, stream};
use nyquest::r#async::Response;
use nyquest::{AsyncClient, Request};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, OnceCell};
use tracing::error;

pub struct Downloader {
    client: AsyncClient,
}

/// 多线程下载分片大小
const BLOCK_SIZE: u64 = 1024 * 1024;
/// 单个文件最大连接数
const CONCURRENCY: usize = 8;

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
        let uri = Cow::clone(&uri.into());
        // 获取文件信息
        let head = self.client.request(Request::head(uri.clone())).await?;
        let file_name = GLOBAL_CACHE.join(parse_filename(head.get_header("Content-Disposition")?));
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
            // 计算分片
            let split_ranges = (0..total_size).step_by(BLOCK_SIZE as usize).map(|start| {
                let end = (start + BLOCK_SIZE - 1).min(total_size - 1);
                (start, end)
            });
            file.set_len(total_size).await?;
            let file = Arc::new(Mutex::new(file));
            // 并发下载
            let _ = stream::iter(split_ranges)
                .map(|(start, end)| {
                    let file = file.clone();
                    let uri = uri.clone();
                    async move {
                        if let Err(e) = download_chunk(&self.client, uri, &file, start, end).await {
                            error!("chunk error: {:?}", e);
                        }
                    }
                })
                .buffer_unordered(CONCURRENCY)
                .collect::<Vec<()>>();
            file.lock().await.flush().await?;
        } else {
            let mut stream = self
                .client
                .request(Request::get(uri))
                .await?
                .into_async_read();

            let mut buffer = Box::new([0u8; BLOCK_SIZE as usize]);
            loop {
                let n = stream.read(&mut buffer[..]).await?;

                if n == 0 {
                    break;
                }
                file.write_all(&buffer[..n]).await?;
            }
            file.flush().await?;
        }

        Ok(file_name)
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
/// 解析文件名
fn parse_filename(dispositions: Vec<String>) -> String {
    let cd = dispositions.join(",");
    // 优先 filename*
    if let Some(encoded) = cd
        .split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with("filename*="))
        && let Some(val) = encoded.splitn(2, '=').nth(1)
    {
        let encoded = percent_encoding::percent_decode(val.as_bytes()).collect::<Vec<_>>();
        return String::from_utf8_lossy(&*encoded).into();
    }
    // fallback filename=
    cd.split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with("filename="))
        .and_then(|s| {
            s.splitn(2, '=')
                .nth(1)
                .map(|v| v.trim_matches('"').to_string())
        })
        // fallback uuid
        .unwrap_or(uuid::Uuid::new_v4().to_string())
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
    file.seek(std::io::SeekFrom::Start(start)).await?;
    file.write_all(&bytes).await?;
    Ok(())
}
