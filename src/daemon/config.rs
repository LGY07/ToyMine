use anyhow::Error;
use home::home_dir;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fs;
use std::net::SocketAddr as TcpAddr;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Daemon 配置文件
#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub(crate) api: Api,
    pub(crate) storage: Storage,
    pub(crate) security: Security,
    pub(crate) token: Vec<Token>,
}

/// API 选项
#[derive(Deserialize, Serialize, Clone)]
pub struct Api {
    /// 监听位置
    pub(crate) listen: ApiAddr,
}
/// 解析的 API
#[derive(Clone)]
pub enum ApiAddr {
    /// /path/to/api.sock
    UnixSocket(PathBuf),
    /// IP:Port
    Tcp(TcpAddr),
}

/// 自定义的反序列化方法
impl<'de> Deserialize<'de> for ApiAddr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // 先读取成字符串
        let s = String::deserialize(deserializer)?;
        // 支持前缀 "unix://" 或 "unix:/" 表示 Unix Socket
        if let Some(path) = s
            .strip_prefix("unix://")
            .or_else(|| s.strip_prefix("unix:/"))
        {
            return Ok(ApiAddr::UnixSocket(PathBuf::from(path)));
        }
        // 尝试解析为 TCP 地址
        if let Ok(addr) = s.parse::<TcpAddr>() {
            return Ok(ApiAddr::Tcp(addr));
        }
        // 否则当作 Unix Socket
        if !s.is_empty() {
            return Ok(ApiAddr::UnixSocket(PathBuf::from(s)));
        }
        Err(serde::de::Error::custom(format!(
            "invalid API address: {} (expected unix://path or ip:port)",
            s
        )))
    }
}
/// 自定义序列化
impl Serialize for ApiAddr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ApiAddr::UnixSocket(path) => {
                // 序列化为 "unix://path/to.sock"
                let s = format!("unix:/{}", path.display());
                serializer.serialize_str(&s)
            }
            ApiAddr::Tcp(addr) => {
                // 直接序列化为 "127.0.0.1:8080"
                serializer.serialize_str(&addr.to_string())
            }
        }
    }
}

/// 储存选项
#[derive(Deserialize, Serialize, Clone)]
pub struct Storage {
    /// 工作目录
    pub(crate) work_dir: PathBuf,
    /// 节约空间选项
    pub(crate) save_space: SaveSpace,
}
#[derive(Deserialize, Serialize, PartialEq, Clone)]
pub enum SaveSpace {
    Disable,
    BindRuntime,
    OverlayFS,
}

/// 安全选项
#[derive(Deserialize, Serialize, Clone)]
pub struct Security {
    /// 用户 UID，负数则不支持 POSIX，安全选项无效
    pub(crate) user: isize,
    /// 宽容模式，允许使用非本用户的文件
    pub(crate) permissive: Option<bool>,
    /// Upload 大小限制，单位 MB，缺省或 0 不限
    pub(crate) upload_limit: Option<usize>,
    /// WebSocket 终端 TTL，单位秒，缺省 10s，0 不限(调试用途，不建议生产环境使用)
    pub(crate) websocket_ttl: Option<usize>,
}

/// Token 列表
#[derive(Deserialize, Serialize, Clone)]
pub struct Token {
    /// Bearer Token 值
    pub(crate) value: String,
    /// Token 过期时间
    pub(crate) expiration: Option<chrono::DateTime<chrono::Utc>>,
}

impl Config {
    /// 从文件读取 TOML
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let content = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// 将 TOML 写入到文件
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    // 检查配置在当前平台是否有效
    pub fn check_config(&self) -> Result<(), Error> {
        // 对非 *nix 的平台进行兼容性检查
        #[cfg(not(target_family = "unix"))]
        {
            // 非 *nix 无法使用安全性功能
            if self.security.user > -1 {
                return Err(Error::msg(
                    "The current platform does not support security features, make sure the configuration is for this platform.",
                ));
            }
            // 非 *nix 无法使用 Unix Socket (Tokio 暂时不支持 Windows 的 Unix Socket)
            if let ApiAddr::UnixSocket(_) = &self.api.listen {
                return Err(Error::msg(
                    "The current platform does not support UnixSocket, make sure the configuration is for that platform.",
                ));
            }
        }

        // 非 linux 无法使用 mount --bind 和 OverlayFS
        #[cfg(not(target_os = "linux"))]
        {
            if self.storage.save_space.ne(&SaveSpace::Disable) {
                return Err(Error::msg(
                    "The space saving feature is not supported on this platform, make sure the configuration is for this platform",
                ));
            }
        }

        Ok(())
    }

    /// 获取用户 UID
    fn getuser() -> isize {
        // 获取用户 UID，仅 *nix 系统有效
        #[cfg(target_family = "unix")]
        {
            use nix::unistd::getuid;
            return getuid().as_raw() as isize;
        }
        // 在非 *nix 禁用安全功能
        #[cfg(not(target_family = "unix"))]
        {
            -1
        }
    }

    /// 设置监听地址
    fn set_listen(work_dir: &str) -> ApiAddr {
        // *nix 默认使用 $HOME/.pacmine/api.sock
        #[cfg(target_family = "unix")]
        return ApiAddr::UnixSocket(PathBuf::from(format!("{}/api.sock", work_dir)));
        // 非 *nix 默认使用 127.0.0.1:8080
        #[cfg(not(target_family = "unix"))]
        ApiAddr::Tcp(TcpAddr::new("127.0.0.1".parse().unwrap(), 8080))
    }
}

/// 为 Config 实现 Default
impl Default for Config {
    /// 创建默认配置
    fn default() -> Config {
        // 获取家目录
        let home = home_dir().expect("Could not get the home directory.");
        let work_dir: PathBuf = home.join(".pacmine");

        // 默认为非 linux 禁用节约空间
        let save_space = SaveSpace::Disable;

        // 默认为 linux 启用节约空间
        #[cfg(target_os = "linux")]
        let save_space = SaveSpace::BindRuntime;

        Config {
            api: Api {
                listen: Self::set_listen(
                    work_dir.to_str().expect("Could not get the home directory"),
                ),
            },
            storage: Storage {
                work_dir,
                save_space,
            },
            security: Security {
                user: Self::getuser(),
                permissive: Some(false),
                upload_limit: Some(2),
                websocket_ttl: Some(10),
            },
            token: vec![Token {
                value: Uuid::new_v4().to_string(),
                expiration: None,
            }],
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Known {
    pub(crate) current_mode: SaveSpace,
    pub(crate) project: Vec<Project>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Project {
    pub(crate) id: usize,
    pub(crate) manual: bool,
    pub(crate) path: PathBuf,
}

impl Known {
    /// 从文件读取 TOML
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let content = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// 将 TOML 写入到文件
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let test_path = Path::new("./target/test_daemon.toml");
        config.to_file(test_path).unwrap();
        let config = Config::from_file(test_path).unwrap();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        println!("{}", toml_str);
    }
}
