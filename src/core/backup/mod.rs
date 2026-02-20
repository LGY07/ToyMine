mod handler;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::sleep;

use tokio::sync::Mutex;
use tokio::task::spawn_blocking;
use tokio::time::sleep_until;
use tokio::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::core::backup::handler::BackupRepo;

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
                repository: PathBuf::from(".toymine").join("backup"),
            },
        }
    }
}

/// 多实例的备份管理器
pub struct BackupManager {
    schedule: Mutex<VecDeque<BackupTask>>,
}

struct BackupTask {
    id: usize,
    next: Instant,
    schedule: Schedule,
    repo: Arc<BackupRepo>,
}

impl BackupManager {
    pub fn new() -> Self {
        BackupManager {
            schedule: Mutex::new(VecDeque::new()),
        }
    }
    pub async fn register(&self, cfg: BackupCfg, id: usize, cache_dir: &Path) {
        let repo = Arc::new(
            BackupRepo::init(&cfg.path.repository, &cache_dir, cfg.path.source.clone())
                .expect("Failed to init backup repo"),
        );

        if let Some(s) = &cfg.option.cron {
            let task = BackupTask {
                id,
                next: BackupManager::next_time(s),
                schedule: s.clone(),
                repo,
            };
            self.schedule.lock().await.push_back(task);
            debug!("Backup plan has been registered.")
        }
    }
    pub async fn remove(&self, id: usize) {
        self.schedule.lock().await.retain(|x| x.id != id)
    }
    pub async fn run_now(&self, id: usize) -> Result<()> {
        let repo = Arc::clone(
            &self
                .schedule
                .lock()
                .await
                .iter()
                .find(|x| x.id == id)
                .context("No such task")?
                .repo,
        );
        spawn_blocking(move || repo.snap("Non-Cron schedule")).await??;
        Ok(())
    }
    pub async fn backup_thread(&self, t: CancellationToken) {
        while !t.is_cancelled() {
            match self.schedule.lock().await.pop_front() {
                None => {
                    // 无计划状态
                    sleep(Duration::from_secs(1));
                    continue;
                }
                Some(t) => {
                    if t.next > Instant::now() {
                        sleep_until(t.next).await;
                    }

                    // 完成一次备份
                    let repo = Arc::clone(&t.repo);
                    let _ = spawn_blocking(move || repo.snap("Cron Schedule"));

                    // 计划下一次备份
                    let task = BackupTask {
                        next: BackupManager::next_time(&t.schedule),
                        ..t
                    };
                    self.schedule.lock().await.push_back(task);
                }
            };
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
}
