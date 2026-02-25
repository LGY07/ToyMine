use crate::command::CommandLoader;
use crate::core::config::project::McServerConfig;
use crate::core::mc_server::runner::{Runner, sync_channel_stdio};
use crate::versions::VersionManager;
use crate::{TASK_MANAGER, command};
use anyhow::Result;
use anyhow::anyhow;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::select;
use tokio::signal::ctrl_c;
use tracing::info;

pub async fn start(generate: bool, _detach: bool, _attach: bool) -> Result<()> {
    // 尝试从当前目录获取配置文件
    let cfg = McServerConfig::current().await;
    // 尝试从当前目录发现服务端
    let server = match cfg {
        None => {
            info!("The configuration file was not found. Attempting to locate the server file.");
            VersionManager::detect_server()?
        }
        Some(c) => VersionManager::from_cfg(&c),
    };
    let server = match server {
        None => return Err(anyhow!("MC Server Not Found")),
        Some(v) => v,
    };
    // 生成运行脚本
    if generate {
        let s = server.gen_script()?;
        let save_path = Path::new(match std::env::consts::OS {
            "windows" => "start.bat",
            _ => "start",
        });
        let mut file = tokio::fs::File::create(save_path).await?;
        file.write_all(s.as_ref()).await?;
        file.flush().await?;

        return Ok(());
    }
    let server = Arc::new(Runner::spawn_server(server.as_ref()).await?);

    let mut command_loader = CommandLoader::new();
    command_loader.register(server.id, vec![Box::new(command::raw::ExamplePlugin)])?;
    let server_clone = Arc::clone(&server);

    TASK_MANAGER
        .spawn_with_cancel(async move |t| {
            sync_channel_stdio(
                server_clone.input.clone(),
                command_loader.load(server_clone.clone().as_ref()).await?,
                t,
            )
            .await?;
            Ok(())
        })
        .await?;

    select! {
        e = server.wait() => {
            info!("Exit: {}",e?)
        }
        _ = ctrl_c() => {
            server.kill_with_timeout(std::time::Duration::from_secs(10)).await?;
            info!("Stop: {}",server.wait().await?)
        }
    }

    TASK_MANAGER.shutdown().await;

    Ok(())
}
