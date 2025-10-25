use crate::project_manager::Config;
use crate::project_manager::config::{
    Backup, Event, Java, JavaType, PluginManage, Project, Runtime, ServerType, Time,
};
use crate::project_manager::get_info::{NotValid, get_info};
use crate::project_manager::tools::{
    JarInfo, VersionInfo, VersionType, analyze_jar, analyze_je_game, get_mime_type,
};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub fn create_project() {
    match get_info() {
        // 项目已存在
        Ok(_) => println!("{}", "The project already exists!".yellow()),
        Err(e) => {
            match e {
                // 项目残留文件存在，要求手动处理
                NotValid::ConfigBroken=>eprintln!("{}","There are files related to NMSL in the current directory, but they may be damaged. Please check the .nmsl directory and the NMSL.toml file. You need to manually delete them to continue creating.".yellow()),

                // 项目不存在，尝试创建
                NotValid::NotConfigured=> {

                    // 判断是否有 server.jar 文件
                    let config = if Path::new("server.jar").exists() {

                        // 尝试分析 server.jar 成功则根据已有 jar 创建
                        create_config_jar_file(PathBuf::from_str("server.jar").unwrap()).unwrap_or_else(|e| {
                            eprintln!("{:?}", e);
                            create_config_empty()
                        })
                    } else if Path::new("server").exists() {

                        // 尝试分析 server 成功则根据已有二进制文件创建，否则按照空项目处理
                        println!("Bedrock Edition version is not supported for the time being!");
                        todo!()
                    } else {

                        // 按空项目创建
                        create_config_empty()
                    };

                    // 初始化项目
                    // 创建配置文件
                    match config.to_file(Path::new("NMSL.toml")) {
                        Ok(_) => (),
                        Err(_) => panic!("The configuration file cannot be created!")
                    }
                    // 创建目录
                    let dir_list = vec![".nmsl",".nmsl/cache",".nmsl/backup",".nmsl/runtime"];
                    for i in dir_list{
                    match fs::create_dir(i) {
                        Ok(_) => (),
                        Err(_) => panic!("Directory cannot be created!")
                    }
                }
                println!("{}","The project has been successfully created".green())
                }
            }
        }
    }
}

fn check_dir(path: PathBuf) -> bool {
    todo!()
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
        let input = match input {
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
        let input = get_input().trim().to_string(); //输入
        new_config.project.version = loop {
            //确保输入完成
            match VersionInfo::get_version_info(&input, ServerType::BDS) {
                Ok(v) => break v.name,
                Err(e) => {
                    eprintln!("{}", e);
                    println!("{}", "Please re-enter the version".yellow());
                    continue;
                }
            }
        };
    } else if new_config.project.server_type.ne(&ServerType::Other) {
        // 再检查Java版
        println!("Set the game version. The default is the latest version."); // 提示信息
        let input = get_input().trim().to_string(); //输入
        new_config.project.version = match &input[..] {
            // 默认为最新Release版本
            "" => VersionInfo::get_latest_version(VersionType::Release).unwrap(),
            "latest" => VersionInfo::get_latest_version(VersionType::Release).unwrap(),
            _ => {
                // 手动设置版本
                loop {
                    //确保输入完成
                    match VersionInfo::get_version_info(
                        &input,
                        new_config.project.server_type.clone(),
                    ) {
                        Ok(v) => {
                            new_config.project.version_type = v.version_type;
                            break v.name;
                        }
                        Err(e) => {
                            eprintln!("{}", e);
                            println!("{}", "Please re-enter the version".yellow());
                            continue;
                        }
                    }
                }
            }
        };
    }

    new_config
}

/// 通过已有的服务端文件创建配置
fn create_config_jar_file(server_file: PathBuf) -> Result<Config, String> {
    // 创建基本配置
    let mut new_config = Config::default();

    let version_info = analyze_je_game(&server_file).map_err(|e| format!("{:?}", e))?;

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
