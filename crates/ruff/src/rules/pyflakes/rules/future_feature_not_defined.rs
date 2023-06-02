use rustpython_parser::ast::{Alias, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::future::ALL_FEATURE_NAMES;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__future__` imports that are not defined in the current Python
/// version.
///
/// ## Why is this bad?
/// Importing undefined or unsupported members from the `__future__` module is
/// a `SyntaxError`.
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/__future__.html)
#[violation]
pub struct FutureFeatureNotDefined {
    name: String,
}

impl Violation for FutureFeatureNotDefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureFeatureNotDefined { name } = self;
        format!("Future feature `{name}` is not defined")
    }
}

pub(crate) fn future_feature_not_defined(checker: &mut Checker, alias: &Alias) {
    if !ALL_FEATURE_NAMES.contains(&alias.name.as_str()) {
        checker.diagnostics.push(Diagnostic::new(
            FutureFeatureNotDefined {
                name: alias.name.to_string(),
            },
            alias.range(),
        ));
    }
}
