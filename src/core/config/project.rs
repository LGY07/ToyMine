use crate::core::backup::BackupCfg;
use crate::core::mc_server::McType;
use crate::core::mc_server::McType::Java;
use crate::core::mc_server::base::McServer;
use anyhow::Result;
use erased_serde::Deserializer;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;
use tokio::io::AsyncReadExt;
use toml::Value;

#[derive(Serialize, Deserialize)]
pub struct McServerConfig {
    /// 项目基本信息
    pub project: ProjectCfg,
    /// 版本内部配置
    /// 惰性更新，此处储存的值不保证最新
    /// 仅在读取时是最新的
    /// 导出配置时被最新的值替换（但不更新）
    pub(crate) version: Value,
    /// 备份配置
    pub backup: BackupCfg,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProjectCfg {
    /// 名称
    name: String,
    /// 描述
    description: String,
    /// 创建时间
    creation_date: chrono::DateTime<chrono::Local>,
    /// 服务端类型
    mc_type: McType,
}

impl Default for ProjectCfg {
    fn default() -> Self {
        Self {
            name: "Example".to_string(),
            description: "A PacMine project".to_string(),
            creation_date: chrono::Local::now(),
            mc_type: Java("vanilla".to_string()),
        }
    }
}

impl FromStr for McServerConfig {
    type Err = toml::de::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(toml::from_str(s)?)
    }
}

impl McServerConfig {
    pub async fn current() -> Option<Self> {
        match Self::open(Path::new("ToyMine.toml")).await {
            Ok(v) => Some(v),
            Err(_) => None,
        }
    }
    pub fn new() -> Self {
        Self {
            project: Default::default(),
            version: Value::String("".to_string()),
            backup: Default::default(),
        }
    }
    pub async fn open(path: &Path) -> Result<Self> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut string = String::new();
        file.read_to_string(&mut string).await?;
        Ok(toml::from_str(string.as_str())?)
    }
    pub fn to_string(&self, version: &dyn McServer) -> Result<String> {
        Ok(toml::to_string(&Self {
            project: self.project.clone(),
            version: Value::try_from(version.to_config()?)?,
            backup: self.backup.clone(),
        })?)
    }
    pub fn load_from_str(config: &str, version: &mut dyn McServer) -> Result<Self> {
        let cfg = toml::from_str::<Self>(config)?;
        let version_cfg = toml::to_string(&cfg.version)?;
        let de = toml::Deserializer::parse(version_cfg.as_str())?;
        version.load_config(&mut <dyn Deserializer>::erase(de))?;
        Ok(cfg)
    }
}
