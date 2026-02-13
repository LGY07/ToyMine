use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, channel};
use tokio_util::sync::CancellationToken;
use tracing::error;

struct CommandLoader;

impl CommandLoader {
    async fn stage(&self, value: String) -> String {
        todo!()
    }
    async fn output_pipe(
        &self,
        receiver: Arc<Mutex<Receiver<String>>>,
        t: CancellationToken,
    ) -> Arc<Mutex<Receiver<String>>> {
        let (tx, rx) = channel::<String>(32);
        let process = async || match receiver.lock().await.recv().await {
            None => {
                error!("channel closed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Some(v) => match tx.send(self.stage(v).await).await {
                Ok(_) => {}
                Err(e) => {
                    error!("{e}")
                }
            },
        };
        loop {
            select! {
                _ = process() => {},
                _ = t.cancelled() => {break}
            }
        }

        Arc::new(Mutex::new(rx))
    }
}
