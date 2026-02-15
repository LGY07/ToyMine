use futures::StreamExt;
pub mod raw;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::select;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use futures::stream;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{debug, error};

use crate::core::mc_server::runner::Runner;
use crate::core::task::TaskManager;

#[async_trait]
pub trait CommandPlugin: Send + Sync {
    async fn process(&self, value: String, sender: Arc<Sender<String>>) -> String;
}

pub struct CommandLoader {
    plugins: HashMap<usize, ArcSwap<Vec<Box<dyn CommandPlugin>>>>,
}

impl CommandLoader {
    pub fn new() -> Self {
        CommandLoader {
            plugins: HashMap::new(),
        }
    }
    /// 为实例注册命令插件
    pub fn register(
        &mut self,
        runner: &Runner,
        plugins: Vec<Box<dyn CommandPlugin>>,
    ) -> Result<()> {
        match self.plugins.get(&runner.id) {
            Some(v) => {
                v.store(Arc::new(plugins));
                Ok(())
            }
            None => {
                if self
                    .plugins
                    .insert(runner.id, ArcSwap::new(Arc::new(plugins)))
                    .is_none()
                {
                    Ok(())
                } else {
                    Err(anyhow!("Failed to register command plugin"))
                }
            }
        }
    }
    /// 将实例加载到命令插件加载器，并返回处理过的 Receiver，此操作会阻塞原有 Receiver，需要输出应使用返回的 Receiver
    pub async fn load(
        &mut self,
        runner: &Runner,
        task_manager: &TaskManager,
    ) -> Result<Arc<Mutex<Receiver<String>>>> {
        let (tx, rx) = channel::<String>(32);
        let tx = Arc::new(tx);
        let rx = Arc::new(Mutex::new(rx));
        let rx_clone = Arc::clone(&rx);
        let input = Arc::clone(&runner.input);
        let output = Arc::clone(&runner.output);

        let plugins = match self.plugins.get(&runner.id) {
            None => {
                self.plugins
                    .insert(runner.id, ArcSwap::new(Arc::new(vec![])));
                self.plugins
                    .get(&runner.id)
                    .expect("Failed to register empty command plugin")
                    .load()
            }
            Some(p) => p.load(),
        };

        async fn pipeline(
            input: Arc<Sender<String>>,
            output: Arc<Mutex<Receiver<String>>>,
            tx: Arc<Sender<String>>,
            rx: Arc<Mutex<Receiver<String>>>,
            plugins: Arc<Vec<Box<dyn CommandPlugin>>>,
        ) -> Result<()> {
            match output.lock().await.recv().await {
                None => Err(anyhow!("channel closed")),
                Some(value) => {
                    let mut value = Some(value);

                    loop {
                        match tx.try_send(
                            stream::iter(plugins.iter())
                                .fold(value.unwrap(), |v, x: &Box<dyn CommandPlugin>| {
                                    x.process(v, input.clone())
                                })
                                .await,
                        ) {
                            // 缓冲满时丢弃最早的输出
                            Err(TrySendError::Full(v)) => {
                                debug!("The channel buffer is full. Attempting to clear it.");
                                match rx.lock().await.try_recv() {
                                    Ok(_) => {}
                                    Err(e) => {
                                        error!("Failed to clear channel: {e}")
                                    }
                                }
                                value = Some(v);
                            }
                            Err(e) => break Err(e.into()),
                            Ok(_) => break Ok(()),
                        }
                    }
                }
            }
        }

        task_manager
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        e = pipeline(input.clone(),output.clone(),tx.clone(),rx_clone.clone(),plugins.clone()) => e?,
                        _ = t.cancelled() => break
                    }
                }
                Ok(())
            })
            .await?;

        Ok(rx)
    }
}
