use crate::cli::SetKey;
use crate::error::NanError;
use crate::store::Store;

pub fn run(_store: &Store, _key: SetKey, _option: String) -> Result<(), NanError> {
    Err(NanError::message("`nan set` is not implemented yet"))
}
