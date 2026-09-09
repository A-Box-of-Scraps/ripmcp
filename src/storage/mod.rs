mod directory;
mod maintenance;
mod paths;
mod transaction;
pub(crate) use maintenance::{Maintenance, acquire_file};

pub use directory::Directory;
pub use paths::{Location, Paths};
pub(crate) use transaction::LockedStore;
pub use transaction::Store;

use crate::error::{Error, ErrorKind};

pub(crate) fn invalid() -> Error {
    Error::new(
        ErrorKind::Configuration,
        "unsafe or inaccessible storage path",
    )
}

pub(crate) fn io_error(_: impl std::fmt::Debug) -> Error {
    Error::new(ErrorKind::Io, "storage operation failed")
}
