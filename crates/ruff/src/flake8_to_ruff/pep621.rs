//! Extract PEP 621 configuration settings from a pyproject.toml.

use crate::settings::types::RequiresVersion;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Project {
    #[serde(alias = "requires-python", alias = "requires_python")]
    pub requires_python: Option<RequiresVersion>,
}
