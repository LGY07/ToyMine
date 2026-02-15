use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::select;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use anyhow::{anyhow, Context};
use tokio::process::Child;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace, warn};

use crate::core::mc_server::base::McServer;
use crate::core::task::TaskManager;

pub struct Runner {
    pub id: usize,
    pub input: Arc<Sender<String>>,
    pub output: Arc<Mutex<Receiver<String>>>,
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
        let (stdin_tx, mut stdin_rx) = channel::<String>(32);
        // stdout: child -> 外部
        let (stdout_tx, stdout_rx) = channel::<String>(32);

        let mut child_stdin = child.stdin.take().context("child stdin not piped")?;
        let child_stdout = child.stdout.take().context("child stdout not piped")?;

        // Child stdin <- rx
        async fn recv_input(
            line: String,
            child_stdin: &mut tokio::process::ChildStdin,
        ) -> tokio::io::Result<()> {
            trace!("Inputs sending: {}", line.as_str());
            child_stdin.write_all(line.as_bytes()).await?;
            child_stdin.write_all(b"\n").await?;
            Ok(())
        }

        // Child stdout -> tx
        let stdout_rx = Arc::new(Mutex::new(stdout_rx));
        let stdout_rx_clone = Arc::clone(&stdout_rx);
        let mut lines = BufReader::new(child_stdout).lines();
        // 发送输出到管道
        async fn send_output(
            line: String,
            stdout_tx: &Sender<String>,
            stdout_rx: Arc<Mutex<Receiver<String>>>,
        ) -> Result<(), TrySendError<String>> {
            let mut line = Some(line);
            loop {
                match stdout_tx.try_send(line.take().unwrap()) {
                    // 缓冲满时丢弃最早的输出
                    Err(TrySendError::Full(v)) => {
                        debug!("The channel buffer is full. Attempting to clear it.");
                        match stdout_rx.lock().await.try_recv() {
                            Ok(_) => {}
                            Err(e) => {
                                error!("Failed to clear channel: {e}")
                            }
                        }
                        line = Some(v)
                    }
                    Err(e) => return Err(e),
                    Ok(_) => break,
                }
            }
            Ok(())
        }

        // Exit Guard
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
        let stdin_tx = Arc::new(stdin_tx);
        let stdin_tx_clone = Arc::clone(&stdin_tx);
        // 根据信号退出
        async fn stop(child: &mut Child, stdin_tx: Arc<Sender<String>>, time: Duration) {
            // 发出退出信号
            let _ = stdin_tx.send("stop".into()).await;
            // 给时间优雅退出
            let graceful = timeout(time, async {
                loop {
                    if let Ok(Some(_)) = child.try_wait() {
                        return;
                    }
                    sleep(Duration::from_millis(200)).await;
                }
            })
            .await
            .is_ok();
            // 强制退出
            if !graceful {
                let _ = child.start_kill();
            }
        }
        // 观察是否主动退出
        async fn watch(child: &mut Child) {
            loop {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                sleep(Duration::from_millis(200)).await;
            }
        }

        // Child stdin <- rx spawn
        task_manager
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        Some(line) = stdin_rx.recv() => recv_input(line,&mut child_stdin).await?,
                        _ = t.cancelled() => break,
                    }
                }
                Ok(())
            })
            .await?;
        // Child stdin -> tx spawn
        task_manager
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        Ok(Some(line)) = lines.next_line() => send_output(line,&stdout_tx,stdout_rx_clone.clone()).await?,
                        _ = t.cancelled() => break
                    }
                }
                Ok(())
            })
            .await?;
        // Exit Guard spawn
        task_manager
            .spawn(async move {
                select! {
                    time = stop_rx => stop(&mut child,stdin_tx_clone,time?).await,
                    _ = watch(&mut child) => {}
                }
                let status = child.wait().await?;
                let _ = exit_tx.send(status);
                Ok(())
            })
            .await?;

        Ok(Self {
            id,
            input: stdin_tx,
            output: stdout_rx,
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
}

/// 将 IO 同步到控制台，此操作会独占 output
pub async fn sync_channel_stdio(
    input: Arc<Sender<String>>,
    output: Arc<Mutex<Receiver<String>>>,
    t: CancellationToken,
) -> anyhow::Result<()> {
    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    let input = input.clone();

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

    let mut pump_stdout = async || match output.lock().await.recv().await {
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
