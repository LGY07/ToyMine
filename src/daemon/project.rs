use crate::daemon::Config as DaemonConfig;
use crate::daemon::config::Known;
use crate::daemon::control::ErrorResponse;
use crate::daemon::task_manager::TaskManager;
use crate::project_manager::run::{backup_thread, server_thread};
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::spawn;

/// GET 启动服务器
pub async fn start(
    config: State<Arc<DaemonConfig>>,
    task_manager: Extension<Arc<TaskManager<String, String>>>,
    AxumPath(id): AxumPath<usize>,
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
    task_manager: Extension<Arc<TaskManager<String, String>>>,
    AxumPath(id): AxumPath<usize>,
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
    AxumPath(id): AxumPath<usize>,
    Json(body): Json<Download>,
) -> Result<Response, Response> {
    #[derive(Serialize)]
    struct DownloadResponse {
        success: bool,
        text: bool,
        file: String,
    }
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
    // 尝试打开文件
    let mut file = File::open(body.path).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 检测是否为文本文件
    let is_text = is_text_from_file(&mut file).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 为非文本文件进行 Base64 编码
    let mut contents = String::new();
    if is_text {
        file.read_to_string(&mut contents).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: e.to_string(),
                }),
            )
                .into_response()
        })?;
    } else {
        contents = file_to_base64(file).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: e.to_string(),
                }),
            )
                .into_response()
        })?;
    }

    Ok((
        StatusCode::OK,
        Json(DownloadResponse {
            success: true,
            text: is_text,
            file: contents,
        }),
    )
        .into_response())
}
/// 判断文件是否为文本文件
async fn is_text_from_file(file: &mut File) -> io::Result<bool> {
    // 保存当前位置
    let pos = file.stream_position().await?;

    // 临时读取前 4 KB
    let mut buf = vec![0u8; 4096];
    let n = file.read(&mut buf).await?;
    let bytes = &buf[..n];

    // 恢复文件指针到原位置
    file.seek(SeekFrom::Start(pos)).await?;

    // 空文件视为文本
    if bytes.is_empty() {
        return Ok(true);
    }

    // 尝试 UTF-8 解码
    if std::str::from_utf8(bytes).is_ok() {
        return Ok(true);
    }

    // 启发式判断（非打印字符比例）
    let mut non_printable = 0usize;
    for &b in bytes {
        match b {
            9 | 10 | 13 => {} // tab、LF、CR 允许
            32..=126 => {}    // 可打印 ASCII
            _ => non_printable += 1,
        }
    }

    let ratio = non_printable as f32 / bytes.len() as f32;
    Ok(ratio < 0.05) // 非打印字符 <5% 认为是文本
}
/// Base64 编码
async fn file_to_base64(mut file: File) -> io::Result<String> {
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await?;
    Ok(general_purpose::STANDARD.encode(&buf))
}

/// POST 上传文件
pub async fn upload(
    config: State<Arc<DaemonConfig>>,
    AxumPath(id): AxumPath<usize>,
    mut multipart: Multipart,
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

    // 读取表单数据
    let mut save_path: Option<String> = None;
    // 遍历表单字段
    while let Some(field_result) = multipart.next_field().await.transpose() {
        let field = match field_result {
            Ok(f) => f,
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        success: false,
                        error: e.to_string(),
                    }),
                )
                    .into_response());
            }
        };
        if let Some(name) = field.name() {
            // 处理 path 字段
            if name == "path" {
                match field.text().await {
                    Ok(text) => save_path = Some(text),
                    Err(e) => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                success: false,
                                error: e.to_string(),
                            }),
                        )
                            .into_response());
                    }
                }
            }
            // 处理 file 字段
            else if name == "file" {
                // 确保 path 已经获取
                let save_path = match &save_path {
                    Some(p) => p,
                    None => {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                success: false,
                                error: "Missing 'path' field".to_string(),
                            }),
                        )
                            .into_response());
                    }
                };
                let path = Path::new(save_path);
                // 创建父目录（如果不存在）
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                success: false,
                                error: e.to_string(),
                            }),
                        )
                            .into_response()
                    })?;
                }
                // 创建目标文件
                let mut file_handle = match File::create(path).await {
                    Ok(f) => f,
                    Err(e) => {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                success: false,
                                error: e.to_string(),
                            }),
                        )
                            .into_response());
                    }
                };
                // 流式写入 multipart chunk
                let mut f = field;
                while let Some(chunk_result) = f.chunk().await.transpose() {
                    let chunk = match chunk_result {
                        Ok(c) => c,
                        Err(e) => {
                            return Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse {
                                    success: false,
                                    error: e.to_string(),
                                }),
                            )
                                .into_response());
                        }
                    };
                    if let Err(e) = file_handle.write_all(&chunk).await {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                success: false,
                                error: e.to_string(),
                            }),
                        )
                            .into_response());
                    }
                }
                return Ok((
                    StatusCode::OK,
                    Json(json!({
                        "success":true
                    })),
                )
                    .into_response());
            }
        }
    }
    // 如果遍历完没有 file 字段
    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            success: false,
            error: "Missing 'file' field".to_string(),
        }),
    )
        .into_response())
}

/// GET 获取 WebSocket 连接
pub async fn connect(
    config: State<Arc<DaemonConfig>>,
    AxumPath(id): AxumPath<usize>,
) -> Result<Response, Response> {
    todo!()
}
