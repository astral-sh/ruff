use itertools::Itertools;
use rustpython_parser::ast::Alias;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AutofixKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_stdlib::future::ALL_FEATURE_NAMES;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum UnusedImportContext {
    ExceptHandler,
    Init,
}

#[violation]
pub struct UnusedImport {
    pub name: String,
    pub context: Option<UnusedImportContext>,
    pub multiple: bool,
}

impl Violation for UnusedImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedImport { name, context, .. } = self;
        match context {
            Some(UnusedImportContext::ExceptHandler) => {
                format!(
                    "`{name}` imported but unused; consider using `importlib.util.find_spec` to test for availability"
                )
            }
            Some(UnusedImportContext::Init) => {
                format!(
                    "`{name}` imported but unused; consider adding to `__all__` or using a redundant \
                     alias"
                )
            }
            None => format!("`{name}` imported but unused"),
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let UnusedImport { context, .. } = self;
        context
            .is_none()
            .then_some(|UnusedImport { name, multiple, .. }| {
                if *multiple {
                    "Remove unused import".to_string()
                } else {
                    format!("Remove unused import: `{name}`")
                }
            })
    }
}
#[violation]
pub struct ImportShadowedByLoopVar {
    pub name: String,
    pub line: usize,
}

impl Violation for ImportShadowedByLoopVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportShadowedByLoopVar { name, line } = self;
        format!("Import `{name}` from line {line} shadowed by loop variable")
    }
}

#[violation]
pub struct UndefinedLocalWithImportStar {
    pub name: String,
}

impl Violation for UndefinedLocalWithImportStar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocalWithImportStar { name } = self;
        format!("`from {name} import *` used; unable to detect undefined names")
    }
}

#[violation]
pub struct LateFutureImport;

impl Violation for LateFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`from __future__` imports must occur at the beginning of the file")
    }
}

#[violation]
pub struct UndefinedLocalWithImportStarUsage {
    pub name: String,
    pub sources: Vec<String>,
}

impl Violation for UndefinedLocalWithImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocalWithImportStarUsage { name, sources } = self;
        let sources = sources
            .iter()
            .map(|source| format!("`{source}`"))
            .join(", ");
        format!("`{name}` may be undefined, or defined from star imports: {sources}")
    }
}

#[violation]
pub struct UndefinedLocalWithNestedImportStarUsage {
    pub name: String,
}

impl Violation for UndefinedLocalWithNestedImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocalWithNestedImportStarUsage { name } = self;
        format!("`from {name} import *` only allowed at module level")
    }
}

#[violation]
pub struct FutureFeatureNotDefined {
    pub name: String,
}

impl Violation for FutureFeatureNotDefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureFeatureNotDefined { name } = self;
        format!("Future feature `{name}` is not defined")
    }
}

pub fn future_feature_not_defined(checker: &mut Checker, alias: &Alias) {
    if !ALL_FEATURE_NAMES.contains(&&*alias.node.name) {
        checker.diagnostics.push(Diagnostic::new(
            FutureFeatureNotDefined {
                name: alias.node.name.to_string(),
            },
            Range::from(alias),
        ));
    }
}
