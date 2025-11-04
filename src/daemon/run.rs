use crate::daemon::control::{add, create, list, remove, status};
use crate::daemon::project::{connect, download, edit, start, stop};
use crate::daemon::websocket::terminal;
use anyhow::Error;
use axum::body::Body;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{
    Json, Router,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use serde_json::json;
use tokio::runtime::Runtime;

/// 运行 Daemon
pub fn server() -> Result<(), Error> {
    let rt = Runtime::new()?;
    rt.block_on(async {
        // 公开路由
        let public = Router::new()
            .route("/control/status", get(status))
            .route("/ws/{terminal}", get(terminal));

        // 受保护的路由
        let protected = Router::new()
            .route("/control/list", get(list))
            .route("/control/add", post(add))
            .route("/control/create", post(create))
            .route("/control/remove", get(remove))
            .route("/project/{id}/start", get(start))
            .route("/project/{id}/stop", get(stop))
            .route("/project/{id}/download", post(download))
            .route("/project/{id}/edit", post(edit))
            .route("/project/{id}/connect", get(connect))
            .route_layer(middleware::from_fn(require_bearer_token));

        // 合并路由
        let app = Router::new().merge(public).merge(protected);

        // 监听
        let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

        // 启动服务
        axum::serve(listener, app).await?;
        Ok::<(), Error>(())
    })?;
    Ok(())
}

/// Bearer Token 验证中间件
async fn require_bearer_token(req: Request<Body>, next: Next) -> Response {
    // 读取 Authorization 头
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    // 检查 Bearer Token
    if let Some(token) = auth_header.and_then(|v| v.strip_prefix("Bearer ")) {
        if token == "abc123" {
            return next.run(req).await;
        }
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
