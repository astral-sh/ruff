use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;

use crate::ROOT_DIR;

pub fn replace_readme_section(content: &str, begin_pragma: &str, end_pragma: &str) -> Result<()> {
    // Read the existing file.
    let file = PathBuf::from(ROOT_DIR).join("README.md");
    let existing = fs::read_to_string(&file)?;

    // Extract the prefix.
    let index = existing
        .find(begin_pragma)
        .expect("Unable to find begin pragma");
    let prefix = &existing[..index + begin_pragma.len()];

    // Extract the suffix.
    let index = existing
        .find(end_pragma)
        .expect("Unable to find end pragma");
    let suffix = &existing[index..];

    // Write the prefix, new contents, and suffix.
    let mut f = OpenOptions::new().write(true).truncate(true).open(&file)?;
    writeln!(f, "{prefix}")?;
    write!(f, "{content}")?;
    write!(f, "{suffix}")?;

    Ok(())
}
