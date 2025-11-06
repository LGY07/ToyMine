use anyhow::Error;
use home::home_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr as TcpAddr;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Daemon 配置文件
#[derive(Deserialize, Serialize)]
pub struct Config {
    pub(crate) api: Api,
    pub(crate) storage: Storage,
    pub(crate) security: Security,
    pub(crate) tokens: Vec<Token>,
}

/// API 选项
#[derive(Deserialize, Serialize)]
pub struct Api {
    /// 监听位置
    pub(crate) listen: String,
}
/// 解析的 API
pub enum ApiAddr {
    /// /path/to/api.sock
    UnixSocket(PathBuf),
    /// IP:Port
    Tcp(TcpAddr),
}

/// 储存选项
#[derive(Deserialize, Serialize)]
pub struct Storage {
    /// 工作目录
    pub(crate) work_dir: String,
    /// 节约空间选项
    pub(crate) save_space: SaveSpace,
}
#[derive(Deserialize, Serialize, PartialEq)]
pub enum SaveSpace {
    Disable,
    BindRuntime,
    OverlayFS,
}

/// 安全选项
#[derive(Deserialize, Serialize)]
pub struct Security {
    /// 用户 UID，负数则不支持 POSIX，安全选项无效
    pub(crate) user: isize,
    /// 宽容模式，允许使用非本用户的文件
    pub(crate) permissive: Option<bool>,
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
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Config, Error> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
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
        // 检查监听地址是否正确
        let api_addr = self.api.parse_url()?;

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
            if let ApiAddr::UnixSocket(_) = api_addr {
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
    fn set_listen(work_dir: &str) -> String {
        // *nix 默认使用 $HOME/.pacmine/api.sock
        #[cfg(target_family = "unix")]
        return format!("{}/api.sock", work_dir).to_string();
        // 非 *nix 默认使用 127.0.0.1:8080
        #[cfg(not(target_family = "unix"))]
        TcpAddr::new("127.0.0.1".parse().unwrap(), 8080).to_string()
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
                work_dir: work_dir
                    .to_str()
                    .expect("Could not get the home directory")
                    .to_string(),
                save_space,
            },
            security: Security {
                user: Self::getuser(),
                permissive: Some(false),
            },
            tokens: vec![Token {
                value: Uuid::new_v4().to_string(),
                expiration: None,
            }],
        }
    }
}

impl Api {
    /// 解析 API 监听地址
    pub fn parse_url(&self) -> Result<ApiAddr, Error> {
        // 尝试解析成 TCP
        if let Ok(addr) = self.listen.parse::<TcpAddr>() {
            return Ok(ApiAddr::Tcp(addr));
        }

        // 否则当作 Unix Socket
        if !self.listen.is_empty() {
            return Ok(ApiAddr::UnixSocket(PathBuf::from(self.listen.clone())));
        }

        Err(Error::msg(format!(
            "Invalid socket address: {}",
            self.listen
        )))
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
