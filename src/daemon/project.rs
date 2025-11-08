use crate::daemon::Config as DaemonConfig;
use crate::daemon::config::Known;
use crate::daemon::control::ErrorResponse;
use crate::daemon::task_manager::TaskManager;
use crate::project_manager::run::{backup_thread, server_thread};
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::spawn;

/// GET 启动服务器
pub async fn start(
    config: State<Arc<DaemonConfig>>,
    task_manager: Extension<Arc<TaskManager<String, String>>>,
    Path(id): Path<usize>,
) -> Result<Response, Response> {
    // 读取已知列表
    let known = Known::from_file(config.storage.work_dir.join("known.toml")).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 查找项目
    let project = known
        .project
        .clone()
        .into_iter()
        .find(|x| x.id == id)
        .ok_or(
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    success: false,
                    error: "The project cannot be found".to_string(),
                }),
            )
                .into_response(),
        )?;
    // 进入目录
    std::env::set_current_dir(project.path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 读取配置
    let project_config = crate::project_manager::get_info().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: format!("The configuration cannot be opened: {:?}", e),
            }),
        )
            .into_response()
    })?;
    // 创建任务
    task_manager.spawn_task(project.id, |rx, tx, stop| async move {
        let config = Arc::from(project_config);
        spawn(backup_thread(config.clone(), stop.clone()));
        spawn(server_thread(rx, tx, stop.clone(), config.clone()));
    });

    Ok((
        StatusCode::OK,
        Json(json!({
            "success":true
        })),
    )
        .into_response())
}

/// GET 停止服务器
pub async fn stop(
    config: State<Arc<DaemonConfig>>,
    task_manager: Extension<Arc<TaskManager<String, String>>>,
    Path(id): Path<usize>,
) -> Result<Response, Response> {
    // 检查任务是否存在
    if !task_manager.exists(id) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                error: "The task cannot be found".to_string(),
            }),
        )
            .into_response());
    }
    // 停止任务
    task_manager.stop_task(id);

    Ok((
        StatusCode::OK,
        Json(json!({
            "success":true
        })),
    )
        .into_response())
}

/// Download 请求体
#[derive(Deserialize)]
pub struct Download {
    path: String,
}
/// POST 获取文件
pub async fn download(
    config: State<Arc<DaemonConfig>>,
    Path(id): Path<usize>,
    Json(body): Json<Download>,
) -> Result<Response, Response> {
    todo!()
}

/// POST 上传文件
pub async fn upload(
    config: State<Arc<DaemonConfig>>,
    Path(id): Path<usize>,
    mut multipart: Multipart,
) -> Result<Response, Response> {
    todo!()
}

/// GET 获取 WebSocket 连接
pub async fn connect(
    config: State<Arc<DaemonConfig>>,
    Path(id): Path<usize>,
) -> Result<Response, Response> {
    todo!()
}
