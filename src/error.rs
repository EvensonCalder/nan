use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct NanError {
    message: String,
}

impl NanError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for NanError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for NanError {}
