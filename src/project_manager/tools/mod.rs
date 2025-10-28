// 暂时为 pub(crate)
pub(crate) mod backup;
mod core_manager;
mod downloader;
mod file_parser;
mod java_manager;
mod version_parser;

const DOWNLOAD_CACHE_DIR: &str = ".nmsl/cache/download";
const DEFAULT_DOWNLOAD_THREAD: usize = 5;

pub use backup::{backup_check_repo, backup_init_repo, backup_new_snap, backup_restore_snap};
pub use core_manager::{install_bds, install_je};
pub use downloader::download_files;
pub use file_parser::{JarInfo, analyze_jar, analyze_je_game, get_mime_type};
pub use java_manager::{check_java, prepare_java};
pub use version_parser::{
    LatestVersions, ManifestVersion, ServerType, VersionInfo, VersionManifest, VersionType,
};
