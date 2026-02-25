use crate::core::mc_server::McVersion;
use crate::core::mc_server::base::McServer;
use crate::core::mc_server::runtime::McServerRuntime;
use crate::core::mc_server::update::McServerUpdate;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::debug;

pub struct PaperConst {
    pub name: &'static str,
    pub main_class: &'static str,
}
pub const PAPER_MAP: &[PaperConst] = &[
    PaperConst {
        name: "paper",
        main_class: "io.papermc.paperclip.Main",
    },
    PaperConst {
        name: "purpur",
        main_class: "io.papermc.paperclip.Main",
    },
    PaperConst {
        name: "folia",
        main_class: "io.papermc.paperclip.Main",
    },
    PaperConst {
        name: "leaves",
        main_class: "org.leavesmc.leavesclip.Main",
    },
];

pub struct PaperLike {
    runtime_path: PathBuf,
    server_path: PathBuf,
}

impl McServer for PaperLike {
    fn new(path: &Path) -> Box<dyn McServer>
    where
        Self: Sized,
    {
        debug!("PaperLike");
        Box::new(PaperLike {
            runtime_path: "java".parse().unwrap(),
            server_path: path.to_path_buf(),
        })
    }

    fn script(&self) -> anyhow::Result<String> {
        unreachable!("It should be implemented in McRuntime.")
    }

    fn start(&self) -> anyhow::Result<tokio::process::Command> {
        let mut command = tokio::process::Command::new("java");
        command.arg("-jar").arg("server.jar").arg("-nogui");
        Ok(command)
    }
    fn impl_update<'a>(&'a self) -> Option<&'a dyn McServerUpdate> {
        Some(self)
    }
    fn impl_runtime<'a>(&'a self) -> Option<&'a dyn McServerRuntime> {
        Some(self)
    }
}

#[async_trait]
impl McServerUpdate for PaperLike {
    async fn latest_version(&self) -> anyhow::Result<McVersion> {
        todo!()
    }

    async fn install_version(&self, target: McVersion) -> anyhow::Result<()> {
        todo!()
    }
}

#[async_trait]
impl McServerRuntime for PaperLike {
    async fn ready_runtime(&self) -> anyhow::Result<bool> {
        todo!()
    }

    async fn setup_runtime(&self) -> anyhow::Result<()> {
        todo!()
    }

    fn ext_script(&self, _: &str, os: &str) -> anyhow::Result<String> {
        let mut s = String::new();
        if os == "windows" {
            s.push_str("@echo off\n");
        } else {
            s.push_str("#!/bin/env bash")
        }
        s.push_str(
            format!(
                "{} -jar {} -nogui",
                self.runtime_path.to_string_lossy(),
                self.server_path.to_string_lossy()
            )
            .as_str(),
        );
        Ok(s)
    }
}
