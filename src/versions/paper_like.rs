use crate::core::mc_server::base::McServer;
use crate::versions::vanilla::Vanilla;
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

pub struct PaperLike;

impl McServer for PaperLike {
    fn new() -> Box<dyn McServer>
    where
        Self: Sized,
    {
        debug!("PaperLike");
        Box::new(Vanilla)
    }

    fn script(&self) -> anyhow::Result<String> {
        unreachable!("It should be implemented in McRuntime.")
    }

    fn start(&self) -> anyhow::Result<tokio::process::Command> {
        let mut command = tokio::process::Command::new("java");
        command.arg("-jar").arg("server.jar").arg("-nogui");
        Ok(command)
    }
}
