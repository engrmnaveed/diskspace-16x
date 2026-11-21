//! Commands of the Pro edition, built with `cargo build --features pro`.
//!
//! Basic builds leave this module out. `main.rs` lists the Pro subcommands, answers them
//! with an install hint in Basic, and hands them to [`parse`] and [`run`] here. Per-node
//! Pro data belongs in side tables keyed by the scan tree's `NodeId`, so Basic's tree stays
//! lean.

use std::ffi::OsString;
use std::io;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Diff,
}

pub fn parse(name: &str, _arguments: impl Iterator<Item = OsString>) -> Result<Command, String> {
    match name {
        "diff" => Ok(Command::Diff),
        _ => Err(format!("Unknown Pro command {name:?}.")),
    }
}

pub fn run(command: Command) -> io::Result<u8> {
    match command {
        Command::Diff => Err(io::Error::other("diff isn't available in this build yet.")),
    }
}
