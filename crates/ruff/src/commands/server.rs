use crate::ExitStatus;
use anyhow::Result;

pub(crate) fn run_server(preview: Option<bool>) -> Result<ExitStatus> {
    ruff_server::run(preview)?;
    Ok(ExitStatus::Success)
}
