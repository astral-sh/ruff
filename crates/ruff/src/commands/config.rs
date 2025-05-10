use anyhow::{anyhow, Result};

use crate::args::HelpFormat;

use ruff_options_metadata::OptionsMetadata;
use ruff_workspace::options::Options;

#[expect(clippy::print_stdout)]
pub(crate) fn config(key: Option<&str>, format: HelpFormat) -> Result<()> {
    match key {
        None => {
            let metadata = Options::metadata();
            match format {
                HelpFormat::Text => {
                    println!("{metadata}");
                }

                HelpFormat::Json => {
                    println!("{}", &serde_json::to_string_pretty(&metadata)?);
                }
            }
        }
        Some(key) => match Options::metadata().find(key) {
            None => {
                return Err(anyhow!("Unknown option: {key}"));
            }
            Some(entry) => match format {
                HelpFormat::Text => {
                    print!("{entry}");
                }

                HelpFormat::Json => {
                    println!("{}", &serde_json::to_string_pretty(&entry)?);
                }
            },
        },
    }
    Ok(())
}
