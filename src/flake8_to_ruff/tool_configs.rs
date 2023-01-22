use std::path::Path;

use anyhow::Result;

use super::{black::Black, isort::Isort, parse_black_options, parse_isort_options};

pub struct ToolConfigs {
    pub black: Option<Black>,
    pub isort: Option<Isort>,
}

pub fn parse_tool_configs<P: AsRef<Path>>(path: P) -> Result<ToolConfigs> {
    Ok(ToolConfigs {
        black: parse_black_options(path.as_ref())?,
        isort: parse_isort_options(path)?,
    })
}
