use anyhow::{anyhow, Result};

use ruff_workspace::options::Options;

#[allow(clippy::print_stdout)]
pub(crate) fn config(key: Option<&str>) -> Result<()> {
    match key {
        None => print!("{}", Options::metadata()),
        Some(key) => match Options::metadata().get(key) {
            None => {
                return Err(anyhow!("Unknown option: {key}"));
            }
            Some(entry) => {
                print!("{entry}");
            }
        },
    }
    Ok(())
}
