use crate::daemon::config::ApiAddr;
use crate::project_manager::get_info;
use anyhow::Error;
use chrono::Utc;
use futures::StreamExt;
use futures_util::SinkExt;
use home::home_dir;
use reqwest_websocket::{Message, RequestBuilderExt, WebSocket};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, stdin};
use tokio::runtime::Runtime;
use tokio::signal::ctrl_c;
use tokio::sync::mpsc;
use tokio::{select, spawn};
use tracing::{debug, error, info};

struct Connection {
    token: String,
    project_id: usize,
    running: bool,
    tcp_addr: String,
    http_client: reqwest::Client,
}

impl Connection {
    async fn new() -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Project {
            id: usize,
            running: bool,
            path: PathBuf,
        }
        #[derive(Deserialize)]
        struct ListResponse {
            projects: Vec<Project>,
        }
        #[derive(Serialize)]
        struct AddRequest {
            path: PathBuf,
        }

        // 检查当前项目
        get_info().expect("Failed to get project info");

        // Work dir 地址
        let work_dir = home_dir().unwrap().join(".pacmine");

        // 偷取 Token 😁
        let daemon_config = crate::daemon::Config::from_file(work_dir.join("config.toml"))
            .expect("Failed to load daemon config");
        let token = &daemon_config
            .token
            .iter()
            .find(|x| x.expiration.is_none_or(|exp| exp > Utc::now()))
            .expect("Failed to find token in daemon config")
            .value;

        // 创建客户端
        let mut tcp_addr = "localhost".to_string();
        let http_client = match &daemon_config.api.listen {
            ApiAddr::UnixSocket(v) => {
                #[cfg(not(target_family = "unix"))]
                {
                    error!("Platform error: Unix Socket is not supported");
                    return;
                }

                #[cfg(target_family = "unix")]
                reqwest::Client::builder()
                    .unix_socket(v.clone())
                    .build()
                    .expect("Failed to build client")
            }
            ApiAddr::Tcp(v) => {
                tcp_addr = v.to_string();
                reqwest::Client::builder()
                    .build()
                    .expect("Failed to build client")
            }
        };

        // 检查运行状态
        http_client
            .get(format!("http://{}/control/status", &tcp_addr))
            .send()
            .await
            .expect("Push to the server failed, make sure the server is running");

        // 获取项目列表
        let res = http_client
            .get(format!("http://{}/control/list", &tcp_addr))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;
        let list: ListResponse = res.json().await?;

        // 获取项目 ID
        let (project_id, running) = if let Some(project) = list
            .projects
            .iter()
            .find(|x| x.path == env::current_dir().expect("Failed to get current directory"))
        {
            (project.id, project.running)
        } else {
            // 添加项目
            http_client
                .post(format!("http://{}/control/add", &tcp_addr))
                .header("Authorization", format!("Bearer {}", token))
                .json(&AddRequest {
                    path: env::current_dir().expect("Failed to get current dir"),
                })
                .send()
                .await?;
            // 测试添加
            let res = http_client
                .get(format!("http://{}/control/list", &tcp_addr))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?;
            let list: ListResponse = res.json().await?;
            let project = list
                .projects
                .iter()
                .find(|x| x.path == env::current_dir().expect("Failed to get current dir"))
                .expect("Failed to add project to daemon");
            (project.id, project.running)
        };

        Ok(Connection {
            token: token.to_owned(),
            project_id,
            tcp_addr,
            running,
            http_client,
        })
    }
}

/// 推送当前项目到服务端运行
pub fn detach_server() {
    // 运行项目
    Runtime::new()
        .expect("Failed to create runtime")
        .block_on(async {
            let connection = Connection::new()
                .await
                .expect("Failed to create connection");
            if connection.running {
                info!("Server is already running");
                return;
            }
            let res = connection
                .http_client
                .get(format!(
                    "http://{}/project/{}/start",
                    connection.tcp_addr, connection.project_id
                ))
                .header("Authorization", format!("Bearer {}", connection.token))
                .send()
                .await
                .expect("Push to the server failed, an unknown error occurred");
            if res.status().is_success() {
                info!("Push to the server successfully");
            } else {
                error!("Failed to push to the server: {:?}", res.text().await);
            }
        })
}

pub fn websocket_client() {
    #[derive(Deserialize)]
    struct ConnectResponse {
        path: String,
    }
    match Runtime::new()
        .expect("Failed to create runtime")
        .block_on(async {
            let connection = Connection::new().await?;
            if !connection.running {
                return Err(Error::msg("Server is not running"));
            }

            let ws_path = connection
                .http_client
                .get(format!(
                    "http://{}/project/{}/connect",
                    &connection.tcp_addr, connection.project_id
                ))
                .header("Authorization", format!("Bearer {}", connection.token))
                .send()
                .await?
                .json::<ConnectResponse>()
                .await?
                .path;

            debug!("Connect to: {}{}", &connection.tcp_addr, &ws_path);

            let ws_client = connection
                .http_client
                .get(format!("ws://{}{}", connection.tcp_addr, ws_path))
                .upgrade()
                .send()
                .await?;

            debug!("Build WebSocket client successfully");

            let websocket = ws_client.into_websocket().await?;

            handle_websocket_and_terminal(websocket).await;

            Ok(())
        }) {
        Ok(_) => (),
        Err(e) => {
            error!("{}", e)
        }
    }
}

async fn handle_websocket_and_terminal(ws_stream: WebSocket) {
    // 通道：终端输入 -> WebSocket
    let (tx, mut rx) = mpsc::channel::<String>(32);

    // ================== 终端输入线程 ==================
    let input_handle = spawn(async move {
        let mut buf = [0u8; 1024];
        let mut stdin = stdin();
        loop {
            select! {
                _ = ctrl_c() => break,
                result = stdin.read(&mut buf) => {
                    match result {
                        Ok(n) if n > 0 => {
                            let line = String::from_utf8_lossy(&buf[..n]).to_string();
                            let _ = tx.send(line).await;
                        }
                        Ok(_) => {}, // 0 字节，忽略
                        Err(_) => break, // 读取错误
                    }
                }
            }
        }
    });

    // ================== WebSocket 拆分 ==================
    let (mut write, mut read) = ws_stream.split();

    // ================== WebSocket 读线程 ==================
    let ws_read_handle = spawn(async move {
        loop {
            select! {
                _ = ctrl_c() => break,
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(s))) => {
                            println!("{}", s);
                        }
                        Some(Ok(Message::Close { code: _, reason: _ })) => break,
                        Some(_) => {},
                        None => break,
                    }
                }
            }
        }
    });

    // ================== WebSocket 写线程 ==================
    loop {
        select! {
            _ = ctrl_c() => break,
            maybe_msg = rx.recv() => {
                match maybe_msg {
                    Some(msg) => {
                        if let Err(e) = write.send(Message::Text(msg)).await {
                            eprintln!("Send message failed: {}", e);
                            break;
                        }
                    }
                    None => break, // 通道关闭，退出
                }
            }
        }
    }

    // 等待所有任务结束
    let _ = input_handle.await;
    let _ = ws_read_handle.await;
}
