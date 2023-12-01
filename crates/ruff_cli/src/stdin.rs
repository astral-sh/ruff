use std::io;
use std::io::{Read, Write};

/// Read a string from `stdin`.
pub(crate) fn read_from_stdin() -> Result<String, io::Error> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}

/// Read bytes from `stdin` and write them to `stdout`.
pub(crate) fn parrot_stdin() -> Result<(), io::Error> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    io::stdout().write_all(buffer.as_bytes())?;
    Ok(())
}
