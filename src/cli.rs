use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum BuildKind {
    Production,
    Develop,
}

#[allow(unused)]
impl BuildKind {
    pub fn is_production(self) -> bool {
        matches!(self, BuildKind::Production)
    }

    pub fn is_develop(self) -> bool {
        matches!(self, BuildKind::Develop)
    }
}

#[derive(Debug, Parser)]
pub struct BuildCommand {
    #[arg(short, long, default_value = "false")]
    pub watch: bool,
    #[arg(short, long, default_value = "false")]
    pub develop: bool,
    #[arg(default_value = "./")]
    pub path: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Build(BuildCommand),
    // Init,
}

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}
