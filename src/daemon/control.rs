use axum::Json;
use log::debug;
use serde::Deserialize;
use serde_json::{Value, json};

/// GET 获取状态
pub async fn status() -> Json<Value> {
    debug!("A status request was responded");
    Json(json!({
        "success":true
    }))
}

/// GET 获取列表
pub async fn list() -> Json<Value> {
    todo!()
}

/// Add 请求体
#[derive(Deserialize)]
pub struct Add {
    path: String,
}
/// POST 添加项目
pub async fn add(Json(body): Json<Add>) -> Json<Value> {
    todo!()
}

/// POST 创建项目
pub async fn create(body: String) -> Json<Value> {
    todo!()
}

/// GET 删除项目
pub async fn remove() -> Json<Value> {
    todo!()
}
