use crate::project_manager::tools::{ServerType, VersionInfo, VersionType};
use std::fs::File;
use std::io::{Error, Read};
use std::path::PathBuf;
use tree_magic_mini;
use zip::read::ZipArchive;

#[derive(Debug)]
pub struct JarInfo {
    pub main_class: String,
    pub java_version: u16, // 映射后的 Java 版本
}

#[derive(Debug)]
pub enum JarError {
    NotJar,
    NoMainClass,
    ClassNotFound,
    IoError(()),
}

impl From<Error> for JarError {
    fn from(err: Error) -> Self {
        JarError::IoError(())
    }
}

/// 根据文件路径获取 MIME 类型（路径传入 &str）
pub fn get_mime_type(path: &PathBuf) -> Result<String, ()> {
    // 使用 tree_magic_mini 检测 MIME 类型
    match tree_magic_mini::from_filepath(path) {
        None => Err(()),
        Some(v) => Ok(v.to_string()),
    }
}

/// 分析 JAR 文件，获取 Main-Class 和 Java 版本（直接 major_version - 45）
pub fn analyze_jar(jar_path: &PathBuf) -> Result<JarInfo, JarError> {
    let file = File::open(jar_path).map_err(|_| JarError::NotJar)?;
    let mut archive = ZipArchive::new(&file).map_err(|_| JarError::NotJar)?;

    // 读取 META-INF/MANIFEST.MF
    let mut manifest_file = archive
        .by_name("META-INF/MANIFEST.MF")
        .map_err(|_| JarError::NoMainClass)?;
    let mut manifest_content = String::new();
    manifest_file.read_to_string(&mut manifest_content)?;

    // 解析 Main-Class
    let main_class = manifest_content
        .lines()
        .find_map(|line| {
            if line.starts_with("Main-Class:") {
                Some(line["Main-Class:".len()..].trim().to_string())
            } else {
                None
            }
        })
        .ok_or(JarError::NoMainClass)?;

    let mut archive = ZipArchive::new(&file).map_err(|_| JarError::NotJar)?;
    // Main-Class 转 class 文件路径
    let class_path = format!("{}.class", main_class.replace('.', "/"));
    let mut class_file = archive
        .by_name(&class_path)
        .map_err(|_| JarError::ClassNotFound)?;

    let mut class_header = [0u8; 8];
    class_file.read_exact(&mut class_header)?;

    // 检查魔术字
    if &class_header[0..4] != &[0xCA, 0xFE, 0xBA, 0xBE] {
        return Err(JarError::NotJar);
    }

    // major version → Java 版本：直接减 45
    let major_version = u16::from_be_bytes([class_header[6], class_header[7]]);
    let java_version = major_version.checked_sub(44).ok_or(JarError::NotJar)?;

    Ok(JarInfo {
        main_class,
        java_version,
    })
}

pub fn analyze_je_game(jar_path: &PathBuf) -> Result<VersionInfo, String> {
    // 读取 Zip 文件和获取 JarInfo
    let info = analyze_jar(jar_path).map_err(|e| format!("{:?}", e))?;
    let file = File::open(jar_path).map_err(|e| format!("{:?}", e))?;
    let mut archive = ZipArchive::new(&file).map_err(|e| format!("{:?}", e))?;

    // 新版本 server.jar 文件格式的分析
    if (info.main_class == "net.minecraft.bundler.Main"
        || info.main_class == "io.papermc.paperclip.Main"
        || info.main_class == "org.leavesmc.leavesclip.Main")
    {
        // 读取 `META-INF/versions.list`
        let mut version_file = archive
            .by_name("META-INF/versions.list")
            .map_err(|_| JarError::NoMainClass)
            .map_err(|e| format!("{:?}", e))?;
        let mut version_list = String::new();
        version_file
            .read_to_string(&mut version_list)
            .map_err(|e| format!("{:?}", e))?;

        // 解析 `META-INF/versions.list`
        // 形如 "2e2867d1c6559bdb660808deaeccb12c9ca41eb04e7b4e2adae87546e1878184	1.21.10	1.21.10/server-1.21.10.jar"
        let info_list = version_list.split("/").collect::<Vec<_>>()[1].replace(".jar", "");
        let info_list: Vec<&str> = info_list.split("-").collect();

        // 分析服务端类型
        let server_type = match info_list[0].trim() {
            "server" => ServerType::Vanilla,
            "paper" => ServerType::Paper,
            "folia" => ServerType::Folia,
            "purpur" => ServerType::Purpur,
            "leaves" => ServerType::Leaves,
            _ => ServerType::Other,
        };

        let version_info = VersionInfo::get_version_info(info_list[1].trim(), server_type)
            .map_err(|e| format!("{:?}", e))?;
        return Ok(version_info);
    }
    // 旧版本文件格式分析
    println!("The old version is not supported for the time being!");
    todo!()
}
