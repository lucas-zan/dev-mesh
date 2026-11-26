//! Framework-independent domain and wire contracts shared by DevMesh programs.
mod issue;
mod lifecycle;
mod wire;
pub use issue::*;
pub use lifecycle::*;
pub use wire::*;

#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct Error {
    pub code: String,
    pub message: String,
}
impl Error {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub mod socket;
