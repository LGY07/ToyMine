use crate::daemon::control::ErrorResponse;
use crate::daemon::task_manager::TaskManager;
use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use axum::extract::{Path as AxumPath, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use futures::{SinkExt, StreamExt};
use log::{debug, info};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

/// WebSocket 管理器
pub struct WebSocketManager {
    pub task_manager: Arc<TaskManager<String, String>>,
    pub uuid_map: Arc<Mutex<HashMap<Uuid, usize>>>, // UUID -> task_id
    pub next_task_id: Arc<Mutex<usize>>,
}

impl WebSocketManager {
    pub fn new(task_manager: Arc<TaskManager<String, String>>) -> Self {
        Self {
            task_manager,
            uuid_map: Arc::new(Mutex::new(HashMap::new())),
            next_task_id: Arc::new(Mutex::new(1)),
        }
    }

    /// 创建一个 UUID 并注册任务
    pub fn register_task(&self, task_id: usize) -> Uuid {
        // 生成 UUID
        let uuid = Uuid::new_v4();

        // 插入映射 uuid -> task_id
        self.uuid_map.lock().unwrap().insert(uuid, task_id);

        // 返回 UUID 给客户端
        uuid
    }

    /// 通过 UUID 获取 task_id
    pub fn get_task_id(&self, uuid: &Uuid) -> Option<usize> {
        self.uuid_map.lock().unwrap().get(uuid).cloned()
    }
}

async fn ws_handler(
    socket: WebSocketUpgrade,
    task_id: usize,
    task_manager: Arc<TaskManager<String, String>>,
    disconnect_timeout: Duration,
) -> impl IntoResponse {
    socket.on_upgrade(move |ws: WebSocket| async move {
        let (mut ws_tx, mut ws_rx) = ws.split();

        // 获取任务通道
        let to_task_tx = match task_manager.get_sender(task_id) {
            Some(tx) => tx,
            None => {
                debug!("Task {} not found when connecting WebSocket", task_id);
                return;
            }
        };
        let from_task_rx_arc = match task_manager.get_receiver(task_id) {
            Some(rx) => rx,
            None => {
                debug!("Task {} not found when connecting WebSocket", task_id);
                return;
            }
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

        // 等待任一方向断开
        tokio::select! {
            _ = send_task => {},
            _ = recv_task => {},
        }

        debug!(
            "WebSocket for task {} disconnected, starting delayed shutdown",
            task_id
        );

        // 延迟关闭任务
        let manager_clone = task_manager.clone();
        tokio::spawn(async move {
            sleep(disconnect_timeout).await;
            if manager_clone.exists(task_id) {
                manager_clone.stop_task(task_id);
                info!("Task {} stopped after disconnect timeout", task_id);
            }
        });
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
    let task_id = match ws_manager.get_task_id(&uuid) {
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
    ws_handler(
        ws,
        task_id,
        ws_manager.task_manager.clone(),
        Duration::from_secs(5),
    )
    .await;

    Ok((
        StatusCode::OK,
        Json(json!({
            "success":true
        })),
    )
        .into_response())
}
