use crate::ExitStatus;
use anyhow::Result;
use ruff_server::WorkspaceTrust;

pub(crate) fn run_server(
    preview: Option<bool>,
    workspace_trust: WorkspaceTrust,
) -> Result<ExitStatus> {
    ruff_server::run(preview, workspace_trust)?;
    Ok(ExitStatus::Success)
}
