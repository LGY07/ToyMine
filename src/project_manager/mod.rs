pub(crate) mod config;
pub(crate) mod create;
mod info;
pub(crate) mod run;
pub mod tools;

pub use config::Config;
pub use create::create_project;
pub use info::{get_info, print_info};
pub use run::{pre_run, start_server};

/// 配置文件
pub const CONFIG_FILE: &str = "PacMine.toml";
/// 工作目录
pub const WORK_DIR: &str = ".pacmine";
/// 缓存目录
pub const CACHE_DIR: &str = ".pacmine/cache";
/// 备份目录
pub const BACKUP_DIR: &str = ".pacmine/backup";
/// 运行环境目录
pub const RUNTIME_DIR: &str = ".pacmine/runtime";
/// 日志目录
pub const LOG_DIR: &str = ".pacmine/log";

// Paper 类服务端 V2 版本 API 的 URL
/// Paper API
const PAPER_PROJECT_API: &str = "https://api.papermc.io/v2/projects/paper";
/// Folia API
const FOLIA_PROJECT_API: &str = "https://api.papermc.io/v2/projects/folia";
/// Leaves API
const LEAVES_PROJECT_API: &str = "https://api.leavesmc.org/v2/projects/leaves";
/// Purpur API
const PURPUR_PROJECT_API: &str = "https://api.purpurmc.org/v2/purpur/";

/// 默认下载线程
const DEFAULT_DOWNLOAD_THREAD: usize = 5;

/// 下载每个分块最大重试次数
const MAX_RETRIES: usize = 3;

/// 默认的备份密码，无意义，所以用空字符串
const PASSWORD: &str = "";

/// MOJANG 下载 API 的 URL
const VERSION_API_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
