use crate::project_manager::config::ServerType;
use crate::project_manager::info::{ConfigErr, get_info};
use crate::project_manager::tools::{
    PaperProject, VersionInfo, VersionType, analyze_je_game, get_mime_type,
};
use crate::project_manager::{
    BACKUP_DIR, CACHE_DIR, CONFIG_FILE, Config, FOLIA_PROJECT_API, LEAVES_PROJECT_API, LOG_DIR,
    PAPER_PROJECT_API, PURPUR_PROJECT_API, RUNTIME_DIR, WORK_DIR,
};
use anyhow::Error;
use colored::Colorize;
use log::{error, info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const DIR_LIST: [&str; 5] = [WORK_DIR, CACHE_DIR, BACKUP_DIR, RUNTIME_DIR, LOG_DIR];

/// 初始化配置文件
pub fn create_project() {
    // 判断配置是否完整存在
    let config_err = match get_info() {
        // 项目已存在
        Ok(_) => {
            warn!("{}", "The project already exists!".yellow());
            return;
        }
        Err(e) => e,
    };

    // 项目残留文件存在，要求手动处理
    if config_err.eq(&ConfigErr::ConfigBroken) {
        error!("{}","There are files related to PacMine in the current directory, but they may be damaged. Please check the .pacmine directory and the PacMine.toml file. You need to manually delete them to continue creating.".yellow());
        return;
    }

    // 项目不存在，尝试创建
    // 判断是否有 server 文件
    let config = if get_mime_type(&PathBuf::from("server.jar")) == "application/zip" {
        // 尝试分析 server.jar 成功则根据已有 jar 创建
        create_config_jar_file(PathBuf::from_str("server.jar").unwrap()).unwrap_or_else(|e| {
            warn!("{:?}", e);
            create_config_empty()
        })
    } else if get_mime_type(&PathBuf::from("bedrock_server")) == "application/x-executable" {
        // 尝试分析 server 成功则根据已有二进制文件创建，否则按照空项目处理
        error!("Bedrock Edition version is not supported for the time being!");
        todo!()
    } else if get_mime_type(&PathBuf::from("bedrock_server.exe"))
        == "application/vnd.microsoft.portable-executable"
    {
        // 尝试分析 server.exe 成功则根据已有二进制文件创建，否则按照空项目处理
        error!("Bedrock Edition version is not supported for the time being!");
        todo!()
    } else {
        // 按空项目创建
        create_config_empty()
    };

    // 初始化项目
    // 创建配置文件
    config
        .to_file(Path::new(CONFIG_FILE))
        .expect("The configuration file cannot be created!");
    // 创建目录
    for i in DIR_LIST {
        fs::create_dir(i).expect("Directory cannot be created!")
    }

    info!("{}", "The project has been successfully created".green())
}

/// 询问用户配置信息并创建配置文件
fn create_config_empty() -> Config {
    // 创建基本配置
    let mut new_config = Config::default();

    // 获取项目名称
    println!("Enter the name of this project:");
    new_config.project.name = get_input().trim().to_string();

    // 列出服务端类型
    println!("Select the type of the server:");
    println!("1: Vanilla(Official)");
    println!("2: PaperMC");
    println!("3: PurpurMC");
    println!("4: LeavesMC");
    println!("5: Bedrock Dedicated Server(Official)");
    println!("0: Other Server");
    new_config.project.server_type = loop {
        // 获取输入
        let input = get_input().trim().parse::<usize>();
        // 解析输入为 usize
        let input = match input {
            Ok(v) => v,
            Err(_) => {
                println!("Please enter a number.");
                continue;
            }
        };
        // 解析输入为 ServerType
        match input {
            1 => break ServerType::Vanilla,
            2 => break ServerType::Paper,
            3 => break ServerType::Purpur,
            4 => break ServerType::Leaves,
            5 => break ServerType::BDS,
            0 => break ServerType::Other,
            _ => {
                println!("Please select within the range.");
                continue;
            }
        };
    };

    // 根据服务端类型设置可执行文件
    new_config.project.execute = match new_config.project.server_type {
        // 自定义的可执行文件
        ServerType::Other => {
            println!("Enter the name of the executable file");
            get_input()
        }
        // BE 版可执行文件
        ServerType::BDS => "server".to_string(),
        // JE 版可执行文件
        _ => "server.jar".to_string(),
    };

    // 获取并验证版本号
    if new_config.project.server_type.eq(&ServerType::BDS) {
        // 先检查基岩版

        println!("Set the game version."); // 提示信息
        // 确保设置格式正确的版本号
        loop {
            let input = get_input().trim().to_string(); //输入
            match VersionInfo::get_version_info(&input, ServerType::BDS) {
                Ok(v) => {
                    // 成功设置
                    new_config.project.version = v.name;
                    break;
                }
                Err(e) => {
                    // 重新输入
                    error!("{}", e);
                    println!("{}", "Please re-enter the version".yellow());
                    continue;
                }
            }
        }
    } else if new_config.project.server_type.ne(&ServerType::Other) {
        // 再检查Java版

        println!("Set the game version. The default is the latest version."); // 提示信息
        // 确保正确设置版本
        loop {
            let input = get_input().trim().to_string(); //输入
            if input.is_empty() || &input == "latest" {
                // 默认的最新版本

                // version_type 默认已经为 Release
                // 获取最新版本
                new_config.project.version = match new_config.project.server_type {
                    ServerType::Vanilla => VersionInfo::get_latest_version(VersionType::Release)
                        .expect("Failed to get the latest version"),
                    ServerType::Paper => PaperProject::fetch(PAPER_PROJECT_API)
                        .expect("Failed to get the latest version")
                        .versions
                        .last()
                        .expect("Failed to get the latest version")
                        .to_string(),
                    ServerType::Folia => PaperProject::fetch(FOLIA_PROJECT_API)
                        .expect("Failed to get the latest version")
                        .versions
                        .last()
                        .expect("Failed to get the latest version")
                        .to_string(),
                    ServerType::Purpur => PaperProject::fetch(PURPUR_PROJECT_API)
                        .expect("Failed to get the latest version")
                        .versions
                        .last()
                        .expect("Failed to get the latest version")
                        .to_string(),
                    ServerType::Leaves => PaperProject::fetch(LEAVES_PROJECT_API)
                        .expect("Failed to get the latest version")
                        .versions
                        .last()
                        .expect("Failed to get the latest version")
                        .to_string(),
                    ServerType::Other => String::new(),
                    _ => unreachable!("不存在的服务端类型"),
                };
                // 成功设置
                break;
            } else {
                // 手动设置的版本

                // 判断输入版本是否存在
                match VersionInfo::get_version_info(&input, new_config.project.server_type.clone())
                {
                    Ok(v) => {
                        // 成功设置
                        new_config.project.version_type = v.version_type;
                        new_config.project.version = v.name;
                        break;
                    }
                    Err(e) => {
                        // 重新输入
                        error!("{}", e);
                        println!("{}", "Please re-enter the version".yellow());
                        continue;
                    }
                }
            }
        }
    }

    new_config
}

/// 通过已有的 jar 服务端文件创建配置
fn create_config_jar_file(server_file: PathBuf) -> Result<Config, Error> {
    // 创建基本配置
    let mut new_config = Config::default();

    // 解析 jar 文件获得版本信息
    let version_info = analyze_je_game(&server_file)?;
    // 设置版本信息
    new_config.project.version = version_info.name.clone();
    new_config.project.server_type = version_info.server_type.clone();
    new_config.project.version_type = version_info.version_type.clone();

    // 获取项目名称
    println!("Enter the name of this project:");
    new_config.project.name = get_input().trim().to_string();

    Ok(new_config)
}

/// 获取一行输入，带有提示符 `>`
fn get_input() -> String {
    // 初始化输入缓存
    let mut input_buffer = String::new();
    // 打印提示符
    print!(">");
    std::io::Write::flush(&mut std::io::stdout())
        .expect("ERROR: Failed to print the prompt message");
    // 处理错误
    match std::io::stdin().read_line(&mut input_buffer) {
        Ok(_) => input_buffer,
        Err(_) => panic!("Unknown input error!"),
    }
}
