use crate::command::CommandPlugin;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tracing::error;

pub struct ExamplePlugin;

#[async_trait]
impl CommandPlugin for ExamplePlugin {
    async fn process(&self, value: String, sender: Arc<Sender<String>>) -> String {
        let re = regex::Regex::new(
            r#"^\[(?P<time>\d{2}:\d{2}:\d{2})] \[(?P<thread>[^/]+)/(?P<level>[^]]+)]: <(?P<user>[^>]+)> (?P<message>.+)$"#
        ).unwrap();
        if let Some(v) = re.captures(&*value) {
            if let Some(command) = v.name("message").unwrap().as_str().strip_prefix('.') {
                let msg = format!(
                    "tellraw {} {}\n",
                    v.name("user").unwrap().as_str(),
                    json!({
                      "text": format!("Unknown command: {}",command),
                      "color": "red"
                    })
                    .to_string(),
                );

                match sender.send(msg).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("{e}")
                    }
                }
            }
        }
        value
    }
}
