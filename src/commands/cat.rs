use crate::error::NanError;
use crate::store::Store;

pub fn run(_store: &Store, _n: Option<usize>) -> Result<(), NanError> {
    Err(NanError::message("`nan cat` is not implemented yet"))
}
