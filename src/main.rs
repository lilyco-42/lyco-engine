mod cli;
mod config;
mod fetch;
mod manifest;
mod params;
mod render;

use colored::Colorize;
use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {:#}", "error:".red(), e);
            ExitCode::FAILURE
        }
    }
}
