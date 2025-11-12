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
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, info};
use uuid::Uuid;

/// WebSocket 管理器
pub struct WebSocketManager {
    pub task_manager: Arc<TaskManager<String, String>>,
    // UUID -> (task_id, Option<空闲开始时间>), None 表示当前有连接
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
        self.uuid_map
            .lock()
            .await
            .insert(uuid, (task_id, Some(Instant::now())));
        uuid
    }

    pub async fn get_task_id(&self, uuid: &Uuid) -> Option<usize> {
        self.uuid_map.lock().await.get(uuid).map(|(id, _)| *id)
    }

    /// 客户端连接时调用，重置空闲时间
    async fn mark_connected(&self, uuid: &Uuid) {
        let mut map = self.uuid_map.lock().await;
        if let Some((_id, idle_since)) = map.get_mut(uuid) {
            *idle_since = None;
        }
    }

    /// 客户端断开时调用，开始 TTL 计时
    async fn mark_disconnected(&self, uuid: &Uuid) {
        let mut map = self.uuid_map.lock().await;
        if let Some((_id, idle_since)) = map.get_mut(uuid) {
            *idle_since = Some(Instant::now());
        }
    }

    /// 定时清理空闲超过 TTL 的 UUID
    pub async fn start_cleanup_task(self: Arc<Self>, ttl: Duration, interval: Duration) {
        tokio::spawn(async move {
            loop {
                sleep(interval).await;
                let now = Instant::now();
                let mut map = self.uuid_map.lock().await;
                map.retain(|uuid, (_id, idle_since)| {
                    if let Some(start) = idle_since {
                        if now.duration_since(*start) >= ttl {
                            info!("UUID {} expired due to idle TTL", uuid);
                            return false;
                        }
                    }
                    true
                });
            }
        });
    }
}

/// WebSocket 处理
async fn ws_handler(
    socket: WebSocketUpgrade,
    uuid: Uuid,
    task_id: usize,
    task_manager: Arc<TaskManager<String, String>>,
    ws_manager: Arc<WebSocketManager>,
) -> impl IntoResponse {
    socket.on_upgrade(move |ws: WebSocket| async move {
        ws_manager.mark_connected(&uuid).await;

        let (ws_tx, mut ws_rx) = ws.split();
        let ws_tx = Arc::new(Mutex::new(ws_tx));

        let to_task_tx = match task_manager.get_sender(task_id) {
            Some(tx) => tx,
            None => return,
        };
        let from_task_rx_arc = match task_manager.get_receiver(task_id) {
            Some(rx) => rx,
            None => return,
        };

        // 监控 task 退出（tx drop）
        {
            let tx_clone = to_task_tx.clone();
            let ws_tx_clone = ws_tx.clone();
            tokio::spawn(async move {
                tx_clone.closed().await;
                let mut tx = ws_tx_clone.lock().await;
                let _ = tx.send(Message::Close(None)).await;
                debug!("Task {} tx dropped -> sent WebSocket Close", task_id);
            });
        }

        // 监控 task 退出（rx drop）
        {
            let from_task_rx_arc_clone = from_task_rx_arc.clone();
            let ws_tx_clone = ws_tx.clone();
            tokio::spawn(async move {
                // 等待任务的 rx 被关闭
                loop {
                    {
                        let rx = from_task_rx_arc_clone.lock().await;
                        if rx.is_closed() {
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }

                let mut tx = ws_tx_clone.lock().await;
                let _ = tx.send(Message::Close(None)).await;
                debug!("Task {} rx dropped -> sent WebSocket Close", task_id);
            });
        }

        // 任务 -> 客户端
        let ws_tx_clone = ws_tx.clone();
        let from_task_rx = from_task_rx_arc.clone();
        let send_task = tokio::spawn(async move {
            let mut rx = from_task_rx.lock().await;
            while let Some(msg) = rx.recv().await {
                let mut tx = ws_tx_clone.lock().await;
                if tx.send(Message::Text(Utf8Bytes::from(msg))).await.is_err() {
                    debug!("Client disconnected while sending from task {}", task_id);
                    break;
                }
            }
        });

        // 客户端 -> 任务
        let recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_rx.next().await {
                if let Message::Text(txt) = msg {
                    if to_task_tx.send(txt.parse().unwrap()).await.is_err() {
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

        debug!("WebSocket for task {} disconnected", task_id);
        ws_manager.mark_disconnected(&uuid).await;
    })
}

/// WebSocket 端点
pub(crate) async fn terminal(
    AxumPath(terminal): AxumPath<String>,
    ws: WebSocketUpgrade,
    Extension(ws_manager): Extension<Arc<WebSocketManager>>,
) -> Result<Response, Response> {
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

    Ok(ws_handler(
        ws,
        uuid,
        task_id,
        ws_manager.task_manager.clone(),
        ws_manager.clone(),
    )
    .await
    .into_response())
}
