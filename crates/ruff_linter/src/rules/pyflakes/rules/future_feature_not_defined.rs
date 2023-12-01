use ruff_python_ast::Alias;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::future::is_feature_name;
use ruff_text_size::Ranged;

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
/// - [Python documentation: `__future__`](https://docs.python.org/3/library/__future__.html)
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
    if is_feature_name(&alias.name) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        FutureFeatureNotDefined {
            name: alias.name.to_string(),
        },
        alias.range(),
    ));
}
