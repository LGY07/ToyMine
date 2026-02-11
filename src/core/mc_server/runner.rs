use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use anyhow::{Context, anyhow};
use tokio_util::sync::CancellationToken;
use tracing::{error, trace, warn};

use crate::core::hooks::hooked_channel::{HookedReceiver, HookedSender};
use crate::core::mc_server::base::McServer;
use crate::core::task::TaskManager;

pub struct Runner {
    pub id: usize,
    pub input: Arc<HookedSender<String>>,
    pub output: Mutex<HookedReceiver<String>>,
    stop: Mutex<Option<tokio::sync::oneshot::Sender<Duration>>>,
    exit: Mutex<tokio::sync::oneshot::Receiver<ExitStatus>>,
}

impl Runner {
    /// 启动服务器
    pub async fn spawn_server(
        server: &dyn McServer,
        task_manager: &TaskManager,
    ) -> anyhow::Result<Self> {
        let mut child = server
            .start()?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        // 生成一个 id
        static NEXT_RUNNER_ID: AtomicUsize = AtomicUsize::new(1);
        let id = NEXT_RUNNER_ID.fetch_add(1, Ordering::Relaxed);

        // stdin: 外部 -> child
        let (stdin_tx, mut stdin_rx) = HookedSender::<String>::new(32, id);
        // stdout: child -> 外部
        let (stdout_tx, stdout_rx) = HookedReceiver::<String>::new(32, id);

        let mut child_stdin = child.stdin.take().context("child stdin not piped")?;
        let child_stdout = child.stdout.take().context("child stdout not piped")?;

        // Child stdin
        task_manager
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        Some(line) = stdin_rx.recv() =>{
                            trace!("Inputs sending: {}", line.as_str());
                            if child_stdin.write_all(line.as_bytes()).await.is_err() {
                                error!("Failed to input");
                                break;
                            }
                            child_stdin.write_all(b"\n").await?;}
                        _ = t.cancelled() => {
                            break;
                        }
                    }
                }
                Ok(())
            })
            .await?;

        // Child stdout
        task_manager
            .spawn_with_cancel(async move |t| {
                let mut lines = BufReader::new(child_stdout).lines();
                loop {
                    select! {
                        Ok(Some(line)) = lines.next_line() => {
                            if stdout_tx.send(line).await.is_err() {
                            break;
                            }
                        }
                        _ = t.cancelled() => {
                            break
                        }
                    }
                }
                Ok(())
            })
            .await?;

        // Exit Guard
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
        let stdin_tx = Arc::new(stdin_tx);
        let stdin_tx_clone = Arc::clone(&stdin_tx);
        task_manager
            .spawn(async move {
                select! {
                    // 根据信号退出
                    time = stop_rx => {
                        // 发出退出信号
                        let _ = stdin_tx_clone.send("stop".into()).await;
                        // 给时间优雅退出
                        let graceful = timeout(time?, async {
                            loop {
                                if let Ok(Some(_)) = child.try_wait() {
                                    return;
                                }
                                sleep(Duration::from_millis(200)).await;
                            }
                        }).await.is_ok();
                        // 强制退出
                        if !graceful {
                            let _ = child.start_kill();
                        }
                    }
                    // 观察是否主动退出
                    _ = async {
                        loop {
                            if let Ok(Some(_)) = child.try_wait() {
                                return;
                            }
                            sleep(Duration::from_millis(200)).await;
                        }
                    } => {}
                }
                let status = child.wait().await?;
                let _ = exit_tx.send(status);

                Ok(())
            })
            .await?;

        Ok(Self {
            id,
            input: stdin_tx,
            output: Mutex::new(stdout_rx),
            stop: Mutex::new(Some(stop_tx)),
            exit: Mutex::new(exit_rx),
        })
    }

    /// 优雅停机，只应调用一次
    pub async fn kill_with_timeout(&self, timeout: Duration) -> anyhow::Result<()> {
        self.stop
            .lock()
            .await
            .take()
            .context("The stop signal is not allowed to be sent repeatedly.")?
            .send(timeout)
            .map_err(|_| anyhow!("Failed to send stop signal"))?;
        Ok(())
    }

    /// 等待退出
    pub async fn wait(&self) -> anyhow::Result<ExitStatus> {
        let mut rx_guard = self.exit.lock().await;
        let rx = &mut *rx_guard;
        rx.await.context("failed waiting for exit")
    }

    /// 将 IO 同步到控制台，此操作会独占 output
    pub async fn sync_channel_stdio(&self, t: CancellationToken) -> anyhow::Result<()> {
        let mut stdin = BufReader::new(tokio::io::stdin()).lines();
        let mut stdout = tokio::io::stdout();
        let input = self.input.clone();

        let mut pump_stdin = async || match stdin.next_line().await {
            Ok(Some(line)) => {
                if input.send(line).await.is_err() {
                    error!("stdin -> channel failed: receiver dropped");
                }
            }
            Ok(None) => {
                warn!("stdin EOF");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                error!("stdin read error: {e}");
            }
        };

        let mut pump_stdout = async || match self.output.lock().await.recv().await {
            Some(line) => {
                if let Err(e) = stdout.write_all(line.as_bytes()).await {
                    error!("stdout write error: {e}");
                }
                let _ = stdout.write_all(b"\n").await;
                let _ = stdout.flush().await;
            }
            None => {
                error!("channel -> stdout closed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        };

        trace!("IO syncing");
        loop {
            select! {
                _ = pump_stdin() => {}
                _ = pump_stdout() => {}
                _ = t.cancelled() => break Ok(())
            }
        }
    }
}
