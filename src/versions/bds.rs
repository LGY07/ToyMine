use crate::core::mc_server::base::McServer;
use tokio::process::Command;

pub struct BDS;

impl McServer for BDS {
    fn new() -> Box<dyn McServer>
    where
        Self: Sized,
    {
        todo!()
    }

    fn script(&self) -> anyhow::Result<String> {
        todo!()
    }

    fn start(&self) -> anyhow::Result<Command> {
        todo!()
    }
}
