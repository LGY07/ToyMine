pub mod raw;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::select;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::TASK_MANAGER;
use crate::core::mc_server::runner::Runner;
use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use futures::{StreamExt, stream};

#[async_trait]
pub trait CommandPlugin: Send + Sync {
    async fn process(&self, value: String, sender: Arc<Sender<String>>) -> String;
}

pub struct CommandLoader {
    pub(crate) plugins: HashMap<usize, ArcSwap<Vec<Box<dyn CommandPlugin>>>>,
}

impl CommandLoader {
    pub fn new() -> Self {
        CommandLoader {
            plugins: HashMap::new(),
        }
    }
    /// 为实例注册命令插件
    pub fn register(&mut self, id: usize, plugins: Vec<Box<dyn CommandPlugin>>) -> Result<()> {
        match self.plugins.get(&id) {
            Some(v) => {
                v.store(Arc::new(plugins));
                Ok(())
            }
            None => {
                if self
                    .plugins
                    .insert(id, ArcSwap::new(Arc::new(plugins)))
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
    pub async fn load(&mut self, runner: &Runner) -> Result<Arc<Mutex<Receiver<String>>>> {
        let (tx, rx) = channel::<String>(32);
        let tx = Arc::new(tx);
        let rx = Arc::new(Mutex::new(rx));
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
            plugins: Arc<Vec<Box<dyn CommandPlugin>>>,
        ) -> Result<()> {
            match output.lock().await.recv().await {
                None => Err(anyhow!("channel closed")),
                Some(value) => {
                    let value = stream::iter(plugins.iter())
                        .fold(value, |v, x: &Box<dyn CommandPlugin>| {
                            x.process(v, input.clone())
                        })
                        .await;
                    match tx.send(value).await {
                        Err(e) => Err(e.into()),
                        Ok(_) => Ok(()),
                    }
                }
            }
        }

        TASK_MANAGER
            .spawn_with_cancel(async move |t| {
                loop {
                    select! {
                        e = pipeline(input.clone(),output.clone(),tx.clone(),plugins.clone()) => e?,
                        _ = t.cancelled() => break
                    }
                }
                Ok(())
            })
            .await?;

        Ok(rx)
    }
}
