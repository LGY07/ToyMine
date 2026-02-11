pub mod base;
pub mod plugin;
pub mod runner;
pub mod runtime;
pub mod update;

use colored::Colorize;
use erased_serde::__private::serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::cmp::Ordering::{Equal, Greater, Less};
use std::fmt::{Display, Formatter};

use crate::core::mc_server::base::McServer;

pub struct VersionLoader {
    pub versions: Vec<Box<dyn McServer>>,
}

impl VersionLoader {
    pub fn new() -> Self {
        VersionLoader { versions: vec![] }
    }
    pub fn register(&mut self, version: Box<dyn McServer>) {
        self.versions.push(version);
    }
}

/// 更新渠道
#[derive(PartialEq, Clone)]
pub enum McChannel {
    Release(u8, u8, u8),
    Snapshot(String),
}

/// 服务端类型
/// 例如 Java(Vanilla) Java(Paper) Bedrock(BDS)
#[derive(serde::Serialize, Deserialize, PartialEq, Clone)]
pub enum McType {
    Java(String),
    Bedrock(String),
}

/// 版本信息
#[derive(serde::Serialize, Deserialize, PartialEq, Clone)]
pub struct McVersion {
    pub server_type: McType,
    pub channel: McChannel,
}

/// 版本信息可对比
impl PartialOrd for McVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // 完全相同
        if self == other {
            return Some(Equal);
        }

        // Java 与 Bedrock 比较
        if matches!(self.server_type, McType::Java(_))
            && matches!(other.server_type, McType::Bedrock(_))
        {
            return None;
        }
        if matches!(self.server_type, McType::Bedrock(_))
            && matches!(other.server_type, McType::Java(_))
        {
            return None;
        }

        // Release 版本比较
        if let McChannel::Release(major, minor, patch) = self.channel {
            if let McChannel::Release(other_major, other_minor, other_patch) = other.channel {
                if major < other_major {
                    return Some(Less);
                }
                if major > other_major {
                    return Some(Greater);
                }

                if minor < other_minor {
                    return Some(Less);
                }
                if minor > other_minor {
                    return Some(Greater);
                }

                if patch < other_patch {
                    return Some(Less);
                }
                if patch > other_patch {
                    return Some(Greater);
                }

                return Some(Equal);
            }
        }

        // 快照版本不可对比
        None
    }
}

/// 简单打印版本信息
impl Display for McVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let McType::Java(v) = &self.server_type {
            writeln!(f, "Minecraft Java Edition Server")?;
            write!(f, "{}", v.bright_green())?;
        }
        if let McType::Bedrock(v) = &self.server_type {
            writeln!(f, "Minecraft Bedrock Edition Server")?;
            write!(f, "{}", v.bright_green())?;
        }
        match &self.channel {
            McChannel::Release(major, minor, patch) => {
                writeln!(f, "v{}.{}.{}", major, minor, patch)
            }
            McChannel::Snapshot(version) => {
                writeln!(f, "Snapshot {}", version)
            }
        }?;
        Ok(())
    }
}

impl Serialize for McChannel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            McChannel::Release(major, minor, patch) => {
                serializer.serialize_str(format!("{}.{}.{}", major, minor, patch).as_str())
            }
            McChannel::Snapshot(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for McChannel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        let parts = s
            .split('.')
            .map(|x| x.parse())
            .collect::<Result<Vec<u8>, _>>();

        match parts {
            Ok(v) => {
                if v.len() == 3 {
                    Ok(Self::Release(v[0], v[1], v[2]))
                } else {
                    Ok(Self::Snapshot(s))
                }
            }
            Err(_) => Ok(Self::Snapshot(s)),
        }
    }
}
