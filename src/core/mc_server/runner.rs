use std::ops::Add;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use crate::core::mc_server::base::McServer;
use crate::TASK_MANAGER;
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace};

pub struct Runner {
    pub id: usize,
    pub input: Arc<Sender<String>>,
    pub output: Arc<Mutex<Receiver<String>>>,
    stop: Mutex<Option<tokio::sync::oneshot::Sender<Duration>>>,
    exit: Mutex<tokio::sync::oneshot::Receiver<ExitStatus>>,
}

impl Runner {
    /// 启动服务器
    pub async fn spawn_server(server: &dyn McServer) -> Result<Self> {
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
        let (stdout_tx, stdout_rx) = channel(32);

        let mut child_stdin = child.stdin.take().context("child stdin not piped")?;
        let child_stdout = child.stdout.take().context("child stdout not piped")?;

        // Child stdin <- rx
        async fn recv_input(
            line: String,
            child_stdin: &mut tokio::process::ChildStdin,
        ) -> tokio::io::Result<()> {
            trace!("Inputs sending: {}", line.as_str());
            child_stdin.write_all(line.as_bytes()).await?;

            Ok(())
        }

        // Child stdout -> tx
        let stdout_rx = Arc::new(Mutex::new(stdout_rx));
        let mut lines = BufReader::new(child_stdout).lines();
        // 发送输出到管道
        async fn send_output(line: String, stdout_tx: &Sender<String>) -> Result<()> {
            let start = tokio::time::Instant::now();
            match stdout_tx.send(line).await {
                Ok(_) => {
                    let spend = start.elapsed();
                    if spend > Duration::from_millis(5) {
                        debug!("High backpressure: {} ms", spend.as_millis());
                    }
                    Ok(())
                }
                Err(e) => Err(e.into()),
            }
        }

        // Exit Guard
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let (exit_tx, exit_rx) = tokio::sync::oneshot::channel();
        let stdin_tx = Arc::new(stdin_tx);
        let stdin_tx_clone = Arc::clone(&stdin_tx);
        // 根据信号退出
        async fn stop(child: &mut Child, stdin_tx: Arc<Sender<String>>, time: Duration) {
            // 发出退出信号
            let _ = stdin_tx.send("stop\n".into()).await;
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
        TASK_MANAGER
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        Some(line) = stdin_rx.recv() => recv_input(line, &mut child_stdin).await?,
                        _ = t.cancelled() => break,
                    }
                }
                Ok(())
            })
            .await?;
        // Child stdout -> tx spawn
        TASK_MANAGER
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        Ok(Some(line)) = lines.next_line() => send_output(line,&stdout_tx ).await?,
                        _ = t.cancelled() => break
                    }
                }
                Ok(())
            })
            .await?;
        // Exit Guard spawn
        TASK_MANAGER
            .spawn(async move || {
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
    pub async fn kill_with_timeout(&self, timeout: Duration) -> Result<()> {
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
    pub async fn wait(&self) -> Result<ExitStatus> {
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
) -> Result<()> {
    let mut stdin = fuck_tokio::AsyncStdin::new();

    async fn pump_stdin(input: Arc<Sender<String>>, stdin: &mut fuck_tokio::AsyncStdin) {
        match stdin.next().await {
            Some(line) => {
                if input.send(line.add("\n")).await.is_err() {
                    error!("stdin -> channel failed: receiver dropped");
                }
            }
            None => {
                error!("stdin read thread stopped");
                sleep(Duration::from_millis(200)).await;
            }
        };
    }

    async fn pump_stdout(output: Arc<Mutex<Receiver<String>>>) {
        match output.lock().await.recv().await {
            Some(line) => {
                match tokio::io::stdout().write_all(line.as_bytes()).await {
                    Err(e) => {
                        error!("Stdout write error {e}")
                    }
                    Ok(_) => {}
                }
                let _ = tokio::io::stdout().write_all(b"\n").await;
                let _ = tokio::io::stdout().flush().await;
            }
            None => {
                error!("channel -> stdout closed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        };
    }

    trace!("IO syncing");
    loop {
        select! {
            _ = pump_stdin(input.clone(),&mut stdin) => {}
            _ = pump_stdout(output.clone()) => {}
            _ = t.cancelled() => break Ok(())
        }
    }
}

/// 以下模块解决 tokio::io::stdin 引发的问题
/// Tokio 并没有真正的异步 Stdio
/// 在使用 Tokio 的 Stdin 时，Tokio会创建系统线程接收输入，但是这个线程接收到标准输入之前会阻碍程序退出
/// 以下模块实现了 tokio::io::stdin 的功能，并且增加了更多销毁线程的机会（程序退出时系统会销毁 std::thread）
/// 确保程序优雅退出
mod fuck_tokio {
    use crate::TASK_MANAGER;
    use futures::task::AtomicWaker;
    use futures::Stream;
    use std::io::BufRead;
    use std::io::BufReader;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::TryRecvError;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, OnceLock};
    use std::task::{Context, Poll};
    use std::thread::JoinHandle;
    use tracing::error;

    pub struct AsyncStdin {
        init: AtomicBool,
        waker: Arc<AtomicWaker>,
        rx: OnceLock<Receiver<String>>,
        join_handle: OnceLock<JoinHandle<()>>,
    }
    impl AsyncStdin {
        pub fn new() -> Self {
            Self {
                init: AtomicBool::new(false),
                waker: Arc::new(AtomicWaker::new()),
                rx: OnceLock::new(),
                join_handle: OnceLock::new(),
            }
        }
        fn thread(tx: Sender<String>, waker: Arc<AtomicWaker>) {
            let mut stdin = BufReader::new(std::io::stdin()).lines();
            while let Some(Ok(line)) = stdin.next() {
                if let Err(e) = tx.send(line) {
                    error!("Error send: {}", e)
                } else {
                    waker.wake();
                }
                if TASK_MANAGER.cancel_token.is_cancelled() {
                    return;
                }
            }
        }
    }
    impl Stream for AsyncStdin {
        type Item = String;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.waker.register(cx.waker());
            if !self.init.load(Ordering::Relaxed) {
                let (tx, rx) = channel::<String>();
                let waker = self.waker.clone();
                self.join_handle
                    .set(std::thread::spawn(move || Self::thread(tx, waker)))
                    .unwrap();
                self.rx.set(rx).unwrap();
                self.init.store(true, Ordering::Relaxed);
                return Poll::Pending;
            }
            match self.rx.get().unwrap().try_recv() {
                Ok(v) => Poll::Ready(Some(v)),
                Err(TryRecvError::Empty) => Poll::Pending,
                Err(TryRecvError::Disconnected) => Poll::Ready(None),
            }
        }
    }
}
