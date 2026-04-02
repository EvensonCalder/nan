mod add;
mod cat;
mod del;
mod list;
mod new;
mod set;

use std::io::{self, IsTerminal, Write};

use crate::cli::{Cli, Command};
use crate::error::NanError;
use crate::model::NativeLanguage;
use crate::store::Store;

pub fn run() -> Result<(), NanError> {
    let cli = Cli::parse_args();
    run_with_cli(cli)
}

pub fn run_with_cli(cli: Cli) -> Result<(), NanError> {
    let store = Store::new()?;
    ensure_language_consistency(&store, &cli.command)?;

    match cli.command {
        Command::Add { sentence, style } => add::run(&store, sentence, style),
        Command::New { first, second } => new::run(&store, first, second),
        Command::Cat { n } => cat::run(&store, n),
        Command::List { first, second } => list::run(&store, first, second),
        Command::Del { n } => del::run(&store, n),
        Command::Set { key, option } => set::run(&store, key, option),
    }
}

fn ensure_language_consistency(store: &Store, command: &Command) -> Result<(), NanError> {
    if matches!(
        command,
        Command::Set {
            key: crate::cli::SetKey::ApiKey
                | crate::cli::SetKey::BaseUrl
                | crate::cli::SetKey::Model
                | crate::cli::SetKey::Lan,
            ..
        }
    ) {
        return Ok(());
    }

    let database = store.load_or_create()?;
    if !set::has_language_mismatch(&database) {
        return Ok(());
    }

    if !io::stdin().is_terminal() {
        return Err(NanError::message(
            "stored data uses inconsistent languages. Run any `nan` command in an interactive terminal to choose a target language and resume rewriting.",
        ));
    }

    let target_language = prompt_for_target_language(database.settings.lan)?;
    set::rewrite_language(store, target_language)
}

fn prompt_for_target_language(current: NativeLanguage) -> Result<NativeLanguage, NanError> {
    let mut stderr = io::stderr().lock();
    writeln!(
        stderr,
        "Stored translations are inconsistent. Choose a target language for rewriting [english/chinese] (current setting: {}).",
        current.as_str()
    )
    .map_err(|error| NanError::message(format!("failed to write prompt: {error}")))?;

    loop {
        write!(stderr, "> ")
            .map_err(|error| NanError::message(format!("failed to write prompt: {error}")))?;
        stderr
            .flush()
            .map_err(|error| NanError::message(format!("failed to flush prompt: {error}")))?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|error| NanError::message(format!("failed to read input: {error}")))?;
        let trimmed = input.trim();
        match trimmed {
            "english" => return Ok(NativeLanguage::English),
            "chinese" => return Ok(NativeLanguage::Chinese),
            _ => {
                writeln!(stderr, "Please enter `english` or `chinese`.").map_err(|error| {
                    NanError::message(format!("failed to write prompt feedback: {error}"))
                })?;
            }
        }
    }
}
