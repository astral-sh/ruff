use std::io::{self, BufWriter, Write};

use anyhow::Result;

use crate::args::HelpFormat;

/// Display version information
pub(crate) fn version(output_format: HelpFormat) -> Result<()> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    let version_info = crate::version::version();

    match output_format {
        HelpFormat::Text => {
            writeln!(stdout, "ruff {}", &version_info)?;
        }
        HelpFormat::Json => {
            serde_json::to_writer_pretty(stdout, &version_info)?;
        }
    }
    Ok(())
}
