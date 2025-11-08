use crate::daemon::Config;
use crate::daemon::config::{Known, Project};
use crate::project_manager;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::{Json, http::StatusCode};
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Deserialize, Serialize)]
struct ErrorResponse {
    success: bool,
    error: String,
}

/// GET 获取状态
pub async fn status() -> Response {
    debug!("A status request was responded");
    (
        StatusCode::OK,
        Json(json!({
            "success":true
        })),
    )
        .into_response()
}

/// GET 获取列表
pub async fn list(config: State<Arc<Config>>) -> Result<Response, Response> {
    debug!("A list request was responded");
    // 定义响应
    #[derive(Deserialize, Serialize)]
    struct Project {
        id: usize,
        running: bool,
        name: String,
        server_type: String,
        version: String,
    }
    #[derive(Deserialize, Serialize)]
    struct ListResponse {
        success: bool,
        projects: Vec<Project>,
    }
    let mut list_response = ListResponse {
        success: true,
        projects: vec![],
    };

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

    // 构造响应列表
    for i in known.project {
        let config =
            project_manager::Config::from_file(i.path.join("PacMine.toml")).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        success: false,
                        error: e.to_string(),
                    }),
                )
                    .into_response()
            })?;
        list_response.projects.push(Project {
            id: i.id,
            running: false, // TODO
            name: config.project.name,
            server_type: format!("{:?}", config.project.server_type),
            version: config.project.version,
        })
    }
    Ok((StatusCode::OK, Json(list_response)).into_response())
}

/// Add 请求体
#[derive(Deserialize)]
pub struct Add {
    path: String,
}
/// POST 添加项目
pub async fn add(config: State<Arc<Config>>, Json(body): Json<Add>) -> Result<Response, Response> {
    debug!("A add request was responded");
    // 读取已知列表
    let mut known = Known::from_file(config.storage.work_dir.join("known.toml")).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;

    // 尝试读取添加的项目
    project_manager::Config::from_file(std::path::Path::new(&body.path).join("PacMine.toml"))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: e.to_string(),
                }),
            )
                .into_response()
        })?;

    // 添加项目
    known.project.push(Project {
        id: known.project.iter().map(|x| x.id).max().unwrap_or(0) + 1,
        manual: true, // 此处为 add API 创建
        path: PathBuf::from(body.path),
    });

    // 写入列表
    known
        .to_file(config.storage.work_dir.join("known.toml"))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: e.to_string(),
                }),
            )
                .into_response()
        })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "success":true
        })),
    )
        .into_response())
}

/// POST 创建项目
pub async fn create(config: State<Arc<Config>>, body: String) -> Result<Response, Response> {
    debug!("A create request was responded");
    // 解析 TOML
    let project = toml::from_str::<project_manager::Config>(body.as_str()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;

    // 创建目录
    let dir = config
        .storage
        .work_dir
        .join("projects")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir(&dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    std::env::set_current_dir(&dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 写入配置文件
    project.to_file("PacMine.toml").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 初始化目录
    for i in project_manager::create::DIR_LIST {
        fs::create_dir(i).map_err(|e| {
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

    // 添加到 known list

    // 读取已知列表
    let mut known = Known::from_file(config.storage.work_dir.join("known.toml")).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 添加项目
    known.project.push(Project {
        id: known.project.iter().map(|x| x.id).max().unwrap_or(0) + 1,
        manual: false, // 此处为 create API 创建
        path: dir,
    });
    // 写入列表
    known
        .to_file(config.storage.work_dir.join("konwn.toml"))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: e.to_string(),
                }),
            )
                .into_response()
        })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "success":true
        })),
    )
        .into_response())
}

/// GET 删除项目
pub async fn remove(
    config: State<Arc<Config>>,
    Path(id): Path<usize>,
) -> Result<Response, Response> {
    debug!("A remove request was responded");
    // 读取已知列表
    let mut known = Known::from_file(config.storage.work_dir.join("known.toml")).map_err(|e| {
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
    // 判断是否应该删除
    if project.manual {
        return Err((
            StatusCode::METHOD_NOT_ALLOWED,
            Json(ErrorResponse {
                success: false,
                error: "Manually created projects are not allowed to be deleted".to_string(),
            }),
        )
            .into_response());
    }
    // 删除项目
    fs::remove_dir_all(project.path.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: e.to_string(),
            }),
        )
            .into_response()
    })?;
    // 删除列表项目
    known.project.retain(|x| x.id == project.id);
    // 写入列表
    known
        .to_file(config.storage.work_dir.join("known.toml"))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: e.to_string(),
                }),
            )
                .into_response()
        })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "success":true
        })),
    )
        .into_response())
}
