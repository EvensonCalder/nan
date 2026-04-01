use crate::cli::resolve_new_args;
use crate::error::NanError;
use crate::store::Store;

pub fn run(_store: &Store, first: Option<String>, second: Option<String>) -> Result<(), NanError> {
    let _resolved = resolve_new_args(first.as_deref(), second.as_deref())?;
    Err(NanError::message("`nan new` is not implemented yet"))
}
