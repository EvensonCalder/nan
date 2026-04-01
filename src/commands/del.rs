use crate::error::NanError;
use crate::store::Store;

pub fn run(_store: &Store, _n: usize) -> Result<(), NanError> {
    Err(NanError::message("`nan del` is not implemented yet"))
}
