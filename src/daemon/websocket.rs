use axum::extract::Path;
use axum::http::HeaderMap;

/// GET 连接 WebSocket
pub async fn terminal(headers: HeaderMap, Path(terminal): Path<String>) {}
