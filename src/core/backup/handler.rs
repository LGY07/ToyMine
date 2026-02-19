use anyhow::Result;
use rustic_backend::BackendOptions;
use rustic_core::{
    BackupOptions, CheckOptions, ConfigOptions, CredentialOptions, IndexedFullStatus, KeyOptions,
    LocalDestination, LsOptions, PathList, Repository, RepositoryOptions, RestoreOptions,
    SnapshotOptions,
};
use std::path::{Path, PathBuf};
use tracing::debug;

pub struct BackupRepo {
    source: Vec<PathBuf>,
    repo: Repository<IndexedFullStatus>,
}

impl BackupRepo {
    pub fn init(path: &Path, cache: &Path, source: Vec<PathBuf>) -> Result<Self> {
        debug!("backup_init_repo : Initialize backup repository");

        // Initialize Backends
        let backends = BackendOptions::default()
            .repository(path.to_string_lossy())
            .to_backends()?;

        // Init repository
        let repo_opts = RepositoryOptions::default().cache_dir(cache);
        let key_opts = KeyOptions::default();
        let cred_opts = CredentialOptions::default()
            .credentials()?
            .expect("Credential Options Error");
        let config_opts = ConfigOptions::default();
        let repo = Repository::new(&repo_opts, &backends)?
            .init(&cred_opts, &key_opts, &config_opts)?
            .to_indexed()?;

        Ok(BackupRepo { source, repo })
    }
    fn check(&self) -> Result<&Self> {
        let opts = CheckOptions::default().trust_cache(false);
        self.repo.check(opts)?;
        Ok(self)
    }
    pub fn snap(&self, tag: &str) -> Result<()> {
        self.check()?;

        let backup_opts = BackupOptions::default();
        let source = PathList::from_iter(self.source.iter()).sanitize()?;
        let snap = SnapshotOptions::default().add_tags(tag)?.to_snapshot()?;

        // Create snapshot
        let snap = self.repo.backup(&backup_opts, &source, snap)?;

        println!("successfully created snapshot:\n{snap:#?}");
        Ok(())
    }
    pub fn restore(&self, snap: &str, destination: &str) -> Result<()> {
        self.check()?;

        // use latest snapshot without filtering snapshots
        let node = self.repo.node_from_snapshot_path(snap, |_| true)?;

        // use list of the snapshot contents using no additional filtering
        let streamer_opts = LsOptions::default();
        let ls = self.repo.ls(&node, &streamer_opts)?;

        let dest = LocalDestination::new(destination, true, !node.is_dir())?;

        let opts = RestoreOptions::default();
        // create restore infos. Note: this also already creates needed dirs in the destination
        let restore_infos = self.repo.prepare_restore(&opts, ls.clone(), &dest, false)?;

        self.repo.restore(restore_infos, &opts, ls, &dest)?;
        Ok(())
    }
}
