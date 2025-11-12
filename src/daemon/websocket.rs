use crate::daemon::control::ErrorResponse;
use crate::daemon::task_manager::TaskManager;
use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use axum::extract::{Path as AxumPath, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};
use tracing::{debug, info};
use uuid::Uuid;

/// WebSocket 管理器
pub struct WebSocketManager {
    pub task_manager: Arc<TaskManager<String, String>>,
    // UUID -> (task_id, Option<断开时间>)
    pub uuid_map: Arc<Mutex<HashMap<Uuid, (usize, Option<Instant>)>>>,
}

impl WebSocketManager {
    pub fn new(task_manager: Arc<TaskManager<String, String>>) -> Self {
        Self {
            task_manager,
            uuid_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register_task(&self, task_id: usize) -> Uuid {
        let uuid = Uuid::new_v4();
        self.uuid_map.lock().await.insert(uuid, (task_id, None));
        uuid
    }

    pub async fn get_task_id(&self, uuid: &Uuid) -> Option<usize> {
        self.uuid_map.lock().await.get(uuid).map(|(id, _)| *id)
    }

    /// 在客户端断开时调用，标记断开时间
    pub async fn mark_disconnected(&self, uuid: &Uuid) {
        let mut map = self.uuid_map.lock().await;
        if let Some((_id, disconnect_time)) = map.get_mut(uuid) {
            *disconnect_time = Some(Instant::now());
        }
    }

    pub async fn start_cleanup_task(self: Arc<Self>, ttl: Duration, interval: Duration) {
        tokio::spawn(async move {
            loop {
                sleep(interval).await;
                let now = Instant::now();
                let mut map = self.uuid_map.lock().await;
                map.retain(|uuid, (_id, disconnect_time)| {
                    if let Some(disconnect) = disconnect_time {
                        if now.duration_since(*disconnect) >= ttl {
                            info!("UUID {} expired after disconnect", uuid);
                            return false;
                        }
                    }
                    true
                });
            }
        });
    }
}

async fn ws_handler(
    socket: WebSocketUpgrade,
    uuid: Uuid,
    task_id: usize,
    ws_manager: Arc<WebSocketManager>,
) -> impl IntoResponse {
    let task_manager = ws_manager.task_manager.clone();
    socket.on_upgrade(move |ws: WebSocket| async move {
        let (mut ws_tx, mut ws_rx) = ws.split();

        let to_task_tx = match task_manager.get_sender(task_id) {
            Some(tx) => tx,
            None => return,
        };
        let from_task_rx_arc = match task_manager.get_receiver(task_id) {
            Some(rx) => rx,
            None => return,
        };

        // 任务 -> 客户端
        let from_task_rx = from_task_rx_arc.clone();
        let send_task = tokio::spawn(async move {
            let mut rx = from_task_rx.lock().await;
            while let Some(msg) = rx.recv().await {
                let utf8_msg = Utf8Bytes::from(msg);
                if ws_tx.send(Message::Text(utf8_msg)).await.is_err() {
                    debug!("Client disconnected while sending from task {}", task_id);
                    break;
                }
            }
        });

        // 客户端 -> 任务
        let recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_rx.next().await {
                if let Message::Text(txt) = msg {
                    let txt: String = txt.to_string();
                    if to_task_tx.send(txt).await.is_err() {
                        debug!("Task {} dropped while receiving from client", task_id);
                        break;
                    }
                }
            }
        });

        tokio::select! {
            _ = send_task => {},
            _ = recv_task => {},
        }

        ws_manager.mark_disconnected(&uuid).await;

        debug!("WebSocket for task {} disconnected", task_id);
    })
}

/// WebSocket 端点
pub(crate) async fn terminal(
    AxumPath(terminal): AxumPath<String>,
    ws: WebSocketUpgrade,
    Extension(ws_manager): Extension<Arc<WebSocketManager>>,
) -> Result<Response, Response> {
    // 读取请求地址
    let uuid = match Uuid::parse_str(&terminal) {
        Ok(u) => u,
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
    // 尝试读取连接
    let task_id = match ws_manager.get_task_id(&uuid).await {
        Some(id) => id,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    success: false,
                    error: "The connection cannot be found".to_string(),
                }),
            )
                .into_response());
        }
    };
    // 开始传输数据
    Ok(ws_handler(ws, uuid, task_id, ws_manager.clone())
        .await
        .into_response())
}
