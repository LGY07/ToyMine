pub(crate) use crate::project_manager::tools::{ServerType, VersionType};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 实例配置文件
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// 实例基本信息
    pub(crate) project: Project,
    /// 运行环境配置
    pub(crate) runtime: Runtime,
    /// 备份配置
    pub(crate) backup: Backup,
    /// 插件管理配置
    pub(crate) plugin_manage: PluginManage,
}

/// 实例的基本信息
#[derive(Debug, Deserialize, Serialize)]
pub struct Project {
    /// 服务器名称
    pub(crate) name: String,
    /// 服务端类型
    pub(crate) server_type: ServerType,
    /// 服务端版本
    pub(crate) version: String,
    /// 服务端版本类型
    pub(crate) version_type: VersionType,
    /// 服务端可执行文件路径
    pub(crate) execute: String,
    /// 服务器创建日期
    pub(crate) birthday: String,
}

/// 运行环境管理
#[derive(Debug, Deserialize, Serialize)]
pub struct Runtime {
    /// Java 运行时
    pub(crate) java: Java,
}

/// Java 环境配置
#[derive(Debug, Deserialize, Serialize)]
pub struct Java {
    /// Java 环境管理方式
    pub(crate) mode: JavaMode,
    /// Java 环境类型，仅当 `mode` 为 `manual` 时生效
    pub(crate) edition: JavaType,
    /// Java 版本，`edition` 生效且不为 `custom` 时生效
    pub(crate) version: usize,
    /// 自定义 Java 的 `JAVA_HOME`，`edition` 生效且为 `custom` 时生效
    #[serde(default)]
    pub(crate) custom: String,
    /// 自定义 Java 额外参数列表
    #[serde(default)]
    pub(crate) arguments: Vec<String>,
    /// 自定义 JVM 堆的初始大小，`0` 为不设置
    pub(crate) xms: usize,
    /// 自定义 JVM 堆的最大大小，`0` 为不限制
    pub(crate) xmx: usize,
}

/// Java环境管理模式
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JavaMode {
    /// 自动根据游戏文件管理
    Auto,
    /// 手动指定版本，或者自定义 Java 环境
    Manual,
}

/// Java 类型
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum JavaType {
    /// GraalVM JDK
    GraalVM,
    /// Oracle JDK
    OracleJDK,
    /// OpenJDK，默认使用 Microsoft 构建
    OpenJDK,
    /// 自定义的 Java 环境，不支持自动管理
    Custom,
}

/// 备份的基本设置
#[derive(Debug, Deserialize, Serialize)]
pub struct Backup {
    /// 备份功能总开关
    pub(crate) enable: bool,
    /// 备份地图开关
    pub(crate) world: bool,
    /// 备份地图以外内容开关，包含游戏配置，服务端文件，插件等
    pub(crate) other: bool,
    /// 根据时间备份
    #[serde(default)]
    pub(crate) time: Option<Time>,
    /// 根据事件备份
    #[serde(default)]
    pub(crate) event: Option<Event>,
}

/// 根据时间备份的选项
#[derive(Debug, Deserialize, Serialize)]
pub struct Time {
    /// 根据运行期间时间间隔备份，`0` 为关闭
    pub(crate) interval: usize,
    /// 根据运行期间的时间点备份，空字符串为关闭，格式为 `Cron` 表达式
    #[serde(default)]
    pub(crate) cron: String,
}

/// 根据事件备份的选项
#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    /// 启用则在服务端启动前运行一次备份
    pub(crate) start: bool,
    /// 启用则在服务端停止后运行一次备份
    pub(crate) stop: bool,
    /// 启用则在更新运行一次备份
    pub(crate) update: bool,
}

/// 插件管理功能
#[derive(Debug, Deserialize, Serialize)]
pub struct PluginManage {
    /// 插件管理总开关
    pub(crate) manage: bool,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Config, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn default() -> Config {
        Config {
            project: Project {
                name: "MyServer".to_string(),
                server_type: ServerType::Vanilla,
                execute: "server.jar".to_string(),
                version: "latest".to_string(),
                birthday: chrono::Utc::now().to_rfc3339(),
                version_type: VersionType::Release,
            },
            runtime: Runtime {
                java: Java {
                    mode: JavaMode::Auto,
                    version: 21,
                    edition: JavaType::GraalVM,
                    custom: String::new(),
                    arguments: vec![],
                    xms: 0,
                    xmx: 0,
                },
            },
            backup: Backup {
                enable: true,
                world: true,
                other: false,
                time: Some(Time {
                    interval: 0,
                    cron: "".to_string(),
                }),
                event: Some(Event {
                    start: false,
                    stop: true,
                    update: true,
                }),
            },
            plugin_manage: PluginManage { manage: true },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let test_path = Path::new("./target/test.toml");
        config.to_file(test_path).unwrap();
        let config = Config::from_file(test_path).unwrap();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        println!("{}", toml_str);
    }
}
