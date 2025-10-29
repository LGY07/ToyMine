use anyhow::Error;
use lazy_static::lazy_static;
use log::{debug, error};
use regex::Regex;
use reqwest::blocking;
use serde::{Deserialize, Serialize};

const VERSION_API_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

/// 可选的服务端类型
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ServerType {
    /// 官方 Minecraft Java Edition 服务端
    Vanilla,
    /// 官方 Minecraft Bedrock Edition 服务端
    BDS,
    /// PaperMC 服务端
    Paper,
    /// PaperMC 的多线程服务端
    Folia,
    /// LeavesMC 服务端
    Leaves,
    /// PurpurMC 服务端
    Purpur,
    /// 自定义服务端，不支持更新功能以及插件管理
    Other,
}

/// 服务端版本类型
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)] // 添加 Clone 和 PartialEq
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    /// 稳定发行版
    Release,
    /// 快照版本
    Snapshot,
    /// 远古 Beta 版本
    OldBeta,
    /// 远古 Alpha 版本
    OldAlpha,
    /// 无法确定类型
    Unknown,
}

/// 结合了版本名称、类型和服务器类型的版本信息结构体
#[derive(Debug)]
pub struct VersionInfo {
    /// 版本名称/编号 (例如: "1.21.1", "24w10a")
    pub name: String,
    /// 版本类型 (Release, Snapshot, etc.)
    pub version_type: VersionType,
    /// 服务端类型 (Vanilla, Paper, BDS, etc.)
    pub server_type: ServerType,
}

//  Manifest 结构体，用于解析 Mojang 的版本清单
/// 用于解析 Mojang API 中最新的 Release 和 Snapshot 版本 ID
#[derive(Deserialize, Debug)]
pub struct LatestVersions {
    pub(crate) release: String,
    pub(crate) snapshot: String,
}

#[derive(Deserialize, Debug)]
pub struct VersionManifest {
    pub(crate) latest: LatestVersions,
    pub(crate) versions: Vec<ManifestVersion>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ManifestVersion {
    pub(crate) id: String,
    // 'type' 字段在 JSON 中，但 Rust 关键字冲突，所以用 rename
    #[serde(rename = "type")]
    pub(crate) version_type_str: String,
    pub(crate) url: String,
}

/// 版本服务端的JSON
#[derive(Deserialize)]
struct VersionJson {
    downloads: VersionJsonDownloads,
}
/// 版本服务端的JSON-downloads字段
#[derive(Deserialize)]
struct VersionJsonDownloads {
    server: VersionJsonServer,
}
/// 版本服务端的JSON-downloads-server字段
#[derive(Deserialize)]
struct VersionJsonServer {
    sha1: String,
    url: String,
}

/// Manifest 下载函数
impl VersionManifest {
    /// 下载并解析 Mojang 官方的 version_manifest_v2.json
    pub fn fetch() -> Result<Self, Error> {
        const URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

        // 使用阻塞客户端，简单易用
        let response = reqwest::blocking::get(URL)?;
        if !response.status().is_success() {
            return Err(anyhow::Error::msg(format!(
                "Request failed: {}",
                response.status()
            )));
        }

        // 直接将响应体反序列化为结构体
        let manifest: VersionManifest = response.json()?;
        Ok(manifest)
    }
    /// 搜索版本
    pub fn search(&self, name: String) -> Result<ManifestVersion, Error> {
        for i in self.versions.clone() {
            if i.id == name {
                return Ok(i);
            }
        }
        Err(anyhow::Error::msg("Unable to find version"))
    }
}

impl ManifestVersion {
    /// 获取服务端下载链接和 SHA1 值，第一个返回值为 URL 第二个为 SHA1
    pub fn to_download(&self) -> Result<(String, String), Error> {
        let response = reqwest::blocking::get(&self.url)?;
        if !response.status().is_success() {
            return Err(anyhow::Error::msg(format!(
                "Request failed: {}",
                response.status()
            )));
        }
        let server_download = response.json::<VersionJson>()?;
        Ok((
            server_download.downloads.server.url,
            server_download.downloads.server.sha1,
        ))
    }
}

// 核心逻辑函数
impl VersionInfo {
    /// 创建一个新的 VersionInfo 实例
    pub fn new(name: String, version_type: VersionType, server_type: ServerType) -> Self {
        VersionInfo {
            name,
            version_type,
            server_type,
        }
    }

    /// 打印版本信息的摘要
    pub fn display_summary(&self) {
        println!("Version Name: {}", self.name);
        println!("Version Type: {:?}", self.version_type);
        println!("Server Type: {:?}", self.server_type);
    }

    /// 1. 根据传入的服务端类型和版本字符串进行分析。
    /// 2. 始终保持返回的 name 和 server_type 不变。
    pub fn get_version_info(
        version_name: &str,
        initial_server_type: ServerType,
    ) -> Result<Self, Error> {
        // Other 类型直接返回
        if initial_server_type == ServerType::Other {
            // 对 Other 类型尝试猜测版本类型
            let version_type = VersionInfo::guess_version_type(version_name);
            return Ok(Self::new(
                version_name.to_string(),
                version_type,
                initial_server_type,
            ));
        }

        // BDS 类型进行语义版本解析
        if initial_server_type == ServerType::BDS {
            // 格式错误则返回 Err
            let version_type = VersionInfo::validate_bds_format(version_name)?;
            return Ok(Self::new(
                version_name.to_string(),
                version_type,
                initial_server_type,
            ));
        }

        // 其他 Java-based 类型 (Vanilla, Paper, Folia, Spigot, Purpur)

        // 格式解析 (格式错误直接返回错误)
        let version_type = VersionInfo::validate_java_format(version_name)?;

        // 查询 Mojang 官方版本清单 (正确格式则查询)
        let manifest_result = VersionManifest::fetch();
        match manifest_result {
            Ok(_) => {
                // 查询成功，版本类型基于格式解析结果
                Ok(Self::new(
                    version_name.to_string(),
                    version_type,
                    initial_server_type,
                ))
            }
            Err(e) => {
                error!(
                    "Error fetching version manifest for Java server type {:?}: {}",
                    initial_server_type, e
                );
                // 查询失败则为 Unknown (版本类型)
                Ok(Self::new(
                    version_name.to_string(),
                    VersionType::Unknown,
                    initial_server_type,
                ))
            }
        }
    }

    /// 查询 Mojang API，根据传入的版本类型返回最新的版本字符串。
    ///
    /// Note: 对于 OldBeta 和 OldAlpha，返回的是 API 列表中对应类型的第一个版本（即最新的）。
    pub fn get_latest_version(version_type: VersionType) -> Result<String, Error> {
        if version_type == VersionType::Unknown {
            return Err(anyhow::Error::msg(
                "Cannot find the latest version for an Unknown type.",
            ));
        }

        // 发起 API 请求
        let manifest = VersionManifest::fetch()?;

        // 根据版本类型查找最新 ID
        let latest_id = match version_type {
            VersionType::Release => Some(manifest.latest.release),
            VersionType::Snapshot => Some(manifest.latest.snapshot),

            // 对于旧版本，需要遍历 versions 列表，匹配 type 字段
            VersionType::OldBeta => manifest
                .versions
                .iter()
                // Mojang API 中的旧 Beta 版本 type 字段为 "old_beta"
                .find(|v| v.version_type_str == "old_beta")
                .map(|v| v.id.clone()),

            VersionType::OldAlpha => manifest
                .versions
                .iter()
                // Mojang API 中的旧 Alpha 版本 type 字段为 "old_alpha"
                .find(|v| v.version_type_str == "old_alpha")
                .map(|v| v.id.clone()),

            // 理论上 Unknown 已在顶部被排除，其他类型（如 BDS）不通过此函数查询
            VersionType::Unknown => unreachable!(),
        };

        match latest_id {
            Some(id) => Ok(id),
            None => Err(anyhow::Error::msg(format!(
                "Could not find any version for type {:?} in the Mojang manifest.",
                version_type
            ))),
        }
    }

    /// 内部函数：仅用于猜测版本类型，不抛出错误 (用于 ServerType::Other)
    fn guess_version_type(version_name: &str) -> VersionType {
        match VersionInfo::validate_java_format(version_name) {
            Ok(vt) => vt,
            Err(_) => {
                VersionInfo::validate_bds_format(version_name).unwrap_or(VersionType::Unknown)
            }
        }
    }

    /// 内部函数：验证 BDS 版本格式 (X.Y.Z.B)，并返回 VersionType
    /// BDS 版本通常是 Release
    fn validate_bds_format(version_name: &str) -> Result<VersionType, Error> {
        lazy_static! {
            // BDS 版本号格式：至少三段，最多四段 (X.Y.Z[.B]，如 1.20.70.21)
            static ref BDS_RE: Regex = Regex::new(r"^\d+\.\d+\.\d+(\.\d+)?$").unwrap();
        }

        if BDS_RE.is_match(version_name) {
            // BDS 正式版和预览版通常都使用这种语义版本格式，没有显式的 'w' 或 'a/b'
            // 在缺乏 BDS 官方清单的情况下，假设有效格式即为 Release。
            Ok(VersionType::Release)
        } else {
            Err(anyhow::Error::msg(format!(
                "Invalid BDS version format (expected X.Y.Z[.B]): {}",
                version_name
            )))
        }
    }

    /// 内部函数：验证 Java 版本格式，格式错误则返回错误
    fn validate_java_format(version_name: &str) -> Result<VersionType, Error> {
        lazy_static! {
            // 匹配 YYwWWa/b/c 格式 (如 24w08a)
            static ref SNAPSHOT_RE: Regex = Regex::new(r"^\d{2}w\d{2}[a-z]$").unwrap();
            // 匹配 X.Y.Z 格式 (如 1.21.1)
            static ref RELEASE_RE: Regex = Regex::new(r"^\d+\.\d+(\.\d+)?$").unwrap();
            // 匹配 aX.Y.Z 或 bX.Y.Z 格式 (如 b1.7.3)
            static ref ALPHA_BETA_RE: Regex = Regex::new(r"^[ab]\d+\.\d+(\.\d+)?$").unwrap();
        }

        if SNAPSHOT_RE.is_match(version_name) {
            Ok(VersionType::Snapshot)
        } else if RELEASE_RE.is_match(version_name) {
            Ok(VersionType::Release)
        } else if ALPHA_BETA_RE.is_match(version_name) {
            // 粗略判断为 OldBeta
            Ok(VersionType::OldBeta)
        } else if version_name.to_lowercase().starts_with('a') {
            // 粗略判断为 OldAlpha
            Ok(VersionType::OldAlpha)
        } else {
            Err(anyhow::Error::msg(format!(
                "Invalid JE version format (expected X.Y.Z or YYwWWa): {}",
                version_name
            )))
        }
    }
}

#[cfg(test)]
fn main() {
    // Java 正式版 (查询成功, Vanilla, Release)
    println!("--- Testing Vanilla Release (1.21.1) ---");
    match VersionInfo::get_version_info("1.21.1", ServerType::Vanilla) {
        Ok(info) => info.display_summary(),
        Err(e) => eprintln!("Error: {}", e),
    }

    // Java 格式错误 (应返回 Err)
    println!("\n--- Testing Paper Invalid Format (bad-v1) ---");
    match VersionInfo::get_version_info("bad-v1", ServerType::Paper) {
        Ok(info) => info.display_summary(),
        Err(e) => println!("Success (Expected Error): {}", e), // 捕获预期错误
    }

    // BDS 有效版本 (BDS, Release)
    println!("\n--- Testing BDS Valid Version (1.20.70.21) ---");
    match VersionInfo::get_version_info("1.20.70.21", ServerType::BDS) {
        Ok(info) => info.display_summary(),
        Err(e) => eprintln!("Error: {}", e),
    }

    // BDS 格式错误 (应返回 Err)
    println!("\n--- Testing BDS Invalid Format (1.20) ---");
    match VersionInfo::get_version_info("1.20", ServerType::BDS) {
        Ok(info) => info.display_summary(),
        Err(e) => println!("Success (Expected Error): {}", e), // 捕获预期错误
    }

    // Other 类型 (直接返回, ServerType不变)
    println!("\n--- Testing Other Type (Some-Mod-v2.0) ---");
    match VersionInfo::get_version_info("Some-Mod-v2.0", ServerType::Other) {
        Ok(info) => info.display_summary(),
        Err(e) => eprintln!("Error: {}", e),
    }

    // Java 快照版 (查询成功, Paper, Snapshot)
    println!("\n--- Testing Paper Snapshot (24w08a) ---");
    match VersionInfo::get_version_info("24w08a", ServerType::Paper) {
        Ok(info) => info.display_summary(),
        Err(e) => eprintln!("Error: {}", e),
    }
}
