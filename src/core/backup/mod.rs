mod handler;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::sleep;

use anyhow::Result;
use chrono::Utc;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::{JoinSet, spawn_blocking};
use tokio::time::sleep_until;
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::core::backup::handler::run_backup;

#[derive(Serialize, Deserialize, Clone)]
pub struct BackupCfg {
    option: BackupOption,
    path: BackupPath,
}

#[derive(Serialize, Deserialize, Clone)]
struct BackupOption {
    /// 启动时备份
    on_start: bool,
    /// 停止时备份
    on_stop: bool,
    /// 更新时备份
    on_update: bool,
    /// 运行期 cron 备份
    cron: Option<Schedule>,
}

#[derive(Serialize, Deserialize, Clone)]
struct BackupPath {
    /// 备份来源
    source: Vec<PathBuf>,
    /// 备份位置
    repository: PathBuf,
}

impl Default for BackupCfg {
    fn default() -> Self {
        BackupCfg {
            option: BackupOption {
                on_start: false,
                on_stop: true,
                on_update: false,
                cron: None,
            },
            path: BackupPath {
                source: vec![PathBuf::from("world")],
                repository: PathBuf::from(".pacmine").join("backup"),
            },
        }
    }
}

impl BackupCfg {
    /// 单实例的备份线程
    async fn backup_thread(&self, t: CancellationToken) {
        while !t.is_cancelled() {
            if let Some(s) = &self.option.cron {
                sleep_until(next_time(s)).await;
                // 运行备份
                let cfg = self.path.clone();
                let _ = spawn_blocking(move || run_backup(cfg, "Cron Schedule".to_string()));
            } else {
                info!("Cron Expression is not set, backup function is disabled.");
            }
        }
    }
}

/// 多实例的备份管理器
struct BackupManager {
    schedule: Mutex<BTreeMap<Instant, BackupCfg>>,
    join_set: Arc<Mutex<JoinSet<Result<()>>>>,
}

impl BackupManager {
    fn new(join_set: Arc<Mutex<JoinSet<Result<()>>>>) -> Self {
        BackupManager {
            schedule: Mutex::new(BTreeMap::new()),
            join_set,
        }
    }
    async fn register(&self, cfg: BackupCfg) {
        if let Some(s) = &cfg.option.cron {
            self.schedule.lock().await.insert(next_time(s), cfg);
            debug!("Backup plan has been registered.")
        }
    }
    async fn backup_thread(self, t: CancellationToken) {
        while !t.is_cancelled() {
            match self.schedule.lock().await.first_key_value() {
                None => {
                    // 无计划状态
                    sleep(Duration::from_secs(1));
                    continue;
                }
                Some((k, v)) => {
                    if *k > Instant::now() {
                        sleep_until(*k).await;
                    }

                    // 完成一次备份
                    let cfg = v.path.clone();
                    let _ = spawn_blocking(move || run_backup(cfg, "Cron Schedule".to_string()));

                    // 计划下一次备份
                    let cfg = self.schedule.lock().await.remove(k).unwrap();
                    self.schedule
                        .lock()
                        .await
                        .insert(next_time(cfg.option.cron.as_ref().unwrap()), cfg);
                }
            };
        }
    }
}

/// 计算下一次运行的时间
fn next_time(schedule: &Schedule) -> Instant {
    let now = Utc::now();
    let dur = schedule
        .upcoming(Utc)
        .next()
        .unwrap()
        .signed_duration_since(now);
    let dur_std = Duration::from_millis(dur.num_milliseconds() as u64);
    Instant::now() + dur_std
}
