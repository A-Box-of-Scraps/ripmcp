use crate::error::{Error, ErrorKind};
use serde::Serialize;
use std::io::Write;

pub fn json(writer: &mut impl Write, value: &impl Serialize) -> Result<(), Error> {
    serde_json::to_writer(&mut *writer, value)
        .map_err(|_| Error::new(ErrorKind::Io, "cannot write JSON output"))?;
    writer
        .write_all(b"\n")
        .map_err(|_| Error::new(ErrorKind::Io, "cannot write output newline"))
}
