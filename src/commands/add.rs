use crate::error::NanError;
use crate::store::Store;

pub fn run(_store: &Store, _sentence: String, _style: Option<String>) -> Result<(), NanError> {
    Err(NanError::message("`nan add` is not implemented yet"))
}
