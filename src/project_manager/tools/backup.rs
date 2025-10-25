use rustic_backend::BackendOptions;
use rustic_core::{BackupOptions, CheckOptions, ConfigOptions, KeyOptions, PathList, Repository, RepositoryOptions, SnapshotOptions};
use std::error::Error;

const CACHE_DIR:&str = ".nmsl/cache/backup";
const PASSWORD:&str = "";

fn init_repository(path:&str) -> Result<(), Box<dyn Error>> {
    // Initialize Backends
    let backends = BackendOptions::default()
        .repository(path)
        .to_backends()?;

    // Init repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(CACHE_DIR).password(PASSWORD);
    let key_opts = KeyOptions::default();
    let config_opts = ConfigOptions::default();
    let _repo = Repository::new(&repo_opts, &backends)?.init(&key_opts, &config_opts)?;

    // -> use _repo for any operation on an open repository
    Ok(())
}


fn check_repository(path:&str) -> Result<(), Box<dyn Error>> {
    // Initialize Backends
    let backends = BackendOptions::default()
        .repository(path)
        .to_backends()?;

    // Open repository
    let repo_opts = RepositoryOptions::default().cache_dir(CACHE_DIR).password(PASSWORD);
    let repo = Repository::new(&repo_opts, &backends)?.open()?;

    // Check repository with standard options but omitting cache checks
    let opts = CheckOptions::default().trust_cache(true);
    repo.check(opts)?;
    Ok(())
}

pub fn backup(paths:Vec<String>,tag:&str,backup_location:&str) -> Result<(), Box<dyn Error>> {
    // Initialize Backends
    let backends = BackendOptions::default()
        .repository(backup_location)
        .to_backends()?;

    // Initialize repository
    let _ = match check_repository(&backup_location) {
        Ok(_) => init_repository(&backup_location),
        Err(_) => Ok(())
    };

    // Open repository
    let repo_opts = RepositoryOptions::default()
        .cache_dir(CACHE_DIR).password(PASSWORD);

    let repo = Repository::new(&repo_opts, &backends)?
        .open()?
        .to_indexed_ids()?;

    let backup_opts = BackupOptions::default();
    let source = PathList::from_iter(paths).sanitize()?;
    let snap = SnapshotOptions::default()
        .add_tags(tag)?
        .to_snapshot()?;

    // Create snapshot
    let snap = repo.backup(&backup_opts, &source, snap)?;

    println!("successfully created snapshot:\n{snap:#?}");
    Ok(())
}