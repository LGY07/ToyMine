use crate::daemon::config;
use crate::daemon::config::{ApiAddr, Known, Token};
use crate::daemon::control::{add, create, list, remove, status};
use crate::daemon::project::{connect, download, start, stop, upload};
use crate::daemon::task_manager::TaskManager;
use crate::daemon::websocket::{WebSocketManager, terminal};
use anyhow::Error;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{
    Extension, Json, Router,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use chrono::Utc;
use log::{debug, info, warn};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

/// 运行 Daemon
pub fn server(config: config::Config) -> Result<(), Error> {
    // 检查配置文件
    config.check_config()?;

    // 初始化工作目录
    let dir_list = [
        &config.storage.work_dir,
        &config.storage.work_dir.join("projects"),
        &config.storage.work_dir.join("upper"),
        &config.storage.work_dir.join("read_only"),
        &config.storage.work_dir.join("read_only").join("resources"),
        &config.storage.work_dir.join("read_only").join("versions"),
        &config.storage.work_dir.join("read_only").join("runtimes"),
    ];
    for i in dir_list {
        if !i.is_dir() {
            std::fs::create_dir(i)?;
        }
    }

    // 创建 known list
    if !&config.storage.work_dir.join("known.toml").is_file() {
        Known {
            current_mode: config.storage.save_space.clone(),
            project: vec![],
        }
        .to_file(config.storage.work_dir.join("known.toml"))?;
    }

    // 配置信息
    let config = Arc::new(config);
    // 创建线程管理器
    let task_manager = Arc::new(TaskManager::<String, String>::new());
    // 创建 WebSocket 管理器
    let ws_manager = Arc::new(WebSocketManager::new(task_manager.clone()));

    let rt = Runtime::new()?;
    rt.block_on(async {
        // 每 1s 清理 WebSocket Token, TTL 默认为 10s
        if let Some(0) = config.security.websocket_ttl {
            warn!("WebSocket Token cleaning has been disabled")
        } else {
            ws_manager.clone().start_cleanup_task(
                if let Some(ttl) = config.security.websocket_ttl {
                    Duration::from_secs(ttl as u64)
                } else {
                    Duration::from_secs(10)
                },
                Duration::from_secs(1),
            );
        }

        // 公开路由
        let public = Router::new()
            .route("/control/status", get(status))
            .route("/ws/{terminal}", get(terminal));

        // 受保护的路由
        let config_clone = Arc::clone(&config);
        let protected = Router::new()
            .route("/control/list", get(list))
            .route("/control/add", post(add))
            .route("/control/create", post(create))
            .route("/control/remove/{id}", get(remove))
            .route("/project/{id}/start", get(start))
            .route("/project/{id}/stop", get(stop))
            .route("/project/{id}/download", post(download))
            .route("/project/{id}/upload", post(upload))
            .route("/project/{id}/connect", get(connect))
            .route_layer(middleware::from_fn(move |req, next| {
                require_bearer_token(req, next, config_clone.token.clone())
            }));

        // 合并路由
        let app = Router::new()
            .merge(public)
            .merge(protected)
            .with_state(config.clone())
            .layer(Extension(task_manager.clone()))
            .layer(Extension(ws_manager.clone()))
            .layer(if config.security.upload_limit.unwrap_or(0) == 0 {
                DefaultBodyLimit::disable()
            } else {
                DefaultBodyLimit::max(config.security.upload_limit.unwrap() * 1024)
            });

        // 启动服务
        match &config.api.listen {
            // 监听 Tcp
            ApiAddr::Tcp(addr) => {
                info!("Listening on TCP: {addr}");
                let listener = TcpListener::bind(addr).await?;
                axum::serve(listener, app).await?;
            }

            // 监听 Unix Socket
            ApiAddr::UnixSocket(path) => {
                #[cfg(not(target_family = "unix"))]
                {
                    return Err(Error::msg("Unix socket not supported on this platform"));
                }

                #[cfg(target_family = "unix")]
                {
                    use std::path::Path;
                    use tokio::net::UnixListener;
                    // 删除旧的 socket 文件
                    if Path::new(&path).exists() {
                        std::fs::remove_file(&path)?;
                    }

                    info!("Listening on Unix socket: {path:?}");
                    let listener = UnixListener::bind(&path)?;
                    axum::serve(listener, app).await?;
                }
            }
        };

        Ok::<(), Error>(())
    })?;
    Ok(())
}

/// Bearer Token 验证中间件
async fn require_bearer_token(req: Request<Body>, next: Next, token_list: Vec<Token>) -> Response {
    // 读取 Authorization 头
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    // 检查 Bearer Token
    if let Some(token) = auth_header.and_then(|v| v.strip_prefix("Bearer ")) {
        if token_list.iter().any(|known_token| {
            // Token 存在且未过期
            known_token.value == token && known_token.expiration.is_none_or(|exp| exp > Utc::now())
        }) {
            debug!("Bearer Token authentication was successful");
            return next.run(req).await;
        }
        debug!("Bearer Token authentication failed")
    }

    // 返回 JSON 错误
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "success": false,
            "error": "Invalid or missing Bearer token"
        })),
    )
        .into_response()
}
