use crate::core::backup::BackupPath;
use anyhow::{Context, Result};
use rustic_backend::BackendOptions;
use rustic_core::{
    BackupOptions, CheckOptions, ConfigOptions, KeyOptions, LocalDestination, LsOptions, PathList,
    Repository, RepositoryOptions, RestoreOptions, SnapshotOptions,
};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::debug;

// 不加密的备份
const PASSWORD: &str = "";

pub fn run_backup(cfg: BackupPath, tag: String) -> Result<()> {
    let cache_dir = PathBuf::from_str(".pacmine")?.join("cache");

    // 初始化仓库
    if backup_check_repo(&cfg.repository, &cache_dir).is_err() {
        backup_init_repo(&cfg.repository, &cache_dir)
            .context("Failed to init backup repository")?;
    }

    // 运行备份
    backup_new_snap(
        cfg.repository.as_path(),
        tag.as_str(),
        &cfg.source,
        cache_dir.as_path(),
    )
    .context("Failed to backup")?;
    Ok(())
}

fn backup_init_repo(path: &Path, cache: &Path) -> Result<()> {
    debug!("backup_init_repo : Initialize backup repository");

    // Initialize Backends
    let backends = BackendOptions::default()
        .repository(path.to_str().expect("Incorrect path"))
        .to_backends()?;

    // Init repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(cache.join("backup"))
        .password(PASSWORD);
    let key_opts = KeyOptions::default();
    let config_opts = ConfigOptions::default();
    let _repo = Repository::new(&repo_opts, &backends)?.init(&key_opts, &config_opts)?;

    // -> use _repo for any operation on an open repository
    Ok(())
}

/// 创建快照
fn backup_new_snap(path: &Path, tag: &str, source: &Vec<PathBuf>, cache: &Path) -> Result<()> {
    debug!("backup_new_snap : Create new snapshot");

    // Initialize Backends
    let backends = BackendOptions::default()
        .repository(path.to_str().expect("Incorrect path"))
        .to_backends()?;

    // Open repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(cache.join("backup"))
        .password(PASSWORD);

    let repo = Repository::new(&repo_opts, &backends)?
        .open()?
        .to_indexed_ids()?;

    let backup_opts = BackupOptions::default();
    let source = PathList::from_iter(source).sanitize()?;
    let snap = SnapshotOptions::default().add_tags(tag)?.to_snapshot()?;

    // Create snapshot
    let snap = repo.backup(&backup_opts, &source, snap)?;

    println!("successfully created snapshot:\n{snap:#?}");
    Ok(())
}

/// 检查仓库
fn backup_check_repo(path: &Path, cache: &Path) -> Result<()> {
    debug!("backup_check_repo : Check backup repository");

    // Initialize Backends
    let backends = BackendOptions::default()
        .repository(path.to_str().expect("Incorrect path"))
        .to_backends()?;

    // Open repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(cache.join("backup"))
        .password(PASSWORD);
    let repo = Repository::new(&repo_opts, &backends)?.open()?;

    // Check repository with standard options but omitting cache checks
    let opts = CheckOptions::default().trust_cache(true);
    repo.check(opts)?;
    Ok(())
}

/// 恢复快照
fn backup_restore_snap(path: &Path, snap: &str, destination: &str, cache: &Path) -> Result<()> {
    debug!("backup_restore_snap : Restore a snapshot");

    // Initialize Backends
    let backends = BackendOptions::default()
        .repository(path.to_str().expect("Incorrect path"))
        .to_backends()?;

    // Open repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(cache.join("backup"))
        .password(PASSWORD);
    let repo = Repository::new(&repo_opts, &backends)?
        .open()?
        .to_indexed()?;

    // use latest snapshot without filtering snapshots
    let node = repo.node_from_snapshot_path(snap, |_| true)?;

    // use list of the snapshot contents using no additional filtering
    let streamer_opts = LsOptions::default();
    let ls = repo.ls(&node, &streamer_opts)?;

    let create = true; // create destination dir, if it doesn't exist
    let dest = LocalDestination::new(destination, create, !node.is_dir())?;

    let opts = RestoreOptions::default();
    let dry_run = false;
    // create restore infos. Note: this also already creates needed dirs in the destination
    let restore_infos = repo.prepare_restore(&opts, ls.clone(), &dest, dry_run)?;

    repo.restore(restore_infos, &opts, ls, &dest)?;
    Ok(())
}
