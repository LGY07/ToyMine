use crate::project_manager::tools::check_java;
pub(crate) use crate::project_manager::tools::{ServerType, VersionType};
use anyhow::Error;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs;
use std::path::{Path, PathBuf};

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
    pub(crate) birthday: chrono::DateTime<chrono::Utc>,
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
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub enum JavaType {
    /// OpenJDK，默认使用 Microsoft 构建
    OpenJDK,
    /// GraalVM JDK
    GraalVM,
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

/// 为 Config 定义方法
impl Config {
    /// 从文件读取 TOML
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Config, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// 将 TOML 写入到文件
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// 创建默认配置
    pub fn default() -> Config {
        Config {
            project: Project {
                name: "MyServer".to_string(),
                server_type: ServerType::Vanilla,
                execute: "server.jar".to_string(),
                version: "latest".to_string(),
                birthday: chrono::Utc::now(),
                version_type: VersionType::Release,
            },
            runtime: Runtime {
                java: Java {
                    mode: JavaMode::Auto,
                    version: 21,
                    edition: JavaType::OpenJDK,
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

/// 为 Config 实现 Display 特征，在打印时输出可读信息
impl Display for Config {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let title = |name: &str| format!("\n{}", name.bold().bright_blue());
        let key = |name: &str| format!("{:<14}", name.bright_yellow());

        writeln!(
            f,
            "{} {}",
            "╭─".bright_black(),
            "Instance Configuration".bold().bright_green()
        )?;

        // === Project ===
        writeln!(f, "{}", title("Project"))?;
        writeln!(f, "  {} {}", key("Name:"), self.project.name)?;
        writeln!(
            f,
            "  {} {:?}",
            key("Server Type:"),
            self.project.server_type
        )?;
        writeln!(f, "  {} {}", key("Version:"), self.project.version)?;
        writeln!(
            f,
            "  {} {:?}",
            key("Version Type:"),
            self.project.version_type
        )?;
        writeln!(f, "  {} {}", key("Executable:"), self.project.execute)?;
        writeln!(
            f,
            "  {} {}",
            key("Created At:"),
            self.project.birthday.format("%Y-%m-%d %H:%M:%S UTC")
        )?;

        // === Runtime ===
        writeln!(f, "{}", title("Runtime → Java"))?;
        writeln!(f, "  {} {:?}", key("Mode:"), self.runtime.java.mode)?;
        writeln!(f, "  {} {:?}", key("Edition:"), self.runtime.java.edition)?;
        writeln!(f, "  {} {}", key("Version:"), self.runtime.java.version)?;
        writeln!(f, "  {} {:?}", key("Custom:"), self.runtime.java.custom)?;
        writeln!(
            f,
            "  {} {:?}",
            key("Arguments:"),
            self.runtime.java.arguments
        )?;
        writeln!(f, "  {} {} MB", key("Xms:"), self.runtime.java.xms)?;
        writeln!(f, "  {} {} MB", key("Xmx:"), self.runtime.java.xmx)?;

        // === Backup ===
        writeln!(f, "{}", title("Backup"))?;
        writeln!(
            f,
            "  {} {}",
            key("Enabled:"),
            if self.backup.enable {
                "true".bright_green()
            } else {
                "false".bright_red()
            }
        )?;
        writeln!(
            f,
            "  {} {}",
            key("World:"),
            if self.backup.world {
                "true".bright_green()
            } else {
                "false".bright_red()
            }
        )?;
        writeln!(
            f,
            "  {} {}",
            key("Other:"),
            if self.backup.other {
                "true".bright_green()
            } else {
                "false".bright_red()
            }
        )?;

        if let Some(ref time) = self.backup.time {
            writeln!(f, "  {}", "[Time Backup]".bright_cyan())?;
            writeln!(f, "    {} {} min", key("Interval:"), time.interval)?;
            writeln!(f, "    {} {:?}", key("Cron:"), time.cron)?;
        }
        if let Some(ref event) = self.backup.event {
            writeln!(f, "  {}", "[Event Backup]".bright_cyan())?;
            writeln!(
                f,
                "    {} {}",
                key("On Start:"),
                if event.start {
                    "true".bright_green()
                } else {
                    "false".bright_red()
                }
            )?;
            writeln!(
                f,
                "    {} {}",
                key("On Stop:"),
                if event.stop {
                    "true".bright_green()
                } else {
                    "false".bright_red()
                }
            )?;
            writeln!(
                f,
                "    {} {}",
                key("On Update:"),
                if event.update {
                    "true".bright_green()
                } else {
                    "false".bright_red()
                }
            )?;
        }

        // === Plugin Manage ===
        writeln!(f, "{}", title("Plugin Manage"))?;
        writeln!(
            f,
            "  {} {}",
            key("Manage:"),
            if self.plugin_manage.manage {
                "true".bright_green()
            } else {
                "false".bright_red()
            }
        )?;

        writeln!(f, "{} {}", "╰─".bright_black(), "End of Config".dimmed())
    }
}

/// 为 JavaType 实现 Display 特征，用于文件命名，使用全小写
impl Display for JavaType {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            JavaType::GraalVM => write!(f, "graalvm"),
            JavaType::OpenJDK => write!(f, "openjdk"),
            JavaType::Custom => write!(f, "custom"),
        }
    }
}

impl Java {
    pub fn to_binary(&self) -> Result<PathBuf, Error> {
        let runtime_path = PathBuf::from(format!(
            ".nmsl/runtime/java-{}-{}-{}-{}",
            &self.version,
            &self.edition,
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        if check_java(&runtime_path) {
            Ok(PathBuf::from(runtime_path))
        } else {
            Err(Error::msg("Java cannot be found"))
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
