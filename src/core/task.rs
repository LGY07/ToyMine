use std::future::Future;
use std::pin::Pin;

use tokio::select;
use tokio::sync::{Mutex, mpsc};
use tokio::task::{JoinHandle, JoinSet};

use anyhow::{Result, anyhow};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, trace};

type Task = Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>;

pub struct TaskManager {
    spawn_channel: Mutex<mpsc::Sender<Task>>,
    pub cancel_token: CancellationToken,
    manager: Mutex<Option<JoinHandle<()>>>,
}

impl TaskManager {
    pub fn new() -> Self {
        let tasks = JoinSet::<Result<()>>::new();

        let (tx, rx) = mpsc::channel::<Task>(32);
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        let manager = Mutex::new(Some(tokio::spawn(async move {
            Self::manager(tasks, rx, cancel_clone).await;
        })));

        Self {
            spawn_channel: Mutex::new(tx),
            cancel_token,
            manager,
        }
    }

    /// spawn 一个新的 async 任务到 TaskManager
    pub async fn spawn<F>(&self, future: F) -> Result<()>
    where
        F: AsyncFnOnce() -> Result<()> + Send + 'static,
        F::CallOnceFuture: Send + 'static,
    {
        let task: Task = Box::pin(future());
        self.spawn_channel
            .lock()
            .await
            .send(task)
            .await
            .map_err(|e| anyhow!("Failed to spawn task: {}", e))?;
        Ok(())
    }

    /// 带有取消令牌的 spawn
    pub async fn spawn_with_cancel<F>(&self, future: F) -> Result<()>
    where
        F: AsyncFnOnce(CancellationToken) -> Result<()> + Send + 'static,
        F::CallOnceFuture: Send + 'static,
    {
        let task = Box::pin(future(self.cancel_token.clone()));
        self.spawn_channel
            .lock()
            .await
            .send(task)
            .await
            .map_err(|e| anyhow!("Failed to spawn task: {}", e))?;
        Ok(())
    }

    /// manager loop，处理 spawn_channel 的任务并管理 JoinSet
    async fn manager(
        mut tasks: JoinSet<Result<()>>,
        mut channel: mpsc::Receiver<Task>,
        cancel_token: CancellationToken,
    ) {
        loop {
            select! {
                _ = cancel_token.cancelled() => {
                    debug!("Manager: received shutdown signal");
                    break;
                }
                Some(task) = channel.recv() => {
                    tasks.spawn(task);
                    trace!("Spawned task")
                }
                Some(res) = tasks.join_next() => {
                    match res {
                        Ok(Ok(_)) => {}
                        Ok(Err(e)) => debug!("{}", e),
                        Err(e) => debug!("{}", e),
                    }
                }
            }
        }
        debug!("Manager: exited loop");
        tasks.shutdown().await;
    }

    /// 优雅 shutdown TaskManager
    pub async fn shutdown(&self) {
        trace!("Shutdown: Start");
        self.cancel_token.cancel();
        let _ = self
            .manager
            .lock()
            .await
            .take()
            .expect("No join handle")
            .await
            .inspect_err(|e| error!("{e}"));
        debug!("Shutdown: Finish");
    }
}
