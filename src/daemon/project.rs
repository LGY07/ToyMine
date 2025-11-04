use axum::Json;
use axum::extract::{Multipart, Path};
use serde::Deserialize;
use serde_json::Value;

/// GET 启动服务器
pub async fn start(Path(id): Path<u32>) -> Json<Value> {
    todo!()
}

/// GET 停止服务器
pub async fn stop(Path(id): Path<u32>) -> Json<Value> {
    todo!()
}

/// Download 请求体
#[derive(Deserialize)]
pub struct Download {
    path: String,
}
/// POST 获取文件
pub async fn download(Path(id): Path<u32>, Json(body): Json<Download>) -> Json<Value> {
    todo!()
}

/// POST 上传文件
pub async fn edit(Path(id): Path<u32>, mut multipart: Multipart) -> Json<Value> {
    todo!()
}

/// GET 获取 WebSocket 连接
pub async fn connect(Path(id): Path<u32>) -> Json<Value> {
    todo!()
}
