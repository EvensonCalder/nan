mod add;
mod cat;
mod del;
mod list;
mod new;
mod set;

use crate::cli::{Cli, Command};
use crate::error::NanError;
use crate::store::Store;

pub fn run() -> Result<(), NanError> {
    let cli = Cli::parse_args();
    run_with_cli(cli)
}

pub fn run_with_cli(cli: Cli) -> Result<(), NanError> {
    let store = Store::new()?;

    match cli.command {
        Command::Add { sentence, style } => add::run(&store, sentence, style),
        Command::New { first, second } => new::run(&store, first, second),
        Command::Cat { n } => cat::run(&store, n),
        Command::List { n, target } => list::run(&store, n, target),
        Command::Del { n } => del::run(&store, n),
        Command::Set { key, option } => set::run(&store, key, option),
    }
}
