use anyhow::Error;
use log::debug;
use rustic_backend::BackendOptions;
use rustic_core::{
    BackupOptions, CheckOptions, ConfigOptions, KeyOptions, LocalDestination, LsOptions, PathList,
    Repository, RepositoryOptions, RestoreOptions, SnapshotOptions,
};
use std::path::PathBuf;

/// 默认的缓存目录
const CACHE_DIR: &str = ".nmsl/cache/backup";
/// 默认的备份密码，无意义，所以用空字符串
const PASSWORD: &str = "";

/// 初始化备份仓库
pub fn backup_init_repo(path: &str) -> Result<(), Error> {
    debug!("backup_init_repo : Initialize backup repository");

    // Initialize Backends
    let backends = BackendOptions::default().repository(path).to_backends()?;

    // Init repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(CACHE_DIR)
        .password(PASSWORD);
    let key_opts = KeyOptions::default();
    let config_opts = ConfigOptions::default();
    let _repo = Repository::new(&repo_opts, &backends)?.init(&key_opts, &config_opts)?;

    // -> use _repo for any operation on an open repository
    Ok(())
}

/// 创建快照
pub fn backup_new_snap(path: &str, tag: &str, source: Vec<PathBuf>) -> Result<(), Error> {
    debug!("backup_new_snap : Create new snapshot");

    // Initialize Backends
    let backends = BackendOptions::default().repository(path).to_backends()?;

    // Open repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(CACHE_DIR)
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
pub fn backup_check_repo(path: &str) -> Result<(), Error> {
    debug!("backup_check_repo : Check backup repository");

    // Initialize Backends
    let backends = BackendOptions::default().repository(path).to_backends()?;

    // Open repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(CACHE_DIR)
        .password(PASSWORD);
    let repo = Repository::new(&repo_opts, &backends)?.open()?;

    // Check repository with standard options but omitting cache checks
    let opts = CheckOptions::default().trust_cache(true);
    repo.check(opts)?;
    Ok(())
}

/// 恢复快照
pub fn backup_restore_snap(path: &str, snap: &str, destination: &str) -> Result<(), Error> {
    debug!("backup_restore_snap : Restore a snapshot");

    // Initialize Backends
    let backends = BackendOptions::default().repository(path).to_backends()?;

    // Open repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(CACHE_DIR)
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
