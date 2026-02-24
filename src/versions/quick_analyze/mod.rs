// 快速判断部分已知的类型

use crate::core::mc_server::base::McVersion;

use crate::core::mc_server::{McChannel, McType};
use crate::versions::paper_like::PAPER_MAP;
use anyhow::{Error, Result, anyhow};
use regex::Regex;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use tracing::debug;
use zip::read::{ZipArchive, ZipFile};

struct JarInfo {
    pub main_class: String,
    pub java_version: u16, // 映射后的 Java 版本
}

/// 分析 JAR 文件，获取 Main-Class 和 Java 版本（直接 major_version - 45）
fn analyze_jar(jar_path: &Path) -> Result<JarInfo> {
    // 打开文件
    let file = File::open(jar_path)?;
    // 读取 zip
    let mut archive = ZipArchive::new(&file)?;

    // 读取 META-INF/MANIFEST.MF
    let mut manifest_file = archive.by_name("META-INF/MANIFEST.MF")?;
    let mut manifest_content = String::new();
    manifest_file.read_to_string(&mut manifest_content)?;

    // 解析 Main-Class
    let main_class = match manifest_content.lines().find_map(|line| {
        if line.starts_with("Main-Class:") {
            Some(line["Main-Class:".len()..].trim().to_string())
        } else {
            None
        }
    }) {
        None => return Err(Error::msg("Not a Jar file")),
        Some(v) => v,
    };

    // 读取 zip
    let mut archive = ZipArchive::new(&file)?;
    // Main-Class 转 class 文件路径
    let class_path = format!("{}.class", main_class.replace('.', "/"));
    let mut class_file = archive.by_name(&class_path)?;
    // 读取魔术字
    let mut class_header = [0u8; 8];
    class_file.read_exact(&mut class_header)?;

    // 检查魔术字
    if class_header[0..4] != [0xCA, 0xFE, 0xBA, 0xBE] {
        return Err(anyhow!("Not a Jar file"));
    }

    // major version → Java 版本：直接减 45
    let major_version = u16::from_be_bytes([class_header[6], class_header[7]]);
    let java_version = match major_version {
        45..52 => 21,
        52..55 => 8,
        55..61 => 11,
        61..65 => 17,
        65..69 => 21,
        _ => {
            return Err(Error::msg(format!(
                "Unsupported major version: {}",
                major_version
            )));
        }
    };

    Ok(JarInfo {
        main_class,
        java_version,
    })
}

/// 分析 server.jar 文件，尝试获得游戏版本
pub fn analyze_je_game(jar_path: &Path) -> Result<McVersion> {
    // 获取 JarInfo 和读取 Zip 文件
    let info = analyze_jar(jar_path)?;
    let file = File::open(jar_path)?;

    // 谨慎使用 `?` `unwrap()` `expect()`，避免影响后续判断

    // 1.18+ 版本获取信息(读取 META-INF/versions.list)
    debug!("analyze_je_game:  Read \"META-INF/versions.list\"");
    // 读取 Jar 文件
    let mut archive = ZipArchive::new(&file)?;

    // 判断主类格式
    if info.main_class == "net.minecraft.bundler.Main"
        || PAPER_MAP
            .iter()
            .filter(|&x| x.main_class == info.main_class)
            .count()
            != 0
    {
        // 读取 `META-INF/versions.list`
        let mut version_file = archive.by_name("META-INF/versions.list")?;
        let mut version_list = String::new();
        version_file.read_to_string(&mut version_list)?;

        // 解析 `META-INF/versions.list`
        // 形如 "2e2867d1c6559bdb660808deaeccb12c9ca41eb04e7b4e2adae87546e1878184	1.21.10	1.21.10/server-1.21.10.jar"
        let info_list = version_list.split("/").collect::<Vec<_>>()[1].replace(".jar", "");
        let info_list: Vec<&str> = info_list.split("-").collect();

        // 解析版本号
        if info_list.len() == 2 {
            return Ok(parse_version(info_list[1].trim(), info_list[0].trim()));
        }
    }

    // 1.14+ 版本获取信息(读取 version.json)
    debug!("analyze_je_game:  Read \"versions.json\"");
    // 读取 Jar 文件
    let mut archive = ZipArchive::new(&file)?;
    // 读取 version.json
    if let Ok(mut file) = archive
        .by_name("version.json")
        .map_err(|e| format!("{:?}", e))
    {
        // 转换 version.json 为字符串
        let mut version_json_string = String::new();
        file.read_to_string(&mut version_json_string)?;
        // 从 json 获得 name 键的值
        // 找到 "name"
        let key = "\"name\"";
        let start = version_json_string.find(key).expect("Problematic JSON.") + key.len();
        // 找到冒号
        let after_colon = version_json_string[start..]
            .find(':')
            .expect("Problematic JSON.");
        let rest = &version_json_string[start + after_colon + 1..];
        // 去掉前面的空白和引号
        let rest = rest.trim_start();
        if rest.starts_with('"') {
            // 普通字符串值
            let end_quote = rest[1..].find('"').expect("Problematic JSON.");
            let version = &rest[1..1 + end_quote];
            // 解析版本号，默认当成 Vanilla
            return Ok(parse_version(version, "vanilla"));
        }
    };

    // Paper 服务端尝试获取信息(尝试读取 patch.properties)
    debug!("analyze_je_game:  Read \"patch.properties\"");
    // 读取 Jar 文件
    let mut archive = ZipArchive::new(&file)?;
    // 读取 patch.properties
    if let Ok(mut file) = archive
        .by_name("patch.properties")
        .map_err(|e| format!("{:?}", e))
    {
        // 转换 patch.properties 为字符串
        let mut properties_string = String::new();
        file.read_to_string(&mut properties_string)?;
        // 从 ini 获取 version 键的值
        for line in properties_string.lines() {
            // 去掉首尾空白字符
            let line = line.trim();
            // 跳过空行或注释
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            // 找到 key=value 格式
            if let Some((key, value)) = line.split_once('=') {
                if let "version" = key.trim() {
                    // 解析版本号
                    return Ok(parse_version(value, "paper"));
                }
            }
        }
    };

    // 其他 Vanilla 版本尝试获取信息(直接读取 MainClass 的字符串常量池)
    debug!("analyze_je_game:  Read string constant pool of MainClass");
    // 读取 Jar 文件
    let mut archive = ZipArchive::new(&file)?;
    // 读取 MainClass
    let mut main_class =
        archive.by_name(format!("{}.class", info.main_class.replace('.', "/")).as_str())?;
    // 读取字符串常量池
    let strings = parse_class_strings_from_zip(&mut main_class);
    // 创建正则表达式，假设为 x.x.x
    let re = Regex::new(r"[0-9]+\.[0-9]+\.[0-9]+")?;
    for s in &strings {
        if let Some(m) = re.find(s) {
            // 解析版本号
            return Ok(parse_version(m.as_str(), "vanilla"));
        }
    }
    // 创建正则表达式，假设为 x.x
    let re = Regex::new(r"[0-9]+\.[0-9]+")?;
    for s in &strings {
        if let Some(m) = re.find(s) {
            // 解析版本号
            return Ok(parse_version(m.as_str(), "vanilla"));
        }
    }

    Err(Error::msg(
        "Version parsing failed: Version information cannot be found.",
    ))
}

/// 从 ZipFile 读取 .class 文件并解析字符串常量池
fn parse_class_strings_from_zip(file: &mut ZipFile<&File>) -> Vec<String> {
    fn read_u1(c: &mut Cursor<Vec<u8>>) -> Option<u8> {
        let mut b = [0u8; 1];
        c.read_exact(&mut b).ok()?;
        Some(b[0])
    }

    fn read_u2(c: &mut Cursor<Vec<u8>>) -> Option<u16> {
        let mut b = [0u8; 2];
        c.read_exact(&mut b).ok()?;
        Some(u16::from_be_bytes(b))
    }

    fn read_u4(c: &mut Cursor<Vec<u8>>) -> Option<u32> {
        let mut b = [0u8; 4];
        c.read_exact(&mut b).ok()?;
        Some(u32::from_be_bytes(b))
    }

    // 读取 ZipFile 内容
    let mut data = Vec::new();
    if file.read_to_end(&mut data).is_err() {
        return Vec::new(); // 失败返回空列表
    }
    let mut c = Cursor::new(data);

    // 跳过魔数和版本号
    let _magic = read_u4(&mut c).unwrap_or(0);
    let _minor = read_u2(&mut c).unwrap_or(0);
    let _major = read_u2(&mut c).unwrap_or(0);

    // 常量池数量
    let cp_count = read_u2(&mut c).unwrap_or(0);
    let mut strings: Vec<String> = Vec::new();
    let mut i = 1;

    while i < cp_count {
        let tag = read_u1(&mut c).unwrap_or(0);
        match tag {
            1 => {
                // CONSTANT_Utf8_info
                let len = read_u2(&mut c).unwrap_or(0) as usize;
                let mut buf = vec![0u8; len];
                if c.read_exact(&mut buf).is_err() {
                    break;
                }
                strings.push(String::from_utf8_lossy(&buf).into_owned());
            }
            3 | 4 => c.set_position(c.position() + 4),
            5 | 6 => {
                c.set_position(c.position() + 8);
                i += 1
            }
            7 | 8 | 16 => c.set_position(c.position() + 2),
            9 | 10 | 11 | 12 | 18 => c.set_position(c.position() + 4),
            15 => c.set_position(c.position() + 3),
            _ => break,
        }
        i += 1;
    }

    strings
}

fn parse_version(version_str: &str, version_type: &str) -> McVersion {
    let chanel = match version_str
        .split('.')
        .map(|x| x.parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) => {
            if v.len() == 3 {
                McChannel::Release(v[0], v[1], v[2])
            } else {
                McChannel::Snapshot(version_str.trim().to_string())
            }
        }
        Err(_) => McChannel::Snapshot(version_str.trim().to_string()),
    };
    McVersion {
        server_type: McType::Java(
            if version_type == "server" {
                "vanilla"
            } else {
                version_type
            }
            .to_string(),
        ),
        channel: chanel,
    }
}
