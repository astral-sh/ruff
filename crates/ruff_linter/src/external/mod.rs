pub mod ast;
pub mod error;
pub mod runtime;

use serde::Deserialize;
use std::path::PathBuf;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct PyprojectExternalLinterEntry {
    pub toml_path: PathBuf,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

pub use ast::definition::ExternalAstLinterFile;
pub use ast::loader::{load_linter_from_entry, load_linter_into_registry};
pub use ast::registry::{ExternalLintRegistry, LinterIndex, RuleLocator};
pub use ast::rule::{
    ExternalAstLinter, ExternalAstRule, ExternalAstRuleSpec, ExternalRuleCode, ExternalRuleScript,
};
pub use ast::target::{AstNodeClass, AstTarget, AstTargetSpec, ExprKind, StmtKind};
pub use error::ExternalLinterError;
pub use runtime::ExternalLintRuntimeHandle;
pub use runtime::verify_registry_scripts;
