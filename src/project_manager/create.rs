use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use colored::Colorize;
use crate::project_manager::Config;
use crate::project_manager::config::{Backup, Event, Java, JavaType, PluginManage, Project, Runtime, ServerType, Time};
use crate::project_manager::get_info::{get_info, NotValid};
use crate::project_manager::tools::{VersionInfo, VersionType};

/// 获取一行输入，带有提示符 `>`
fn get_input()->String{
    // 初始化输入缓存
    let mut input_buffer = String::new();
    // 打印提示符
    print!(">");
    std::io::Write::flush(&mut std::io::stdout()).expect("ERROR: Failed to print the prompt message");
    // 处理错误
    match std::io::stdin().read_line(&mut input_buffer) {
        Ok(_) => input_buffer,
        Err(_)=> panic!("Unknown input error!")
    }
}

fn check_dir(path: PathBuf)->bool{todo!()}

/// 询问用户配置信息
fn create_config()->Config{

    // 创建基本配置
    let mut new_config = Config::default();

    // 获取实例名称
    println!("Enter the name of this project:");
    new_config.project.name = get_input().trim().to_string();

    // 列出服务端类型
    println!("Select the type of the server:");
    println!("1: Vanilla(Official)");
    println!("2: PaperMC");
    println!("3: PurpurMC");
    println!("4: SpigotMC");
    println!("5: Bedrock Dedicated Server(Official)");
    println!("0: Other Server");
    new_config.project.server_type = loop {
        // 获取输入
        let input = get_input().trim().parse::<usize>();
        // 解析输入为 usize
        let input = match input {
            Ok(v)=>v,
            Err(_)=>{
                println!("Please enter a number.");
                continue
            }
        };
        // 解析输入为 ServerType
        let input= match input {
            1 => break ServerType::Vanilla,
            2 => break ServerType::Paper,
            3 => break ServerType::Purpur,
            4 => break ServerType::Spigot,
            5 => break ServerType::BDS,
            0 => break ServerType::Other,
            _ => {
                println!("Please select within the range.");
                continue
            }
        };
    };

    // 根据服务端类型设置可执行文件
    new_config.project.execute = match new_config.project.server_type {
        // 自定义的可执行文件
        ServerType::Other=>{
            println!("Enter the name of the executable file");
            get_input()
        },
        // BE 版可执行文件
        ServerType::BDS=>"server".to_string(),
        // JE 版可执行文件
        _ => "server.jar".to_string()
    };

    // 获取并验证版本号
    if new_config.project.server_type.eq(&ServerType::BDS){// 先检查基岩版
        println!("Set the game version.");// 提示信息
        let input = get_input().trim().to_string();//输入
        new_config.project.version = loop {//确保输入完成
            match VersionInfo::get_version_info(&input, ServerType::BDS) {
                Ok(v) => {
                    break v.name
                }
                Err(e) => {
                    eprintln!("{}", e);
                    println!("{}", "Please re-enter the version".yellow());
                    continue;
                }
            }
        };
    }else if new_config.project.server_type.ne(&ServerType::Other){// 再检查Java版
    println!("Set the game version. The default is the latest version.");// 提示信息
    let input = get_input().trim().to_string();//输入
    new_config.project.version = match &input[..]{
        // 默认为最新Release版本
        ""=>VersionInfo::get_latest_version(VersionType::Release).unwrap(),
        "latest"=>VersionInfo::get_latest_version(VersionType::Release).unwrap(),
        _ => {
            // 手动设置版本
            loop {//确保输入完成
                match VersionInfo::get_version_info(&input, new_config.project.server_type.clone()) {
                    Ok(v) => {
                        new_config.project.version_type=v.version_type;
                        break v.name
                    }
                    Err(e) => {
                        eprintln!("{}", e);
                        println!("{}", "Please re-enter the version".yellow());
                        continue;
                    }
                }
            }
        }
    };}

    new_config
}

pub fn create_project(){
    match get_info() {
        Ok(_)=>println!("{}","The project already exists!".yellow()),
        Err(e) => {
            match e {
                NotValid::ConfigBroken=>eprintln!("{}","There are files related to NMSL in the current directory, but they may be damaged. Please check the .nmsl directory and the NMSL.toml file. You need to manually delete them to continue creating.".yellow()),
                NotValid::NotConfigured=>{
                    let config =create_config();
                    match config.to_file(Path::new("NMSL.toml")){
                        Ok(_)=>(),
                        Err(_)=>panic!("The configuration file cannot be created!")
                    }
                    match fs::create_dir(".nmsl") {
                        Ok(_)=>(),
                        Err(_)=>panic!("Directory cannot be created!")
                    }
                    match fs::create_dir(".nmsl/cache") {
                        Ok(_)=>(),
                        Err(_)=>panic!("Directory cannot be created!")
                    }
                    match fs::create_dir(".nmsl/backup") {
                        Ok(_)=>(),
                        Err(_)=>panic!("Directory cannot be created!")
                    }
                    println!("{}","The project has been successfully created".green())
                }
            }
        }
    }
}
