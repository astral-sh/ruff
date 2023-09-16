use std::io;
use std::io::Read;

/// Read a string from `stdin`.
pub(crate) fn read_from_stdin() -> Result<String, io::Error> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}
