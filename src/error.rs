use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorKind {
    Io = 1,
    Usage = 2,
    Configuration = 3,
    Connection = 4,
    Protocol = 5,
    Authentication = 6,
    ToolResult = 7,
    PartialFailure = 8,
    Timeout = 9,
    Unsupported = 10,
    Cancelled = 130,
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: &'static str,
}

impl Error {
    pub const fn new(kind: ErrorKind, message: &'static str) -> Self {
        Self { kind, message }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for Error {}
