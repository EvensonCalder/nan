#![allow(dead_code)]

mod cli;
mod commands;
mod error;
mod model;
mod prompt;
mod render;
mod review;
mod store;

use std::process::ExitCode;

fn main() -> ExitCode {
    match commands::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
