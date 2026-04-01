use crate::cli::ListTarget;
use crate::error::NanError;
use crate::store::Store;

pub fn run(_store: &Store, _n: Option<isize>, _target: Option<ListTarget>) -> Result<(), NanError> {
    Err(NanError::message("`nan list` is not implemented yet"))
}
