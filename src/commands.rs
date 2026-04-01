use crate::cli::Cli;
use crate::error::NanError;

pub fn run() -> Result<(), NanError> {
    let _cli = Cli::parse();
    Ok(())
}
